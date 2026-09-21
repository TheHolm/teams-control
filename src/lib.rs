//! `teams-control` drives the Microsoft Teams web client from Unix signals.
//!
//! The binary launches Chromium against the Teams web app with a dedicated
//! profile and the remote-debugging pipe enabled, attaches to the page over the
//! Chrome DevTools Protocol, and dispatches a Teams keyboard shortcut whenever
//! one of the mapped real-time signals arrives. The library exposes each layer
//! ([`cdp`], [`shortcut`], [`signals`], [`paths`], [`teams`]) so it can be
//! exercised without a browser.

pub mod cdp;
pub mod desktop;
pub mod paths;
pub mod shortcut;
pub mod signals;
pub mod teams;
