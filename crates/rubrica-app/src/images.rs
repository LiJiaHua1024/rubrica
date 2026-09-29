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

/// How a file looked when it was last decoded: its length and when it was last written.
///
/// A figure is named, not identified. The same `fig.png` is a different picture every
/// time a build script regenerates it, and the layout worker -- which builds its own
/// store, and so measures the new file -- would otherwise leave the window painting the
/// old one inside the new one's box for the rest of the session. `Ctrl`+`R` does not
/// help, because nothing in it clears the window's store.
type Stamp = (u64, Option<SystemTime>);

/// A natural size, held against the state of the file it was read from.
type CachedSize = (Stamp, Option<(Pt, Pt)>);

/// A bitmap, held against the file it was decoded from and the DPI it was made for.
type CachedBitmap = (PathBuf, Stamp, (u32, u32));

/// The state of the file behind `path`, or `None` when there is no file there at all.
///
/// A figure that is not written yet is a figure that will be, so a missing file is
/// never remembered: it answers `None` and is asked again next time, rather than
/// becoming a permanently blank box.
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
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
    bitmaps: RefCell<HashMap<CachedBitmap, ID2D1Bitmap>>,
}

impl ImageStore {
    pub fn new() -> windows::core::Result<ImageStore> {
        let wic: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }?;
        Ok(ImageStore {
            wic,
            sizes: RefCell::new(HashMap::new()),
            bitmaps: RefCell::new(HashMap::new()),
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

    /// Fit an image into `column` points of measure, shrinking only.
    ///
    /// Upscaling a small figure to fill the column would blur it and let a diagram
    /// compete with the prose it illustrates, so a narrow image keeps its size.
    pub fn fit(&self, path: &Path, column: Pt) -> Option<(Pt, Pt)> {
        let (w, h) = self.natural_size(path)?;
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
        let key = (path.to_path_buf(), stamp(path)?, target_dpi(target));
        if let Some(hit) = self.bitmaps.borrow().get(&key) {
            return Some(hit.clone());
        }
        let made = (|| {
            let frame = self.first_frame(path)?;
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
            self.bitmaps.borrow_mut().insert(key, made.clone());
        }
        made
    }

    /// The pixel size of a frame, if this reader will decode a figure that size at all.
    ///
    /// The size is read from the header, which costs nothing, while the decode asks the
    /// device for `w * h * 4` bytes whatever the file weighs on disk -- and a flat
    /// 20 000 x 20 000 PNG is a hundred kilobytes of file asking for 1.6 GB of texture.
    /// A figure past the cap degrades to its alt text, which is a thing a reader can
    /// still read.
    fn size_of(frame: &IWICBitmapFrameDecode) -> Option<(u32, u32)> {
        let (mut w, mut h) = (0u32, 0u32);
        unsafe { frame.GetSize(&mut w, &mut h).ok()? };
        decodable(w, h).then_some((w, h))
    }

    fn first_frame(&self, path: &Path) -> Option<IWICBitmapFrameDecode> {
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
        unsafe { decoder.GetFrame(0).ok() }
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
}
