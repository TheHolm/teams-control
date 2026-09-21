# teams-control

`teams-control` drives the Microsoft Teams web client from Unix signals. It
launches Chromium against `https://teams.microsoft.com` with a dedicated
profile and the remote-debugging pipe enabled, attaches to the Teams page over
the Chrome DevTools Protocol (CDP), then waits. Sending the daemon a mapped
real-time signal makes it synthesize the matching Teams keyboard shortcut, so
call controls can be bound to global hotkeys, a macro keypad, or anything else
that can send a signal.

## Controls

Each action is triggered by a real-time signal above `SIGRTMIN`. The mapping
starts at `SIGRTMIN+2` because `SIGRTMIN+0` and `+1` are reserved by glibc.

| Action | Keys | Signal |
| --- | --- | --- |
| Toggle mute | `Ctrl+Shift+M` | `SIGRTMIN+2` |
| Toggle video | `Ctrl+Shift+O` | `SIGRTMIN+3` |
| Accept audio call | `Ctrl+Shift+S` | `SIGRTMIN+4` |
| Decline call | `Ctrl+Shift+D` | `SIGRTMIN+5` |
| Join from meeting started toast | `Ctrl+Shift+J` | `SIGRTMIN+6` |
| Leave meeting or call | `Ctrl+Shift+H` | `SIGRTMIN+7` |
| Stop the daemon | — | `SIGTERM` or `SIGINT` |

`SIGRTMIN` is resolved at runtime, so the numeric signal values depend on the
system's libc. Use the `SIGRTMIN+n` names with `kill`; it resolves them for you:

```sh
# Read the PID from the file the daemon writes on startup.
pid=$(cat "${XDG_RUNTIME_DIR:-/tmp}/teams-control.pid")

kill -SIGRTMIN+2 "$pid"   # toggle mute
kill -SIGRTMIN+3 "$pid"   # toggle video
kill -SIGRTMIN+4 "$pid"   # accept audio call
kill -SIGRTMIN+5 "$pid"   # decline call
kill -SIGRTMIN+6 "$pid"   # join meeting
kill -SIGRTMIN+7 "$pid"   # leave meeting or call
kill "$pid"               # stop (SIGTERM)
```

## Requirements

- Linux on glibc is the primary platform: the daemon uses `signalfd`, real-time
  signals, raw descriptor handling, and Chromium's `--remote-debugging-pipe`
  protocol.
- FreeBSD is supported on a best-effort basis only. It compiles and uses a
  `sigtimedwait`-based signal backend, but the FreeBSD path is not validated at
  runtime.
- Chromium at `/usr/bin/chromium` (Linux) or `/usr/local/bin/chrome` (FreeBSD).
- A Teams account signed in through the daemon's dedicated profile. Sign in
  once; the profile persists.

## Build and run

```sh
cargo build --release
./target/release/teams-control
```

On startup the daemon launches Chromium, waits (up to 60 seconds) for the Teams
page to load, prints the signal table, and writes its PID to a file. It runs in
the foreground and shuts Chromium down on `SIGTERM`/`SIGINT`.

## Files

Paths follow the XDG Base Directory specification:

- Chromium profile: `${XDG_DATA_HOME:-~/.local/share}/chromium-teams`
- PID file: `${XDG_RUNTIME_DIR:-/tmp}/teams-control.pid`

The PID file is created with `create_new` and mode `0600`, so a second instance
refuses to start rather than taking over, and other users cannot read it.

## Tests

```sh
cargo test
```

The suite runs without Chromium or a Teams account. The CDP client is exercised
against a fake peer built from ordinary pipes, and the signal handling is
tested with self-delivered signals (`signalfd` on Linux; the FreeBSD backend
compiles but is not run here). Only launching Chromium and the daemon's own run
loop are not covered, since they need a live browser.

## Packages

Tagged releases (`v*`) are built by the Woodpecker pipeline in
`.woodpecker/release.yaml` and published to GitHub Releases:

- `teams-control_<version>-1~trixie_amd64.deb` (Debian trixie)
- `teams-control_<version>-1~ubuntu2604_amd64.deb` (Ubuntu 26.04)
- `teams-control-<version>-freebsd-amd64.pkg` (FreeBSD, best effort)

The `.deb` packages install the binary to `/usr/bin/teams-control`; the FreeBSD
`.pkg` installs to `/usr/local/bin/teams-control`.

## License

AGPL-3.0-or-later. See `LICENSE`.

## How it works

1. Chromium is started with `--user-data-dir=<profile>`, `--remote-debugging-pipe`,
   and `--app=https://teams.microsoft.com`.
2. The child's ends of two pipes are duplicated onto file descriptors 3 and 4,
   the layout Chromium expects for the debugging pipe.
3. The daemon attaches to the Teams page target and keeps its flattened CDP
   session id.
4. It blocks the mapped signals and reads them synchronously (`signalfd` on
   Linux, `sigtimedwait` on FreeBSD).
5. On a shortcut signal it dispatches the `Ctrl+Shift+<letter>` sequence. If the
   session has gone stale (the page was recreated), it re-attaches and retries
   once.
