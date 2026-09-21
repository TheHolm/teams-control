//! Binary entry point for `teams-control`.
//!
//! Owns the pieces that cannot be tested without launching Chromium: the PID
//! file lifecycle, the Chromium process and its debugging-pipe hand-off, and the
//! main signal-dispatch loop. Everything else lives in the library.

use libc::{SIGINT, SIGTERM, c_int, close, dup2};
use std::{
    io,
    os::unix::process::CommandExt,
    process::{Child, Command},
    sync::Arc,
    thread,
    time::Duration,
};
use teams_control::{
    cdp::{CDP_TIMEOUT, Cdp, make_fd_safe, make_pipe},
    desktop,
    paths::{pid_path, profile_dir, remove_pid_file, write_pid_file},
    shortcut::{ALL as ALL_SHORTCUTS, Shortcut},
    signals::{Signals, resolve_base},
    teams::{POLL_INTERVAL, send_shortcut, wait_for_teams},
};

/// Path to the Chromium binary.
///
/// FreeBSD (best effort) installs Chromium as `chrome` under `/usr/local`.
#[cfg(target_os = "linux")]
const CHROMIUM: &str = "/usr/bin/chromium";
#[cfg(target_os = "freebsd")]
const CHROMIUM: &str = "/usr/local/bin/chrome";

/// How long to wait for the Teams page to appear after Chromium starts.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for the Teams page to reappear before retrying a shortcut.
const RECONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Launches Chromium with the remote-debugging pipe and connects a [`Cdp`].
///
/// Two pipes are created and their child ends are duplicated onto descriptors 3
/// and 4, which is the layout Chromium's `--remote-debugging-pipe` expects. The
/// parent keeps the other ends and drives them through the CDP client.
fn start_chromium() -> io::Result<(Child, Arc<Cdp>)> {
    let profile_dir = profile_dir();

    std::fs::create_dir_all(&profile_dir)?;

    let (child_read, parent_write) = make_pipe()?;
    let (parent_read, child_write) = make_pipe()?;

    let child_read = make_fd_safe(child_read)?;
    let child_write = make_fd_safe(child_write)?;

    let mut command = Command::new(CHROMIUM);

    command
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--remote-debugging-pipe")
        .arg(format!("--class={}", desktop::WM_CLASS))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-features=DialMediaRouteProvider")
        .arg("--app=https://teams.microsoft.com");

    // Chromium expects FD 3 to read commands and FD 4 to write responses.
    // dup2() also clears FD_CLOEXEC on the destination descriptors.
    unsafe {
        command.pre_exec(move || {
            if dup2(child_read, 3) < 0 {
                return Err(io::Error::last_os_error());
            }

            if dup2(child_write, 4) < 0 {
                return Err(io::Error::last_os_error());
            }

            close(child_read);
            close(child_write);

            Ok(())
        });
    }

    let child = command.spawn()?;

    unsafe {
        close(child_read);
        close(child_write);
    }

    let cdp = Cdp::connect(parent_write, parent_read, CDP_TIMEOUT);

    Ok((child, cdp))
}

/// Builds the list of signals to block, from the shortcut map plus shutdown.
fn install_signals(base: c_int) -> io::Result<Signals> {
    let mut signals: Vec<c_int> = ALL_SHORTCUTS.iter().map(|s| base + s.offset()).collect();

    signals.push(SIGTERM);
    signals.push(SIGINT);

    Signals::install(&signals)
}

/// Prints the startup banner listing every mapped signal.
fn print_banner(base: c_int) {
    println!("teams-control v{}", env!("CARGO_PKG_VERSION"));
    println!("Ready. SIGRTMIN = {base}");

    for shortcut in ALL_SHORTCUTS {
        let signal = base + shortcut.offset();

        println!(
            "  {:<34} SIGRTMIN+{} ({signal})  kill -SIGRTMIN+{} <pid>",
            shortcut.label(),
            shortcut.offset(),
            shortcut.offset(),
        );
    }

    println!("  {:<34} SIGTERM/SIGINT  kill <pid>", "stop");
}

/// Sends `shortcut`, re-attaching to Teams once if the first attempt fails.
///
/// The page can be recreated while the daemon runs, which invalidates the
/// session id; reconnecting and retrying once recovers from that case.
fn dispatch(cdp: &Cdp, session_id: &mut String, shortcut: Shortcut) {
    println!("{}: sending shortcut", shortcut.label());

    match send_shortcut(cdp, session_id, shortcut) {
        Ok(()) => println!("Shortcut sent"),

        Err(error) => {
            eprintln!("Failed to send shortcut: {error}");

            match wait_for_teams(cdp, RECONNECT_TIMEOUT, POLL_INTERVAL) {
                Ok(new_session) => {
                    *session_id = new_session;

                    if let Err(error) = send_shortcut(cdp, session_id, shortcut) {
                        eprintln!("Retry failed: {error}");
                    }
                }

                Err(error) => eprintln!("Could not reconnect to Teams: {error}"),
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pid_path = match write_pid_file() {
        Ok(path) => path,

        Err(error) => {
            eprintln!("Cannot create PID file {}: {}", pid_path().display(), error);

            std::process::exit(1);
        }
    };

    let base = resolve_base();

    // Block signals before spawning Chromium (and its CDP reader thread) so
    // every thread inherits the block.
    let signals = install_signals(base)?;

    // The desktop entry is cosmetic and best effort: a failure only means the
    // window keeps the default Chromium icon, so it must not abort startup.
    if let Err(error) = desktop::write() {
        eprintln!("Cannot write desktop entry: {error}");
    }

    let (mut chromium, cdp) = start_chromium()?;

    println!("Chromium PID: {}", chromium.id());

    let mut session_id = match wait_for_teams(&cdp, STARTUP_TIMEOUT, POLL_INTERVAL) {
        Ok(session) => {
            println!("Teams connected");
            session
        }

        Err(error) => {
            eprintln!("Teams startup failed: {error}");
            let _ = chromium.kill();
            remove_pid_file(&pid_path);
            return Err(error.into());
        }
    };

    print_banner(base);
    println!("PID: {}", std::process::id());
    println!("PID file: {}", pid_path.display());

    loop {
        if let Some(signal) = signals.next()? {
            if signal == SIGTERM || signal == SIGINT {
                println!("Stopping");

                let _ = chromium.kill();
                let _ = chromium.wait();

                break;
            }

            if let Some(shortcut) = Shortcut::from_signal(signal, base) {
                dispatch(&cdp, &mut session_id, shortcut);
            }
        }

        if let Some(_status) = chromium.try_wait()? {
            eprintln!("Chromium exited");
            break;
        }

        thread::sleep(Duration::from_millis(50));
    }

    remove_pid_file(&pid_path);

    Ok(())
}
