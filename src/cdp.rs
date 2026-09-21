//! Chrome DevTools Protocol client speaking Chromium's remote-debugging pipe.
//!
//! Chromium's `--remote-debugging-pipe` mode exchanges NUL-terminated JSON
//! messages over two inherited file descriptors: commands are written to one
//! end and responses arrive on the other. A background thread reads responses
//! and hands each one to the pending command that shares its JSON `id`.
//!
//! The client is deliberately transport-only: it knows how to frame, write and
//! correlate messages, but nothing about Teams. That keeps it testable against
//! a fake peer built from ordinary pipes.

use libc::{F_DUPFD_CLOEXEC, c_void, close, fcntl, pipe, read, write};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io,
    os::fd::RawFd,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

/// Default time to wait for a response to a single CDP command.
pub const CDP_TIMEOUT: Duration = Duration::from_secs(5);

/// A CDP client connected to a Chromium debugging pipe.
pub struct Cdp {
    write_fd: RawFd,
    timeout: Duration,
    next_id: AtomicU64,
    responses: Mutex<HashMap<u64, Sender<Value>>>,
}

impl Cdp {
    /// Wraps an already-connected pipe pair in a client.
    ///
    /// `write_fd` receives outgoing commands and `read_fd` is polled by a
    /// background thread for responses; the thread exits when the peer closes
    /// its end. Taking the descriptors as arguments (rather than launching
    /// Chromium itself) is what lets tests drive the client against a fake
    /// peer.
    pub fn connect(write_fd: RawFd, read_fd: RawFd, timeout: Duration) -> Arc<Self> {
        let cdp = Arc::new(Cdp {
            write_fd,
            timeout,
            next_id: AtomicU64::new(1),
            responses: Mutex::new(HashMap::new()),
        });

        let reader_cdp = Arc::clone(&cdp);

        thread::spawn(move || cdp_reader(read_fd, reader_cdp));

        cdp
    }

    /// Sends a command and waits for the response with the matching id.
    ///
    /// `session_id` is attached when calling a method on an attached target
    /// rather than the browser itself. On write failure or timeout the pending
    /// response slot is removed so a late reply cannot leak into the map.
    pub fn send(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();

        self.responses.lock().unwrap().insert(id, tx);

        let data = encode_request(id, method, params, session_id);

        if let Err(error) = write_all_fd(self.write_fd, &data) {
            self.responses.lock().unwrap().remove(&id);
            return Err(format!("CDP write failed: {error}"));
        }

        match rx.recv_timeout(self.timeout) {
            Ok(value) => Ok(value),
            Err(_) => {
                self.responses.lock().unwrap().remove(&id);
                Err(format!("CDP timeout waiting for {method}"))
            }
        }
    }
}

/// Serialises a command into the NUL-terminated byte form Chromium expects.
pub fn encode_request(id: u64, method: &str, params: Value, session_id: Option<&str>) -> Vec<u8> {
    let mut message = json!({
        "id": id,
        "method": method,
        "params": params,
    });

    if let Some(session_id) = session_id {
        message["sessionId"] = json!(session_id);
    }

    let mut data = serde_json::to_vec(&message).expect("CDP message is always serialisable");

    data.push(0);

    data
}

/// Removes and returns the next NUL-terminated message from `buffer`.
///
/// Returns [`None`] when no complete message is buffered yet, which is how a
/// message split across several reads is held until it is whole. A leading NUL
/// yields `Some` with an empty vector; callers should skip those.
pub fn take_message(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let position = buffer.iter().position(|&byte| byte == 0)?;
    let message: Vec<u8> = buffer.drain(..position).collect();

    buffer.drain(..1);

    Some(message)
}

/// Reads responses from `read_fd` and dispatches them to waiting senders.
///
/// Runs until the pipe is closed or a read fails; malformed frames are logged
/// and skipped rather than tearing the connection down.
fn cdp_reader(read_fd: RawFd, cdp: Arc<Cdp>) {
    let mut buffer = Vec::<u8>::new();
    let mut tmp = [0u8; 8192];

    loop {
        let n = unsafe { read(read_fd, tmp.as_mut_ptr() as *mut c_void, tmp.len()) };

        if n == 0 {
            eprintln!("Chromium closed the CDP pipe");
            break;
        }

        if n < 0 {
            let error = io::Error::last_os_error();

            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }

            eprintln!("CDP read error: {error}");
            break;
        }

        buffer.extend_from_slice(&tmp[..n as usize]);

        while let Some(message) = take_message(&mut buffer) {
            if message.is_empty() {
                continue;
            }

            let value: Value = match serde_json::from_slice(&message) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Invalid CDP JSON: {error}");
                    continue;
                }
            };

            if let Some(id) = value.get("id").and_then(Value::as_u64)
                && let Some(tx) = cdp.responses.lock().unwrap().remove(&id)
            {
                let _ = tx.send(value);
            }
        }
    }
}

/// Creates a pipe and returns its `(read, write)` descriptors.
pub fn make_pipe() -> io::Result<(RawFd, RawFd)> {
    let mut fds = [0; 2];

    if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }

    Ok((fds[0], fds[1]))
}

/// Duplicates `fd` to a high, close-on-exec descriptor and closes the original.
///
/// Pushing the descriptors above the low range keeps them away from
/// stdin/stdout/stderr and from the file descriptors 3 and 4 that Chromium
/// expects for the debugging pipe.
pub fn make_fd_safe(fd: RawFd) -> io::Result<RawFd> {
    let new_fd = unsafe { fcntl(fd, F_DUPFD_CLOEXEC, 10) };

    if new_fd < 0 {
        return Err(io::Error::last_os_error());
    }

    unsafe {
        close(fd);
    }

    Ok(new_fd)
}

/// Writes all of `data` to `fd`, retrying on interruption.
pub fn write_all_fd(fd: RawFd, mut data: &[u8]) -> io::Result<()> {
    while !data.is_empty() {
        let n = unsafe { write(fd, data.as_ptr() as *const c_void, data.len()) };

        if n < 0 {
            let error = io::Error::last_os_error();

            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }

            return Err(error);
        }

        data = &data[n as usize..];
    }

    Ok(())
}
