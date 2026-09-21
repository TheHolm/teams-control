//! Tests for the generated desktop entry that labels the Teams window.

use std::{fs, path::PathBuf};
use teams_control::desktop::{
    DESKTOP_FILE, ICON_DIR, ICON_FILE, NAME, WM_CLASS, entry_path_from, icon_path_from, render,
    write_at,
};
use tempfile::tempdir;

/// The icon path is the data home plus the fixed icon directory and file name.
#[test]
fn icon_path_appends_icon_file() {
    let data_home = PathBuf::from("/data");

    assert_eq!(
        icon_path_from(&data_home),
        data_home.join(ICON_DIR).join(ICON_FILE)
    );
}

/// The entry path is the data home plus `applications/` and the fixed file name.
#[test]
fn entry_path_appends_desktop_file() {
    let data_home = PathBuf::from("/data");

    assert_eq!(
        entry_path_from(&data_home),
        data_home.join("applications").join(DESKTOP_FILE)
    );
}

/// The rendered entry carries the class, icon and name needed for DE matching.
#[test]
fn render_contains_matching_fields() {
    let icon = PathBuf::from("/home/user/.local/share/Icons/Microsoft_Office_Teams.svg.webp");
    let entry = render(NAME, WM_CLASS, &icon, "/usr/bin/teams-control");

    assert!(entry.starts_with("[Desktop Entry]\n"));
    assert!(entry.contains(&format!("Name={NAME}\n")));
    assert!(entry.contains(&format!("StartupWMClass={WM_CLASS}\n")));
    assert!(entry.contains(&format!("Icon={}\n", icon.display())));
    assert!(entry.contains("Exec=/usr/bin/teams-control\n"));
}

/// The icon is written as an absolute path, never a shell-expanded `~`.
#[test]
fn render_does_not_use_tilde() {
    let icon = PathBuf::from("/home/user/.local/share/Icons/Microsoft_Office_Teams.svg.webp");
    let entry = render(NAME, WM_CLASS, &icon, "/usr/bin/teams-control");

    let icon_line = entry
        .lines()
        .find(|line| line.starts_with("Icon="))
        .unwrap();

    assert!(!icon_line.contains('~'));
    assert!(icon_line.starts_with("Icon=/"));
}

/// An `Exec` path without special characters is left unquoted.
#[test]
fn render_leaves_plain_exec_unquoted() {
    let entry = render(
        NAME,
        WM_CLASS,
        &PathBuf::from("/icon"),
        "/usr/bin/teams-control",
    );

    assert!(entry.contains("Exec=/usr/bin/teams-control\n"));
}

/// An `Exec` path containing spaces is quoted per the Desktop Entry spec.
#[test]
fn render_quotes_exec_with_spaces() {
    let entry = render(
        NAME,
        WM_CLASS,
        &PathBuf::from("/icon"),
        "/opt/my apps/teams-control",
    );

    assert!(entry.contains("Exec=\"/opt/my apps/teams-control\"\n"));
}

/// `write_at` creates missing parent directories and writes the rendered entry.
#[test]
fn write_at_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let data_home = dir.path().join("nested").join("data");

    let path = write_at(&data_home, "/usr/bin/teams-control").unwrap();

    assert_eq!(path, entry_path_from(&data_home));
    assert!(path.exists());

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(
        contents,
        render(
            NAME,
            WM_CLASS,
            &icon_path_from(&data_home),
            "/usr/bin/teams-control"
        )
    );
}

/// A second write replaces the previous entry rather than failing.
#[test]
fn write_at_overwrites_existing_entry() {
    let dir = tempdir().unwrap();
    let data_home = dir.path();

    write_at(data_home, "/usr/bin/teams-control").unwrap();
    write_at(data_home, "/other/teams-control").unwrap();

    let contents = fs::read_to_string(entry_path_from(data_home)).unwrap();
    assert!(contents.contains("Exec=/other/teams-control\n"));
    assert!(!contents.contains("/usr/bin/teams-control"));
}

/// The class advertised by the entry is exactly the one passed to Chromium.
#[test]
fn startup_wm_class_matches_chromium_class() {
    let entry = render(
        NAME,
        WM_CLASS,
        &PathBuf::from("/icon"),
        "/usr/bin/teams-control",
    );

    assert!(entry.contains(&format!("StartupWMClass={WM_CLASS}\n")));
}
