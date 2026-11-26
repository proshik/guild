//! Reporting the mouse to the program.
//!
//! A full-screen program can ask to receive the mouse itself, and `claude`
//! does: it turns on `MOUSE_MOTION` and `SGR_MOUSE` the moment it starts.
//! From then on the terminal must not act on the wheel — it must forward
//! it, and the program decides what scrolling means in its own interface.
//!
//! Without this the trackpad did nothing at all in `claude`: there was no
//! history to move over, and inventing cursor keys under a program that is
//! watching the mouse would make it act on presses nobody made.
//!
//! Two encodings, because programs ask for different ones:
//!
//! - **SGR** (mode 1006), what everything modern turns on, and the only
//!   one without a coordinate limit;
//! - **normal**, the original X10-style report, where a coordinate is one
//!   byte and stops at 223.

use alacritty_terminal::term::TermMode;

/// The button number a wheel notch reports as.
///
/// 64 and 65 are what xterm defined and every program expects; they are
/// not ordinary button numbers but the wheel's own.
const WHEEL_UP: u8 = 64;
const WHEEL_DOWN: u8 = 65;

/// The offset added to every value in a normal-mode report.
///
/// The bytes have to stay printable, so 32 is added to each. It also caps
/// what can be expressed: a coordinate above 223 does not fit in a byte.
const NORMAL_OFFSET: u8 = 32;

/// The largest coordinate a normal-mode report can carry.
const NORMAL_LIMIT: usize = 223;

/// A button the program can be told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

impl Button {
    /// xterm's number for it.
    fn code(self) -> u8 {
        match self {
            Button::Left => 0,
            Button::Middle => 1,
            Button::Right => 2,
        }
    }
}

/// The modifiers a report carries: xterm adds 4 for shift, 8 for meta
/// and 16 for control to the button's number.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

impl Mods {
    fn code(self) -> u8 {
        u8::from(self.shift) * 4 + u8::from(self.alt) * 8 + u8::from(self.control) * 16
    }
}

/// What xterm adds to a button's number when the report is a motion.
const MOTION: u8 = 32;

/// The number a motion with no button held, and a release in the byte
/// form, report as.
const NO_BUTTON: u8 = 3;

/// Whether a press is the program's rather than ours — 2026-09-24.
///
/// A program that asked for the mouse gets it: `claude` selects, autoscrolls
/// and copies on its own, and has buttons to click. **Option keeps the press
/// ours**, as macOS terminals commonly do and as `claude` itself tells a Mac
/// to: *option+click to native select*.
pub fn reported(mode: TermMode, bypass: bool) -> bool {
    mode.intersects(TermMode::MOUSE_MODE) && !bypass
}

/// A button going down.
pub fn press(button: Button, column: usize, line: usize, mods: Mods, mode: TermMode) -> Vec<u8> {
    report(button.code() + mods.code(), column, line, b'M', mode)
}

/// The pointer moving, with `held` down or nothing — `None` when the
/// program did not ask for this kind: a drag needs `MOUSE_DRAG` or
/// `MOUSE_MOTION`, a move with nothing held needs `MOUSE_MOTION`.
pub fn motion(
    held: Option<Button>,
    column: usize,
    line: usize,
    mods: Mods,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let wanted = match held {
        Some(_) => mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION),
        None => mode.contains(TermMode::MOUSE_MOTION),
    };
    wanted.then(|| {
        let button = held.map_or(NO_BUTTON, Button::code);
        report(button + MOTION + mods.code(), column, line, b'M', mode)
    })
}

/// A button coming up. SGR keeps the button and ends with `m`; the byte
/// form has no room for which, and says button 3.
pub fn release(button: Button, column: usize, line: usize, mods: Mods, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::SGR_MOUSE) {
        report(button.code() + mods.code(), column, line, b'm', mode)
    } else {
        report(NO_BUTTON + mods.code(), column, line, b'M', mode)
    }
}

/// One wheel notch, encoded for the program.
///
/// `column` and `line` are zero-based, as the grid counts them; the report
/// is one-based, as the protocol counts them.
pub fn wheel(up: bool, column: usize, line: usize, mode: TermMode) -> Vec<u8> {
    let button = if up { WHEEL_UP } else { WHEEL_DOWN };
    // `M` is a press. A wheel notch has no release, and sending one would
    // look like a button coming up that never went down.
    report(button, column, line, b'M', mode)
}

