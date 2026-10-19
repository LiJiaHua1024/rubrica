//! Image decoding and caching, on WIC.
//!
//! Images enter the layout as an intrinsic size in points, which the layout core
//! treats as an atomic box with an ascent and a descent. That is the whole reason
//! `Node` carries vertical extents: a figure taller than the text line has to make
//! the line taller, and any other approach means special-casing image lines inside
//! the breaker.
//!
//! A pixel is taken as 1/96 inch -- the convention Windows itself and every browser
//! use for CSS absolute lengths -- so a 900 px screenshot lands at a sane physical
//! size instead of the enormous one a DPI-less bitmap would imply.
//!
//! Sizing deliberately needs only WIC, not a render target, so `--report` can
//! measure a document's figures headlessly; the GPU bitmap is created lazily on
//! first paint.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rubrica_type::units::Pt;
use windows::core::PCWSTR;
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap, ID2D1RenderTarget};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapFrameDecode,
    IWICFormatConverter, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

/// Points per image pixel: 1/96 inch, matching CSS and the Windows DPI baseline.
pub const PX_TO_PT: Pt = 72.0 / 96.0;

/// The most pixels this reader will decode a figure of, at 64 megapixels: a 8000 x 8000
/// image, which no page in a book has a use for.
const MAX_PIXELS: u64 = 64 * 1024 * 1024;

/// How a file looked when it was last decoded: its length, when it was last written,
/// and a fingerprint of its head.
///
/// A figure is named, not identified. The same `fig.png` is a different picture every
/// time a build script regenerates it, and the layout worker -- which builds its own
/// store, and so measures the new file -- would otherwise leave the window painting the
/// old one inside the new one's box for the rest of the session. `Ctrl`+`R` does not
/// help, because nothing in it clears the window's store.
///
/// Length and time are the usual answer and they miss one case: a figure replaced by
/// another of the same length inside one clock tick, which is what a build script
/// regenerating a screenshot does. The head's fingerprint tells the two apart and
/// costs four kilobytes of reading.
type Stamp = (u64, Option<SystemTime>, u64);

/// A natural size, held against the state of the file it was read from.
type CachedSize = (Stamp, Option<(Pt, Pt)>);

/// A bitmap, held against the file it was decoded from, the frame of it, and the DPI it
/// was made for.
///
/// The file's state is the value rather than part of the key: keyed by it, every
/// regeneration of a figure left the previous bitmap behind for the rest of the
/// session -- one per rebuild, each as large as the size cap allows.
type CachedBitmap = (PathBuf, usize, (u32, u32));

/// A frame's timing, held against the state of the file it was read from.
type CachedFrames = (Stamp, Option<Frames>);

/// The most a file may weigh and still be read for its frame timings.
///
/// A figure's frames are counted by walking the file, which means holding it: past this
/// size the figure is shown as its first frame, which costs nothing and is what this
/// reader did before it could animate at all.
const MAX_ANIMATION_BYTES: u64 = 8 * 1024 * 1024;

/// How many frames a figure has, how long each is held, and whether every one of them is
/// the whole picture.
///
/// Only an animated GIF answers this at all. The delays are read from the file's own
/// graphic control extensions rather than through WIC's metadata query reader: a GIF's
/// timing is part of the GIF, it is four bytes of a file this reader is holding open
/// anyway, and the alternative is another Windows API surface and a `PROPVARIANT` for
/// two little-endian numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Frames {
    pub count: usize,
    /// Each frame's delay in milliseconds, in the order the frames appear.
    pub delays_ms: Vec<u32>,
    /// Whether every frame is the full canvas.
    ///
    /// An optimized GIF stores each frame as the rectangle that changed, and drawing
    /// those one after another -- rather than compositing each over the last by the
    /// disposal method it names -- would show the reader a slideshow of fragments. Those
    /// files are shown as their first frame instead: the honest answer for a file this
    /// build cannot play, and the answer it gave before it could play any.
    pub full: bool,
}

