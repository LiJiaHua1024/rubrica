//! DirectWrite script analysis, independent of fonts or a render target.
use std::cell::RefCell;
use std::rc::Rc;
use windows::core::{implement, OutRef, Ref, Result};
use windows::Win32::Graphics::DirectWrite::*;

#[implement(IDWriteTextAnalysisSource, IDWriteTextAnalysisSink)]
struct Analysis {
    text: Vec<u16>,
    scripts: Rc<RefCell<Vec<DWRITE_SCRIPT_ANALYSIS>>>,
}

impl IDWriteTextAnalysisSource_Impl for Analysis_Impl {
    fn GetTextAtPosition(&self, position: u32, text: *mut *mut u16, length: *mut u32) -> Result<()> {
        let at = (position as usize).min(self.text.len());
        unsafe {
            *text = if at == self.text.len() { std::ptr::null_mut() } else {
                self.text.as_ptr().add(at) as *mut u16
            };
            *length = (self.text.len() - at) as u32;
        }
        Ok(())
    }

    fn GetTextBeforePosition(&self, position: u32, text: *mut *mut u16, length: *mut u32) -> Result<()> {
        let at = (position as usize).min(self.text.len());
        unsafe {
            *text = if at == 0 { std::ptr::null_mut() } else { self.text.as_ptr() as *mut u16 };
            *length = at as u32;
        }
        Ok(())
    }

    fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
        // Only AnalyzeScript uses this source. UBA is resolved by the shared core.
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }

    fn GetLocaleName(&self, position: u32, length: *mut u32, name: *mut *mut u16) -> Result<()> {
        static LOCALE: [u16; 1] = [0];
        unsafe {
            *length = self.text.len().saturating_sub(position as usize) as u32;
            *name = LOCALE.as_ptr() as *mut u16;
        }
        Ok(())
    }

    fn GetNumberSubstitution(&self, position: u32, length: *mut u32, substitution: OutRef<IDWriteNumberSubstitution>) -> Result<()> {
        unsafe { *length = self.text.len().saturating_sub(position as usize) as u32; }
        substitution.write(None)
    }
}

impl IDWriteTextAnalysisSink_Impl for Analysis_Impl {
    fn SetScriptAnalysis(&self, position: u32, length: u32, script: *const DWRITE_SCRIPT_ANALYSIS) -> Result<()> {
        let mut scripts = self.scripts.borrow_mut();
        let end = (position as usize + length as usize).min(scripts.len());
        for s in &mut scripts[position as usize..end] {
            *s = unsafe { *script };
        }
        Ok(())
    }
    fn SetLineBreakpoints(&self, _: u32, _: u32, _: *const DWRITE_LINE_BREAKPOINT) -> Result<()> { Ok(()) }
    fn SetBidiLevel(&self, _: u32, _: u32, _: u8, _: u8) -> Result<()> { Ok(()) }
    fn SetNumberSubstitution(&self, _: u32, _: u32, _: Ref<IDWriteNumberSubstitution>) -> Result<()> { Ok(()) }
}

pub(super) fn scripts(analyzer: &IDWriteTextAnalyzer, text: &str) -> Result<Vec<DWRITE_SCRIPT_ANALYSIS>> {
    use windows::core::Interface;
    let units: Vec<u16> = text.encode_utf16().collect();
    let len = units.len();
    let scripts = Rc::new(RefCell::new(vec![DWRITE_SCRIPT_ANALYSIS::default(); len]));
    let source: IDWriteTextAnalysisSource = Analysis { text: units, scripts: scripts.clone() }.into();
    let sink: IDWriteTextAnalysisSink = source.cast()?;
    unsafe { analyzer.AnalyzeScript(&source, 0, len as u32, &sink)?; }
    let result = scripts.borrow().clone();
    Ok(result)
}
