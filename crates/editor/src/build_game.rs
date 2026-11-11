//! Build Game: a project packed into `game.axgame` beside a copy of the
//! player for Windows, macOS or Linux (`docs/project/export.md`).
//!
//! File > Build Game calls [`build_game`] through `app/files/export.rs`;
//! `axcenity-editor --build-game <project> <folder> [--target os]` does the
//! same from a terminal, so a packaged editor can be checked on each OS
//! without opening a window (`scripts/check-packages.py`).

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use axcenity_project as project;

/// The command-line flag that builds a game and exits.
pub const BUILD_FLAG: &str = "--build-game";

/// The OS a built game runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportTarget {
    Windows,
    MacOS,
    Linux,
}

impl ExportTarget {
    /// The OS the editor runs on.
    pub fn current() -> ExportTarget {
        if cfg!(windows) {
            ExportTarget::Windows
        } else if cfg!(target_os = "macos") {
            ExportTarget::MacOS
        } else {
            ExportTarget::Linux
        }
    }

    /// `windows`, `macos` or `linux`: the folder under `players/` and the
    /// name `--target` takes.
    pub fn directory(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOS => "macos",
            Self::Linux => "linux",
        }
    }

    fn from_name(name: &str) -> Option<ExportTarget> {
        [Self::Windows, Self::MacOS, Self::Linux]
            .into_iter()
            .find(|target| target.directory() == name)
    }

    fn executable(self) -> &'static str {
        match self {
            Self::Windows => "axcenity-player.exe",
            Self::MacOS | Self::Linux => "axcenity-player",
        }
    }

    /// The player's name in the built game.
    pub fn output_name(self) -> &'static str {
        match self {
            Self::Windows => "AxcenityPlayer.exe",
            Self::MacOS | Self::Linux => "AxcenityPlayer",
        }
    }

    /// The player for this OS that ships with the editor at `editor`: the
    /// editor's own OS's beside it, the others in `players/<os>/`.
    pub fn player_beside(self, editor: &Path) -> PathBuf {
        let folder = editor.parent().unwrap_or_else(|| Path::new("."));
        if self == Self::current() {
            folder.join(self.executable())
        } else {
            folder
                .join("players")
                .join(self.directory())
                .join(self.executable())
        }
    }

    /// Where [`player_beside`](Self::player_beside) looks, in words.
    fn player_place(self) -> String {
        if self == Self::current() {
            "beside the editor".to_owned()
        } else {
            format!("in `players/{}` beside the editor", self.directory())
        }
    }
}

/// What [`build_game`] made.
#[derive(Debug)]
pub struct BuiltGame {
    /// The file to run (for macOS, the `.app` folder).
    pub run: PathBuf,
    /// Textures cooked for this build.
    pub cooked: usize,
    /// Textures that ship as they are, and why they couldn't be cooked.
    pub uncooked: Vec<(String, String)>,
    /// The game as a zip that keeps the player's executable mark, when one
    /// was written (see [`needs_archive`]).
    pub archive: Option<PathBuf>,
    /// Why the macOS app bundle couldn't be signed, when signing was tried
    /// and failed (see [`sign_app`]). The game still runs on this Mac.
    pub unsigned: Option<String>,
}

/// Built on Windows for macOS or Linux, the copied player can't be marked
/// executable (Windows doesn't keep that mark), so the game wouldn't start
/// once copied over. A zip records the mark, and unzipping restores it.
pub fn needs_archive(target: ExportTarget) -> bool {
    cfg!(windows) && target != ExportTarget::Windows
}

const MAC_INFO_PLIST: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>CFBundleDevelopmentRegion</key><string>en</string>\n\
  <key>CFBundleExecutable</key><string>AxcenityPlayer</string>\n\
  <key>CFBundleIdentifier</key><string>com.axcenity.player</string>\n\
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>\n\
  <key>CFBundleName</key><string>Axcenity Player</string>\n\
  <key>CFBundlePackageType</key><string>APPL</string>\n\
  <key>CFBundleShortVersionString</key><string>1.0</string>\n\
  <key>CFBundleVersion</key><string>1</string>\n\
  <key>LSMinimumSystemVersion</key><string>11.0</string>\n\
  <key>NSHighResolutionCapable</key><true/>\n\
