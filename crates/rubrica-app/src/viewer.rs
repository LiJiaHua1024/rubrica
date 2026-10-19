//! The image viewer: what a click on a figure opens, and the arithmetic behind it.
//!
//! A figure on the page is measured for the column it sits in, which is the right size
//! for reading prose and the wrong size for reading a screenshot. The viewer is the
//! second size: the figure alone, over the whole window, with the reader deciding how
//! close to stand.
//!
//! Everything here is arithmetic on numbers and answers, deliberately: a scale, an
//! offset, the box a figure is drawn in, the frame of an animation that is due. The
//! window owns the pixels and the messages, and this file owns the questions -- which is
//! the only way any of it can be tested, since the one thing this build may never do is
//! put a window on screen and look at it.
//!
//! Two units meet here and only one is used. A figure arrives at its natural size in
//! points -- a pixel is 1/96 inch, the Windows and CSS convention -- and the panel it is
//! drawn in arrives in device independent pixels, which are the window's own drawing
//! units. The caller converts the figure once (`dip = point * dpi / 72`) and hands both
//! in the same unit; `scale` is then the factor applied to that natural size, so
//! `scale == 1.0` is the figure at its own size, one image pixel to one CSS pixel, which
//! is what "actual size" means everywhere else a reader has met the phrase.

/// A box in the viewer's own coordinates: left, top, width, height.
pub type Rect = (f32, f32, f32, f32);

/// What one wheel notch does to the zoom.
///
/// A ratio rather than a step, because zoom is a ratio: a notch has to feel the same
/// whether the figure is at 5% or at 500%, and adding a fixed number of percent is how
/// one ends up with a viewer that crawls when it is far out and overshoots when it is
/// close in.
pub const ZOOM_STEP: f32 = 1.25;

/// The closest and furthest the reader may stand.
///
/// The far end is what keeps a figure from being zoomed into a single pixel of colour
/// and the near end is what keeps a 4000 pixel screenshot from being shrunk into
/// nothing at all -- both ends are reachable by a wheel that is held down.
pub const MIN_SCALE: f32 = 0.05;
pub const MAX_SCALE: f32 = 32.0;

/// The room a figure keeps from the window's edges.
///
/// Not decoration: a figure drawn edge to edge leaves nowhere for the pointer to be
/// that is not the figure, and the click that closes the viewer -- the one outside the
/// picture -- would stop existing.
pub const MARGIN: f32 = 16.0;

/// The delay given to an animation frame that states none, and the shortest a frame may
/// be held.
///
/// GIF's own default is a tenth of a second, which is what an encoder writes when it
/// does not care. The floor is for the other end: a delay of zero, or of one hundredth,
/// is a file that would have the window repainting as fast as the timer can be armed,
/// and no animation is worth a core.
pub const DEFAULT_FRAME_MS: u32 = 100;
pub const MIN_FRAME_MS: u32 = 20;

/// The panel a viewer draws in: the whole window, less the margin.
///
/// The whole window is the point. A panel inset by a tenth of the client area is a
/// dialog; the viewer is the figure and nothing else, which is what the overlay it
/// answers to looks like on the platform this reader is imitating.
pub fn panel_of(client: (f32, f32)) -> Rect {
    let (w, h) = ((client.0 - 2.0 * MARGIN).max(1.0), (client.1 - 2.0 * MARGIN).max(1.0));
    (MARGIN, MARGIN, w, h)
}

/// The scale at which a figure of `figure` fills as much of `panel` as it can without
/// being cut off.
///
/// Filling is what "fit" means, upscaling included: a 16 pixel icon asked for at fit is
/// a 16 pixel icon the reader wants to see, and honouring that with a blurred square is
/// still better than honouring it with a speck. The cap is the one protection against
/// the degenerate case, a one pixel figure in a thousand pixel panel.
pub fn fit_scale(figure: (f32, f32), panel: Rect) -> f32 {
    let (w, h) = figure;
    if w <= 0.0 || h <= 0.0 {
        return 1.0;
    }
    clamp_scale((panel.2 / w).min(panel.3 / h))
}