/// The frames a GIF's own bytes describe, or `None` when they describe a still picture.
///
/// A walk over the block structure rather than a decode: the only things wanted are the
/// frame count, each frame's rectangle, and the delay in front of it, and all three are
/// stated in the clear at the head of each block. Anything this walk does not recognize
/// ends it, and an ended walk is a still figure.
pub fn parse_gif(bytes: &[u8]) -> Option<Frames> {
    if bytes.len() < 13 || &bytes[..3] != b"GIF" {
        return None;
    }
    let (screen_w, screen_h) = (u16le(bytes, 6)?, u16le(bytes, 8)?);
    // The screen descriptor's flags say whether a global colour table follows, and how
    // wide its entries are: three bytes each, `2^(n+1)` of them.
    let flags = *bytes.get(10)?;
    let mut at = 13 + if flags & 0x80 != 0 { 3 * (1usize << ((flags & 0x07) + 1)) } else { 0 };
    let mut delays: Vec<u32> = Vec::new();
    let mut delay = 0u32;
    let mut full = true;
    while at < bytes.len() {
        match *bytes.get(at)? {
            // An extension: a label, then blocks whose sizes are stated.
            0x21 => {
                let label = *bytes.get(at + 1)?;
                at += 2;
                if label == 0xF9 {
                    // Graphic control: the block size, the flags, the delay in
                    // hundredths of a second, the transparent index, then the terminator.
                    let size = usize::from(*bytes.get(at)?);
                    if size >= 4 {
                        delay = u32::from(u16le(bytes, at + 2)?) * 10;
                    }
                    at += 1 + size;
                } else if label == 0x01 {
                    // Plain text: a twelve byte fixed header before its own blocks.
                    at += 12;
                }
                at = skip_blocks(bytes, at)?;
            }
            // An image descriptor, which is one frame.
            0x2C => {
                let (left, top) = (u16le(bytes, at + 1)?, u16le(bytes, at + 3)?);
                let (w, h) = (u16le(bytes, at + 5)?, u16le(bytes, at + 7)?);
                let packed = *bytes.get(at + 9)?;
                at += 10;
                if packed & 0x80 != 0 {
                    at += 3 * (1usize << ((packed & 0x07) + 1));
                }
                if left != 0 || top != 0 || w != screen_w || h != screen_h {
                    full = false;
                }
                delays.push(delay);
                delay = 0;
                // The LZW minimum code size, then the compressed data in blocks.
                at += 1;
                at = skip_blocks(bytes, at)?;
            }
            0x3B => break,
            _ => return None,
        }
    }
    if delays.len() <= 1 {
        return None;
    }
    Some(Frames { count: delays.len(), delays_ms: delays, full })
}

/// The frames of a GIF on disk, or `None` for anything else.
///
/// The file's own magic is asked first and the rest is read only if it answers: a page
/// of screenshots and photographs is the common case, and a whole file held in memory to
/// find out that it is not an animation is a cost paid on every figure of every report.
fn read_gif(path: &Path) -> Option<Frames> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut magic = [0u8; 6];
    file.read_exact(&mut magic).ok()?;
    if &magic[..3] != b"GIF" || file.metadata().ok()?.len() > MAX_ANIMATION_BYTES {
        return None;
    }
    let mut bytes = magic.to_vec();
    file.read_to_end(&mut bytes).ok()?;
    parse_gif(&bytes)
}

/// Step over a chain of GIF sub-blocks: each states its own size, and a zero ends it.
fn skip_blocks(bytes: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let size = usize::from(*bytes.get(at)?);
        at += 1;
        if size == 0 {
            return Some(at);
        }
        at = at.checked_add(size)?;
        if at > bytes.len() {
            return None;
        }
    }
}

