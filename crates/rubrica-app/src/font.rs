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
mod analysis;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_GLYPH_METRICS, DWRITE_GLYPH_OFFSET, DWRITE_SCRIPT_ANALYSIS,
    DWRITE_SHAPING_GLYPH_PROPERTIES,
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
    collection: IDWriteFontCollection,
    faces: RefCell<Vec<Face>>,
    resolved: RefCell<HashMap<(String, u16, bool), Option<usize>>>,
    shaped: RefCell<HashMap<ShapeKey, Vec<GlyphRun>>>,
    analyzed: RefCell<HashMap<String, Vec<TextRun>>>,
    /// `(position, thickness)` of this face's strikeout rule, in design units, read
    /// from its file: a face is asked once, however many struck runs it carries.
    strikeout: RefCell<HashMap<usize, Option<(f32, f32)>>>,
    styles: RefCell<Vec<Style>>,
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
                analyzed: RefCell::new(HashMap::new()),
                strikeout: RefCell::new(HashMap::new()),
                styles: RefCell::new(Vec::new()),
            })
        }
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
    pub fn begin_layout(&self, styles: Vec<Style>) {
        *self.styles.borrow_mut() = styles;
        self.analyzed.borrow_mut().clear();
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
        let script = text.chars().next().map_or(0, east_asian_script);
        let preferred = match script { 1 => &req.japanese_family, 2 => &req.korean_family, _ => "" };
        if !preferred.is_empty() { order.insert(0, preferred); }
        order.extend(req.fallback.iter().map(String::as_str));
        let mut partial = None;
        for family in order {
            let italic = if cjk { req.cjk_italic.unwrap_or(req.italic) } else { req.italic };
            let Some(idx) = self.face_for(family, req.weight, italic) else { continue };
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
        if !self.analyzed.borrow().contains_key(text) {
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
            self.analyzed.borrow_mut().insert(text.to_owned(), runs);
        }
        let analysis: Vec<_> = self.analyzed.borrow()[text].iter().filter_map(|r| {
            let start = r.range.start.max(range.start);
            let end = r.range.end.min(range.end);
            (start < end).then(|| TextRun { range: start..end, ..r.clone() })
        }).collect();
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
                // Nothing on the request's list has the next character. Draw it from the
                // requested face anyway: `.notdef` is a box a reader can see and a report can
                // count, while stopping here turns the rest of the run into invisible text.
                let face = match self.resolve_face(req, rest) {
                    Some(f) => f,
                    None => match self.face_for(&req.family, req.weight, req.italic) {
                        Some(f) => f,
                        // No face at all -- a machine with no fonts, or a family that was
                        // uninstalled mid-run. There is nothing left to draw with.
                        None => break,
                    },
                };
                let taken = self.covered_prefix(face, rest).max(first_char_len(rest));
                let chunk = &rest[..taken];
                let mut runs = self.shape_with_face(chunk, item.range.start + at, face, size, tracking, &item);
                out.append(&mut runs);
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

fn east_asian_language(text: &str) -> u8 {
    if text.chars().any(|c| east_asian_script(c) == 1) { 1 }
    else if text.chars().any(|c| east_asian_script(c) == 2) { 2 }
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
}