/// A scale the viewer is willing to hold.
pub fn clamp_scale(scale: f32) -> f32 {
    if scale.is_finite() { scale.clamp(MIN_SCALE, MAX_SCALE) } else { 1.0 }
}

/// The size a figure of `figure` is drawn at, at `scale`.
pub fn drawn(figure: (f32, f32), scale: f32) -> (f32, f32) {
    (figure.0 * scale, figure.1 * scale)
}

/// Bring an offset back inside the panel.
///
/// Two rules, and the first is the one that makes the viewer feel like a viewer: a
/// figure that fits in the panel is centred in it, exactly, whatever it was dragged to.
/// A figure larger than the panel is clamped so that it always covers the panel --
/// `[panel - drawn, panel]` is the range of offsets that leaves no gap on any side --
/// which is what stops a pan from throwing the picture off the screen and leaving the
/// reader looking at an empty backdrop with no way back.
pub fn clamp_offset(offset: (f32, f32), figure: (f32, f32), panel: Rect, scale: f32) -> (f32, f32) {
    let (dw, dh) = drawn(figure, scale);
    let (px, py, pw, ph) = panel;
    let x = if dw <= pw { px + (pw - dw) * 0.5 } else { offset.0.clamp(px + pw - dw, px) };
    let y = if dh <= ph { py + (ph - dh) * 0.5 } else { offset.1.clamp(py + ph - dh, py) };
    (x, y)
}

/// The offset that centres a figure in a panel.
pub fn centred(figure: (f32, f32), panel: Rect, scale: f32) -> (f32, f32) {
    let (dw, dh) = drawn(figure, scale);
    (panel.0 + (panel.2 - dw) * 0.5, panel.1 + (panel.3 - dh) * 0.5)
}

/// Which frame of an animation is due after `elapsed_ms`, given each frame's delay.
///
/// The clock is the whole of the animation's state, which is what makes a sequence that
/// must survive a repaint, a resize and a zoom testable at all: nothing here knows what
/// a frame looks like, only how long each one is shown. Delays of zero are read as the
/// default rather than as "skip", because a zero delay is a file saying nothing.
///
/// The sequence wraps, so an animation runs for as long as the reader looks at it, and
/// the sum is taken in `u64`: a hundred frames of a day each would overflow the `u32`
/// the caller counts in.
pub fn frame_at(delays_ms: &[u32], elapsed_ms: u32) -> usize {
    if delays_ms.is_empty() {
        return 0;
    }
    let total: u64 = delays_ms.iter().map(|d| u64::from(frame_ms(*d))).sum();
    let mut at = u64::from(elapsed_ms) % total;
    for (i, delay) in delays_ms.iter().enumerate() {
        let delay = u64::from(frame_ms(*delay));
        if at < delay {
            return i;
        }
        at -= delay;
    }
    delays_ms.len() - 1
}

/// How long a frame is really held: what it states, or the default when it states
/// nothing, and never less than the floor.
pub fn frame_ms(stated: u32) -> u32 {
    if stated == 0 { DEFAULT_FRAME_MS } else { stated.max(MIN_FRAME_MS) }
}

/// Where the reader is standing in a figure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewer {
    /// The factor applied to the figure's natural size; 1.0 is actual size.
    pub scale: f32,
    /// The drawn figure's top-left, in the window's drawing units.
    pub offset: (f32, f32),
    /// The frame of an animation on screen, and when it began.
    pub frame: usize,
    pub since: u32,
}

impl Viewer {
    /// The figure, as large as the window will hold it.
    pub fn fit(figure: (f32, f32), panel: Rect, now: u32) -> Viewer {
        let scale = fit_scale(figure, panel);
        Viewer { scale, offset: centred(figure, panel, scale), frame: 0, since: now }
    }

    /// The figure at its own size, centred.
    ///
    /// Actual size is a *scale*, not a size: a figure smaller than the window is drawn
    /// small and centred, and one larger is drawn large and centred, which is the
    /// difference between the two questions a reader can ask -- "make it fit" and "show
    /// me the pixels".
    pub fn actual(figure: (f32, f32), panel: Rect, now: u32) -> Viewer {
        let scale = clamp_scale(1.0);
        Viewer { scale, offset: centred(figure, panel, scale), frame: 0, since: now }
    }

