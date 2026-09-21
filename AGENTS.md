# AGENTS.md

## Repository visibility

**This repository is pushed to a public GitHub repo.** Before committing,
pushing, or writing anything into a tracked file (code, docs, scripts, test
fixtures, commit messages), make sure it contains no sensitive or identifying
information: no real hostnames/IPs, credentials/passwords/API keys/private
keys, serial numbers of specific physical devices, personal file paths, or
other details tied to a particular person's or machine's identity. Genuinely
throwaway material (e.g. ad hoc test scripts/VM connection details written for
a single session) belongs outside the repo entirely (e.g. under `/tmp`), never
committed - see the "Never commit changes unless the user explicitly asks to
commit" rule below, which exists partly for this reason.

## Project overview

`teams-control` is a Linux daemon that drives the Microsoft Teams web client
from Unix signals. It launches Chromium against `https://teams.microsoft.com`
with a dedicated profile and the remote-debugging pipe enabled, attaches to the
Teams page over the Chrome DevTools Protocol (CDP), and synthesizes a Teams
keyboard shortcut (`Ctrl+Shift+<letter>`) whenever a mapped real-time signal
arrives. The package, library and binary are all named `teams-control`.
Version: v0.1.0 (declared as `0.1.0` in `Cargo.toml`, also printed on startup).

## Stack

- Rust 2024 edition, std threads and `mpsc` (no async runtime)
- `libc` — pipes, `dup2`, `signalfd`, `fcntl`, `read`/`write`, and
  `Command::pre_exec` for the descriptor hand-off to Chromium
- `serde_json` — encoding/decoding CDP JSON messages
- Chromium (external, at `/usr/bin/chromium`) as the Teams runtime
- Dev-dependencies: `tempfile` for the path/PID tests, `serial_test` for the
  tests that mutate the process environment or signal mask

## Target platforms

Linux on glibc only. The code depends on Linux-specific facilities:
`signalfd`/`signalfd_siginfo`, runtime-resolved kernel real-time signals,
`pipe`/`dup2` placement of descriptors 3 and 4 for Chromium's
`--remote-debugging-pipe`, and `F_DUPFD_CLOEXEC`. `libc::SIGRTMIN()` is a
function on glibc (musl exposes a constant instead), which is another reason
glibc is assumed. No effort is made to support other platforms.

## Layout

- `src/main.rs` — thin binary: PID-file lifecycle, Chromium launch and
  debugging-pipe hand-off, the signal-dispatch loop, and the startup banner.
  This is the only place that cannot be tested without a live browser
- `src/lib.rs` — the library crate (`teams_control`) exposing every other layer
  so the integration tests can drive it
- `src/cdp.rs` — CDP client over the pipe pair: NUL-delimited framing, request
  encoding, id→response routing, and the pipe/`dup2` helpers
- `src/shortcut.rs` — the `Shortcut` enum, its signal offsets, key letters and
  the `Ctrl+Shift+<letter>` keystroke builder
- `src/signals.rs` — `Signals::install`/`next` over `signalfd`, plus
  `resolve_base` for the runtime `SIGRTMIN`
- `src/teams.rs` — Teams target discovery, attach, `wait_for_teams`, and
  `send_shortcut` dispatch
- `src/paths.rs` — XDG base-directory resolution, the Chromium profile path, and
  PID-file read/write
- `tests/` — integration tests, all Chromium-free: `common/mod.rs` holds the
  `FakeCdp` pipe-based CDP peer and an `EnvGuard`; `shortcut.rs`, `paths.rs`,
  `signals.rs`, `cdp.rs` and `teams.rs` cover the matching modules
- `README.markdown` — user-facing docs: description, requirements, build/run,
  the XDG paths, and the signal→shortcut table with `kill` examples
- `.gitignore` — ignores `/target` and `Cargo.lock` (this is a binary crate but
  the lock file is deliberately not tracked)

## Signals

Each action maps to a real-time signal above `SIGRTMIN`. The mapping starts at
`SIGRTMIN+2` because glibc reserves `SIGRTMIN+0`/`+1` (`SIGCANCEL`/`SIGSETXID`)
and `libc::SIGRTMIN()` is resolved at runtime, so the numeric values are not
hardcoded anywhere.

| Signal | Action | Keys |
| --- | --- | --- |
| `SIGRTMIN+2` | Toggle mute | `Ctrl+Shift+M` |
| `SIGRTMIN+3` | Toggle video | `Ctrl+Shift+O` |
| `SIGRTMIN+4` | Accept audio call | `Ctrl+Shift+S` |
| `SIGRTMIN+5` | Decline call | `Ctrl+Shift+D` |
| `SIGRTMIN+6` | Join from meeting started toast | `Ctrl+Shift+J` |
| `SIGRTMIN+7` | Leave meeting or call | `Ctrl+Shift+H` |
| `SIGTERM`/`SIGINT` | Shut Chromium down and exit | — |

`README.markdown` documents the same table; keep the two in sync, and update
`Shortcut::ALL`/`offset` in `src/shortcut.rs` when adding or moving a signal.

## Status / known gaps

Work in progress. Current known issues:

- `src/main.rs` (`start_chromium`, the `pre_exec` descriptor hand-off, and the
  dispatch loop) has no automated coverage; it needs a real Chromium and a
  signed-in Teams profile. Everything else is covered by the Chromium-free
  integration tests.
- Chromium is hardcoded to `/usr/bin/chromium`; there is no PATH search or
  override.
- Only the first Teams page target is tracked. The code re-attaches if the page
  is recreated, but additional targets are ignored.
- The PID file is created with `create_new`, so a crash leaves a stale file that
  must be removed before the daemon will start again.
- Sign-in is manual: the dedicated profile must be authenticated once through
  Chromium before the shortcuts mean anything.

## Commands

- Build/check: `cargo build`
- Run: `cargo run` (requires Chromium at `/usr/bin/chromium`; see README)
- Tests: `cargo test` (all Chromium-free; `tests/common` builds the fake CDP
  peer from real pipes)
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt` / check with `cargo fmt --check`

## Conventions

- `cargo fmt` style, no external formatters
- Every function and every test is documented with a `///` doc comment describing its purpose and any non-obvious behavior
- All new code must be covered by tests — unit tests for private helpers and integration tests in `tests/` for public behaviour; never add production code without accompanying tests
- Commits include a detailed description of what changed and why
- When merging a branch to master that will **not** be tagged as a release,
  summarise all changes in the code since the branch started (or the last
  merge to master) and use that summary as the merge description
- When merging a branch to master that **will** be tagged as a release, the
  merge commit description contains only user-affecting changes (new
  features, bug fixes, changed behavior) — no low-level implementation
  detail. Add a new entry to `RELEASE_NOTES.md` (which holds the full
  history of releases) with that same user-facing summary plus all the
  low-level detail that would otherwise have gone in the merge commit
  description
- When starting work on each new branch, ask the user whether to bump the version number (and if so, to what value) before writing any code
- Version numbers follow `X.Y.Z`: `X` (major) is bumped only when the user
  explicitly asks for it; `Y` (minor) is bumped when a change adds a new
  feature; `Z` (patch) is bumped for bugfixes and other changes that don't
  add, remove, or change functionality
- Never commit changes unless the user explicitly asks to commit
