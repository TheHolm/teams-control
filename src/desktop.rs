//! Generates the per-user desktop entry that labels the Teams app window.
//!
//! Chromium has no way to set an app window's icon directly. On Linux a desktop
//! environment instead matches the window's `WM_CLASS` against the
//! `StartupWMClass` of a `.desktop` file and draws that file's `Icon=`. The
//! daemon therefore launches Chromium with [`WM_CLASS`] and writes a matching
//! entry into `$XDG_DATA_HOME/applications`, pointing at an icon the user
//! downloads themselves (Microsoft's Teams logo cannot be redistributed with
//! this AGPL project). Every composer is a pure function over its inputs so the
//! path and content logic can be tested without touching the process
//! environment; [`write`] is the thin env-reading wrapper.

use std::{
    env, io,
    path::{Path, PathBuf},
};

use crate::paths::xdg_data_home;

/// `WM_CLASS` the Chromium window is given, and the `StartupWMClass` the
/// generated desktop entry advertises. The two must stay identical or the
/// desktop environment cannot associate the window with the entry.
pub const WM_CLASS: &str = "teams-control";

/// Display name of the generated desktop entry.
pub const NAME: &str = "M$ Teams";

/// File name of the generated desktop entry under `applications/`.
pub const DESKTOP_FILE: &str = "teams-control.desktop";

/// Directory, under the data home, that the user drops the Teams icon into.
pub const ICON_DIR: &str = "Icons";

/// File name of the Teams icon the user downloads from Wikipedia.
///
/// The `.svg.webp` suffix is deliberate: it is the name a browser gives when the
/// SVG is saved from the web. Desktop Entry implementations are only required to
/// support PNG/SVG/XPM, so the README notes that an SVG or PNG is safer if the
/// icon fails to render.
pub const ICON_FILE: &str = "Microsoft_Office_Teams.svg.webp";

/// Composes the icon path from the data home.
///
/// The path is absolute, as required by the `Icon=` field; a `~` would not be
/// expanded inside a desktop entry.
pub fn icon_path_from(data_home: &Path) -> PathBuf {
    data_home.join(ICON_DIR).join(ICON_FILE)
}

/// Composes the desktop-entry path from the data home.
pub fn entry_path_from(data_home: &Path) -> PathBuf {
    data_home.join("applications").join(DESKTOP_FILE)
}

/// Escapes `exec` for a Desktop Entry `Exec=` value, quoting only when needed.
///
/// The specification treats space and a few shell-like characters as special, so
/// the value is wrapped in double quotes and those characters backslash-escaped
/// when present. A plain path is emitted unquoted.
fn quote_exec(exec: &str) -> String {
    if exec.contains([' ', '\t', '"', '`', '$', '\\']) {
        let escaped = exec
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$");

        format!("\"{escaped}\"")
    } else {
        exec.to_string()
    }
}

/// Renders the `teams-control.desktop` contents.
///
/// `icon` must be an absolute path. `exec` is the command a desktop environment
/// would run if the entry is launched; it is escaped through [`quote_exec`].
pub fn render(name: &str, wm_class: &str, icon: &Path, exec: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={name}\n\
         Comment=Microsoft Teams driven by teams-control\n\
         Exec={exec}\n\
         Icon={icon}\n\
         StartupWMClass={wm_class}\n\
         Terminal=false\n\
         NoDisplay=false\n\
         Categories=Network;InstantMessaging;\n",
        exec = quote_exec(exec),
        icon = icon.display(),
    )
}

/// Writes the desktop entry for `data_home`, creating `applications/`.
///
/// Any existing entry is overwritten so it tracks the current binary path and
/// icon location. Returns the path that was written.
pub fn write_at(data_home: &Path, exec: &str) -> io::Result<PathBuf> {
    let path = entry_path_from(data_home);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(
        &path,
        render(NAME, WM_CLASS, &icon_path_from(data_home), exec),
    )?;

    Ok(path)
}

/// Writes the desktop entry for the current environment.
///
/// Uses `$XDG_DATA_HOME` for the location and the running binary's path for the
/// entry's `Exec=`, so the generated launcher works for both `cargo run` and an
/// installed `/usr/bin/teams-control`.
pub fn write() -> io::Result<PathBuf> {
    let exec = env::current_exe()?;

    write_at(&xdg_data_home(), &exec.to_string_lossy())
}
