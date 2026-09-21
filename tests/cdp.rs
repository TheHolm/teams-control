//! Tests for the CDP client, message framing and pipe helpers.

mod common;

use common::FakeCdp;
use serde_json::json;
use std::{thread, time::Duration};
use teams_control::cdp::{Cdp, encode_request, make_pipe, take_message, write_all_fd};

/// A command's response is matched back to it by id.
#[test]
fn response_is_matched_to_request() {
    let fake = FakeCdp::start(Duration::from_secs(1), |request| {
        Some(json!({"result": {"echo": request["method"].clone()}}))
    })
    .unwrap();

    let response = fake.cdp.send("Test.Method", json!({"a": 1}), None).unwrap();

    assert_eq!(response["result"]["echo"], "Test.Method");
}

/// A session id is attached to the command when one is supplied.
#[test]
fn session_id_is_attached_to_request() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| Some(json!({"result": {}}))).unwrap();

    fake.cdp
        .send("Test.Method", json!({}), Some("session-1"))
        .unwrap();

    let requests = fake.wait_for_requests(1, Duration::from_secs(1));

    assert_eq!(requests[0]["sessionId"], "session-1");
}

/// A command with no session id omits the field entirely.
#[test]
fn session_id_is_omitted_when_absent() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| Some(json!({"result": {}}))).unwrap();

    fake.cdp.send("Test.Method", json!({}), None).unwrap();

    let requests = fake.wait_for_requests(1, Duration::from_secs(1));

    assert!(requests[0].get("sessionId").is_none());
}

/// An unanswered command fails with a timeout instead of hanging.
#[test]
fn unanswered_command_times_out() {
    let fake = FakeCdp::start(Duration::from_millis(50), |_| None).unwrap();

    let error = fake.cdp.send("Test.Method", json!({}), None).unwrap_err();

    assert!(error.contains("timeout"), "unexpected error: {error}");
}

/// The encoded request carries the id, method and params plus a NUL terminator.
#[test]
fn encode_request_is_nul_terminated_json() {
    let encoded = encode_request(7, "Test.Method", json!({"a": 1}), None);

    assert_eq!(encoded.last(), Some(&0));

    let value: serde_json::Value = serde_json::from_slice(&encoded[..encoded.len() - 1]).unwrap();

    assert_eq!(value["id"], 7);
    assert_eq!(value["method"], "Test.Method");
    assert_eq!(value["params"]["a"], 1);
    assert!(value.get("sessionId").is_none());
}

/// The encoded request includes the session id when one is given.
#[test]
fn encode_request_includes_session_id() {
    let encoded = encode_request(7, "Test.Method", json!({}), Some("sess"));

    let value: serde_json::Value = serde_json::from_slice(&encoded[..encoded.len() - 1]).unwrap();

    assert_eq!(value["sessionId"], "sess");
}

/// Framing returns whole messages, keeps partial ones buffered, and reports
/// empty frames rather than swallowing them.
#[test]
fn take_message_frames_buffered_input() {
    let mut buffer = b"one\0two\0".to_vec();

    assert_eq!(take_message(&mut buffer), Some(b"one".to_vec()));
    assert_eq!(take_message(&mut buffer), Some(b"two".to_vec()));
    assert_eq!(take_message(&mut buffer), None);

    let mut partial = b"part".to_vec();
    assert_eq!(take_message(&mut partial), None);

    partial.push(0);
    assert_eq!(take_message(&mut partial), Some(b"part".to_vec()));

    let mut leading = b"\0rest\0".to_vec();
    assert_eq!(take_message(&mut leading), Some(Vec::new()));
    assert_eq!(take_message(&mut leading), Some(b"rest".to_vec()));
}

/// A malformed frame is skipped without disturbing later valid frames.
#[test]
fn invalid_frame_is_skipped() {
    let (requests_read, client_write) = make_pipe().unwrap();
    let (client_read, responses_write) = make_pipe().unwrap();

    let cdp = Cdp::connect(client_write, client_read, Duration::from_secs(1));

    let server = thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        let n = unsafe {
            libc::read(
                requests_read,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        };

        assert!(n > 0);

        write_all_fd(responses_write, b"this is not json\0").unwrap();
        write_all_fd(responses_write, b"{\"id\":1,\"result\":{\"ok\":true}}\0").unwrap();
    });

    let response = cdp.send("Test.Method", json!({}), None).unwrap();

    assert_eq!(response["result"]["ok"], true);

    server.join().unwrap();
}

/// `write_all_fd` writes the whole buffer to a real pipe.
#[test]
fn write_all_fd_writes_every_byte() {
    let (read_fd, write_fd) = make_pipe().unwrap();

    write_all_fd(write_fd, b"hello world").unwrap();

    unsafe {
        libc::close(write_fd);
    }

    let mut collected = Vec::new();
    let mut buffer = [0u8; 32];

    loop {
        let n = unsafe {
            libc::read(
                read_fd,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        };

        if n <= 0 {
            break;
        }

        collected.extend_from_slice(&buffer[..n as usize]);
    }

    unsafe {
        libc::close(read_fd);
    }

    assert_eq!(collected, b"hello world");
}