    /// The box the figure is drawn in.
    pub fn rect(self, figure: (f32, f32)) -> Rect {
        let (dw, dh) = drawn(figure, self.scale);
        (self.offset.0, self.offset.1, dw, dh)
    }

    /// Whether a point is on the figure rather than on the backdrop around it.
    pub fn covers(self, figure: (f32, f32), x: f32, y: f32) -> bool {
        let (rx, ry, rw, rh) = self.rect(figure);
        x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
    }

    /// Zoom by `factor`, keeping the figure's point under `anchor` under it.
    ///
    /// This is the whole reason a wheel zoom feels like a magnifier and a zoom button
    /// feels like a slideshow: the pixel the reader is pointing at is the pixel they
    /// want more of, so it must not move. The anchor is in window units, the figure's
    /// own coordinates are recovered by undoing the current scale and offset, and the
    /// new offset puts that same point back under the pointer.
    pub fn zoomed(self, factor: f32, anchor: (f32, f32), figure: (f32, f32), panel: Rect) -> Viewer {
        let scale = clamp_scale(self.scale * factor);
        if self.scale <= 0.0 {
            return Viewer { scale, offset: centred(figure, panel, scale), ..self };
        }
        let at = ((anchor.0 - self.offset.0) / self.scale, (anchor.1 - self.offset.1) / self.scale);
        let offset = (anchor.0 - at.0 * scale, anchor.1 - at.1 * scale);
        Viewer { scale, offset: clamp_offset(offset, figure, panel, scale), ..self }
    }

    /// Move the figure by a drag's worth of window units.
    pub fn panned(self, dx: f32, dy: f32, figure: (f32, f32), panel: Rect) -> Viewer {
        let offset = (self.offset.0 + dx, self.offset.1 + dy);
        Viewer { offset: clamp_offset(offset, figure, panel, self.scale), ..self }
    }

