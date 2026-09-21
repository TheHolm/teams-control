//! Shared helpers for the integration tests.
//!
//! The central piece is [`FakeCdp`], a CDP server built from ordinary pipes
//! that lets the real [`teams_control::cdp::Cdp`] client run end to end without
//! a browser. [`EnvGuard`] supports the few tests that must mutate the process
//! environment.

#![allow(dead_code)]

use serde_json::Value;
use std::{
    ffi::OsString,
    io,
    os::fd::RawFd,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use teams_control::cdp::{Cdp, make_pipe, take_message, write_all_fd};

/// A fake CDP peer wired to a real client through two pipes.
pub struct FakeCdp {
    /// The client under test.
    pub cdp: Arc<Cdp>,
    /// Every request the peer has received, in arrival order.
    pub requests: Arc<Mutex<Vec<Value>>>,
}

impl FakeCdp {
    /// Starts a peer that answers each request using `handler`.
    ///
    /// The request's `id` is copied into whatever the handler returns.
    /// Returning [`None`] leaves a request unanswered, which is how the client's
    /// timeout path is exercised.
    pub fn start<F>(timeout: Duration, handler: F) -> io::Result<Self>
    where
        F: Fn(&Value) -> Option<Value> + Send + 'static,
    {
        let (requests_read, client_write) = make_pipe()?;
        let (client_read, responses_write) = make_pipe()?;

        let cdp = Cdp::connect(client_write, client_read, timeout);
        let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
        let server_requests = Arc::clone(&requests);

        thread::spawn(move || serve(requests_read, responses_write, server_requests, handler));

        Ok(FakeCdp { cdp, requests })
    }

    /// Waits until at least `count` requests have arrived, then returns a copy.
    ///
    /// Panics if `count` is not reached within `timeout`, so a broken test hangs
    /// loudly instead of blocking forever.
    pub fn wait_for_requests(&self, count: usize, timeout: Duration) -> Vec<Value> {
        let deadline = Instant::now() + timeout;

        loop {
            {
                let requests = self.requests.lock().unwrap();

                if requests.len() >= count {
                    return requests.clone();
                }
            }

            if Instant::now() >= deadline {
                panic!(
                    "only {} of {count} requests arrived",
                    self.requests.lock().unwrap().len()
                );
            }

            thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Reads framed requests from `read_fd` and replies on `write_fd`.
fn serve<F>(read_fd: RawFd, write_fd: RawFd, requests: Arc<Mutex<Vec<Value>>>, handler: F)
where
    F: Fn(&Value) -> Option<Value>,
{
    let mut buffer = Vec::<u8>::new();
    let mut tmp = [0u8; 8192];

    loop {
        let n = unsafe { libc::read(read_fd, tmp.as_mut_ptr() as *mut libc::c_void, tmp.len()) };

        if n < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }

            break;
        }

        if n == 0 {
            break;
        }

        buffer.extend_from_slice(&tmp[..n as usize]);

        while let Some(message) = take_message(&mut buffer) {
            if message.is_empty() {
                continue;
            }

            let request: Value = match serde_json::from_slice(&message) {
                Ok(request) => request,
                Err(_) => continue,
            };

            requests.lock().unwrap().push(request.clone());

            if let Some(mut response) = handler(&request) {
                if let Some(id) = request.get("id") {
                    response["id"] = id.clone();
                }

                let mut data = serde_json::to_vec(&response).unwrap();

                data.push(0);

                let _ = write_all_fd(write_fd, &data);
            }
        }
    }
}

/// Sets or removes an environment variable and restores it on drop.
///
/// Environment variables are process-global, so every test using this must also
/// be annotated `#[serial]`.
pub struct EnvGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvGuard {
    /// Sets `key` to `value`, restoring the prior value on drop.
    pub fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var_os(key);

        unsafe {
            std::env::set_var(key, value);
        }

        EnvGuard { key, original }
    }

    /// Removes `key`, restoring the prior value on drop.
    pub fn remove(key: &'static str) -> Self {
        let original = std::env::var_os(key);

        unsafe {
            std::env::remove_var(key);
        }

        EnvGuard { key, original }
    }
}

impl Drop for EnvGuard {
    /// Restores the variable to its original state.
    fn drop(&mut self) {
        unsafe {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
