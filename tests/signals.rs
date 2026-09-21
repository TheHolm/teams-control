//! Tests for `signalfd` setup and signal delivery.
//!
//! These tests block signals in the test thread and raise them at themselves,
//! so they are serialized to keep their process-global signal state from
//! interfering with one another.

use libc::c_int;
use serial_test::serial;
use std::{
    thread,
    time::{Duration, Instant},
};
use teams_control::{
    shortcut::ALL,
    signals::{Signals, resolve_base},
};

/// Drains any signals left pending by an earlier test.
fn drain(signals: &Signals) {
    while signals.next().unwrap().is_some() {}
}

/// The base matches libc's own `SIGRTMIN`, since glibc reserves the first
/// real-time signals for itself.
#[test]
fn base_matches_libc() {
    assert_eq!(resolve_base(), libc::SIGRTMIN());
}

/// An installed signal raised at the process is read back from the descriptor.
#[test]
#[serial]
fn installed_signal_is_delivered() {
    let base = resolve_base();
    let signals = Signals::install(&[base + 2]).unwrap();

    drain(&signals);

    unsafe {
        libc::raise(base + 2);
    }

    let deadline = Instant::now() + Duration::from_secs(2);

    loop {
        if let Some(signal) = signals.next().unwrap() {
            assert_eq!(signal, base + 2);
            return;
        }

        assert!(Instant::now() < deadline, "signal was not delivered");

        thread::sleep(Duration::from_millis(1));
    }
}

/// With nothing pending, the non-blocking descriptor reports no signal.
#[test]
#[serial]
fn idle_descriptor_yields_none() {
    let signals = Signals::install(&[resolve_base() + 3]).unwrap();

    drain(&signals);

    assert_eq!(signals.next().unwrap(), None);
}

/// Every signal in the shortcut map can be delivered through the descriptor.
#[test]
#[serial]
fn every_mapped_signal_is_delivered() {
    let base = resolve_base();
    let expected: Vec<c_int> = ALL
        .iter()
        .map(|shortcut| base + shortcut.offset())
        .collect();
    let signals = Signals::install(&expected).unwrap();

    drain(&signals);

    for signal in &expected {
        unsafe {
            libc::raise(*signal);
        }
    }

    let mut received = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);

    while received.len() < expected.len() {
        match signals.next().unwrap() {
            Some(signal) => received.push(signal),
            None => {
                assert!(
                    Instant::now() < deadline,
                    "only received {received:?} of {expected:?}"
                );

                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    received.sort_unstable();

    let mut expected_sorted = expected;
    expected_sorted.sort_unstable();

    assert_eq!(received, expected_sorted);
}
