//! Tests for the Teams shortcut definitions and their signal mapping.

use libc::{SIGINT, SIGTERM, c_int};
use teams_control::shortcut::{ALL, FIRST_OFFSET, Shortcut};

/// A stand-in `SIGRTMIN` used to prove the mapping is base-relative.
const BASE: c_int = 34;

/// Every shortcut is reachable from its own signal number.
#[test]
fn from_signal_maps_every_shortcut() {
    for shortcut in ALL {
        assert_eq!(
            Shortcut::from_signal(BASE + shortcut.offset(), BASE),
            Some(shortcut)
        );
    }
}

/// The first application signal sits at `SIGRTMIN+2`, above glibc's reserved
/// `SIGRTMIN` and `SIGRTMIN+1`.
#[test]
fn first_shortcut_starts_at_offset_two() {
    assert_eq!(ALL[0].offset(), FIRST_OFFSET);
}

/// Offsets are unique and contiguous, so no two shortcuts share a signal.
#[test]
fn offsets_are_contiguous_and_unique() {
    let mut offsets: Vec<c_int> = ALL.iter().map(|shortcut| shortcut.offset()).collect();

    offsets.sort_unstable();

    let expected: Vec<c_int> = (FIRST_OFFSET..FIRST_OFFSET + ALL.len() as c_int).collect();

    assert_eq!(offsets, expected);
}

/// Lifecycle signals and unrelated real-time signals map to nothing.
#[test]
fn unknown_signals_are_not_shortcuts() {
    assert_eq!(Shortcut::from_signal(SIGTERM, BASE), None);
    assert_eq!(Shortcut::from_signal(SIGINT, BASE), None);
    assert_eq!(Shortcut::from_signal(BASE + 1, BASE), None);
    assert_eq!(Shortcut::from_signal(BASE + 8, BASE), None);
}

/// The same absolute signal number maps differently under a different base.
#[test]
fn mapping_is_relative_to_base() {
    let base = 64;

    assert_eq!(
        Shortcut::from_signal(base + 2, base),
        Some(Shortcut::ToggleMute)
    );
    assert_eq!(Shortcut::from_signal(BASE + 2, base), None);
}

/// Each shortcut carries the letter of its documented Teams accelerator.
#[test]
fn letters_match_teams_accelerators() {
    assert_eq!(Shortcut::ToggleMute.key(), ("m", "KeyM"));
    assert_eq!(Shortcut::ToggleVideo.key(), ("o", "KeyO"));
    assert_eq!(Shortcut::AcceptAudioCall.key(), ("s", "KeyS"));
    assert_eq!(Shortcut::DeclineCall.key(), ("d", "KeyD"));
    assert_eq!(Shortcut::JoinMeeting.key(), ("j", "KeyJ"));
    assert_eq!(Shortcut::LeaveMeeting.key(), ("h", "KeyH"));
}

/// A keystroke sequence holds Ctrl and Shift, presses the letter, then releases
/// everything, with CDP modifier bits tracking the held modifiers.
#[test]
fn keystrokes_press_and_release_ctrl_shift_letter() {
    let events = Shortcut::ToggleMute.keystrokes();

    let types: Vec<_> = events.iter().map(|event| event.event_type).collect();
    assert_eq!(
        types,
        ["keyDown", "keyDown", "keyDown", "keyUp", "keyUp", "keyUp"]
    );

    let keys: Vec<_> = events.iter().map(|event| event.key).collect();
    assert_eq!(keys, ["Control", "Shift", "m", "m", "Shift", "Control"]);

    let codes: Vec<_> = events.iter().map(|event| event.code).collect();
    assert_eq!(
        codes,
        [
            "ControlLeft",
            "ShiftLeft",
            "KeyM",
            "KeyM",
            "ShiftLeft",
            "ControlLeft"
        ]
    );

    let modifiers: Vec<_> = events.iter().map(|event| event.modifiers).collect();
    assert_eq!(modifiers, [2, 10, 10, 10, 2, 0]);
}

/// Every shortcut's sequence targets that shortcut's own letter.
#[test]
fn each_shortcut_uses_its_own_letter() {
    for shortcut in ALL {
        let (key, code) = shortcut.key();
        let events = shortcut.keystrokes();

        assert_eq!(events[2].key, key);
        assert_eq!(events[2].code, code);
        assert_eq!(events[3].key, key);
    }
}

/// Labels are non-empty and distinct, since they name the action in logs.
#[test]
fn labels_are_unique_and_non_empty() {
    let mut labels: Vec<&str> = ALL.iter().map(|shortcut| shortcut.label()).collect();

    for label in &labels {
        assert!(!label.is_empty());
    }

    labels.sort_unstable();
    labels.dedup();

    assert_eq!(labels.len(), ALL.len());
}
