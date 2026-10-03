//! Font resolution and shaping on DirectWrite.
//!
//! Shaped at the analyzer level rather than by handing a string to
//! `IDWriteTextLayout`, because the layout core owns line breaking: it needs the
//! per-glyph advances to break from, and the painter then has to draw exactly those
//! glyphs. Both go through [`FontEngine::shape_runs`], so a line cannot measure one
//! width and paint another.
//!
//! Fallback is resolved per run by asking each candidate face whether it covers the
//! run, in theme order -- the node's Latin family, its CJK family, then the declared
//! fallbacks. That is what lets one paragraph set Chinese and Latin from two faces
//! without the caller saying which is which.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::OnceLock;

use rubrica_type::classify::is_cjk_punct;
use rubrica_type::paragraph::{Measure, StyleId};
use rubrica_type::units::Pt;
use windows::core::{BOOL, PCWSTR, Result as WResult};
use windows::Win32::Foundation::E_FAIL;
mod analysis;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_METRICS,
    DWRITE_GLYPH_METRICS, DWRITE_GLYPH_OFFSET, DWRITE_SCRIPT_ANALYSIS,
    DWRITE_SHAPING_GLYPH_PROPERTIES,
    DWRITE_SHAPING_TEXT_PROPERTIES, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWriteCreateFactory,
    IDWriteFactory, IDWriteFontCollection, IDWriteFont, IDWriteFontFace, IDWriteFontFile,
    IDWriteTextAnalyzer, IDWriteTextFormat, IDWriteTextLayout,
};

#[derive(Clone)]
struct Face {
    face: IDWriteFontFace,
    metrics: DWRITE_FONT_METRICS,
    family: String,
}

/// A shaped, positioned run: the painter's unit of work.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub bidi_level: u8,
    /// Index into the engine's face table.
    pub face: usize,
    pub size: Pt,
    /// Byte range within the block's text.
    pub text: Range<usize>,
    pub glyphs: Vec<u16>,
    /// Horizontal advance per glyph, in points, tracking folded in.
    pub advances: Vec<f32>,
    pub offsets: Vec<DWRITE_GLYPH_OFFSET>,
    /// Glyph index for every UTF-16 code unit, including all units of a ligature.
    pub clusters: Vec<u16>,
    /// Ascender height at `size`, in points, for placing the baseline.
    pub ascent: Pt,
    pub descent: Pt,
    pub line_gap: Pt,
}

impl GlyphRun {
    pub fn width(&self) -> Pt {
        self.advances.iter().sum()
    }
}

/// A family pair plus the properties that pick a face within it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FaceRequest {
    pub family: String,
    pub cjk_family: String,
    pub japanese_family: String,
    pub korean_family: String,
    pub cjk_italic: Option<bool>,
    /// Tried only when neither named family covers the run, so a rare glyph still
    /// renders instead of being dropped.
    pub fallback: Vec<String>,
    pub weight: u16,
    pub italic: bool,
}

/// The vertical box an inline object claims around the baseline, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectBox {
    pub advance: Pt,
    pub ascent: Pt,
    pub descent: Pt,
}

/// Style table entry: what a `StyleId` means to the renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub face: FaceRequest,
    pub size: Pt,
    pub tracking: f32,
    /// Set for an inline object, which is measured from its own box rather than
    /// from any font.
    pub object: Option<ObjectBox>,
}

pub struct FontEngine {
    analyzer: IDWriteTextAnalyzer,
    /// The system font collection this engine resolves faces against, resolved
    /// from the process-wide one on first use.
    collection: OnceLock<Option<&'static IDWriteFontCollection>>,
    faces: RefCell<Vec<Face>>,
    resolved: RefCell<HashMap<(String, u16, bool), Option<usize>>>,
    shaped: RefCell<LayeredMap<ShapeKey, Vec<GlyphRun>>>,
    analyzed: RefCell<LayeredMap<String, Rc<Vec<TextRun>>>>,
    /// `(position, thickness)` of this face's strikeout rule, in design units, read
    /// from its file: a face is asked once, however many struck runs it carries.
    strikeout: RefCell<HashMap<usize, Option<(f32, f32)>>>,
    /// The source sfnt bytes behind a face, read through DirectWrite when an export
    /// needs an embeddable font rather than a COM draw handle. The reader window draws
    /// through COM and never asks for the file itself.
    #[cfg_attr(not(feature = "pdf"), allow(dead_code))]
    font_files: RefCell<HashMap<usize, (Vec<u8>, usize)>>,
    styles: RefCell<Vec<Style>>,
    /// The paragraph whose itemization is current.
    ///
    /// Measurement arrives with the paragraph's text and a range inside it, once per
    /// word. Looking the itemization up by hashing that text answers each word with a
    /// pass over the whole paragraph, which is what makes one long paragraph cost the
    /// square of its length. The layout opens a paragraph here, and while one is open
    /// its itemization is found by the text's own address: the paragraph is alive for
    /// as long as its words are measured, so an equal address and length can only be
    /// the paragraph itself. Any other text -- a label, a marker, a table cell -- has
    /// an address of its own and goes to the cache.
    paragraph: RefCell<Option<OpenParagraph>>,
}

/// The paragraph [`FontEngine::begin_paragraph`] opened.
struct OpenParagraph {
    /// The open paragraph's address and length, which identify it while it lives.
    owner: (*const u8, usize),
    runs: Rc<Vec<TextRun>>,
}

/// A content-addressed cache that retires a whole generation at a time.
///
/// Clearing a cache on overflow is the usual cheap answer, but what it throws away
/// is the half a long document is still using: the next layout re-measures every
/// one of those runs, and a document that outruns the cap pays that cost on every
/// pass. Two generations keep the entries a document is still using alive for one
/// more pass around the cache, which is the difference between a book being set
/// twice and being set once per generation.
/// Runs kept shaped per generation. A run is a face, a size, a script and the text,
/// so a page of prose repeats most of its runs and a second layout of the same page
/// costs one lookup per run rather than one `GetGlyphs` per run.
const SHAPED_CACHE_CAP: usize = 65_536;

/// Itemized runs kept per generation. Keyed by the whole string handed to the
/// itemizer, so a paragraph is analysed once however often its pieces are measured.
const ANALYZED_CACHE_CAP: usize = 65_536;

/// The family drawn with when the request names none that this machine has. Every
/// Windows that can create a font engine has it, so a run of text is never dropped for
/// want of a face to draw it with.
const LAST_RESORT_FAMILY: &str = "Segoe UI";