</dict>\n\
</plist>\n";

/// The Linux launcher. A desktop entry runs from the user's home folder,
/// so `Exec` finds the player through `%k`, this file's own path. Desktop
/// entries quote only with double quotes, and the file format doubles each
/// backslash: the command is `exec "$(dirname "$0")/AxcenityPlayer"`.
const LINUX_DESKTOP_ENTRY: &str = "[Desktop Entry]\n\
Type=Application\n\
Name=Axcenity Player\n\
Exec=sh -c \"exec \\\\\"\\\\$(dirname \\\\\"\\\\$0\\\\\")/AxcenityPlayer\\\\\"\" %k\n\
Terminal=false\n\
Categories=Game;\n";

/// Builds the game in the project at `folder` into `destination` for
/// `target`, with the player at `player`: cooks textures that aren't yet,
/// packs `game.axgame` and copies the player beside it (inside
/// `AxcenityPlayer.app` for macOS, with the game in `Contents/Resources`),
/// with `LICENSES.txt`. With `archive`,
/// also writes those files as `AxcenityPlayer-<os>.zip` in `destination`.
pub fn build_game(
    folder: &Path,
    destination: &Path,
    target: ExportTarget,
    player: &Path,
    archive: bool,
) -> Result<BuiltGame, String> {
    if !player.is_file() {
        return Err(format!(
            "the player binary `{}` is missing. Build `axcenity-player` for {} and put it {}.",
            player.display(),
            target.directory(),
            target.player_place()
        ));
    }
    // Textures the background cooking hasn't reached yet are cooked now,
    // so the game ships every texture ready for the GPU.
    let report = axcenity_cook::cook_project(folder, &AtomicBool::new(false), |_, _| {});
    let game =
        project::pack_game(folder).map_err(|err| format!("could not pack the project: {err}"))?;

    let app = destination.join("AxcenityPlayer.app");
    // In the macOS app, only the player goes in `Contents/MacOS`: codesign
    // treats every file there as code to sign ("code object is not signed
    // at all, in subcomponent game.axgame"), so the game and the notices
    // go in `Contents/Resources`.
    let (bundle, credits) = match target {
        ExportTarget::MacOS => (app.join("Contents/MacOS"), app.join("Contents/Resources")),
        _ => (destination.to_owned(), destination.to_owned()),
    };
    // Every file written, relative to `destination`, and whether it runs.
    let mut written: Vec<(PathBuf, bool)> = Vec::new();
    let mut write = |path: PathBuf, contents: Contents, runs: bool| -> Result<(), String> {
        let failed = |err: io::Error| format!("could not write `{}`: {err}", path.display());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(failed)?;
        }
        match contents {
            Contents::Copy(from) => fs::copy(from, &path).map(|_| ()),
            Contents::Bytes(bytes) => fs::write(&path, bytes),
        }
        .map_err(failed)?;
        if runs {
            mark_executable(&path).map_err(failed)?;
        }
        let relative = path.strip_prefix(destination).unwrap_or(&path).to_owned();
        written.push((relative, runs));
        Ok(())
    };

    let run_file = bundle.join(target.output_name());
    write(
        run_file.clone(),
        Contents::Copy(player),
        target != ExportTarget::Windows,
    )?;
    write(credits.join("game.axgame"), Contents::Bytes(&game), false)?;
    drop(game);
    match target {
        ExportTarget::MacOS => write(
            app.join("Contents/Info.plist"),
            Contents::Bytes(MAC_INFO_PLIST.as_bytes()),
            false,
        )?,
        // Some file managers launch only desktop entries marked executable.
        ExportTarget::Linux => write(
            destination.join("AxcenityPlayer.desktop"),
            Contents::Bytes(LINUX_DESKTOP_ENTRY.as_bytes()),
            true,
        )?,
        ExportTarget::Windows => {}
    }
    let notices = format!(
        "Axcenity Engine and third-party license notices\n\n{}",
        include_str!("../../../NOTICE")
    );
    write(
        credits.join("LICENSES.txt"),
        Contents::Bytes(notices.as_bytes()),
        false,
    )?;

    // The copied player keeps only the linker's signature, which seals no
    // bundle: `codesign --verify --deep --strict` rejects the app ("code
    // has no resources but signature indicates they must be present").
    // Signing the whole bundle seals Info.plist and the game with it.
    let unsigned = if cfg!(target_os = "macos") && target == ExportTarget::MacOS {
        sign_app(&app).err()
    } else {
        None
    };

    let archive = if archive {
        let path = destination.join(format!("AxcenityPlayer-{}.zip", target.directory()));
        write_archive(destination, &written, &path)
            .map_err(|err| format!("could not write `{}`: {err}", path.display()))?;
        Some(path)
    } else {
        None
    };
    Ok(BuiltGame {
        run: if target == ExportTarget::MacOS {
            app
        } else {
            run_file
        },
        cooked: report.cooked,
        uncooked: report.failed,
        archive,
        unsigned,
    })
}

