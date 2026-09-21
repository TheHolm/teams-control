//! Tests for Teams target discovery and shortcut dispatch.

mod common;

use common::FakeCdp;
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use teams_control::{
    shortcut::Shortcut,
    teams::{attach_to_teams, find_teams_target, send_shortcut, wait_for_teams},
};

/// A JSON response listing a single Teams page target.
fn teams_targets(id: &str) -> serde_json::Value {
    json!({
        "result": {
            "targetInfos": [
                {
                    "type": "page",
                    "url": "https://teams.microsoft.com/v2/",
                    "targetId": id
                }
            ]
        }
    })
}

/// The Teams page target id is returned when the page is open.
#[test]
fn find_teams_target_returns_teams_page() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| Some(teams_targets("teams-1"))).unwrap();

    assert_eq!(
        find_teams_target(&fake.cdp).unwrap(),
        Some("teams-1".to_string())
    );
}

/// Non-page targets and unrelated pages are ignored.
#[test]
fn find_teams_target_ignores_other_targets() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| {
        Some(json!({
            "result": {
                "targetInfos": [
                    {"type": "background_page", "url": "https://teams.microsoft.com/", "targetId": "bg"},
                    {"type": "page", "url": "https://example.com/", "targetId": "other"}
                ]
            }
        }))
    })
    .unwrap();

    assert_eq!(find_teams_target(&fake.cdp).unwrap(), None);
}

/// A response without a target list is treated as an error.
#[test]
fn find_teams_target_rejects_missing_list() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| Some(json!({"result": {}}))).unwrap();

    assert!(find_teams_target(&fake.cdp).is_err());
}

/// Attaching returns the flattened session id.
#[test]
fn attach_to_teams_returns_session() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| {
        Some(json!({"result": {"sessionId": "sess-1"}}))
    })
    .unwrap();

    assert_eq!(attach_to_teams(&fake.cdp, "teams-1").unwrap(), "sess-1");
}

/// Attaching without a session id in the response is an error.
#[test]
fn attach_to_teams_requires_session() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| Some(json!({"result": {}}))).unwrap();

    assert!(attach_to_teams(&fake.cdp, "teams-1").is_err());
}

/// Dispatching a shortcut sends the full key sequence for that shortcut.
#[test]
fn send_shortcut_dispatches_full_sequence() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| Some(json!({"result": {}}))).unwrap();

    send_shortcut(&fake.cdp, "sess-1", Shortcut::ToggleVideo).unwrap();

    let requests = fake.wait_for_requests(6, Duration::from_secs(1));
    let events = Shortcut::ToggleVideo.keystrokes();

    assert_eq!(requests.len(), events.len());

    for (request, event) in requests.iter().zip(events.iter()) {
        assert_eq!(request["method"], "Input.dispatchKeyEvent");
        assert_eq!(request["sessionId"], "sess-1");
        assert_eq!(request["params"]["type"], event.event_type);
        assert_eq!(request["params"]["key"], event.key);
        assert_eq!(request["params"]["code"], event.code);
        assert_eq!(request["params"]["modifiers"], json!(event.modifiers));
    }
}

/// A CDP error response aborts the sequence and surfaces the failure.
#[test]
fn send_shortcut_propagates_errors() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| {
        Some(json!({"error": {"message": "nope"}}))
    })
    .unwrap();

    let error = send_shortcut(&fake.cdp, "sess-1", Shortcut::ToggleMute).unwrap_err();

    assert!(
        error.contains("key event failed"),
        "unexpected error: {error}"
    );
}

/// Waiting skips a first poll with no page, then attaches on the next.
#[test]
fn wait_for_teams_retries_until_found() {
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = Arc::clone(&calls);

    let fake = FakeCdp::start(Duration::from_secs(1), move |request| {
        if request["method"] == "Target.getTargets" {
            let call = handler_calls.fetch_add(1, Ordering::SeqCst);

            return Some(if call == 0 {
                json!({"result": {"targetInfos": []}})
            } else {
                teams_targets("teams-9")
            });
        }

        Some(json!({"result": {"sessionId": "sess-9"}}))
    })
    .unwrap();

    let session =
        wait_for_teams(&fake.cdp, Duration::from_secs(2), Duration::from_millis(1)).unwrap();

    assert_eq!(session, "sess-9");
    assert!(calls.load(Ordering::SeqCst) >= 2);
}

/// Waiting fails once the timeout elapses without a Teams page.
#[test]
fn wait_for_teams_times_out() {
    let fake = FakeCdp::start(Duration::from_secs(1), |_| {
        Some(json!({"result": {"targetInfos": []}}))
    })
    .unwrap();

    let error = wait_for_teams(
        &fake.cdp,
        Duration::from_millis(30),
        Duration::from_millis(1),
    )
    .unwrap_err();

    assert!(error.contains("Timed out"), "unexpected error: {error}");
}