struct LayeredMap<K: std::hash::Hash + Eq, V> {
    hot: HashMap<K, V>,
    cold: HashMap<K, V>,
}

impl<K: std::hash::Hash + Eq, V> Default for LayeredMap<K, V> {
    fn default() -> Self {
        Self { hot: HashMap::new(), cold: HashMap::new() }
    }
}

impl<K: std::hash::Hash + Eq, V> LayeredMap<K, V> {
    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.hot.get(key).or_else(|| self.cold.get(key))
    }

    fn len(&self) -> usize {
        self.hot.len() + self.cold.len()
    }

    fn clear(&mut self) {
        self.hot.clear();
        self.cold.clear();
    }

    /// Offer `key` to the hot generation, retiring the one before it if it is full.
    fn insert(&mut self, key: K, value: V, cap: usize) {
        if self.hot.len() >= cap {
            self.cold = std::mem::take(&mut self.hot);
        }
        self.hot.insert(key, value);
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct ShapeKey {
    bidi_level: u8,
    script: u16,
    shapes: i32,
    /// Face index, not a family name: two weights of one family would otherwise
    /// collide and hand back kerning measured off the wrong face.
    face: usize,
    /// Sizes and tracking quantised to 1/64 pt so the key is integral.
    size_q: i32,
    track_q: i32,
    /// Content-addressed: the same characters at the same size shape the same way
    /// wherever they appear, so repeated text across a document stays a hit.
    text: Box<[u16]>,
}

#[derive(Clone)]
struct TextRun {
    range: Range<usize>,
    level: u8,
    language: u8,
    script: DWRITE_SCRIPT_ANALYSIS,
}

/// The one system font collection the process builds, on first use.
///
/// Enumerating every installed font costs tens to hundreds of milliseconds on a
/// fresh process -- and that was paid once per engine, so also on every relayout
/// of the async layout worker's. One collection answers every engine for the life
/// of the process; a machine where the enumeration fails keeps the `None`, so the
/// cost is not paid again on every call.
static SYSTEM_FONTS: OnceLock<Option<IDWriteFontCollection>> = OnceLock::new();

/// The shared system font collection, building it on first call.
fn system_font_collection() -> Option<&'static IDWriteFontCollection> {
    SYSTEM_FONTS
        .get_or_init(|| unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let mut collection = None;
            factory.GetSystemFontCollection(&mut collection, false).ok()?;
            collection
        })
        .as_ref()
}

