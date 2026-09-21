# Release notes

## v0.1.1

### User-facing changes

- The Teams window can now be labelled with the Microsoft Teams icon instead of
  Chromium's. The daemon launches Chromium with `--class=teams-control` and
  writes `$XDG_DATA_HOME/applications/teams-control.desktop` on startup. Place a
  Teams logo at `~/.local/share/Icons/Microsoft_Office_Teams.svg.webp` and the
  desktop environment picks it up. The logo is Microsoft's trademark and is not
  bundled; download it yourself (see README).

### Low-level detail

- Added `src/desktop.rs`, which composes the desktop-entry path and icon path
  from the XDG data home, renders a `StartupWMClass=teams-control` entry
  pointing at an absolute `Icon=` path, and writes it idempotently. Covered by
  the new Chromium-free `tests/desktop.rs`.
- `start_chromium` now passes `--class=teams-control` (the `WM_CLASS` constant)
  so the window can be matched to the generated entry.
- The desktop entry is written before Chromium launches and a failure is a
  warning only, since the icon is cosmetic.
- Icon association remains desktop-environment dependent and is not verified in
  CI; the icon file is never installed by the packages.

## v0.1.0

Initial release.

### User-facing changes

- Control Microsoft Teams calls from Unix real-time signals: toggle mute
  (`SIGRTMIN+2`), toggle video (`SIGRTMIN+3`), accept audio call
  (`SIGRTMIN+4`), decline call (`SIGRTMIN+5`), join from the
  meeting-started toast (`SIGRTMIN+6`), and leave a meeting or call
  (`SIGRTMIN+7`). `SIGTERM`/`SIGINT` stop the daemon.
- Packages: a Debian trixie `.deb`, an Ubuntu 26.04 `.deb`, and a
  best-effort FreeBSD `.pkg`.
- Linux on glibc is the primary platform; FreeBSD support is best-effort
  (compile-checked and packaged, but not run or validated).

### Low-level detail

- Project renamed from `teams-answer` to `teams-control`, version reset to
  `0.1.0`, licensed AGPL-3.0-or-later.
- Restructured the single-file binary into a library (`src/lib.rs`) plus
  `cdp`, `shortcut`, `signals`, `paths` and `teams` modules; `main.rs` is a
  thin binary.
- Replaced the single `SIGUSR1` action with runtime-resolved real-time
  signals starting at `SIGRTMIN+2` (`SIGRTMIN+0`/`+1` are reserved by
  glibc, so the numeric values are never hardcoded).
- Signal handling has a Linux `signalfd` backend and a FreeBSD
  `sigtimedwait` backend. `resolve_base()` uses `libc::SIGRTMIN()` on
  Linux and a hardcoded `65` on FreeBSD, where the `libc` crate does not
  export `SIGRTMIN`.
- Corrected the CDP key-event modifier bits for `Ctrl+Shift` from `12` to
  `10`.
- Moved runtime paths to XDG locations: the Chromium profile to
  `$XDG_DATA_HOME/chromium-teams` and the PID file to
  `$XDG_RUNTIME_DIR/teams-control.pid` (temp-dir fallback).
- Added a Chromium-free integration test suite in `tests/` (47 tests)
  built on a pipe-based `FakeCdp` peer, covering CDP framing and routing,
  shortcut dispatch, signal delivery, and path resolution.
- Added the tag-triggered Woodpecker release pipeline
  (`.woodpecker/release.yaml`) and the FreeBSD `.pkg` builder
  (`scripts/build-freebsd-pkg.py`).
