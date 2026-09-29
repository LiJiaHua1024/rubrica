//! DirectWrite script analysis, independent of fonts or a render target.
use std::cell::RefCell;
use std::rc::Rc;
use windows::core::{implement, OutRef, Ref, Result};
use windows::Win32::Graphics::DirectWrite::*;

#[implement(IDWriteTextAnalysisSource, IDWriteTextAnalysisSink)]
struct Analysis {
    text: Vec<u16>,
    locale: Vec<u16>,
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

    fn GetLocaleName(&self, _position: u32, length: *mut u32, name: *mut *mut u16) -> Result<()> {
        unsafe {
            *length = self.locale.len().saturating_sub(1) as u32;
            *name = self.locale.as_ptr() as *mut u16;
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
        // This is the one raw slice into a COM-supplied index in the reader, and a
        // panic inside one of these callbacks aborts the process with no caller to
        // report to. `position` is the caller's to name and `length` runs from wherever
        // it lands, so both are clipped to the window rather than believed.
        let start = (position as usize).min(scripts.len());
        let end = start.saturating_add(length as usize).min(scripts.len());
        for s in &mut scripts[start..end] {
            *s = unsafe { *script };
        }
        Ok(())
    }
    fn SetLineBreakpoints(&self, _: u32, _: u32, _: *const DWRITE_LINE_BREAKPOINT) -> Result<()> { Ok(()) }
    fn SetBidiLevel(&self, _: u32, _: u32, _: u8, _: u8) -> Result<()> { Ok(()) }
    fn SetNumberSubstitution(&self, _: u32, _: u32, _: Ref<IDWriteNumberSubstitution>) -> Result<()> { Ok(()) }
}

pub(super) fn scripts(
    analyzer: &IDWriteTextAnalyzer,
    text: &str,
    locale: &str,
) -> Result<Vec<DWRITE_SCRIPT_ANALYSIS>> {
    use windows::core::Interface;
    let units: Vec<u16> = text.encode_utf16().collect();
    let len = units.len();
    let scripts = Rc::new(RefCell::new(vec![DWRITE_SCRIPT_ANALYSIS::default(); len]));
    let source: IDWriteTextAnalysisSource = Analysis {
        text: units,
        locale: locale.encode_utf16().chain(std::iter::once(0)).collect(),
        scripts: scripts.clone(),
    }
    .into();
    let sink: IDWriteTextAnalysisSink = source.cast()?;
    unsafe { analyzer.AnalyzeScript(&source, 0, len as u32, &sink)?; }
    let result = scripts.borrow().clone();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::Interface;

    const MARK: u16 = 0x1234;
    const UNREACHED: u16 = 0xbeef;

    fn marked(script: u16) -> DWRITE_SCRIPT_ANALYSIS {
        DWRITE_SCRIPT_ANALYSIS { script, ..Default::default() }
    }

    /// A sink over a window of `len` slots, and the window itself to read back.
    fn sink(len: usize) -> (IDWriteTextAnalysisSink, Rc<RefCell<Vec<DWRITE_SCRIPT_ANALYSIS>>>) {
        let scripts = Rc::new(RefCell::new(vec![DWRITE_SCRIPT_ANALYSIS::default(); len]));
        let source: IDWriteTextAnalysisSource = Analysis {
            text: vec![0u16; len],
            locale: vec![0u16],
            scripts: scripts.clone(),
        }
        .into();
        (source.cast().expect("the sink of the same source"), scripts)
    }

    /// A position and a length are both the caller's to name, and a range taken as
    /// given reaches past the allocation -- `scripts[position..end]` with a `position`
    /// of nine and a window of four is a panic inside a COM callback, which under
    /// `panic = "abort"` is the process gone. Clipped, both cases keep their meaning:
    /// a length running off the end fills what is left of the window, and a start
    /// beyond it has no part of itself inside and writes nothing at all.
    #[test]
    fn a_script_range_is_clipped_to_the_window_behind_it() {
        let (sink, scripts) = sink(4);
        // The ordinary case: a run inside the window, set as named.
        unsafe { sink.SetScriptAnalysis(1, 2, &marked(MARK)) }.expect("inside the window");
        assert_eq!(scripts.borrow()[1].script, MARK);
        assert_eq!(scripts.borrow()[2].script, MARK);
        assert_eq!(scripts.borrow()[0].script, 0, "the run took more than it named");
        assert_eq!(scripts.borrow()[3].script, 0);
        // A length far past the end of the window: the two slots that are there, and
        // nothing beyond them.
        unsafe { sink.SetScriptAnalysis(2, 4_000_000_000, &marked(UNREACHED)) }
            .expect("off the end");
        assert_eq!(scripts.borrow()[2].script, UNREACHED);
        assert_eq!(scripts.borrow()[3].script, UNREACHED);
        assert_eq!(scripts.borrow().len(), 4, "the window was written past");
        // A start past the end of the window writes nothing at all, and in particular
        // does not wrap round to its front.
        unsafe { sink.SetScriptAnalysis(9, 1, &marked(UNREACHED)) }.expect("past the end");
        assert_eq!(scripts.borrow()[0].script, 0, "a start past the end wrapped to the front");
        assert_eq!(scripts.borrow().len(), 4);
    }
}
