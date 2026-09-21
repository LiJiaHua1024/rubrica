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

use rubrica_type::paragraph::{Measure, StyleId};
use rubrica_type::units::Pt;
use windows::core::{BOOL, PCWSTR, Result as WResult};
use windows::Win32::Foundation::E_FAIL;
/// Script handed to the analyzer.
///
/// Zero is USP10's common, neutral script: it asks for no script-specific glyph
/// forms. Naming the real script (Han, Latin, ...) would need `IDWriteTextAnalysisSource`
/// and `..Sink` COM implementations, since `AnalyzeScript` takes interfaces rather than
/// buffers -- a deliberate follow-up, not an oversight, and shaping degrades to
/// font-default behaviour until then.
const SCRIPT_COMMON: u16 = 0;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_GLYPH_METRICS, DWRITE_GLYPH_OFFSET, DWRITE_SCRIPT_ANALYSIS,
    DWRITE_SCRIPT_SHAPES_DEFAULT, DWRITE_SHAPING_GLYPH_PROPERTIES,
    DWRITE_SHAPING_TEXT_PROPERTIES, DWriteCreateFactory,
    IDWriteFactory, IDWriteFontCollection, IDWriteFont, IDWriteFontFace, IDWriteTextAnalyzer,
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
    /// Index into the engine's face table.
    pub face: usize,
    pub size: Pt,
    /// Byte range within the block's text.
    pub text: Range<usize>,
    pub glyphs: Vec<u16>,
    /// Horizontal advance per glyph, in points, tracking folded in.
    pub advances: Vec<f32>,
    pub offsets: Vec<DWRITE_GLYPH_OFFSET>,
    /// Text position each glyph came from, for caret and hit-testing later.
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FaceRequest {
    pub family: String,
    pub cjk_family: String,
    /// Tried only when neither named family covers the run, so a rare glyph still
    /// renders instead of being dropped.
    pub fallback: Vec<String>,
    pub weight: u16,
    pub italic: bool,
}

/// Style table entry: what a `StyleId` means to the renderer.
#[derive(Clone, Debug)]
pub struct Style {
    pub face: FaceRequest,
    pub size: Pt,
    pub tracking: f32,
}

