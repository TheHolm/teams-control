//! Signal handling for the daemon.
//!
//! Signals are blocked in the calling thread and read back synchronously, so the
//! main loop never runs an async-signal-unsafe handler and can multiplex signals
//! with `Child::try_wait`.
//!
//! Linux uses `signalfd`; FreeBSD (best effort) has no `signalfd`, so it polls
//! with `sigtimedwait` and a zero timeout instead. Both expose the same
//! [`Signals::install`]/[`Signals::next`] API.
//!
//! The mask is installed before any other threads are spawned so those threads
//! inherit the block; otherwise a `SIGTERM` could be delivered to an unblocked
//! thread and trigger the default action before the main loop ever sees it.

use libc::{SIG_BLOCK, c_int, sigaddset, sigemptyset, sigprocmask};
use std::io;

#[cfg(target_os = "linux")]
use libc::{SFD_CLOEXEC, SFD_NONBLOCK, SIGRTMIN, c_void, read, signalfd};
#[cfg(target_os = "linux")]
use std::os::fd::RawFd;

/// FreeBSD's `SIGRTMIN` from `sys/signal.h`.
///
/// The `libc` crate does not export `SIGRTMIN`/`SIGRTMAX` for FreeBSD, so the
/// value is hardcoded; everything below it is reserved for the system.
#[cfg(target_os = "freebsd")]
const FREEBSD_SIGRTMIN: c_int = 65;

/// Returns the runtime value of `SIGRTMIN`.
///
/// On Linux the number is resolved through glibc rather than hardcoded because
/// glibc reserves the first real-time signals for its own use, so the kernel
/// and libc numbers differ.
pub fn resolve_base() -> c_int {
    #[cfg(target_os = "linux")]
    {
        SIGRTMIN()
    }

    #[cfg(target_os = "freebsd")]
    {
        FREEBSD_SIGRTMIN
    }
}

/// A set of blocked signals readable one at a time.
///
/// Installing the set blocks the signals in the calling thread and, because it
/// is meant to run before other threads start, in every thread subsequently
/// created by the process.
pub struct Signals {
    #[cfg(target_os = "linux")]
    fd: RawFd,
    #[cfg(target_os = "freebsd")]
    set: libc::sigset_t,
}

impl Signals {
    /// Blocks `signals` and returns a reader for them.
    pub fn install(signals: &[c_int]) -> io::Result<Self> {
        let mut mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };

        unsafe {
            sigemptyset(&mut mask);

            for &signal in signals {
                sigaddset(&mut mask, signal);
            }

            if sigprocmask(SIG_BLOCK, &mask, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        #[cfg(target_os = "linux")]
        {
            let fd = unsafe { signalfd(-1, &mask, SFD_CLOEXEC | SFD_NONBLOCK) };

            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            Ok(Signals { fd })
        }

        #[cfg(target_os = "freebsd")]
        {
            Ok(Signals { set: mask })
        }
    }

    /// Returns the next pending signal, or [`None`] if none is waiting.
    ///
    /// The read is non-blocking, so this never stalls the main loop.
    #[cfg(target_os = "linux")]
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

    /// Returns the next pending signal, or [`None`] if none is waiting.
    ///
    /// FreeBSD has no `signalfd`; a zero-timeout `sigtimedwait` gives the same
    /// non-blocking poll. `EAGAIN` means nothing is pending.
    #[cfg(target_os = "freebsd")]
    pub fn next(&self) -> io::Result<Option<c_int>> {
        let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
        let timeout = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };

        let signal = unsafe { libc::sigtimedwait(&self.set, &mut info, &timeout) };

        if signal < 0 {
            let error = io::Error::last_os_error();

            if error.raw_os_error() == Some(libc::EAGAIN) {
                return Ok(None);
            }

            return Err(error);
        }

        Ok(Some(signal))
    }
}

impl Drop for Signals {
    /// Closes the underlying `signalfd` descriptor (Linux only).
    #[cfg(target_os = "linux")]
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }

    /// Nothing to release on FreeBSD; the blocked mask needs no cleanup.
    #[cfg(target_os = "freebsd")]
    fn drop(&mut self) {}
}