impl FontEngine {
    pub fn new() -> WResult<FontEngine> {
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let analyzer = factory.CreateTextAnalyzer()?;
            Ok(FontEngine {
                analyzer,
                collection: OnceLock::new(),
                faces: RefCell::new(Vec::new()),
                resolved: RefCell::new(HashMap::new()),
                shaped: RefCell::new(LayeredMap::default()),
                analyzed: RefCell::new(LayeredMap::default()),
                paragraph: RefCell::new(None),
                strikeout: RefCell::new(HashMap::new()),
                font_files: RefCell::new(HashMap::new()),
                styles: RefCell::new(Vec::new()),
            })
        }
    }

    /// The system font collection behind face resolution, shared by the process.
    fn collection(&self) -> Option<&'static IDWriteFontCollection> {
        *self.collection.get_or_init(system_font_collection)
    }

    /// Look up the style a `StyleId` denotes.
    fn style_of(&self, style: StyleId) -> Style {
        let s = self.styles.borrow();
        match s.get(style.0 as usize) {
            Some(st) => st.clone(),
            // Measuring must not panic inside a paint callback, where there is no
            // caller to report to, so an unknown id gets a plain body style.
            None => Style {
                face: FaceRequest {
                    family: "Segoe UI".into(),
                    cjk_family: "Microsoft YaHei".into(),
                    fallback: vec![],
                    weight: 400,
                    italic: false,
                    ..Default::default()
                },
                size: 13.5,
                tracking: 0.0,
                object: None,
            },
        }
    }

    /// Install the style table the layout pass will index with its `StyleId`s.
    ///
    /// The whole table every time, never just the entries a window added: the ids in
    /// flight for the blocks already laid out resolve through this same table, and an
    /// id past its end is not an error here but a silent substitution of a default body
    /// style, which would set a page in the wrong face at the wrong size with no sign
    /// of having done so. Installing a longer table never disturbs a shorter one's
    /// indices, since the table is only ever appended to.
    pub fn begin_layout(&self, styles: Vec<Style>) {
        *self.styles.borrow_mut() = styles;
    }

    /// Police the itemization cache, once per layout rather than once per style table.
    ///
    /// The cache is content-addressed -- script, level and language come out of the
    /// text alone -- so it stays valid across layouts and documents, and dropping it
    /// per layout would re-analyse every paragraph of every switch. Only its size is
    /// policed, since the reader can walk through many documents without the window
    /// ever going away.
    ///
    /// It is not policed by [`FontEngine::begin_layout`] because a windowed layout
    /// installs one table per window: bound there, a long document would have its
    /// cache emptied between every window of itself and re-analyse the whole of the
    /// text behind it each time.
    pub fn trim_caches(&self) {
        const ANALYZED_CACHE_CAP: usize = 8192;
        if self.analyzed.borrow().len() > ANALYZED_CACHE_CAP {
            self.analyzed.borrow_mut().clear();
        }
    }

    fn find_family(&self, name: &str) -> Option<u32> {
        let w = utf16(name);
        let mut index = 0u32;
        let mut exists = BOOL(0);
        let collection = self.collection()?;
        unsafe {
            collection.FindFamilyName(PCWSTR(w.as_ptr()), &mut index, &mut exists).ok()?
        };
        (exists.0 != 0).then_some(index)
    }

    /// Whether this machine can actually set body-weight text in this family, which is
    /// what decides whether a face the reader is being offered is offered or greyed out.
    /// Asking for a face rather than for a name: a family registered with no readable
    /// file behind it is a name nothing can be drawn from.
    pub fn has_family(&self, family: &str) -> bool {
        self.face_for(family, 400, false).is_some()
    }

    /// Resolve (family, weight, slant) to a cached face.
    fn face_for(&self, family: &str, weight: u16, italic: bool) -> Option<usize> {
        let key = (family.to_string(), weight, italic);
        if let Some(hit) = self.resolved.borrow().get(&key) {
            return *hit;
        }
        let found = (|| {
            let index = self.find_family(family)?;
            let collection = self.collection()?;
            let fam = unsafe { collection.GetFontFamily(index).ok()? };
            let font: IDWriteFont = unsafe {
                fam.GetFirstMatchingFont(
                    DWRITE_FONT_WEIGHT(weight as i32),
                    DWRITE_FONT_STRETCH_NORMAL,
                    if italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL },
                )
                .ok()?
            };
            let face = unsafe { font.CreateFontFace().ok()? };
            let mut metrics = DWRITE_FONT_METRICS::default();
            unsafe { face.GetMetrics(&mut metrics) };
            let mut faces = self.faces.borrow_mut();
            let idx = faces.len();
            faces.push(Face { face, metrics, family: family.to_string() });
            Some(idx)
        })();
        self.resolved.borrow_mut().insert(key, found);
        found
    }

    /// Glyph id per character, or `None` if the face cannot be queried. Zero means
    /// .notdef, i.e. that character is missing from this face.
    fn glyphs_for(&self, idx: usize, text: &str) -> Option<Vec<u16>> {
        let faces = self.faces.borrow();
        let f = faces.get(idx)?;
        let cps: Vec<u32> = text.chars().map(|c| c as u32).collect();
        if cps.is_empty() {
            return Some(Vec::new());
        }
        let mut gs = vec![0u16; cps.len()];
        unsafe { f.face.GetGlyphIndices(cps.as_ptr(), cps.len() as u32, gs.as_mut_ptr()) }.ok()?;
        Some(gs)
    }

    /// The leading run of `text` the face can render, in bytes, from one
    /// `GetGlyphIndices` over the whole of it.
    ///
    /// Asked as a number rather than as two questions because each question is a full
    /// pass over the text: a face either covers it to the end or it stops at a
    /// character, and which of the two has to be known before either can be answered.
    fn coverage(&self, idx: usize, text: &str) -> Option<usize> {
        let glyphs = self.glyphs_for(idx, text)?;
        let mut n = 0usize;
        for (c, g) in text.chars().zip(glyphs) {
            // A variation selector has no glyph of its own -- it asks for another form
            // of the character before it -- so every face covers it. Read as an ordinary
            // character it is one nothing has, which is how the `️` of `⚠️` came to be
            // drawn as a `.notdef` box beside the warning sign.
            if g == 0 && !is_variation_selector(c) {
                break;
            }
            n += c.len_utf8();
        }
        Some(n)
    }

    /// Pick the face for `text`, honouring the Latin/CJK split, and report with it how
    /// much of the text that face can render.
    ///
    /// `None` means no candidate face could be opened at all, or none of them has even
    /// the first character: callers draw what is left with [`Self::last_resort_face`]
    /// rather than dropping it, because a wrong font is harder to notice than a
    /// `.notdef` box and a dropped run is not drawn at all.
    fn resolve_face(&self, req: &FaceRequest, text: &str) -> Option<(usize, usize)> {
        let cjk = text.chars().next().is_some_and(cjk_char);
        let mut order: Vec<&str> = if cjk {
            vec![req.cjk_family.as_str(), req.family.as_str()]
        } else {
            vec![req.family.as_str(), req.cjk_family.as_str()]
        };
        let script = text.chars().next().map_or(0, east_asian_script);
        let preferred = match script { 1 => &req.japanese_family, 2 => &req.korean_family, _ => "" };
        if !preferred.is_empty() { order.insert(0, preferred); }
        order.extend(req.fallback.iter().map(String::as_str));
        let mut partial = None;
        for family in order {
            let italic = if cjk { req.cjk_italic.unwrap_or(req.italic) } else { req.italic };
            let Some(idx) = self.face_for(family, req.weight, italic) else { continue };
            // One probe answers both halves of the question, so a candidate that comes
            // back neither covering nor empty costs a single pass over the text.
            let Some(covered) = self.coverage(idx, text) else { continue };
            if covered == text.len() {
                return Some((idx, covered));
            }
            // Keep the first face that at least rendered something: a partial
            // prefix still beats dropping the run.
            if partial.is_none() && covered > 0 {
                partial = Some((idx, covered));
            }
        }
        partial
    }

    /// A face to draw with when nothing on the request's list has the text at all.
    ///
    /// The requested family if this machine has it, then the first fallback that is
    /// installed, then Segoe UI -- which is on every Windows that has a font engine to
    /// ask. The run then shapes to `.notdef`, which is a box a reader can see and a
    /// report can count, instead of vanishing. Every candidate goes through the
    /// resolved-face cache, so a request made once per character of a long run costs
    /// one `String` key and no new COM object after the first.
    fn last_resort_face(&self, req: &FaceRequest) -> Option<usize> {
        let cjk = req.cjk_italic.unwrap_or(req.italic);
        self.face_for(&req.family, req.weight, req.italic)
            .or_else(|| self.face_for(&req.cjk_family, req.weight, cjk))
            .or_else(|| {
                req.fallback.iter().find_map(|f| self.face_for(f, req.weight, req.italic))
            })
            .or_else(|| self.face_for(LAST_RESORT_FAMILY, req.weight, req.italic))
    }

    /// The itemization of the paragraph under measurement, or of any other text the
    /// cache already holds.
    fn itemization(&self, text: &str) -> Rc<Vec<TextRun>> {
        if let Some(open) = self.paragraph.borrow().as_ref() {
            if open.owner == (text.as_ptr(), text.len()) {
                return open.runs.clone();
            }
        }
        if let Some(hit) = self.analyzed.borrow().get(text) {
            return hit.clone();
        }
        let language = east_asian_language(text);
        let bidi = rubrica_type::BidiInfo::new(text, None);
        let scripts =
            analysis::scripts(&self.analyzer, text, locale_for_text(text, language)).unwrap_or_default();
        let mut runs: Vec<TextRun> = Vec::new();
        let mut unit = 0;
        for (at, c) in text.char_indices() {
            let level = bidi.levels[at].number();
            let script = scripts.get(unit).copied().unwrap_or_default();
            unit += c.len_utf16();
            if let Some(last) = runs.last_mut().filter(|r| r.level == level && r.script == script) {
                last.range.end = at + c.len_utf8();
            } else {
                runs.push(TextRun { range: at..at + c.len_utf8(), level, script, language });
            }
        }
        let runs = Rc::new(runs);
        self.analyzed.borrow_mut().insert(text.to_owned(), runs.clone(), ANALYZED_CACHE_CAP);
        runs
    }

    /// Open a paragraph for measurement: its words are asked of [`Self::shape_runs`]
    /// with the paragraph's own text, and the calls this saves are what keep a long
    /// paragraph from costing its own length once per word.
    pub fn begin_paragraph(&self, text: &str) {
        let runs = self.itemization(text);
        *self.paragraph.borrow_mut() =
            Some(OpenParagraph { owner: (text.as_ptr(), text.len()), runs });
    }

    /// Shape `text[range]`, starting a new run wherever the face must change.
    pub fn shape_runs(
        &self,
        text: &str,
        range: Range<usize>,
        req: &FaceRequest,
        size: Pt,
        tracking: f32,
    ) -> Vec<GlyphRun> {
        let runs = self.itemization(text);
        let analysis: Vec<TextRun> = runs
            .iter()
            .filter_map(|r| {
                let start = r.range.start.max(range.start);
                let end = r.range.end.min(range.end);
                (start < end).then(|| TextRun { range: start..end, ..r.clone() })
            })
            .collect();
        // Cache the paragraph language with its script analysis: measuring each
        // ideograph must not scan a whole book paragraph again.
        let mut localized = req.clone();
        let family = match analysis.first().map(|r| r.language) {
            Some(1) => &req.japanese_family, Some(2) => &req.korean_family, _ => &req.cjk_family,
        };
        if !family.is_empty() { localized.cjk_family = family.clone(); }
        let req = &localized;
        let mut out = Vec::new();
        for item in analysis {
            let slice = &text[item.range.clone()];
            let mut at = 0usize;
            while at < slice.len() {
                let rest = &slice[at..];
                // Nothing on the request's list has even the next character. Draw what
                // is left of the run in one piece from the last resort face: `.notdef` is
                // a box a reader can see and a report can count, while stopping here
                // turns the rest of the run into invisible text.
                //
                // One piece rather than one character is also what keeps this loop linear
                // in the length of the run. Taking a single character and asking again
                // re-probed the whole remainder against every candidate family on every
                // iteration, so text no installed font covers -- a vendor's private-use
                // glyphs, a rare script -- cost the square of its own length to measure.
                let (face, taken) = match self.resolve_face(req, rest) {
                    Some((face, covered)) => {
                        (face, covered.max(first_char_len(rest)))
                    }
                    None => {
                        // No face at all: a machine with no fonts, and nothing to draw with.
                        let Some(face) = self.last_resort_face(req) else { break };
                        (face, rest.len())
                    }
                };
                let chunk = &rest[..taken];
                // A piece that is nothing but variation selectors draws nothing at all:
                // no face has a glyph for one standing on its own, and shaping it anyway
                // printed the missing-glyph box -- a box for a character that is not
                // there, which is what a selector whose base was edited away looks like.
                // It still occupies its own (zero) width, and the index still holds the
                // character, so copying the line hands back what the file says.
                if !chunk.chars().all(is_variation_selector) {
                    let mut runs = self.shape_with_face(chunk, item.range.start + at, face, size, tracking, &item);
                    out.append(&mut runs);
                }
                at += taken;
            }
        }
        let levels: Vec<_> = out.iter().map(|r| rubrica_type::Level::new(r.bidi_level).unwrap()).collect();
        rubrica_type::BidiInfo::reorder_visual(&levels).into_iter().map(|i| out[i].clone()).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_with_face(
        &self,
        text: &str,
        text_start: usize,
        face: usize,
        size: Pt,
        tracking: f32,
        item: &TextRun,
    ) -> Vec<GlyphRun> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let key = ShapeKey {
            bidi_level: item.level,
            script: item.script.script,
            shapes: item.script.shapes.0,
            face,
            size_q: (size * 64.0).round() as i32,
            track_q: (tracking * 4096.0).round() as i32,
            text: units.clone().into_boxed_slice(),
        };
        if let Some(hit) = self.shaped.borrow().get(&key) {
            // A cached run's `text` is an address in whichever string the first pass was
            // handed, not a fact about the shaping: the glyphs, clusters and advances
            // are all offset-free, so the hit is good and only its range needs moving.
            return hit
                .iter()
                .map(|r| GlyphRun {
                    text: text_start..text_start + (r.text.end - r.text.start),
                    ..r.clone()
                })
                .collect();
        }
        if units.is_empty() {
            return Vec::new();
        }
        // Clone the handle out instead of holding the borrow across the analyzer
        // calls: measuring can re-enter this engine to resolve a fallback face, and
        // a live borrow would panic the RefCell.
        let (face_obj, metrics) = {
            let borrow = self.faces.borrow();
            match borrow.get(face) {
                Some(f) => (f.face.clone(), f.metrics),
                None => return Vec::new(),
            }
        };
        let upem = metrics.designUnitsPerEm.max(1) as f32;
        let scale = size / upem;
        let n = units.len() as u32;

        let sa = item.script;
        let rtl = item.level % 2 == 1;
        let nul = utf16("");

        // A shaping pass can expand past the character count; grow and retry.
        let mut cap = units.len() + 16;
        let (glyphs, glyph_props, cluster_map, text_props) = loop {
            let mut cm = vec![0u16; units.len()];
            let mut tp = vec![DWRITE_SHAPING_TEXT_PROPERTIES::default(); units.len()];
            let mut gl = vec![0u16; cap];
            let mut gp = vec![DWRITE_SHAPING_GLYPH_PROPERTIES::default(); cap];
            let mut count = 0u32;
            let res = unsafe {
                self.analyzer.GetGlyphs(
                    PCWSTR(units.as_ptr()),
                    n,
                    Some(&face_obj),
                    false,
                    rtl,
                    &sa,
                    PCWSTR(nul.as_ptr()),
                    None,
                    None,
                    None,
                    0,
                    cap as u32,
                    cm.as_mut_ptr(),
                    tp.as_mut_ptr(),
                    gl.as_mut_ptr(),
                    gp.as_mut_ptr(),
                    &mut count,
                )
            };
            if res.is_ok() && (count as usize) <= cap {
                gl.truncate(count as usize);
                gp.truncate(count as usize);
                break (gl, gp, cm, tp);
            }
            if cap > units.len() * 8 {
                return Vec::new();
            }
            cap *= 2;
        };
        if glyphs.is_empty() {
            return Vec::new();
        }

        let count = glyphs.len() as u32;
        let mut advances = vec![0.0f32; count as usize];
        let mut offsets = vec![DWRITE_GLYPH_OFFSET::default(); count as usize];
        let placed = unsafe {
            self.analyzer.GetGlyphPlacements(
                PCWSTR(units.as_ptr()),
                cluster_map.as_ptr(),
                text_props.as_ptr() as *mut _,
                n,
                glyphs.as_ptr(),
                glyph_props.as_ptr(),
                count,
                Some(&face_obj),
                size,
                false,
                rtl,
                &sa,
                PCWSTR(nul.as_ptr()),
                None,
                None,
                0,
                advances.as_mut_ptr(),
                offsets.as_mut_ptr(),
            )
        };
        if placed.is_err() {
            // No kerning, no positioning, but the text still measures and draws.
            let mut dm = vec![DWRITE_GLYPH_METRICS::default(); count as usize];
            if unsafe { face_obj.GetDesignGlyphMetrics(glyphs.as_ptr(), count, dm.as_mut_ptr(), false) }
                .is_err()
            {
                return Vec::new();
            }
            advances = dm.iter().map(|m| m.advanceWidth as f32 * scale).collect();
            offsets = vec![DWRITE_GLYPH_OFFSET::default(); count as usize];
        }
        if tracking != 0.0 {
            for a in advances.iter_mut() {
                // Combining marks and bidi controls have no advance of their own.
                if *a > 0.0 { *a += tracking * size; }
            }
        }
        let run = GlyphRun {
            bidi_level: item.level,
            face,
            size,
            text: text_start..text_start + text.len(),
            clusters: cluster_map,
            glyphs,
            advances,
            offsets,
            ascent: metrics.ascent as f32 * scale,
            descent: metrics.descent as f32 * scale,
            line_gap: metrics.lineGap as f32 * scale,
        };
        self.shaped.borrow_mut().insert(key, vec![run.clone()], SHAPED_CACHE_CAP);
        vec![run]
    }

    /// The COM face handle, for `DrawGlyphRun`.
    pub fn font_face(&self, idx: usize) -> Option<IDWriteFontFace> {
        self.faces.borrow().get(idx).map(|f| f.face.clone())
    }

    /// Read the sfnt file and face index behind a resolved face.
    ///
    /// Direct2D only needs a COM face, but a PDF has to carry the font program so
    /// that its glyphs remain visible on another machine. Reading the same face through
    /// DirectWrite keeps the PDF's glyph ids and advances tied to the ones the reader
    /// actually painted, instead of guessing from a family name and a second font file.
    #[cfg_attr(not(feature = "pdf"), allow(dead_code))]
    pub(crate) fn font_file(&self, idx: usize) -> Option<(Vec<u8>, usize)> {
        if let Some(hit) = self.font_files.borrow().get(&idx) {
            return Some(hit.clone());
        }
        let face = self.font_face(idx)?;
        let mut count = 0u32;
        unsafe { face.GetFiles(&mut count, None).ok()? };
        if count == 0 {
            return None;
        }
        let mut files: Vec<Option<IDWriteFontFile>> = vec![None; count as usize];
        unsafe { face.GetFiles(&mut count, Some(files.as_mut_ptr())).ok()? };
        let file = files.into_iter().flatten().next()?;
        let mut key = std::ptr::null_mut();
        let mut key_size = 0u32;
        unsafe { file.GetReferenceKey(&mut key, &mut key_size).ok()? };
        let loader = unsafe { file.GetLoader().ok()? };
        let stream = unsafe { loader.CreateStreamFromKey(key, key_size).ok()? };
        let size = unsafe { stream.GetFileSize().ok()? };
        // A font file larger than this is not a sane document export and copying it
        // would turn a failed export into an unbounded allocation.
        if size == 0 || size > 256 * 1024 * 1024 {
            return None;
        }
        let mut fragment = std::ptr::null_mut();
        let mut context = std::ptr::null_mut();
        unsafe { stream.ReadFileFragment(&mut fragment, 0, size, &mut context).ok()? };
        if fragment.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(fragment.cast::<u8>(), size as usize) }.to_vec();
        unsafe { stream.ReleaseFileFragment(context) };
        let face_index = unsafe { face.GetIndex() } as usize;
        let result = (bytes, face_index);
        self.font_files.borrow_mut().insert(idx, result.clone());
        Some(result)
    }

    pub fn face_family(&self, idx: usize) -> String {
        self.faces.borrow().get(idx).map(|f| f.family.clone()).unwrap_or_default()
    }

    pub(crate) fn text_format(&self, family: &str, size: f32) -> WResult<IDWriteTextFormat> {
        let family = utf16(family);
        let locale = utf16("en-us");
        let Some(collection) = self.collection() else {
            return Err(windows::core::Error::from(E_FAIL));
        };
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = factory.CreateTextFormat(
                PCWSTR(family.as_ptr()),
                Some(collection),
                DWRITE_FONT_WEIGHT(400),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                PCWSTR(locale.as_ptr()),
            )?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            Ok(format)
        }
    }

    /// A single-line UI label at `size` dip, with the width it was measured at and the
    /// height of its layout box, both in dip.
    ///
    /// A label wider than `max_width` is trimmed with an ellipsis rather than run past
    /// the room it has, so the caller can size its control from the width alone. The
    /// layout comes back left-aligned and ready to draw: the text starts at the origin
    /// it is drawn at, which is what a control sized from `width` has to be able to
    /// assume -- a centred layout would set a short label in the middle of the wide
    /// measuring box and send it spilling out of the control that fits it.
    pub fn ui_label(&self, text: &str, family: &str, size: f32, max_width: f32) -> Option<(IDWriteTextLayout, f32, f32)> {
        let format = self.text_format(family, size).ok()?;
        // A layout takes the string's length as its own, so the terminator that a
        // PCWSTR would have wanted stays out of the measured text.
        let text = utf16(text);
        let text = &text[..text.len() - 1];
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let layout: IDWriteTextLayout = factory.CreateTextLayout(
                text,
                &format,
                max_width,
                size * 2.0,
            ).ok()?;
            layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING).ok()?;
            let sign = factory.CreateEllipsisTrimmingSign(&format).ok()?;
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            layout.SetTrimming(&trimming, &sign).ok()?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics).ok()?;
            Some((layout, metrics.width, metrics.layoutHeight))
        }
    }

    /// Where a strike through this face's text belongs: `(height above the baseline,
    /// thickness)`, in points at `size`.
    ///
    /// Read from the face's own `OS/2` rather than guessed from an average cap height,
    /// for the same reason the math engine reads `MATH`: a font that draws its hyphen
    /// low and a font that draws it high are struck through at different heights, and
    /// a rule that ignores that is a rule drawn on top of the glyphs rather than
    /// through them.
    pub fn strike_rule(&self, face: usize, size: Pt) -> (Pt, Pt) {
        let Some((face_obj, metrics)) =
            self.faces.borrow().get(face).map(|f| (f.face.clone(), f.metrics))
        else {
            return (size * STRIKE_FALLBACK_POS, size * STRIKE_FALLBACK_WEIGHT);
        };
        let upm = metrics.designUnitsPerEm.max(1) as f32;
        let s = size / upm;
        let cached = self.strikeout.borrow().get(&face).copied();
        let design = match cached {
            Some(hit) => hit,
            None => {
                let read = crate::tables::table_bytes(&face_obj, b"OS/2")
                    .as_deref()
                    .and_then(os2_strikeout);
                self.strikeout.borrow_mut().insert(face, read);
                read
            }
        };
        match design {
            Some((pos, weight)) => (pos * s, weight * s),
            None => (size * STRIKE_FALLBACK_POS, size * STRIKE_FALLBACK_WEIGHT),
        }
    }

    /// Resolve a family by name to a face index, for a backend that needs the face
    /// itself rather than shaped text -- the math engine reads its `MATH` table.
    pub(crate) fn open_face(&self, family: &str, weight: u16, italic: bool) -> Option<usize> {
        self.face_for(family, weight, italic)
    }

    /// Glyph ids for `text` in one face; zero where that face has no glyph.
    pub(crate) fn glyph_ids(&self, face: usize, text: &str) -> Option<Vec<u16>> {
        self.glyphs_for(face, text)
    }

    /// `(advance, ink above the baseline, ink below it)` for one glyph, in points.
    ///
    /// The bearing box, not the face's global ascent: a formula places its bar and its
    /// accent against the top of the base that is actually there, and `x` and `h` are
    /// not the same height.
    pub(crate) fn glyph_extents(&self, face: usize, glyph: u16, size: Pt) -> (Pt, Pt, Pt) {
        let Some((face_obj, metrics)) =
            self.faces.borrow().get(face).map(|f| (f.face.clone(), f.metrics))
        else {
            return (0.0, 0.0, 0.0);
        };
        let mut m = [DWRITE_GLYPH_METRICS::default()];
        if unsafe { face_obj.GetDesignGlyphMetrics(&glyph, 1, m.as_mut_ptr(), false) }.is_err() {
            return (0.0, 0.0, 0.0);
        }
        let s = size / metrics.designUnitsPerEm.max(1) as f32;
        let g = m[0];
        (
            g.advanceWidth as f32 * s,
            (g.verticalOriginY - g.topSideBearing) as f32 * s,
            (g.advanceHeight as i32 - g.verticalOriginY - g.bottomSideBearing) as f32 * s,
        )
    }

    /// Sanity probe used at startup and by tests: can we shape anything at all.
    pub fn probe(&self) -> bool {
        let req = FaceRequest {
            family: "Segoe UI".into(),
            cjk_family: "Microsoft YaHei".into(),
            fallback: vec![],
            weight: 400,
            italic: false,
            ..Default::default()
        };
        let latin = "Ag";
        let han = "\u{4e2d}\u{6587}";
        !self.shape_runs(latin, 0..latin.len(), &req, 13.5, 0.0).is_empty()
            && !self.shape_runs(han, 0..han.len(), &req, 13.5, 0.0).is_empty()
    }
}

