//! Horizontal measurement units.
//!
//! The engine works entirely in points so that line-breaking decisions are
//! independent of display DPI; the renderer converts once, at paint time.

/// A PostScript point, 1/72". The single unit used across the layout core.
pub type Pt = f32;

/// 1 em at `size`.
#[inline]
pub const fn em(size: Pt, ratio: Pt) -> Pt {
    size * ratio
}

/// Infinite stretch, matching TeX's `1fil` well enough for a screen engine.
pub const INFINITY: Pt = 1.0e7;

/// Widths this far apart are considered equal; absorbs float drift in prefix sums.
pub const EPSILON: Pt = 0.015625; // 2^-6, TeX's `eps` in sp terms

/// Points to device pixels at `dpi`.
#[inline]
pub const fn to_px(pt: Pt, dpi: f32) -> f32 {
    pt * dpi / 72.0
}