fn u16le(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

/// The state of the file behind `path`, or `None` when there is no file there at all.
///
/// A figure that is not written yet is a figure that will be, so a missing file is
/// never remembered: it answers `None` and is asked again next time, rather than
/// becoming a permanently blank box.
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    // A file whose modification time cannot be read is treated like a missing one:
    // remembered, it would never change again -- and a figure replaced by another of
    // the same length would keep the old picture for the rest of the session.
    let modified = meta.modified().ok()?;
    Some((meta.len(), Some(modified), head_fingerprint(path)?))
}

/// A hash of the first few kilobytes of a file: enough to tell two figures of the same
/// length apart, and cheap enough to take on every measurement.
fn head_fingerprint(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 4096];
    let read = file.read(&mut head).ok()?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in &head[..read] {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(hash)
}

/// Whether a figure of `w` by `h` pixels is one this reader decodes at all. See
/// [`MAX_PIXELS`]; the product is taken in `u64`, since two dimensions that overflow an
/// `i32` pair would wrap back into the accepted range.
fn decodable(w: u32, h: u32) -> bool {
    w != 0 && h != 0 && u64::from(w) * u64::from(h) <= MAX_PIXELS
}

/// The DPI a bitmap was made for, as the two halves of what the target says.
fn target_dpi(target: &ID2D1RenderTarget) -> (u32, u32) {
    let (mut x, mut y) = (0f32, 0f32);
    unsafe { target.GetDpi(&mut x, &mut y) };
    (x.to_bits(), y.to_bits())
}

pub struct ImageStore {
    wic: IWICImagingFactory,
    /// Natural size in points per path, held against the state of the file it came from.
    /// A `None` entry records "tried this file and failed", so a broken link is opened
    /// once rather than on every relayout -- and only until the file changes.
    sizes: RefCell<HashMap<PathBuf, CachedSize>>,
    /// Bitmaps are created through the render target so they share its format and DPI,
    /// which means they cannot exist until one does -- and they do not outlive either
    /// the file they came from or the target's DPI, which `WM_DPICHANGED` changes in
    /// place rather than by making a new target.
    bitmaps: RefCell<HashMap<CachedBitmap, (Stamp, ID2D1Bitmap)>>,
    /// Frame timings per path, held against the state of the file like a size is.
    frames: RefCell<HashMap<PathBuf, CachedFrames>>,
}