/// Signs the built macOS app ad hoc (`codesign --sign -`), as
/// `packaging/macos/build-dmg.sh` does for the editor. Only on a Mac:
/// elsewhere there's no `codesign`.
fn sign_app(app: &Path) -> Result<(), String> {
    let output = std::process::Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(app)
        .output()
        .map_err(|err| format!("could not run codesign: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

enum Contents<'a> {
    Copy(&'a Path),
    Bytes(&'a [u8]),
}

#[cfg(unix)]
fn mark_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn mark_executable(_: &Path) -> io::Result<()> {
    Ok(())
}

/// Zips `files` (relative to `root`), marking the ones that run as
/// executable for macOS and Linux.
fn write_archive(root: &Path, files: &[(PathBuf, bool)], path: &Path) -> io::Result<()> {
    use zip::write::SimpleFileOptions;
    let zip_error = |err: zip::result::ZipError| io::Error::other(err.to_string());
    let mut writer = zip::ZipWriter::new(fs::File::create(path)?);
    for (relative, runs) in files {
        let source = root.join(relative);
        let size = fs::metadata(&source)?.len();
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(if *runs { 0o755 } else { 0o644 })
            .large_file(size >= u64::from(u32::MAX));
        // Zip paths always use `/`.
        let name = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        writer.start_file(name, options).map_err(zip_error)?;
        io::copy(&mut fs::File::open(&source)?, &mut writer)?;
    }
    writer.finish().map_err(zip_error)?;
    Ok(())
}

/// `axcenity-editor --build-game <project> <folder> [--target os]`, with
/// the arguments after the flag. Prints what it did; returns whether it
/// built the game.
pub fn build_from_command_line(mut args: impl Iterator<Item = OsString>) -> bool {
    let usage = format!(
        "usage: axcenity-editor {BUILD_FLAG} <project folder> <output folder> [--target windows|macos|linux]"
    );
    let (Some(folder), Some(destination)) = (args.next(), args.next()) else {
        eprintln!("{usage}");
        return false;
    };
    let target = match (args.next(), args.next()) {
        (None, _) => ExportTarget::current(),
        (Some(flag), Some(name)) if flag == "--target" => {
            match name.to_str().and_then(ExportTarget::from_name) {
                Some(target) => target,
                None => {
                    eprintln!("unknown target {}\n{usage}", name.to_string_lossy());
                    return false;
                }
            }
        }
        _ => {
            eprintln!("{usage}");
            return false;
        }
    };
    let player = match std::env::current_exe() {
        Ok(editor) => target.player_beside(&editor),
        Err(err) => {
            eprintln!("could not locate the editor program: {err}");
            return false;
        }
    };
    let (folder, destination) = (PathBuf::from(folder), PathBuf::from(destination));
    match build_game(
        &folder,
        &destination,
        target,
        &player,
        needs_archive(target),
    ) {
        Ok(built) => {
            if built.cooked > 0 {
                println!("cooked {} textures", built.cooked);
            }
            for (path, reason) in &built.uncooked {
                println!("ships as it is (couldn't be cooked: {reason}): {path}");
            }
            println!(
                "built the game for {} at {}",
                target.directory(),
                built.run.display()
            );
            if let Some(archive) = &built.archive {
                println!("zipped, executable mark kept, at {}", archive.display());
            }
            if let Some(reason) = &built.unsigned {
                println!("the app isn't signed (codesign: {reason})");
            }
            true
        }
        Err(err) => {
            eprintln!("could not build the game: {err}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn example(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    /// A scratch folder with a copy of `hello-part`'s project files (the
    /// build cooks into the project's `.axcenity`), a pretend player, and
    /// the folder to build into.
    struct Scratch {
        root: PathBuf,
    }

    impl Scratch {
        fn new(test: &str) -> Scratch {
            let root = std::env::temp_dir()
                .join(format!("axcenity-build-game-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let project = root.join("project");
            fs::create_dir_all(&project).unwrap();
            for file in ["axcenity.json", "world.json"] {
                fs::copy(example("hello-part").join(file), project.join(file)).unwrap();
            }
            fs::write(root.join("player"), b"pretend player").unwrap();
            Scratch { root }
        }

        fn build(&self, target: ExportTarget, archive: bool) -> BuiltGame {
            build_game(
                &self.root.join("project"),
                &self.root.join("out"),
                target,
                &self.root.join("player"),
                archive,
            )
            .unwrap()
        }

        fn out(&self, path: &str) -> PathBuf {
            self.root.join("out").join(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_windows_build_is_the_player_and_the_game_side_by_side() {
        let scratch = Scratch::new("windows");
        let built = scratch.build(ExportTarget::Windows, false);
        assert_eq!(built.run, scratch.out("AxcenityPlayer.exe"));
        assert_eq!(fs::read(&built.run).unwrap(), b"pretend player");
        let game = fs::read(scratch.out("game.axgame")).unwrap();
        project::unpack_game(&game, &scratch.root.join("unpacked")).unwrap();
        assert!(scratch.root.join("unpacked/world.json").is_file());
        assert!(scratch.out("LICENSES.txt").is_file());
        assert!(built.archive.is_none());
    }

    #[test]
    fn a_macos_build_is_an_app_bundle_whose_info_names_the_player() {
        let scratch = Scratch::new("macos");
        let built = scratch.build(ExportTarget::MacOS, false);
        assert_eq!(built.run, scratch.out("AxcenityPlayer.app"));
        assert!(
            scratch
                .out("AxcenityPlayer.app/Contents/MacOS/AxcenityPlayer")
                .is_file()
        );
        assert!(
            scratch
                .out("AxcenityPlayer.app/Contents/Resources/game.axgame")
                .is_file()
        );
        assert!(
            scratch
                .out("AxcenityPlayer.app/Contents/Resources/LICENSES.txt")
                .is_file()
        );
        let info =
            fs::read_to_string(scratch.out("AxcenityPlayer.app/Contents/Info.plist")).unwrap();
        assert!(info.contains("<key>CFBundleExecutable</key><string>AxcenityPlayer</string>"));
        assert!(info.contains("<key>LSMinimumSystemVersion</key><string>11.0</string>"));
    }

    #[test]
    fn the_linux_launcher_runs_the_player_beside_it_wherever_it_is_opened_from() {
        let scratch = Scratch::new("linux");
        scratch.build(ExportTarget::Linux, false);
        let entry = fs::read_to_string(scratch.out("AxcenityPlayer.desktop")).unwrap();
        let exec = entry
            .lines()
            .find_map(|line| line.strip_prefix("Exec="))
            .unwrap();
        // Undo the file format's backslash doubling, then the Exec quoting,
        // as a desktop environment does.
        let exec = exec.replace("\\\\", "\\");
        assert_eq!(
            exec,
            r#"sh -c "exec \"\$(dirname \"\$0\")/AxcenityPlayer\"" %k"#
        );
        #[cfg(unix)]
        {
            let script = r#"exec "$(dirname "$0")/AxcenityPlayer""#;
            let player = scratch.out("AxcenityPlayer");
            fs::write(&player, "#!/bin/sh\necho started from \"$0\"\n").unwrap();
            mark_executable(&player).unwrap();
            let output = std::process::Command::new("sh")
                .args(["-c", script])
                .arg(scratch.out("AxcenityPlayer.desktop"))
                .current_dir(std::env::temp_dir())
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("started from"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_player_is_marked_executable_for_macos_and_linux() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("executable");
        // The source player isn't executable (as one from a Windows copy).
        for (target, run) in [
            (ExportTarget::Linux, "AxcenityPlayer"),
            (
                ExportTarget::MacOS,
                "AxcenityPlayer.app/Contents/MacOS/AxcenityPlayer",
            ),
        ] {
            scratch.build(target, false);
            let mode = fs::metadata(scratch.out(run)).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "{run}");
        }
    }

    #[test]
    fn the_zip_keeps_the_executable_mark_and_holds_only_the_build() {
        let scratch = Scratch::new("zip");
        fs::create_dir_all(scratch.out("")).unwrap();
        fs::write(scratch.out("someone-elses-file.txt"), "keep out").unwrap();
        let built = scratch.build(ExportTarget::MacOS, true);
        let archive = built.archive.unwrap();
        assert_eq!(archive, scratch.out("AxcenityPlayer-macos.zip"));
        let mut zip = zip::ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
        let mut names: Vec<_> = zip.file_names().map(str::to_owned).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "AxcenityPlayer.app/Contents/Info.plist",
                "AxcenityPlayer.app/Contents/MacOS/AxcenityPlayer",
                "AxcenityPlayer.app/Contents/Resources/LICENSES.txt",
                "AxcenityPlayer.app/Contents/Resources/game.axgame",
            ]
        );
        let mut player = zip
            .by_name("AxcenityPlayer.app/Contents/MacOS/AxcenityPlayer")
            .unwrap();
        assert_eq!(player.unix_mode().unwrap() & 0o777, 0o755);
        let mut bytes = Vec::new();
        player.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"pretend player");
        drop(player);
        let game = zip
            .by_name("AxcenityPlayer.app/Contents/Resources/game.axgame")
            .unwrap();
        assert_eq!(game.unix_mode().unwrap() & 0o777, 0o644);
    }

    #[test]
    fn a_missing_player_is_an_error_that_says_where_it_belongs() {
        let scratch = Scratch::new("missing");
        let other = [
            ExportTarget::Windows,
            ExportTarget::MacOS,
            ExportTarget::Linux,
        ]
        .into_iter()
        .find(|&target| target != ExportTarget::current())
        .unwrap();
        let err = build_game(
            &scratch.root.join("project"),
            &scratch.root.join("out"),
            other,
            &scratch.root.join("no-player"),
            false,
        )
        .unwrap_err();
        assert!(
            err.contains(&format!("players/{}", other.directory())),
            "{err}"
        );
        assert!(!scratch.out("game.axgame").exists());
        assert_eq!(
            other.player_beside(Path::new("/opt/axcenity/axcenity-editor")),
            Path::new("/opt/axcenity/players")
                .join(other.directory())
                .join(other.executable())
        );
    }
}