fn locale_for_text(text: &str, language: u8) -> &'static str {
    match language {
        1 => "ja-JP",
        2 => "ko-KR",
        _ if text.chars().any(cjk_char) => "zh-CN",
        _ => "en-US",
    }
}

/// Which East Asian face a block is written in, read from how much of its CJK text is
/// kana or Hangul rather than from whether any of that is present.
///
/// One face serves every ideograph in the block, so a single character of another
/// script must not be able to re-face a whole paragraph. Tested for mere presence, one
/// `の` was enough: `这是一段…含日文の注記。` had its leading Han drawn in Microsoft YaHei
/// and its trailing `注記。` in Yu Gothic, because the particle had declared the
/// paragraph Japanese -- one Chinese sentence set in two faces, which is exactly the
/// kind of seam that reads as broken typesetting. Weighting the mix is what says which
/// face the block is really written for, since a Japanese paragraph is kana throughout
/// while Han beside an occasional particle is Chinese.
///
/// A Japanese paragraph is written with kana throughout, so kana have to be a real
/// part of it rather than a character: one `の` quoted inside Chinese is a quotation,
/// not a change of language, and two is the smallest run that is writing. Hangul is all
/// of Korean, so Korean is read more loosely and survives the hanja it still writes in.
/// Punctuation counts for neither -- `。` is not a Han character, and letting it stand
/// in for one turned `日本語です。` into a Chinese paragraph.
fn east_asian_language(text: &str) -> u8 {
    let (mut kana, mut hangul, mut han) = (0u32, 0u32, 0u32);
    for c in text.chars() {
        match east_asian_script(c) {
            1 => kana += 1,
            2 => hangul += 1,
            _ if cjk_char(c) && !is_cjk_punct(c) => han += 1,
            _ => {}
        }
    }
    if hangul > 0 && hangul * 2 >= han { 2 }
    else if kana >= 2 && kana * 2 >= han { 1 }
    else { 0 }
}

