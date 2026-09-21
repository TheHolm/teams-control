//! Filesystem locations used by `teams-control`.
//!
//! The Chromium profile is treated as application data and the PID file as
//! runtime state, so both follow the XDG Base Directory specification: the
//! profile lives under `$XDG_DATA_HOME` and the PID file under
//! `$XDG_RUNTIME_DIR`. Every resolver and composer is written as a pure
//! function over its inputs so the logic can be exercised without mutating the
//! process environment; the thin env-reading wrappers sit on top.

use std::{
    env,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process,
};

/// Name of the Chromium profile directory created under `$XDG_DATA_HOME`.
pub const PROFILE: &str = "chromium-teams";

/// Name of the PID file created under `$XDG_RUNTIME_DIR`.
pub const PID_FILE: &str = "teams-control.pid";

/// Resolves the XDG data directory.
///
/// Returns `$XDG_DATA_HOME` when set and non-empty, otherwise
/// `$HOME/.local/share`. Returns [`None`] only when neither input is usable,
/// which the env-reading wrapper turns into a panic.
pub fn xdg_data_home_from(xdg: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(value) = xdg.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value));
    }

    home.filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".local").join("share"))
}

/// Resolves the XDG data directory from the process environment.
///
/// Panics if neither `XDG_DATA_HOME` nor `HOME` is set, mirroring the original
/// assumption that the process runs in a normal user session.
pub fn xdg_data_home() -> PathBuf {
    xdg_data_home_from(
        env::var_os("XDG_DATA_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )
    .expect("neither XDG_DATA_HOME nor HOME is set")
}

/// Resolves the XDG runtime directory.
///
/// Returns `$XDG_RUNTIME_DIR` when set and non-empty, otherwise `fallback`.
/// The fallback is injected so callers (and tests) can choose it explicitly.
pub fn xdg_runtime_dir_from(xdg: Option<&OsStr>, fallback: &Path) -> PathBuf {
    match xdg.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => fallback.to_path_buf(),
    }
}

/// Resolves the XDG runtime directory from the process environment.
///
/// Falls back to the system temporary directory when `XDG_RUNTIME_DIR` is
/// unset, which is the case for processes started outside a login session.
pub fn xdg_runtime_dir() -> PathBuf {
    xdg_runtime_dir_from(env::var_os("XDG_RUNTIME_DIR").as_deref(), &env::temp_dir())
}

/// Composes the Chromium profile path from the data directory.
pub fn profile_dir_from(data_home: &Path) -> PathBuf {
    data_home.join(PROFILE)
}

/// Returns the Chromium profile path for the current environment.
pub fn profile_dir() -> PathBuf {
    profile_dir_from(&xdg_data_home())
}

/// Composes the PID-file path from the runtime directory.
pub fn pid_path_from(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(PID_FILE)
}

/// Returns the PID-file path for the current environment.
pub fn pid_path() -> PathBuf {
    pid_path_from(&xdg_runtime_dir())
}

/// Writes the current process id to `path`, creating parent directories.
///
/// The file is created with `create_new`, so a second live instance fails
/// instead of silently clobbering the first, and with mode `0600` so other
/// users on the machine cannot read or replace it.
pub fn write_pid_file_at(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;

    writeln!(file, "{}", process::id())
}

/// Writes the current process id to the resolved PID-file path.
pub fn write_pid_file() -> io::Result<PathBuf> {
    let path = pid_path();
    write_pid_file_at(&path)?;
    Ok(path)
}

/// Removes `path` if it exists, ignoring any error.
///
/// Cleanup runs on both the normal and the failure exit paths, where there is
/// nothing useful to do with an error from removing a file we may not own.
pub fn remove_pid_file(path: &Path) {
    let _ = fs::remove_file(path);
}
