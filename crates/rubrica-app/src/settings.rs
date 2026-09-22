//! What the reader has chosen about how the page looks, kept for the next window.
//!
//! Four numbers under `Software\Rubrica` in the current user's registry, which is where
//! a Windows program puts four numbers: no path to choose for a settings file, no
//! format to invent, no dependency to carry, and the palette the system prefers is
//! already read out of the same store.
//!
//! The encoding of a state into those numbers and back is kept apart from the calls
//! that carry them, so what a stored number means can be read -- and tested -- without a
//! live registry in the way.

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, RegGetValueW, RegSetKeyValueW, RRF_RT_DWORD, REG_DWORD,
};

use crate::theme::{Measure, TextFace, Zoom};
use crate::view::utf16;

const SUBKEY: &str = "Software\\Rubrica";
/// The names the numbers are stored under, in the order [`words`] writes them.
const NAMES: [&str; 4] = ["Zoom", "Dark", "Face", "Measure"];
/// What `Dark` holds when the reader asked for the system's own setting to decide. The
/// same answer as no value at all, which is why nothing has to be deleted to get back
/// there: a reader who chooses `Follow System` is not asking for a different number, they
/// are asking for the number to stop mattering.
const FOLLOW_SYSTEM: u32 = 2;

/// The reader's appearance, as it was written down. Every part is optional because a
/// value that is not in the registry is not a choice that was taken away -- it is a
/// choice that was never made, and the difference is what the defaults are for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub zoom: Option<Zoom>,
    pub dark: Option<bool>,
    pub face: Option<usize>,
    pub measure: Option<usize>,
}

/// The numbers a state is carried by: the size as a percentage of the design, the
/// palette as one of three answers, and the face and the measure as their index in the
/// list the menu shows them in.
fn words(zoom: Zoom, dark: Option<bool>, face: usize, measure: usize) -> [u32; 4] {
    [
        zoom.percent(),
        dark.map_or(FOLLOW_SYSTEM, |d| d as u32),
        face as u32,
        measure as u32,
    ]
}

/// What those numbers -- or the absence of any of them -- mean. Nothing here is trusted:
/// a number from a later version of this program, or one edited by hand, becomes no
/// choice rather than a crash or a page set in a face that has no index.
fn read(zoom: Option<u32>, dark: Option<u32>, face: Option<u32>, measure: Option<u32>) -> Settings {
    Settings {
        // Zero is not a percentage of the design; it is what an unfinished write leaves
        // behind, so it reads as no choice rather than as the smallest step.
        zoom: zoom.filter(|p| *p > 0).map(|p| Zoom::nearest_percent(p as f32)),
        dark: match dark {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        },
        face: face.map(|i| i as usize).filter(|i| *i < TextFace::ALL.len()),
        measure: measure.map(|i| i as usize).filter(|i| *i < Measure::ALL.len()),
    }
}

/// One number, or `None` when this machine has nothing of that name to give.
fn word(sub: &str, name: &str) -> Option<u32> {
    let sub = utf16(sub);
    let name = utf16(name);
    let mut value = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            Some(&mut len),
        )
    };
    // A short read is a value that is not a number this program wrote.
    if r.is_err() || len != std::mem::size_of::<u32>() as u32 {
        return None;
    }
    Some(value)
}

/// Whatever was written down under this key, or an empty state for a first run.
fn read_words(sub: &str) -> Settings {
    read(
        word(sub, NAMES[0]),
        word(sub, NAMES[1]),
        word(sub, NAMES[2]),
        word(sub, NAMES[3]),
    )
}

fn write_words(sub: &str, w: &[u32; 4]) {
    let sub = utf16(sub);
    for (n, v) in NAMES.iter().zip(w) {
        let name = utf16(n);
        let r = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
                PCWSTR(name.as_ptr()),
                REG_DWORD.0,
                Some(v as *const u32 as *const core::ffi::c_void),
                std::mem::size_of::<u32>() as u32,
            )
        };
        if r.is_err() {
            // A settings write that fails costs the reader their next start-up, which is
            // worth one line on the console even though nothing else can be done about it.
            eprintln!("settings: cannot write {n}: {r:?}");
        }
    }
}

