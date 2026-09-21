//! `signalfd`-based signal handling.
//!
//! Signals are blocked in the calling thread and read synchronously from a file
//! descriptor. That keeps the main loop simple (no async-signal-unsafe
//! handlers) and lets it multiplex signals with `Child::try_wait`.
//!
//! The mask is installed before any other threads are spawned so those threads
//! inherit the block; otherwise a `SIGTERM` could be delivered to an unblocked
//! thread and trigger the default action before the main loop ever sees it.

use libc::{
    SFD_CLOEXEC, SFD_NONBLOCK, SIG_BLOCK, SIGRTMIN, c_int, c_void, read, sigaddset, sigemptyset,
    signalfd, sigprocmask,
};
use std::{io, os::fd::RawFd};

/// Returns the runtime value of `SIGRTMIN`.
///
/// The number is resolved through glibc rather than hardcoded because glibc
/// reserves the first real-time signals for its own use, so the kernel and
/// libc numbers differ.
pub fn resolve_base() -> c_int {
    SIGRTMIN()
}

/// A set of blocked signals readable through a file descriptor.
///
/// Installing the set blocks the signals in the calling thread and, because it
/// is meant to run before other threads start, in every thread subsequently
/// created by the process.
pub struct Signals {
    fd: RawFd,
}

impl Signals {
    /// Blocks `signals` and returns a reader for them.
    pub fn install(signals: &[c_int]) -> io::Result<Self> {
        let mut mask = unsafe { std::mem::zeroed() };

        unsafe {
            sigemptyset(&mut mask);

            for &signal in signals {
                sigaddset(&mut mask, signal);
            }

            if sigprocmask(SIG_BLOCK, &mask, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        let fd = unsafe { signalfd(-1, &mask, SFD_CLOEXEC | SFD_NONBLOCK) };

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Signals { fd })
    }

    /// Returns the next pending signal, or [`None`] if none is waiting.
    ///
    /// The descriptor is non-blocking, so this never stalls the main loop.
    pub fn next(&self) -> io::Result<Option<c_int>> {
        let mut info = unsafe { std::mem::zeroed::<libc::signalfd_siginfo>() };

        let n = unsafe {
            read(
                self.fd,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of_val(&info),
            )
        };

        if n < 0 {
            let error = io::Error::last_os_error();

            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(None);
            }

            return Err(error);
        }

        if n as usize != std::mem::size_of_val(&info) {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "short signalfd read",
            ));
        }

        Ok(Some(info.ssi_signo as c_int))
    }
}

impl Drop for Signals {
    /// Closes the underlying `signalfd` descriptor.
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}