    /// Hold a frame, and start its clock again.
    pub fn showing(self, frame: usize, now: u32) -> Viewer {
        Viewer { frame, since: now, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A figure of 1000 by 800 points in a panel of 800 by 600, the arithmetic the
    /// ledger quotes: 800/1000 is 0.8, 600/800 is 0.75, and the smaller of the two is
    /// the one that fits.
    #[test]
    fn a_figure_fits_the_panel_on_its_tighter_side() {
        let panel = (0.0, 0.0, 800.0, 600.0);
        let figure = (1000.0, 800.0);
        assert!((fit_scale(figure, panel) - 0.75).abs() < 1e-6);
        let view = Viewer::fit(figure, panel, 0);
        assert_eq!(view.rect(figure), (25.0, 0.0, 750.0, 600.0));
    }

    /// The panel is the window less the margin, and it never collapses: a window
    /// narrower than two margins is a window with a one unit panel rather than a
    /// negative one, which is the difference between a strange drawing and a division
    /// by zero on the first paint.
    #[test]
    fn the_panel_is_the_window_less_its_margins() {
        assert_eq!(panel_of((800.0, 600.0)), (16.0, 16.0, 768.0, 568.0));
        let (_, _, w, h) = panel_of((20.0, 10.0));
        assert_eq!((w, h), (1.0, 1.0));
    }

    /// A figure smaller than the panel is centred exactly, however it was dragged
    /// there; one larger is clamped so that no edge of the panel shows through.
    #[test]
    fn a_pan_is_clamped_and_a_figure_that_fits_is_centred() {
        let panel = (0.0, 0.0, 800.0, 600.0);
        let small = (200.0, 100.0);
        assert_eq!(clamp_offset((700.0, 700.0), small, panel, 1.0), (300.0, 250.0));
        let large = (1000.0, 800.0);
        assert_eq!(clamp_offset((500.0, 500.0), large, panel, 1.0), (0.0, 0.0));
        assert_eq!(clamp_offset((-5000.0, -5000.0), large, panel, 1.0), (-200.0, -200.0));
    }

    /// Actual size is 1.0 whatever the panel, and a figure larger than the window is
    /// centred at that size rather than shrunk to fit it.
    #[test]
    fn actual_size_is_the_figures_own_size_in_the_middle_of_the_window() {
        let panel = (0.0, 0.0, 800.0, 600.0);
        let view = Viewer::actual((1000.0, 800.0), panel, 0);
        assert_eq!(view.scale, 1.0);
        assert_eq!(view.rect((1000.0, 800.0)), (-100.0, -100.0, 1000.0, 800.0));
        // And a figure smaller than the window keeps its own size instead of being
        // stretched to the panel, which is the whole difference from `fit`.
        let small = Viewer::actual((200.0, 100.0), panel, 0);
        assert_eq!(small.rect((200.0, 100.0)), (300.0, 250.0, 200.0, 100.0));
    }

    /// The point under the pointer is the point that stays under it. At 2x, a figure
    /// whose top-left is at (100, 50) has the panel point (300, 250) over its own
    /// (200, 200); after zooming, that same figure point must still be at (300, 250).
    #[test]
    fn a_zoom_keeps_the_point_under_the_pointer() {
        let panel = (0.0, 0.0, 800.0, 600.0);
        let figure = (1000.0, 1000.0);
        let start = Viewer { scale: 1.0, offset: (100.0, 50.0), frame: 0, since: 0 };
        let zoomed = start.zoomed(2.0, (300.0, 250.0), figure, panel);
        assert_eq!(zoomed.scale, 2.0);
        // The figure point under the pointer, before and after.
        let before = ((300.0 - start.offset.0) / start.scale, (250.0 - start.offset.1) / start.scale);
        let after = ((300.0 - zoomed.offset.0) / zoomed.scale, (250.0 - zoomed.offset.1) / zoomed.scale);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
        assert_eq!(after, (200.0, 200.0));
        assert_eq!(zoomed.offset, (-100.0, -150.0), "and the figure moved to keep it there");
    }

    /// Zoom stops at both ends rather than wrapping, and a figure zoomed out until it
    /// fits is re-centred by the same rule a pan obeys.
    #[test]
    fn a_zoom_is_bounded_and_recentres_a_figure_that_comes_back_inside() {
        let panel = (0.0, 0.0, 800.0, 600.0);
        let figure = (1000.0, 800.0);
        let mut view = Viewer { scale: 1.0, offset: (0.0, 0.0), frame: 0, since: 0 };
        for _ in 0..40 {
            view = view.zoomed(ZOOM_STEP, (400.0, 300.0), figure, panel);
        }
        assert_eq!(view.scale, MAX_SCALE);
        for _ in 0..80 {
            view = view.zoomed(1.0 / ZOOM_STEP, (400.0, 300.0), figure, panel);
        }
        assert_eq!(view.scale, MIN_SCALE);
        // Drawn at a tenth of a percent of the panel, so it is centred, not clamped.
        assert_eq!(view.offset, (400.0 - 1000.0 * MIN_SCALE * 0.5, 300.0 - 800.0 * MIN_SCALE * 0.5));
    }

    /// The backdrop is what closes the viewer, so the test that matters is that the
    /// point outside the figure is outside it -- at fit, where the backdrop is widest,
    /// and after a zoom, where it may be gone entirely.
    #[test]
    fn the_backdrop_around_the_figure_is_what_a_click_can_close() {
        let panel = (0.0, 0.0, 800.0, 600.0);
        let figure = (1000.0, 800.0);
        let fit = Viewer::fit(figure, panel, 0);
        assert!(fit.covers(figure, 400.0, 300.0), "the middle of the figure is the figure");
        assert!(fit.covers(figure, 25.0, 0.0), "its own corner is the figure");
        // Fit fills one axis exactly -- 600 of 600 here -- so the backdrop is the margin
        // on either side of it and nothing above or below.
        assert!(fit.covers(figure, 400.0, 599.0), "the figure fills the panel's height");
        assert!(!fit.covers(figure, 5.0, 300.0), "the margin beside it is the backdrop");
        assert!(!fit.covers(figure, 780.0, 300.0), "and the margin on the other side");
        let zoomed = fit.zoomed(4.0, (400.0, 300.0), figure, panel);
        assert!(zoomed.covers(figure, 1.0, 1.0), "zoomed in, the backdrop is off screen");
    }

    /// The animation clock: each frame is shown for its own delay and the sequence
    /// wraps, which is what an animation is.
    #[test]
    fn a_frame_is_due_for_its_own_delay_and_the_sequence_wraps() {
        let delays = [100, 250, 50];
        assert_eq!(frame_at(&delays, 0), 0);
        assert_eq!(frame_at(&delays, 99), 0);
        assert_eq!(frame_at(&delays, 100), 1);
        assert_eq!(frame_at(&delays, 349), 1);
        assert_eq!(frame_at(&delays, 350), 2);
        assert_eq!(frame_at(&delays, 400), 0, "the sequence wraps");
        assert_eq!(frame_at(&delays, 1200), 0);
        assert_eq!(frame_at(&[], 5000), 0, "a still figure is its own first frame");
    }

    /// A delay of zero is a file saying nothing, and a delay of one hundredth of a
    /// second is a file that would spin the timer: both are read as the default and the
    /// floor respectively, so no animation can cost the window more than fifty frames a
    /// second.
    #[test]
    fn a_stated_delay_of_nothing_is_the_default_and_a_tiny_one_is_floored() {
        assert_eq!(frame_ms(0), DEFAULT_FRAME_MS);
        assert_eq!(frame_ms(1), MIN_FRAME_MS);
        assert_eq!(frame_ms(7), MIN_FRAME_MS);
        assert_eq!(frame_ms(120), 120);
        let delays = [0, 0];
        assert_eq!(frame_at(&delays, 0), 0);
        assert_eq!(frame_at(&delays, DEFAULT_FRAME_MS), 1);
        assert_eq!(frame_at(&delays, 2 * DEFAULT_FRAME_MS), 0);
    }

    /// A frame count that would overflow a `u32` sum still wraps where it should, and a
    /// sequence of absurd delays is a sequence rather than a panic.
    #[test]
    fn a_very_long_sequence_still_wraps() {
        let delays = [u32::MAX; 3];
        assert_eq!(frame_at(&delays, 0), 0);
        assert_eq!(frame_at(&delays, u32::MAX - 1), 0);
        assert_eq!(frame_at(&delays, u32::MAX), 1);
    }

    /// Panning is what a drag does, and a drag on a figure that fits does nothing at
    /// all -- the centring rule answers before the drag does.
    #[test]
    fn a_drag_moves_a_large_figure_and_leaves_a_small_one_centred() {
        let panel = (0.0, 0.0, 800.0, 600.0);
        let large = (1000.0, 800.0);
        let view = Viewer { scale: 1.0, offset: (-100.0, -100.0), frame: 0, since: 0 };
        assert_eq!(view.panned(40.0, 30.0, large, panel).offset, (-60.0, -70.0));
        assert_eq!(view.panned(-400.0, -400.0, large, panel).offset, (-200.0, -200.0));
        // A figure smaller than the panel is centred by the same rule, whatever the drag
        // says: at 1.0 the 200 by 100 figure sits at (300, 250) and stays there.
        let small = (200.0, 100.0);
        let at_size = Viewer { scale: 1.0, offset: (300.0, 250.0), frame: 0, since: 0 };
        assert_eq!(at_size.panned(40.0, 30.0, small, panel).offset, (300.0, 250.0));
        // Fit fills one axis -- 800 of 800 here -- and centres the other, so the drag
        // that moved the figure off its own axis is undone rather than followed.
        let fitted = Viewer::fit(small, panel, 0);
        assert_eq!(fitted.offset, (0.0, 100.0));
        assert_eq!(fitted.panned(40.0, 30.0, small, panel).offset, (0.0, 100.0));
    }
}
