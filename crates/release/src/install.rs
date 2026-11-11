//! How this copy of Axcenity was installed: `install.json`, which the
//! packaging writes into the install folder (`packaging/stage.py` and
//! `packaging/linux/build-packages.sh`). It decides whether the editor can
//! update itself (Windows setup, Linux tar.gz and AppImage) or should point
//! to a new package instead. No file means a build from source.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The marker's file name, beside the editor executable.
pub const INSTALL_FILE: &str = "install.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallKind {
    /// The per-user Windows setup.
    WindowsSetup,
    /// The portable Linux `.tar.gz`.
    LinuxTarball,
    #[serde(rename = "appimage")]
    AppImage,
    /// The Linux system packages, updated by their package manager.
    Deb,
    Rpm,
    Pacman,
    MacosDmg,
    /// Built from source, or a folder without `install.json`.
    Source,
}

impl InstallKind {
    /// Whether the editor may download and install updates itself.
    pub fn updates_itself(self) -> bool {
        matches!(
            self,
            InstallKind::WindowsSetup | InstallKind::LinuxTarball | InstallKind::AppImage
        )
    }

    /// How it reads in diagnostics and settings.
    pub fn label(self) -> &'static str {
        match self {
            InstallKind::WindowsSetup => "Windows setup",
            InstallKind::LinuxTarball => "Linux tar.gz",
            InstallKind::AppImage => "AppImage",
            InstallKind::Deb => "Debian package",
            InstallKind::Rpm => "RPM package",
            InstallKind::Pacman => "Arch package",
            InstallKind::MacosDmg => "macOS disk image",
            InstallKind::Source => "built from source",
        }
    }

    /// How the running program was installed: the `install.json` beside its
    /// executable.
    pub fn current() -> InstallKind {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(InstallKind::of_folder))
            .unwrap_or(InstallKind::Source)
    }

    /// The kind recorded in `dir`'s `install.json`; [`InstallKind::Source`]
    /// when there is none or it can't be read.
    pub fn of_folder(dir: &Path) -> InstallKind {
        std::fs::read_to_string(dir.join(INSTALL_FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<InstallMarker>(&text).ok())
            .map_or(InstallKind::Source, |marker| marker.kind)
    }
}

/// `install.json`. Unknown fields are ignored, so later packaging can add
/// some.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallMarker {
    pub schema: u32,
    pub kind: InstallKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_names_how_it_was_installed() {
        let dir = std::env::temp_dir().join(format!("axcenity-install-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(InstallKind::of_folder(&dir), InstallKind::Source);

        // The exact text packaging/stage.py writes.
        std::fs::write(
            dir.join(INSTALL_FILE),
            "{\"schema\": 1, \"kind\": \"windows-setup\"}\n",
        )
        .unwrap();
        assert_eq!(InstallKind::of_folder(&dir), InstallKind::WindowsSetup);
        assert!(InstallKind::WindowsSetup.updates_itself());

        std::fs::write(
            dir.join(INSTALL_FILE),
            "{\"schema\": 1, \"kind\": \"deb\", \"later\": true}",
        )
        .unwrap();
        assert_eq!(InstallKind::of_folder(&dir), InstallKind::Deb);
        assert!(!InstallKind::Deb.updates_itself());

        std::fs::write(dir.join(INSTALL_FILE), "not json").unwrap();
        assert_eq!(InstallKind::of_folder(&dir), InstallKind::Source);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn kinds_use_the_names_the_packaging_writes() {
        for (kind, name) in [
            (InstallKind::WindowsSetup, "windows-setup"),
            (InstallKind::LinuxTarball, "linux-tarball"),
            (InstallKind::AppImage, "appimage"),
            (InstallKind::MacosDmg, "macos-dmg"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), name);
        }
    }
}
