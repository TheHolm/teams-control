//! Teams keyboard shortcuts and the Unix signals that trigger them.
//!
//! Microsoft Teams exposes call controls as `Ctrl+Shift+<letter>` accelerators.
//! Each [`Shortcut`] maps to one such accelerator and to a fixed offset above
//! `SIGRTMIN`, so the mapping between a signal number and an action is defined
//! in exactly one place.

use libc::c_int;

/// The CDP modifier bit for Control.
const CTRL: c_int = 2;

/// The CDP modifier bits for Control plus Shift.
const CTRL_SHIFT: c_int = 10;

/// The offset of the first shortcut above `SIGRTMIN`.
///
/// `SIGRTMIN+0` and `+1` are reserved by glibc (`SIGCANCEL`/`SIGSETXID`), so
/// the application's signals start at `+2`.
pub const FIRST_OFFSET: c_int = 2;

/// Every shortcut supported by the daemon, in signal order.
pub const ALL: [Shortcut; 6] = [
    Shortcut::ToggleMute,
    Shortcut::ToggleVideo,
    Shortcut::AcceptAudioCall,
    Shortcut::DeclineCall,
    Shortcut::JoinMeeting,
    Shortcut::LeaveMeeting,
];

/// A Teams call action that can be triggered by a real-time signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// Mute or unmute the microphone (`Ctrl+Shift+M`).
    ToggleMute,
    /// Turn the camera on or off (`Ctrl+Shift+O`).
    ToggleVideo,
    /// Accept an incoming audio call (`Ctrl+Shift+S`).
    AcceptAudioCall,
    /// Decline an incoming call (`Ctrl+Shift+D`).
    DeclineCall,
    /// Join a meeting from the "meeting started" toast (`Ctrl+Shift+J`).
    JoinMeeting,
    /// Leave the current meeting or call (`Ctrl+Shift+H`).
    LeaveMeeting,
}

/// One `Input.dispatchKeyEvent` command in a shortcut's key sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    /// CDP event type, either `"keyDown"` or `"keyUp"`.
    pub event_type: &'static str,
    /// The value reported as `key` to the page.
    pub key: &'static str,
    /// The physical key code, e.g. `"KeyM"`.
    pub code: &'static str,
    /// Bit field of modifiers held while the event fires.
    pub modifiers: c_int,
}

impl Shortcut {
    /// Returns the shortcut triggered by `signal` when `base` is `SIGRTMIN`.
    ///
    /// Any signal that does not fall on a known offset (including `SIGTERM` and
    /// `SIGINT`) yields [`None`], which lets the dispatcher distinguish
    /// shortcuts from lifecycle signals.
    pub fn from_signal(signal: c_int, base: c_int) -> Option<Self> {
        ALL.into_iter()
            .find(|shortcut| base + shortcut.offset() == signal)
    }

    /// Returns the offset above `SIGRTMIN` assigned to this shortcut.
    pub fn offset(self) -> c_int {
        match self {
            Shortcut::ToggleMute => 2,
            Shortcut::ToggleVideo => 3,
            Shortcut::AcceptAudioCall => 4,
            Shortcut::DeclineCall => 5,
            Shortcut::JoinMeeting => 6,
            Shortcut::LeaveMeeting => 7,
        }
    }

    /// Returns the accelerator's `(key, code)` pair.
    pub fn key(self) -> (&'static str, &'static str) {
        match self {
            Shortcut::ToggleMute => ("m", "KeyM"),
            Shortcut::ToggleVideo => ("o", "KeyO"),
            Shortcut::AcceptAudioCall => ("s", "KeyS"),
            Shortcut::DeclineCall => ("d", "KeyD"),
            Shortcut::JoinMeeting => ("j", "KeyJ"),
            Shortcut::LeaveMeeting => ("h", "KeyH"),
        }
    }

    /// Returns a human-readable name used in logs and documentation.
    pub fn label(self) -> &'static str {
        match self {
            Shortcut::ToggleMute => "toggle mute",
            Shortcut::ToggleVideo => "toggle video",
            Shortcut::AcceptAudioCall => "accept audio call",
            Shortcut::DeclineCall => "decline call",
            Shortcut::JoinMeeting => "join from meeting started toast",
            Shortcut::LeaveMeeting => "leave meeting or call",
        }
    }

    /// Builds the full `Ctrl+Shift+<letter>` press-and-release sequence.
    ///
    /// The modifiers field tracks which modifiers are held at each step: Control
    /// alone while it is pressed, Control+Shift once Shift joins, and back down
    /// as each key is released. Chromium derives the page's `ctrlKey`/`shiftKey`
    /// flags from these events, so the values must stay consistent with the
    /// explicit modifier key events around them.
    pub fn keystrokes(self) -> Vec<KeyEvent> {
        let (key, code) = self.key();

        vec![
            KeyEvent {
                event_type: "keyDown",
                key: "Control",
                code: "ControlLeft",
                modifiers: CTRL,
            },
            KeyEvent {
                event_type: "keyDown",
                key: "Shift",
                code: "ShiftLeft",
                modifiers: CTRL_SHIFT,
            },
            KeyEvent {
                event_type: "keyDown",
                key,
                code,
                modifiers: CTRL_SHIFT,
            },
            KeyEvent {
                event_type: "keyUp",
                key,
                code,
                modifiers: CTRL_SHIFT,
            },
            KeyEvent {
                event_type: "keyUp",
                key: "Shift",
                code: "ShiftLeft",
                modifiers: CTRL,
            },
            KeyEvent {
                event_type: "keyUp",
                key: "Control",
                code: "ControlLeft",
                modifiers: 0,
            },
        ]
    }
}