/// A report of `code` at a zero-based cell, in the encoding the program
/// asked for.
fn report(code: u8, column: usize, line: usize, end: u8, mode: TermMode) -> Vec<u8> {
    let x = column + 1;
    let y = line + 1;

    if mode.contains(TermMode::SGR_MOUSE) {
        return format!("\x1b[<{code};{x};{y}{}", end as char).into_bytes();
    }

    // Normal mode cannot express a coordinate past its limit. Clamping
    // rather than dropping the report: a notch or a click in the far
    // corner of a very wide window still means the person did it.
    let x = x.min(NORMAL_LIMIT) as u8;
    let y = y.min(NORMAL_LIMIT) as u8;

    vec![
        0x1b,
        b'[',
        b'M',
        NORMAL_OFFSET + code,
        NORMAL_OFFSET + x,
        NORMAL_OFFSET + y,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xterm's numbers: left 0, middle 1, right 2; +4 shift, +8 alt
    /// (meta), +16 control; +32 for a motion; SGR ends a press with `M`
    /// and a release with `m`, and keeps the button in the release.
    #[test]
    fn a_press_a_drag_and_a_release_in_sgr() {
        let mode = TermMode::SGR_MOUSE | TermMode::MOUSE_DRAG;
        let none = Mods::default();
        assert_eq!(
            press(Button::Left, 4, 9, none, mode),
            b"\x1b[<0;5;10M".to_vec()
        );
        assert_eq!(
            motion(Some(Button::Left), 4, 0, none, mode),
            Some(b"\x1b[<32;5;1M".to_vec()),
            "a drag above the grid is reported at its first row"
        );
        assert_eq!(
            release(Button::Left, 4, 0, none, mode),
            b"\x1b[<0;5;1m".to_vec()
        );
        let ctrl_shift = Mods {
            shift: true,
            control: true,
            ..Mods::default()
        };
        assert_eq!(
            press(Button::Right, 0, 0, ctrl_shift, mode),
            b"\x1b[<22;1;1M".to_vec()
        );
    }

    /// The byte form: 32 added to each value, a release is button 3,
    /// and a coordinate stops at 223.
    #[test]
    fn a_press_and_a_release_in_the_byte_form() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        let none = Mods::default();
        assert_eq!(
            press(Button::Left, 0, 0, none, mode),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
        assert_eq!(
            release(Button::Left, 300, 0, none, mode),
            vec![0x1b, b'[', b'M', 35, 255, 33]
        );
    }

    /// A motion is sent only when the program asked for that kind.
    #[test]
    fn a_motion_is_sent_only_when_asked_for() {
        let none = Mods::default();
        assert_eq!(
            motion(
                Some(Button::Left),
                1,
                1,
                none,
                TermMode::SGR_MOUSE | TermMode::MOUSE_REPORT_CLICK
            ),
            None
        );
        assert_eq!(
            motion(None, 1, 1, none, TermMode::SGR_MOUSE | TermMode::MOUSE_DRAG),
            None
        );
        assert_eq!(
            motion(
                None,
                1,
                1,
                none,
                TermMode::SGR_MOUSE | TermMode::MOUSE_MOTION
            ),
            Some(b"\x1b[<35;2;2M".to_vec()),
            "a move with nothing held is button 3 plus 32"
        );
    }

    /// Option is ours: it selects locally over a program that reads the
    /// mouse, as macOS terminals commonly do and as `claude` itself tells a
    /// Mac to.
    #[test]
    fn option_bypasses_the_report() {
        assert!(reported(TermMode::MOUSE_DRAG, false));
        assert!(!reported(TermMode::MOUSE_DRAG, true));
        assert!(!reported(TermMode::default(), false));
    }

    /// What `claude` asks for, and what the trackpad was silent without.
    #[test]
    fn sgr_is_used_when_the_program_asked_for_it() {
        let mode = TermMode::SGR_MOUSE;

        assert_eq!(wheel(true, 0, 0, mode), b"\x1b[<64;1;1M".to_vec());
        assert_eq!(wheel(false, 9, 4, mode), b"\x1b[<65;10;5M".to_vec());
    }

    /// The grid counts from zero and the protocol counts from one. Off by
    /// one here means the program acts a row or a column away from the
    /// pointer.
    #[test]
    fn coordinates_are_one_based() {
        let report = String::from_utf8(wheel(true, 4, 7, TermMode::SGR_MOUSE)).unwrap();

        assert_eq!(report, "\x1b[<64;5;8M");
    }

    /// A wheel notch is a press with no release. Sending a release would
    /// look like a button coming up that never went down.
    #[test]
    fn a_notch_is_a_press_and_nothing_else() {
        let report = String::from_utf8(wheel(true, 0, 0, TermMode::SGR_MOUSE)).unwrap();

        assert!(report.ends_with('M'), "a press: {report}");
        assert!(!report.ends_with('m'), "never a release: {report}");
    }

    /// Without SGR the report is the original byte form, with 32 added to
    /// each value to keep it printable.
    #[test]
    fn without_sgr_the_report_is_the_byte_form() {
        let report = wheel(true, 0, 0, TermMode::default());

        assert_eq!(
            report,
            vec![0x1b, b'[', b'M', 32 + 64, 32 + 1, 32 + 1],
            "escape, bracket, M, then button and coordinates offset by 32"
        );
    }

    #[test]
    fn the_two_wheel_directions_differ() {
        let up = wheel(true, 3, 3, TermMode::SGR_MOUSE);
        let down = wheel(false, 3, 3, TermMode::SGR_MOUSE);

        assert_ne!(up, down, "up and down must not encode the same");
    }

    /// A coordinate past the byte form's limit is clamped rather than
    /// dropped: a notch in the corner of a very wide window still means
    /// the person scrolled.
    #[test]
    fn a_far_coordinate_is_clamped_in_the_byte_form() {
        let report = wheel(true, 500, 500, TermMode::default());

        assert_eq!(report[4], NORMAL_OFFSET + NORMAL_LIMIT as u8);
        assert_eq!(report[5], NORMAL_OFFSET + NORMAL_LIMIT as u8);
        assert_eq!(report.len(), 6, "still one well-formed report");
    }

    /// SGR has no such limit, and clamping there would send the program to
    /// the wrong column in a wide window.
    #[test]
    fn sgr_does_not_clamp() {
        let report = String::from_utf8(wheel(true, 499, 299, TermMode::SGR_MOUSE)).unwrap();

        assert_eq!(report, "\x1b[<64;500;300M");
    }
}
