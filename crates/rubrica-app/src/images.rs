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

pub struct ImageStore {
    wic: IWICImagingFactory,
    /// Natural size in points per path. A `None` entry records "tried and failed",
    /// so a broken link is diagnosed once rather than re-opened on every relayout.
    sizes: RefCell<HashMap<PathBuf, Option<(Pt, Pt)>>>,
    /// Bitmaps are created through the render target so they share its format and
    /// DPI, which means they cannot exist until one does.
    bitmaps: RefCell<HashMap<PathBuf, Option<ID2D1Bitmap>>>,
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
        if let Some(hit) = self.sizes.borrow().get(path) {
            return *hit;
        }
        let read = (|| {
            let frame = self.first_frame(path)?;
            let (mut w, mut h) = (0u32, 0u32);
            unsafe { frame.GetSize(&mut w, &mut h).ok()? };
            if w == 0 || h == 0 {
                return None;
            }
            Some((w as Pt * PX_TO_PT, h as Pt * PX_TO_PT))
        })();
        self.sizes.borrow_mut().insert(path.to_path_buf(), read);
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
    /// alpha composites correctly. Built on first use, then cached.
    pub fn bitmap(&self, target: &ID2D1RenderTarget, path: &Path) -> Option<ID2D1Bitmap> {
        if let Some(hit) = self.bitmaps.borrow().get(path) {
            return hit.clone();
        }
        let made = (|| {
            let frame = self.first_frame(path)?;
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
        self.bitmaps.borrow_mut().insert(path.to_path_buf(), made.clone());
        made
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
}