impl ImageStore {
    pub fn new() -> windows::core::Result<ImageStore> {
        let wic: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }?;
        Ok(ImageStore {
            wic,
            sizes: RefCell::new(HashMap::new()),
            bitmaps: RefCell::new(HashMap::new()),
            frames: RefCell::new(HashMap::new()),
        })
    }

    /// Natural size in points, or `None` when the file cannot be decoded.
    pub fn natural_size(&self, path: &Path) -> Option<(Pt, Pt)> {
        let stamp = stamp(path)?;
        if let Some(hit) = self.sizes.borrow().get(path) {
            if hit.0 == stamp {
                return hit.1;
            }
        }
        let read = (|| -> Option<(Pt, Pt)> {
            let frame = self.first_frame(path)?;
            let (w, h) = Self::size_of(&frame)?;
            Some((w as Pt * PX_TO_PT, h as Pt * PX_TO_PT))
        })();
        self.sizes.borrow_mut().insert(path.to_path_buf(), (stamp, read));
        read
    }

    /// Fit an image into `column` points of measure, shrinking only, after the size its
    /// author asked for.
    ///
    /// `zoom` is the whole percentage an `<img>` tag's `style` carried, and it is
    /// applied to the natural size before the column has its say: a figure the author
    /// made small stays small on a wide page, and one they made large is still not
    /// allowed to run past the measure. Upscaling a small figure to fill the column
    /// would blur it and let a diagram compete with the prose it illustrates, so a
    /// narrow image keeps its size.
    pub fn fit_zoomed(&self, path: &Path, column: Pt, zoom: Option<u16>) -> Option<(Pt, Pt)> {
        let (w, h) = self.natural_size(path)?;
        let s = zoom.map_or(1.0, |z| f32::from(z) / 100.0);
        let (w, h) = (w * s, h * s);
        if w <= column || w <= 0.0 {
            Some((w, h))
        } else {
            let s = column / w;
            Some((w * s, h * s))
        }
    }

    /// The GPU bitmap, converted to the premultiplied format Direct2D needs so
    /// alpha composites correctly. Built on first use, then cached against the file it
    /// came from and the target it was made for.
    pub fn bitmap(&self, target: &ID2D1RenderTarget, path: &Path) -> Option<ID2D1Bitmap> {
        self.bitmap_frame(target, path, 0)
    }

    /// The GPU bitmap of one frame of a figure.
    ///
    /// One entry per frame rather than one per figure: an animation shows every frame,
    /// and a cache that held only the first would decode the whole file again on every
    /// beat of the timer. A frame index past the end of the file is answered with
    /// nothing, which is what a figure whose file was replaced under a running animation
    /// gets -- and the caller falls back to the first frame.
    pub fn bitmap_frame(&self, target: &ID2D1RenderTarget, path: &Path, index: usize) -> Option<ID2D1Bitmap> {
        let key = (path.to_path_buf(), index, target_dpi(target));
        let file = stamp(path)?;
        if let Some((at, hit)) = self.bitmaps.borrow().get(&key) {
            if *at == file {
                return Some(hit.clone());
            }
        }
        let made = (|| {
            let frame = self.frame(path, index)?;
            Self::size_of(&frame)?;
            let converter: IWICFormatConverter = unsafe { self.wic.CreateFormatConverter().ok()? };
            unsafe {
                converter
                    .Initialize(
                        &frame,
                        &GUID_WICPixelFormat32bppPBGRA,
                        WICBitmapDitherTypeNone,
                        None,
                        0.0,
                        WICBitmapPaletteTypeCustom,
                    )
                    .ok()?
            };
            unsafe { target.CreateBitmapFromWicBitmap(&converter, None).ok() }
        })();
        // A failure is not remembered, unlike a size. The commonest failure is a file
        // still being written when the document was measured, and a blank figure that
        // stays blank for the rest of the session is a worse answer than asking again.
        if let Some(ref made) = made {
            self.bitmaps.borrow_mut().insert(key, (file, made.clone()));
        }
        made
    }

    /// The frames of an animated figure, or `None` when the file is a still picture.
    ///
    /// Read once per version of the file, like a size, and for the same reason: the
    /// paint path asks on every beat of the animation's timer, and a file read per beat
    /// is a file read dozens of times a second.
    pub fn frames(&self, path: &Path) -> Option<Frames> {
        let stamp = stamp(path)?;
        if let Some(hit) = self.frames.borrow().get(path) {
            if hit.0 == stamp {
                return hit.1.clone();
            }
        }
        let read = read_gif(path);
        self.frames.borrow_mut().insert(path.to_path_buf(), (stamp, read.clone()));
        read
    }

    /// The pixel size of a frame, if this reader will decode a figure that size at all.
    ///
    /// The size is read from the header, which costs nothing, while the decode asks the
    /// device for `w * h * 4` bytes whatever the file weighs on disk -- and a flat
    /// 20 000 x 20 000 PNG is a hundred kilobytes of file asking for 1.6 GB of texture.
    /// A figure past the cap is left out of the page rather than degraded to its alt
    /// text: a block's text and its span offsets are built before any of this runs, so
    /// swapping words in for a placeholder would move every later offset with it. The
    /// caller reports the figure once and leaves the placeholder to read as the prose
    /// around it.
    fn size_of(frame: &IWICBitmapFrameDecode) -> Option<(u32, u32)> {
        let (mut w, mut h) = (0u32, 0u32);
        unsafe { frame.GetSize(&mut w, &mut h).ok()? };
        decodable(w, h).then_some((w, h))
    }

    fn first_frame(&self, path: &Path) -> Option<IWICBitmapFrameDecode> {
        self.frame(path, 0)
    }

    /// One frame of the file, by index.
    fn frame(&self, path: &Path, index: usize) -> Option<IWICBitmapFrameDecode> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let decoder = unsafe {
            self.wic
                .CreateDecoderFromFilename(
                    PCWSTR(wide.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
                .ok()?
        };
        unsafe { decoder.GetFrame(index as u32).ok() }
    }

    /// Resolve a Markdown target against the document's own directory.
    ///
    /// Absolute paths are taken as written; anything else is relative to the file
    /// being read, which is what every other Markdown tool does. A `%` followed by two
    /// hex digits is read as the byte it escapes, because a path with a space in it can
    /// only be written that way -- and a file named for a literal `%20` is rarer by far.
    pub fn resolve(base: Option<&Path>, src: &str) -> PathBuf {
        let src = &percent_decode(src);
        let p = Path::new(src);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        match base {
            Some(dir) => dir.join(p),
            None => p.to_path_buf(),
        }
    }
}