/// Write down what the reader has just chosen.
///
/// Called at the moment of choosing rather than on the way out, because the usual way to
/// close a window is its own `X`, which ends the process without asking anything of it.
pub fn record(zoom: Zoom, dark: Option<bool>, face: usize, measure: usize) {
    write_words(SUBKEY, &words(zoom, dark, face, measure));
}

/// The reader's own choices, as the last run left them.
pub fn load() -> Settings {
    read_words(SUBKEY)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the tests tidy up after themselves; nothing else this module writes is ever
    // taken away again.
    use windows::Win32::System::Registry::RegDeleteTreeW;

    #[test]
    fn every_step_of_the_ladder_comes_back_as_itself() {
        // A remembered size that came back one step off would be a page the reader did not
        // leave. The percentage is the only thing carried, so the ladder has to survive it.
        assert_eq!(zoom_of(0), Zoom::DESIGN);
        for step in -3..=4 {
            let z = zoom_of(step);
            assert_eq!(read(Some(z.percent()), None, None, None).zoom, Some(z), "step {step}");
        }
    }

    #[test]
    fn a_stored_number_that_is_not_a_choice_becomes_no_choice() {
        let s = read(Some(120), Some(1), Some(2), Some(0));
        assert_eq!(s.zoom, Some(Zoom::nearest_percent(120.0)));
        assert_eq!(s.dark, Some(true));
        assert_eq!(s.face, Some(2));
        assert_eq!(s.measure, Some(0));

        // The palette asked of the system, and no value at all, mean the same thing.
        assert_eq!(read(None, Some(FOLLOW_SYSTEM), None, None).dark, None);
        assert_eq!(read(None, None, None, None), Settings::default());
        // A face or a measure past the end of its list, a zoom of no size at all, and a
        // palette number that has never meant anything: none of them is a choice to make.
        let bogus = read(
            Some(0),
            Some(7),
            Some(TextFace::ALL.len() as u32),
            Some(Measure::ALL.len() as u32),
        );
        assert_eq!(bogus.zoom, None);
        assert_eq!(bogus.dark, None);
        assert_eq!(bogus.face, None);
        assert_eq!(bogus.measure, None);
        // The top rung of each list is a real choice; only the one past it is not.
        assert_eq!(
            read(None, None, Some(TextFace::ALL.len() as u32 - 1), Some(Measure::ALL.len() as u32 - 1)),
            Settings {
                face: Some(TextFace::ALL.len() - 1),
                measure: Some(Measure::ALL.len() - 1),
                ..Settings::default()
            }
        );
    }

    #[test]
    fn the_words_a_state_writes_are_the_words_that_read_back() {
        let w = words(Zoom::DESIGN.up(), Some(false), 3, 2);
        let s = read(Some(w[0]), Some(w[1]), Some(w[2]), Some(w[3]));
        assert_eq!(s.zoom, Some(Zoom::DESIGN.up()));
        assert_eq!(s.dark, Some(false));
        assert_eq!(s.face, Some(3));
        assert_eq!(s.measure, Some(2));
        // `Follow System` round-trips into having no opinion, which is what it asks for.
        let w = words(Zoom::DESIGN, None, 0, 1);
        assert_eq!(w[1], FOLLOW_SYSTEM);
        assert_eq!(read(Some(w[0]), Some(w[1]), Some(w[2]), Some(w[3])).dark, None);
    }

    /// The whole road a setting travels: out to the registry and back. What the tests
    /// above check is the encoding, and an encoding nothing can read is no use to anyone.
    #[test]
    fn a_choice_written_down_is_a_choice_read_back() {
        let sub = "Software\\Rubrica Test";
        write_words(sub, &words(Zoom::DESIGN.up().up(), Some(true), 2, 0));
        assert_eq!(
            read_words(sub),
            Settings {
                zoom: Some(Zoom::DESIGN.up().up()),
                dark: Some(true),
                face: Some(2),
                measure: Some(0),
            }
        );
        // A key that was never written is a first run, not a broken one.
        assert_eq!(read_words(&format!("{sub}\\Absent")), Settings::default());
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(utf16(sub).as_ptr())) };
    }

    /// A step of the ladder, reached by walking it rather than by naming its ratio.
    fn zoom_of(step: i32) -> Zoom {
        let mut z = Zoom::DESIGN;
        for _ in 0..step.max(0) {
            z = z.up();
        }
        for _ in 0..(-step).max(0) {
            z = z.down();
        }
        z
    }
}
