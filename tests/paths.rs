//! Tests for XDG path resolution and PID-file handling.

mod common;

use common::EnvGuard;
use serial_test::serial;
use std::{ffi::OsStr, fs, os::unix::fs::PermissionsExt, path::PathBuf};
use teams_control::paths::{
    PID_FILE, PROFILE, pid_path, pid_path_from, profile_dir, profile_dir_from, remove_pid_file,
    write_pid_file_at, xdg_data_home, xdg_data_home_from, xdg_runtime_dir, xdg_runtime_dir_from,
};
use tempfile::tempdir;

/// `$XDG_DATA_HOME` wins when it is set.
#[test]
fn data_home_prefers_xdg_when_set() {
    let resolved = xdg_data_home_from(
        Some(OsStr::new("/xdg/data")),
        Some(OsStr::new("/home/user")),
    );

    assert_eq!(resolved, Some(PathBuf::from("/xdg/data")));
}

/// Without `$XDG_DATA_HOME`, the data dir falls back to `$HOME/.local/share`.
#[test]
fn data_home_falls_back_to_home() {
    let resolved = xdg_data_home_from(None, Some(OsStr::new("/home/user")));

    assert_eq!(resolved, Some(PathBuf::from("/home/user/.local/share")));
}

/// Empty environment values are treated as unset.
#[test]
fn data_home_ignores_empty_values() {
    let resolved = xdg_data_home_from(Some(OsStr::new("")), Some(OsStr::new("/home/user")));

    assert_eq!(resolved, Some(PathBuf::from("/home/user/.local/share")));

    assert_eq!(xdg_data_home_from(Some(OsStr::new("")), None), None);
    assert_eq!(xdg_data_home_from(None, None), None);
}

/// `$XDG_RUNTIME_DIR` wins when it is set.
#[test]
fn runtime_dir_prefers_xdg_when_set() {
    let fallback = PathBuf::from("/tmp");

    assert_eq!(
        xdg_runtime_dir_from(Some(OsStr::new("/run/user/1000")), &fallback),
        PathBuf::from("/run/user/1000")
    );
}

/// Without `$XDG_RUNTIME_DIR`, the injected fallback is used.
#[test]
fn runtime_dir_uses_fallback_when_unset() {
    let fallback = PathBuf::from("/tmp");

    assert_eq!(xdg_runtime_dir_from(None, &fallback), PathBuf::from("/tmp"));

    assert_eq!(
        xdg_runtime_dir_from(Some(OsStr::new("")), &fallback),
        PathBuf::from("/tmp")
    );
}

/// The profile path is the data directory plus the fixed profile name.
#[test]
fn profile_dir_appends_profile_name() {
    assert_eq!(
        profile_dir_from(&PathBuf::from("/data")),
        PathBuf::from("/data").join(PROFILE)
    );
}

/// The PID path is the runtime directory plus the fixed file name.
#[test]
fn pid_path_appends_file_name() {
    assert_eq!(
        pid_path_from(&PathBuf::from("/run")),
        PathBuf::from("/run").join(PID_FILE)
    );
}

/// Writing a PID file records the current process id with mode `0600`.
#[test]
fn write_pid_file_records_pid_and_permissions() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("teams-control.pid");

    write_pid_file_at(&path).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents, format!("{}\n", std::process::id()));

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

/// A second write to the same path fails rather than clobbering the first.
#[test]
fn write_pid_file_is_create_new() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("teams-control.pid");

    write_pid_file_at(&path).unwrap();

    let error = write_pid_file_at(&path).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
}

/// Writing creates missing parent directories.
#[test]
fn write_pid_file_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let path = dir
        .path()
        .join("nested")
        .join("run")
        .join("teams-control.pid");

    write_pid_file_at(&path).unwrap();

    assert!(path.exists());
}

/// Removing an existing PID file deletes it; removing a missing one is a no-op.
#[test]
fn remove_pid_file_is_idempotent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("teams-control.pid");

    write_pid_file_at(&path).unwrap();
    remove_pid_file(&path);
    assert!(!path.exists());

    remove_pid_file(&path);
}

/// The env-reading data-home wrapper honours `$XDG_DATA_HOME`.
#[test]
#[serial]
fn xdg_data_home_wrapper_reads_env() {
    let dir = tempdir().unwrap();
    let value = dir.path().to_str().unwrap();
    let _guard = EnvGuard::set("XDG_DATA_HOME", value);

    assert_eq!(xdg_data_home(), dir.path());
}

/// The env-reading data-home wrapper falls back to `$HOME/.local/share`.
#[test]
#[serial]
fn xdg_data_home_wrapper_falls_back_to_home() {
    let dir = tempdir().unwrap();
    let value = dir.path().to_str().unwrap();
    let _removed = EnvGuard::remove("XDG_DATA_HOME");
    let _home = EnvGuard::set("HOME", value);

    assert_eq!(xdg_data_home(), dir.path().join(".local").join("share"));
}

/// The env-reading runtime-dir wrapper honours `$XDG_RUNTIME_DIR`.
#[test]
#[serial]
fn xdg_runtime_dir_wrapper_reads_env() {
    let dir = tempdir().unwrap();
    let value = dir.path().to_str().unwrap();
    let _guard = EnvGuard::set("XDG_RUNTIME_DIR", value);

    assert_eq!(xdg_runtime_dir(), dir.path());
}

/// The env-reading runtime-dir wrapper falls back to the system temp dir.
#[test]
#[serial]
fn xdg_runtime_dir_wrapper_falls_back_to_temp() {
    let _removed = EnvGuard::remove("XDG_RUNTIME_DIR");

    assert_eq!(xdg_runtime_dir(), std::env::temp_dir());
}

/// The convenience wrappers compose the same paths as the pure helpers.
#[test]
#[serial]
fn convenience_wrappers_compose_paths() {
    let dir = tempdir().unwrap();
    let value = dir.path().to_str().unwrap();
    let _data = EnvGuard::set("XDG_DATA_HOME", value);
    let _runtime = EnvGuard::set("XDG_RUNTIME_DIR", value);

    assert_eq!(profile_dir(), dir.path().join(PROFILE));
    assert_eq!(pid_path(), dir.path().join(PID_FILE));
}