fn east_asian_script(c: char) -> u8 {
    match c as u32 {
        0x3040..=0x30ff | 0x31f0..=0x31ff | 0xff66..=0xff9d => 1,
        0x1100..=0x11ff | 0x3130..=0x318f | 0xa960..=0xa97f | 0xac00..=0xd7ff => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;
    #[test]
    fn paragraph_context_selects_han_forms_without_misclassifying_latin() {
        assert_eq!(east_asian_language("中文，with Latin"), 0);
        assert_eq!(east_asian_language("漢字とかな"), 1);
        assert_eq!(east_asian_language("한글만"), 2);
        assert_eq!(east_asian_language("漢字 한글"), 2);
        assert_eq!(east_asian_script('한'), 2);
        assert_eq!(east_asian_script('あ'), 1);
        assert!(cjk_char('한'));
        assert!(!cjk_char('A'));
    }

    /// One face serves every ideograph in a block, so one character of another script
    /// must not be able to re-face the whole paragraph. Asked for mere presence, a
    /// single `の` declared a Chinese sentence Japanese: its leading Han came out in
    /// Microsoft YaHei and its trailing `注記。` in Yu Gothic, so one paragraph of
    /// Chinese was set in two faces -- the same seam a reader sees as broken
    /// typesetting, and far more visible than a stray gap.
    #[test]
    fn one_particle_does_not_re_face_a_chinese_paragraph() {
        // The Chinese cases: a particle, a kana word quoted inside Chinese, a Japanese
        // title. Each is Chinese written with something else in it.
        for text in [
            "这是一段普通的中文文字，含日文の注記。",
            "中文 with some English の mixed",
            "参考《日本語の文法》这本书的写法",
        ] {
            assert_eq!(east_asian_language(text), 0, "{text:?} was re-faced by its own kana");
            assert_eq!(locale_for_text(text, 0), "zh-CN");
        }
        // And the languages still win their own paragraphs, kana and Hangul being what
        // those paragraphs are mostly made of.
        for text in ["日本語のテキストです。", "漢字とかな", "これは日本語の文章です。"] {
            assert_eq!(east_asian_language(text), 1, "{text:?} lost its Japanese face");
        }
        for text in ["한글만", "漢字 한글", "한국어 문장입니다"] {
            assert_eq!(east_asian_language(text), 2, "{text:?} lost its Korean face");
        }
        // A paragraph that is one kanji and one particle is not a Japanese paragraph.
        assert_eq!(east_asian_language("文の"), 0);
    }

    /// The language a block is written in has to reach the page: with the real theme
    /// faces, a Chinese paragraph containing one `の` used to reach Yu Gothic for its
    /// trailing Han, which is where the mismatch became visible rather than theoretical.
    #[test]
    fn the_face_a_block_gets_follows_the_language_it_is_written_in() {
        let Ok(engine) = FontEngine::new() else { return };
        if !engine.probe() {
            return;
        }
        let req = FaceRequest {
            family: "Segoe UI".into(),
            cjk_family: "Microsoft YaHei".into(),
            japanese_family: "Yu Gothic".into(),
            korean_family: "Malgun Gothic".into(),
            fallback: vec![
                "Segoe UI".into(),
                "Microsoft YaHei".into(),
                "Segoe UI Symbol".into(),
                "Segoe UI Emoji".into(),
            ],
            weight: 400,
            italic: false,
            ..Default::default()
        };
        let text = "这是一段普通的中文文字，含日文の注記。";
        let runs = engine.shape_runs(text, 0..text.len(), &req, 13.5, 0.0);
        let families: Vec<String> =
            runs.iter().map(|r| engine.face_family(r.face).to_string()).collect();
        let han: Vec<&String> = families.iter().filter(|f| f.contains("YaHei")).collect();
        let japanese: Vec<&String> = families.iter().filter(|f| f.contains("Gothic")).collect();
        // Every Han character in the block is Chinese, so they all share one face...
        assert!(han.len() >= 2, "the Chinese of the block was not drawn in one face: {families:?}");
        // ...and only the kana itself is drawn in the Japanese face.
        assert!(
            japanese.len() <= 1 && text.contains('の'),
            "more than the kana was drawn in a Japanese face: {families:?}"
        );
        // Nothing fell through to a face that owns none of this and draws it as boxes.
        for run in &runs {
            assert!(!run.glyphs.contains(&0), "a box was drawn for {text:?}");
        }
    }

    #[test]
    fn paragraph_locale_follows_the_language_being_shaped() {
        assert_eq!(locale_for_text("中文", east_asian_language("中文")), "zh-CN");
        assert_eq!(locale_for_text("日本語です", east_asian_language("日本語です")), "ja-JP");
        assert_eq!(locale_for_text("한국어", east_asian_language("한국어")), "ko-KR");
        assert_eq!(locale_for_text("Latin", east_asian_language("Latin")), "en-US");
    }
}

/// Advances for the layout core. The style table was installed by `begin_layout`,
/// which is what keeps `rubrica-type` free of any theme knowledge.
impl Measure for FontEngine {
    fn advance(&mut self, text: &str, range: Range<usize>, style: StyleId) -> Pt {
        let st = self.style_of(style);
        if let Some(o) = st.object {
            return o.advance;
        }
        self.shape_runs(text, range, &st.face, st.size, st.tracking)
            .iter()
            .map(|r| r.width())
            .sum()
    }

    /// An inline object reports its own vertical extent; text leaves it zero so
    /// the line box keeps coming from the fonts' metrics.
    fn extent(&mut self, _text: &str, _range: Range<usize>, style: StyleId) -> (Pt, Pt) {
        match self.style_of(style).object {
            Some(o) => (o.ascent, o.descent),
            None => (0.0, 0.0),
        }
    }
}

fn first_char_len(s: &str) -> usize {
    s.chars().next().map_or(1, |c| c.len_utf8())
}

/// Whether a character belongs to a script that writes words without spaces between
/// them, which is what decides both the face it is shaped in and how a double-click
/// finds its extent.
pub fn cjk_char(c: char) -> bool {
    east_asian_script(c) != 0 || matches!(c as u32,
        0x2E80..=0x2EFF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FA1F)
}

/// U+FE00..=U+FE0F and U+E0100..=U+E01EF: an invisible modifier that selects a variant
/// form of the character before it, most often the emoji presentation of a symbol that
/// also has a text one. It draws nothing itself, so no face needs to own one -- see
/// [`FontEngine::coverage`](FontEngine).
fn is_variation_selector(c: char) -> bool {
    matches!(c as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF)
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A face that does not say where its strike goes -- no file, no `OS/2`, or a size of
/// zero -- gets one at a little under half the em, thin enough to read as a line drawn
/// through the word rather than as a word sitting on a rule: high enough to clear
/// lowercase, low enough to stay off the ascenders.
const STRIKE_FALLBACK_POS: f32 = 0.28;
const STRIKE_FALLBACK_WEIGHT: f32 = 0.06;

/// A face's own strikeout rule, read from its `OS/2` table: `(position, thickness)` in
/// design units.
///
/// The two fields are the INT16s at bytes 26 and 28 of every version of the table --
/// size first, then position, which is the order the spec gives and not the order the
/// names suggest. A size of zero is the font declining to answer, which is a `None`
/// rather than a rule drawn on the baseline.
fn os2_strikeout(bytes: &[u8]) -> Option<(f32, f32)> {
    let fields = bytes.get(26..30)?;
    let weight = i16::from_be_bytes([fields[0], fields[1]]) as f32;
    let pos = i16::from_be_bytes([fields[2], fields[3]]) as f32;
    (weight > 0.0).then_some((pos, weight))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `OS/2` version-4 table with only the two strikeout fields set.
    fn table(size: i16, position: i16) -> Vec<u8> {
        let mut b = vec![0u8; 96];
        b[0..2].copy_from_slice(&4u16.to_be_bytes());
        b[26..28].copy_from_slice(&size.to_be_bytes());
        b[28..30].copy_from_slice(&position.to_be_bytes());
        b
    }

    #[test]
    fn reads_the_strikeout_fields_where_the_spec_puts_them() {
        // Size at byte 26, position at 28 -- and the pair comes back the other way
        // round, because a caller wants height and then weight.
        assert_eq!(os2_strikeout(&table(50, 275)), Some((275.0, 50.0)));
    }

    #[test]
    fn a_font_that_declines_no_rule_gets_the_fallback() {
        assert_eq!(os2_strikeout(&table(0, 275)), None);
        // A table too short to hold the fields is the same answer: nothing to read.
        assert_eq!(os2_strikeout(&[0u8; 12]), None);
    }

    /// A run of `count` private-use codepoints: what a PDF-to-Markdown conversion of a
    /// Chinese textbook with vendor glyphs leaves behind, and what no installed family
    /// is likely to cover.
    fn private_use(count: u32) -> String {
        (0..count).map(|i| char::from_u32(0xE000 + i % 0x1000).expect("in the PUA")).collect()
    }

    /// Whether the runs come back covering `text` from end to end, which is what "the
    /// text was drawn" means: a run that was dropped is not a shorter run, it is a gap.
    fn covers_text(runs: &[GlyphRun], text: &str) -> bool {
        if runs.is_empty() {
            return false;
        }
        // The runs come back in visual order, so their extents are merged rather than
        // walked in the order they happen to have been drawn in.
        let mut spans: Vec<(usize, usize)> =
            runs.iter().map(|r| (r.text.start, r.text.end)).collect();
        spans.sort_unstable();
        let mut at = 0usize;
        for (start, end) in spans {
            if start > at {
                return false;
            }
            at = at.max(end);
        }
        at >= text.len()
    }

    /// A request whose family this machine has, with the fallbacks a theme carries
    /// beside it -- five candidates, so a run that finds no face in any of them pays
    /// five probes.
    fn installed_request() -> FaceRequest {
        FaceRequest {
            family: "Segoe UI".into(),
            cjk_family: "Microsoft YaHei".into(),
            fallback: vec!["Segoe UI Symbol".into(), "Segoe UI Emoji".into(), "Arial".into()],
            weight: 400,
            italic: false,
            ..Default::default()
        }
    }

    /// A variation selector is a modifier, not a character: it asks for another form of
    /// the glyph before it and draws nothing of its own, so no face has to have one.
    /// Read as an ordinary character it is one nothing owns, and the piece it landed in
    /// fell through to the last resort face -- which is how the `️` of `⚠️` came to be
    /// printed as a `.notdef` box right beside the warning sign.
    #[test]
    fn a_variation_selector_needs_no_face_of_its_own() {
        let Ok(engine) = FontEngine::new() else { return };
        if !engine.probe() {
            return;
        }
        for text in ["⚠️", "⚠️ 动手前必读", "✅ 完成 🎉"] {
            let runs = engine.shape_runs(text, 0..text.len(), &installed_request(), 13.5, 0.0);
            assert!(covers_text(&runs, text), "{text:?} was dropped");
            assert!(
                !runs.iter().any(|r| r.glyphs.contains(&0)),
                "{text:?} drew a box for a character a face here owns: {:?}",
                runs.iter().map(|r| (r.face, r.glyphs.clone())).collect::<Vec<_>>()
            );
        }
        // And with no emoji face on the list at all -- a profile whose fallback the
        // reader has replaced, or a machine without the font: the ⚠ is an old symbol
        // several text faces own, and the selector after it is owned by none of them,
        // but it must still draw nothing rather than a box.
        let req = FaceRequest {
            fallback: vec!["Segoe UI Symbol".into(), "Arial".into()],
            ..installed_request()
        };
        let text = "⚠️ 动手前必读";
        let runs = engine.shape_runs(text, 0..text.len(), &req, 13.5, 0.0);
        assert!(covers_text(&runs, text), "text was dropped without an emoji face");
        assert!(
            !runs.iter().any(|r| r.glyphs.contains(&0)),
            "a variation selector was drawn as a box: {:?}",
            runs.iter().map(|r| (r.face, r.glyphs.clone())).collect::<Vec<_>>()
        );
        // And one whose base character has been edited away: there is nothing for it to
        // select and nothing to draw, so it is skipped rather than printed as the box an
        // ownerless character gets.
        let text = "⚠ ️ 警告";
        let runs = engine.shape_runs(text, 0..text.len(), &req, 13.5, 0.0);
        assert!(
            !runs.iter().any(|r| r.glyphs.contains(&0)),
            "an orphaned variation selector drew a box: {:?}",
            runs.iter().map(|r| (r.face, r.glyphs.clone())).collect::<Vec<_>>()
        );
    }

    /// Measuring a run nothing can render is linear in its length, not the square of
    /// it. The old loop took one character and asked again, re-probing the whole
    /// remainder against every candidate each time: 20 000 characters cost 100 000
    /// `GetGlyphIndices` over 20 000 characters, and again on every repaint.
    #[test]
    fn a_run_no_installed_face_covers_is_measured_in_one_pass() {
        let Ok(engine) = FontEngine::new() else { return };
        if !engine.probe() {
            return;
        }
        let text = private_use(20_000);
        let started = std::time::Instant::now();
        let runs = engine.shape_runs(&text, 0..text.len(), &installed_request(), 13.5, 0.0);
        let took = started.elapsed();
        assert!(covers_text(&runs, &text), "uncovered text was not all drawn");
        // Generous, because this is a bound rather than a measurement -- but the old
        // shape of the loop needed minutes here, and the whole point is that it does not.
        assert!(took.as_secs() < 10, "20 000 uncovered characters took {took:?}");
    }

    /// Text that no family on the request can render is still drawn, as `.notdef`.
    ///
    /// The families of a `Book` profile or a typography preset are named whether or not
    /// they are installed, and a Windows Server Core image or a container is missing
    /// most of them. When the request's own fallback list ran out as well, the run used
    /// to stop there and the text from that character to the end of the line simply was
    /// not on the page -- no glyph, no box, nothing for a reader to notice.
    #[test]
    fn a_family_this_machine_does_not_have_does_not_take_the_text_with_it() {
        let Ok(engine) = FontEngine::new() else { return };
        if !engine.probe() {
            return;
        }
        let text = private_use(4_000);
        let req = FaceRequest {
            family: "Rubrica No Such Family".into(),
            cjk_family: "Rubrica Neither Does This One".into(),
            fallback: vec![],
            weight: 400,
            italic: false,
            ..Default::default()
        };
        let runs = engine.shape_runs(&text, 0..text.len(), &req, 13.5, 0.0);
        assert!(covers_text(&runs, &text), "text was dropped from a missing family");
        assert!(runs.iter().any(|r| r.glyphs.contains(&0)),
            "and it should be drawn as .notdef boxes, not silently substituted");
    }
}

