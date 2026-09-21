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

    /// Resolve a Markdown image target against the document's own directory.
    ///
    /// Absolute paths are taken as written; anything else is relative to the file
    /// being read, which is what every other Markdown tool does.
    pub fn resolve(base: Option<&Path>, src: &str) -> PathBuf {
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