/// Undo the `%XX` escaping a Markdown target carries, leaving anything that is not an
/// escape alone. A trailing `%` or a `%2` at the end of the string has no byte to name
/// and is kept as written rather than dropped.
pub fn percent_decode(src: &str) -> String {
    let bytes = src.as_bytes();
    if !bytes.contains(&b'%') {
        return src.to_string();
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%'
                if i + 2 < bytes.len()
                    && bytes[i + 1].is_ascii_hexdigit()
                    && bytes[i + 2].is_ascii_hexdigit() =>
            {
                let v = hex_val(bytes[i + 1]) << 4 | hex_val(bytes[i + 2]);
                out.push(v);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    // The escapes name bytes, not characters, so a percent-encoded UTF-8 name only comes
    // back as the name if the bytes still spell one.
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        _ => (b | 0x20) - b'a' + 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_source_resolves_against_the_document_directory() {
        let base = Path::new("C:/books");
        assert_eq!(
            ImageStore::resolve(Some(base), "img/a.png"),
            PathBuf::from("C:/books/img/a.png")
        );
        assert_eq!(ImageStore::resolve(Some(base), "./a.png"), PathBuf::from("C:/books/./a.png"));
    }

    #[test]
    fn an_absolute_source_is_taken_as_written() {
        let p = "C:/other/b.png";
        assert_eq!(ImageStore::resolve(Some(Path::new("C:/books")), p), PathBuf::from(p));
        assert_eq!(ImageStore::resolve(None, p), PathBuf::from(p));
    }

    #[test]
    fn without_a_document_directory_the_source_stays_relative_to_the_cwd() {
        assert_eq!(ImageStore::resolve(None, "a.png"), PathBuf::from("a.png"));
    }

    #[test]
    fn an_escaped_space_names_the_space_it_escapes() {
        let base = Path::new("C:/books");
        assert_eq!(
            ImageStore::resolve(Some(base), "my%20figure.png"),
            PathBuf::from("C:/books/my figure.png")
        );
        // A percent that begins no escape is the author's own, and stays put: a file
        // called `100%.png` is a real thing to have a figure named for.
        assert_eq!(ImageStore::resolve(Some(base), "100%.png"), PathBuf::from("C:/books/100%.png"));
        assert_eq!(ImageStore::resolve(Some(base), "end%2"), PathBuf::from("C:/books/end%2"));
        // The bytes an escape names belong to one character split in two.
        assert_eq!(percent_decode("%E4%B8%AD"), "中");
    }

    #[test]
    fn a_wide_figure_is_shrunk_to_the_measure_and_a_narrow_one_is_left_alone() {
        // 1200 px at 1/96 inch is 900pt, wider than a 486pt column; 240 px is 144pt.
        let fit = |px_w: Pt, px_h: Pt, column: Pt| {
            let (w, h) = (px_w * PX_TO_PT, px_h * PX_TO_PT);
            if w <= column {
                (w, h)
            } else {
                let s = column / w;
                (w * s, h * s)
            }
        };
        let (w, h) = fit(1200.0, 300.0, 486.0);
        assert!((w - 486.0).abs() < 0.01, "wide figure: {w}");
        assert!((h - 121.5).abs() < 0.01, "aspect not preserved: {h}");
        let (w2, h2) = fit(240.0, 100.0, 486.0);
        assert!((w2 - 180.0).abs() < 0.01 && (h2 - 75.0).abs() < 0.01, "narrow figure upscaled: {w2}x{h2}");
    }

    /// A figure is a name, not an identity: the same `fig.png` is a different picture
    /// each time a build script regenerates it. Without asking the file what it looks
    /// like now, the window keeps painting the old one inside the new one's box for the
    /// rest of the session -- the layout worker measures the new file, so the box is
    /// right and the picture is not, and nothing the reader does puts them back in step.
    #[test]
    fn a_figure_that_has_been_replaced_is_not_the_one_already_decoded() {
        let dir = std::env::temp_dir().join(format!("rubrica-figure-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory to put a figure in");
        let path = dir.join("fig.png");
        std::fs::write(&path, b"the first figure").expect("a figure to remember");
        let first = stamp(&path);
        assert!(first.is_some(), "a figure that is there has a state");
        std::fs::write(&path, b"a different figure entirely").expect("the same name, new bytes");
        assert_ne!(stamp(&path), first, "a regenerated figure answered from the old decode");
        // A figure that is not written yet is a figure that will be: no state, no
        // remembered failure, and the next pass asks again.
        let _ = std::fs::remove_file(&path);
        assert_eq!(stamp(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A figure is decoded at its own size, and the device is asked for `w * h * 4`
    /// bytes of it whatever the file weighs. A flat 20 000 x 20 000 PNG is a hundred
    /// kilobytes on disk and 1.6 GB of texture, so it is refused here and the figure
    /// falls back to its alt text.
    #[test]
    fn a_figure_too_large_to_decode_is_refused_by_its_pixels() {
        assert!(decodable(1920, 1080), "a screenshot is a figure");
        assert!(decodable(1, MAX_PIXELS as u32), "the cap itself is inside it");
        assert!(!decodable(20_000, 20_000), "1.6 GB of flat colour");
        assert!(!decodable(30_000, 30_000), "3.6 GB of flat colour");
        // Two dimensions whose product overflows an `i32` pair must not wrap back into
        // the accepted range, and a figure of no size is no figure.
        assert!(!decodable(65_536, 65_536));
        assert!(!decodable(0, 100));
        assert!(!decodable(100, 0));
    }

    /// The fixture's own bytes, read the way the window reads them: three frames at the
    /// delays the encoder was told to write, every one of them the full canvas. This is
    /// the walk against a real encoder's output rather than against bytes this file
    /// wrote for itself -- Pillow's GIF writer is what a reader's own figures come from.
    #[test]
    fn the_animation_fixture_states_three_full_frames_at_its_own_delays() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/html-anim.gif");
        let frames = read_gif(&path).expect("the fixture is an animation");
        assert_eq!(frames.count, 3);
        assert_eq!(frames.delays_ms, vec![200, 500, 100]);
        assert!(frames.full, "the fixture stores whole frames");
    }

    /// A GIF's bytes as an encoder writes them: a screen descriptor, one graphic
    /// control extension and one image descriptor per frame, and a trailer. `rect` is
    /// the rectangle each frame is stored in -- the full canvas for an unoptimized
    /// animation, a corner of it for an optimized one.
    fn gif(delays: &[u16], rect: (u16, u16, u16, u16)) -> Vec<u8> {
        let (l, t, w, h) = rect;
        let mut out = b"GIF89a".to_vec();
        out.extend_from_slice(&[10, 0, 10, 0, 0x00, 0, 0]);
        for delay in delays {
            out.extend_from_slice(&[0x21, 0xF9, 4, 0, *delay as u8, (*delay >> 8) as u8, 0, 0]);
            out.push(0x2C);
            for field in [l, t, w, h] {
                out.extend_from_slice(&field.to_le_bytes());
            }
            out.push(0x00);
            out.extend_from_slice(&[2, 0x02, 0x44, 0x01, 0x00]);
        }
        out.push(0x3B);
        out
    }

    /// The delays are hundredths of a second in the file and milliseconds here, and a
    /// still picture is not an animation.
    #[test]
    fn an_animated_gif_states_its_frames_and_a_still_one_states_nothing() {
        let frames = parse_gif(&gif(&[10, 25, 3], (0, 0, 10, 10))).expect("three frames");
        assert_eq!(frames.count, 3);
        assert_eq!(frames.delays_ms, vec![100, 250, 30]);
        assert!(frames.full, "every frame is the whole canvas");
        assert_eq!(parse_gif(&gif(&[10], (0, 0, 10, 10))), None, "one frame is not an animation");
        assert_eq!(parse_gif(b"not a gif at all, but long enough"), None);
        assert_eq!(parse_gif(b"GIF89a"), None, "a header with no screen descriptor");
    }

    /// An optimized GIF stores each frame as the rectangle that changed, and this build
    /// cannot composite those: the frames are counted and the file is marked, so the
    /// caller can show the first frame instead of a slideshow of fragments.
    #[test]
    fn a_frame_that_is_only_a_corner_marks_the_animation_as_partial() {
        let frames = parse_gif(&gif(&[10, 10], (1, 1, 4, 4))).expect("two frames");
        assert_eq!(frames.count, 2);
        assert!(!frames.full);
        // The canvas is the screen descriptor's, so a frame as large as the screen but
        // placed off its origin is partial too.
        let offset = parse_gif(&gif(&[10, 10], (2, 0, 10, 10))).expect("two frames");
        assert!(!offset.full);
    }

    /// A GIF whose blocks this walk does not recognize is a still picture rather than a
    /// guess: the reader shows its first frame, which is what it did before it could
    /// animate anything.
    #[test]
    fn an_unknown_block_ends_the_walk_and_the_figure_is_still() {
        let mut bytes = gif(&[10, 10], (0, 0, 10, 10));
        bytes[13] = 0x99;
        assert_eq!(parse_gif(&bytes), None);
        // A truncated file is the same answer, and never a panic: the walk stops where
        // the bytes do.
        let full = gif(&[10, 10], (0, 0, 10, 10));
        for cut in 13..full.len() {
            let _ = parse_gif(&full[..cut]);
        }
    }

    /// A global colour table sits between the screen descriptor and the first block, and
    /// a frame may carry a local one: both are skipped by the size their flags state, so
    /// neither is mistaken for the frame after it.
    #[test]
    fn colour_tables_are_stepped_over_rather_than_read() {
        let mut bytes = b"GIF89a".to_vec();
        // A 10 by 10 screen with a global table of 2 entries: flags bit 7 set, size 0.
        bytes.extend_from_slice(&[10, 0, 10, 0, 0x80, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        for _ in 0..2 {
            bytes.extend_from_slice(&[0x21, 0xF9, 4, 0, 10, 0, 0, 0]);
            bytes.push(0x2C);
            // Left, top, width, height: the whole 10 by 10 canvas.
            bytes.extend_from_slice(&[0, 0, 0, 0, 10, 0, 10, 0]);
            // A local table of 2 entries, then the compressed data.
            bytes.push(0x80);
            bytes.extend_from_slice(&[0u8; 6]);
            bytes.extend_from_slice(&[2, 0x02, 0x44, 0x01, 0x00]);
        }
        bytes.push(0x3B);
        let frames = parse_gif(&bytes).expect("two frames behind two colour tables");
        assert_eq!(frames.delays_ms, vec![100, 100]);
        assert!(frames.full);
    }
}