pub struct FontEngine {
    analyzer: IDWriteTextAnalyzer,
    collection: IDWriteFontCollection,
    faces: RefCell<Vec<Face>>,
    resolved: RefCell<HashMap<(String, u16, bool), Option<usize>>>,
    shaped: RefCell<HashMap<ShapeKey, Vec<GlyphRun>>>,
    styles: RefCell<Vec<Style>>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct ShapeKey {
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

impl FontEngine {
    pub fn new() -> WResult<FontEngine> {
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let analyzer = factory.CreateTextAnalyzer()?;
            let mut collection: Option<IDWriteFontCollection> = None;
            factory.GetSystemFontCollection(&mut collection, false)?;
            let collection = collection.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
            Ok(FontEngine {
                analyzer,
                collection,
                faces: RefCell::new(Vec::new()),
                resolved: RefCell::new(HashMap::new()),
                shaped: RefCell::new(HashMap::new()),
                styles: RefCell::new(Vec::new()),
            })
        }
    }

    /// Install the style table the layout pass will index with its `StyleId`s.
    pub fn begin_layout(&self, styles: Vec<Style>) {
        *self.styles.borrow_mut() = styles;
        // Content-addressed run cache stays valid across documents; face handles
        // depend only on (family, weight, slant) so they do too.
    }

    fn find_family(&self, name: &str) -> Option<u32> {
        let w = utf16(name);
        let mut index = 0u32;
        let mut exists = BOOL(0);
        unsafe {
            self.collection.FindFamilyName(PCWSTR(w.as_ptr()), &mut index, &mut exists).ok()?
        };
        (exists.0 != 0).then_some(index)
    }

    /// Resolve (family, weight, slant) to a cached face.
    fn face_for(&self, family: &str, weight: u16, italic: bool) -> Option<usize> {
        let key = (family.to_string(), weight, italic);
        if let Some(hit) = self.resolved.borrow().get(&key) {
            return *hit;
        }
        let found = (|| {
            let index = self.find_family(family)?;
            let fam = unsafe { self.collection.GetFontFamily(index).ok()? };
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

    fn covers(&self, idx: usize, text: &str) -> bool {
        self.glyphs_for(idx, text).is_some_and(|g| g.iter().all(|&x| x != 0))
    }

    /// Longest prefix of `text` the face can render.
    fn covered_prefix(&self, idx: usize, text: &str) -> usize {
        let Some(glyphs) = self.glyphs_for(idx, text) else { return 0 };
        let mut n = 0usize;
        for (c, g) in text.chars().zip(glyphs) {
            if g == 0 {
                break;
            }
            n += c.len_utf8();
        }
        n
    }

    /// Pick the face for `text`, honouring the Latin/CJK split.
    ///
    /// `None` means no candidate face could be opened at all, which only happens on
    /// a machine with no fonts; callers treat that as "render nothing" rather than
    /// substituting a face silently, because a wrong font is harder to notice than
    /// a missing one.
    fn resolve_face(&self, req: &FaceRequest, text: &str) -> Option<usize> {
        let cjk = text.chars().next().is_some_and(cjk_char);
        let mut order: Vec<&str> = if cjk {
            vec![req.cjk_family.as_str(), req.family.as_str()]
        } else {
            vec![req.family.as_str(), req.cjk_family.as_str()]
        };
        order.extend(req.fallback.iter().map(String::as_str));
        let mut partial = None;
        for family in order {
            let Some(idx) = self.face_for(family, req.weight, req.italic) else { continue };
            if self.covers(idx, text) {
                return Some(idx);
            }
            // Keep the first face that at least rendered something: a partial
            // prefix still beats dropping the run.
            if partial.is_none() && self.covered_prefix(idx, text) > 0 {
                partial = Some(idx);
            }
        }
        partial
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
        let slice = &text[range.clone()];
        let mut out = Vec::new();
        let mut at = 0usize;
        while at < slice.len() {
            let rest = &slice[at..];
            let Some(face) = self.resolve_face(req, rest) else { break };
            let taken = self.covered_prefix(face, rest).max(first_char_len(rest));
            let chunk = &rest[..taken];
            let mut runs = self.shape_with_face(chunk, range.start + at, face, size, tracking);
            out.append(&mut runs);
            at += taken;
        }
        out
    }

    fn shape_with_face(
        &self,
        text: &str,
        text_start: usize,
        face: usize,
        size: Pt,
        tracking: f32,
    ) -> Vec<GlyphRun> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let key = ShapeKey {
            face,
            size_q: (size * 64.0).round() as i32,
            track_q: (tracking * 4096.0).round() as i32,
            text: units.clone().into_boxed_slice(),
        };
        if let Some(hit) = self.shaped.borrow().get(&key) {
            return hit.clone();
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

        let sa =
            DWRITE_SCRIPT_ANALYSIS { script: SCRIPT_COMMON, shapes: DWRITE_SCRIPT_SHAPES_DEFAULT };
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
                    false,
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
                false,
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
                *a += tracking * size;
            }
        }
        let run = GlyphRun {
            face,
            size,
            text: text_start..text_start + text.len(),
            clusters: cluster_map.iter().take(count as usize).copied().collect(),
            glyphs,
            advances,
            offsets,
            ascent: metrics.ascent as f32 * scale,
            descent: metrics.descent as f32 * scale,
            line_gap: metrics.lineGap as f32 * scale,
        };
        self.shaped.borrow_mut().insert(key, vec![run.clone()]);
        vec![run]
    }

    /// The COM face handle, for `DrawGlyphRun`.
    pub fn font_face(&self, idx: usize) -> Option<IDWriteFontFace> {
        self.faces.borrow().get(idx).map(|f| f.face.clone())
    }

    pub fn face_family(&self, idx: usize) -> String {
        self.faces.borrow().get(idx).map(|f| f.family.clone()).unwrap_or_default()
    }

    /// Sanity probe used at startup and by tests: can we shape anything at all.
    pub fn probe(&self) -> bool {
        let req = FaceRequest {
            family: "Segoe UI".into(),
            cjk_family: "Microsoft YaHei".into(),
            fallback: vec![],
            weight: 400,
            italic: false,
        };
        let latin = "Ag";
        let han = "\u{4e2d}\u{6587}";
        !self.shape_runs(latin, 0..latin.len(), &req, 13.5, 0.0).is_empty()
            && !self.shape_runs(han, 0..han.len(), &req, 13.5, 0.0).is_empty()
    }
}

/// Advances for the layout core. The style table was installed by `begin_layout`,
/// which is what keeps `rubrica-type` free of any theme knowledge.
impl Measure for FontEngine {
    fn advance(&mut self, text: &str, range: Range<usize>, style: StyleId) -> Pt {
        let (face, size, tracking) = {
            let s = self.styles.borrow();
            match s.get(style.0 as usize) {
                Some(st) => (st.face.clone(), st.size, st.tracking),
                None => return default_advance(text, range),
            }
        };
        self.shape_runs(text, range, &face, size, tracking).iter().map(|r| r.width()).sum()
    }
}

/// Only used when a style id escapes the installed table: keeps a bug from
/// panicking inside a paint callback, where there is nothing to report to.
fn default_advance(text: &str, range: Range<usize>) -> Pt {
    text[range].chars().count() as Pt * 13.5 * 0.5
}

fn first_char_len(s: &str) -> usize {
    s.chars().next().map_or(1, |c| c.len_utf8())
}

fn cjk_char(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2EFF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FA1F)
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
