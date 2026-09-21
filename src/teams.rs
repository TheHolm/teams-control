//! Teams target discovery and shortcut dispatch over CDP.
//!
//! These functions sit on top of [`crate::cdp`] and only concern themselves
//! with the Teams page: finding its target, attaching a flattened session, and
//! forwarding a [`Shortcut`] as synthesized key events.

use crate::{cdp::Cdp, shortcut::Shortcut};
use serde_json::json;
use std::{
    thread,
    time::{Duration, Instant},
};

/// How long to sleep between attempts while waiting for the Teams page.
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Finds the CDP target id of the Teams page, if one is open.
///
/// Only top-level `page` targets are considered, and only those whose URL
/// contains the Teams host; everything else is ignored. Returns [`Ok(None)`]
/// when the browser is up but the Teams page has not appeared yet.
pub fn find_teams_target(cdp: &Cdp) -> Result<Option<String>, String> {
    let response = cdp.send("Target.getTargets", json!({}), None)?;

    let targets = response["result"]["targetInfos"]
        .as_array()
        .ok_or("Target.getTargets returned no target list")?;

    for target in targets {
        if target["type"] != "page" {
            continue;
        }

        let url = target["url"].as_str().unwrap_or("");

        if url.contains("teams.microsoft.com") {
            let target_id = target["targetId"]
                .as_str()
                .ok_or("Teams target has no targetId")?;

            return Ok(Some(target_id.to_string()));
        }
    }

    Ok(None)
}

/// Attaches to a Teams target and returns its flattened session id.
pub fn attach_to_teams(cdp: &Cdp, target_id: &str) -> Result<String, String> {
    let response = cdp.send(
        "Target.attachToTarget",
        json!({
            "targetId": target_id,
            "flatten": true
        }),
        None,
    )?;

    response["result"]["sessionId"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "Could not obtain Teams CDP session".into())
}

/// Waits for the Teams page to appear and attaches to it.
///
/// Polls every `poll` until `timeout` elapses. The poll interval is a parameter
/// so tests can keep this fast.
pub fn wait_for_teams(cdp: &Cdp, timeout: Duration, poll: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;

    loop {
        if Instant::now() >= deadline {
            return Err("Timed out waiting for Teams".into());
        }

        if let Some(target_id) = find_teams_target(cdp)? {
            return attach_to_teams(cdp, &target_id);
        }

        thread::sleep(poll);
    }
}

/// Sends the keyboard shortcut for `shortcut` into the attached Teams session.
///
/// Any CDP-level `error` in a response aborts the sequence so a partial
/// accelerator is not left half-dispatched.
pub fn send_shortcut(cdp: &Cdp, session_id: &str, shortcut: Shortcut) -> Result<(), String> {
    for event in shortcut.keystrokes() {
        let response = cdp.send(
            "Input.dispatchKeyEvent",
            json!({
                "type": event.event_type,
                "key": event.key,
                "code": event.code,
                "modifiers": event.modifiers
            }),
            Some(session_id),
        )?;

        if response.get("error").is_some() {
            return Err(format!("CDP key event failed: {response}"));
        }
    }

    Ok(())
}
