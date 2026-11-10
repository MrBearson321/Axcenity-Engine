#!/usr/bin/env python3
"""Builds this computer's editor packages and checks them, without GitHub.

The same packaging steps as `.github/workflows/release.yml` (turned off on
purpose), run here, then checked: the files every package must hold, the
packaged editor building a game for each OS it has a player for
(`axcenity-editor --build-game`), and the built game for this OS starting
and drawing frames. See docs/project/releases.md ("Checking packages").

    python3 scripts/check-packages.py                  # dist profile, as released
    python3 scripts/check-packages.py --profile release  # quicker build
    python3 scripts/check-packages.py --players DIR    # DIR/<os>/axcenity-player[.exe]
                                                       # from the other computers

Packages are made only where their tool is installed (nfpm and appimagetool
on Linux, makensis on Windows, dmgbuild on macOS); a missing tool is a SKIP,
not a FAIL. Every step runs even if one fails. The summary is printed at the
end and saved to target/package-check-<os>.txt; the exit code is 1 if any
step failed.
"""

from __future__ import annotations

import argparse
import os
import platform
import plistlib
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HOST = {"win32": "windows", "darwin": "macos"}.get(sys.platform, "linux")
OSES = ("windows", "macos", "linux")
EXAMPLE = ROOT / "examples" / "hello-part"
VERSION = "0.0.0-local.1"


def exe(os_name: str, stem: str) -> str:
    return stem + ".exe" if os_name == "windows" else stem


# ---------------------------------------------------------------- results


class Check:
    def __init__(self) -> None:
        self.results: list[str] = []
        self.failed = False

    def step(self, name: str, run) -> object:
        print(f"\n==> {name}", flush=True)
        try:
            outcome = run()
        except Skip as skip:
            print(f"skipped: {skip}")
            self.results.append(f"SKIP  {name} ({skip})")
            return None
        except Exception as err:  # noqa: BLE001 - every failure is reported, then the next step runs
            print(f"FAILED: {err}")
            self.results.append(f"FAIL  {name}")
            self.failed = True
            return None
        self.results.append(f"PASS  {name}")
        return outcome if outcome is not None else True


class Skip(Exception):
    pass


def run(command: list, **kwargs) -> subprocess.CompletedProcess:
    print("$ " + " ".join(str(part) for part in command), flush=True)
    done = subprocess.run([str(part) for part in command], **kwargs)
    if done.returncode != 0:
        raise RuntimeError(f"`{Path(str(command[0])).name}` exited with {done.returncode}")
    return done


def need(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


# --------------------------------------------------------- binary formats


def binary_kind(path: Path) -> str:
    """`elf-x86_64`, `pe-x86_64`, `macho-<archs>`, or what else it is."""
    with open(path, "rb") as file:
        head = file.read(4096)
    if head[:4] == b"\x7fELF":
        machine = struct.unpack_from("<H", head, 18)[0]
        return {0x3E: "elf-x86_64", 0xB7: "elf-aarch64"}.get(machine, f"elf-{machine:#x}")
    if head[:2] == b"MZ":
        offset = struct.unpack_from("<I", head, 0x3C)[0]
        if head[offset : offset + 4] == b"PE\0\0":
            machine = struct.unpack_from("<H", head, offset + 4)[0]
            return {0x8664: "pe-x86_64", 0xAA64: "pe-arm64"}.get(machine, f"pe-{machine:#x}")
    archs = {0x01000007: "x86_64", 0x0100000C: "arm64"}
    if head[:4] == b"\xca\xfe\xba\xbe":  # universal (fat) Mach-O, big-endian
        count = struct.unpack_from(">I", head, 4)[0]
        cpus = [struct.unpack_from(">I", head, 8 + 20 * i)[0] for i in range(count)]
        return "macho-" + "+".join(archs.get(cpu, hex(cpu)) for cpu in cpus)
    if head[:4] == b"\xcf\xfa\xed\xfe":  # 64-bit Mach-O, little-endian
        cpu = struct.unpack_from("<I", head, 4)[0]
        return "macho-" + archs.get(cpu, hex(cpu))
    return "unknown"


KINDS = {"windows": "pe-", "macos": "macho-", "linux": "elf-"}


def check_binary(path: Path, os_name: str) -> str:
    need(path.is_file(), f"`{path}` is missing")
    kind = binary_kind(path)
    need(kind.startswith(KINDS[os_name]), f"`{path}` is {kind}, not a {os_name} program")
    if os_name != "windows" and HOST != "windows":
        need(os.access(path, os.X_OK), f"`{path}` isn't marked executable")
    print(f"  {path}: {kind}")
    return kind


# ------------------------------------------------------- payload layout


def check_payload(folder: Path, host: str, players: dict) -> None:
    """The files every package holds (`packaging/stage.py`)."""
    check_binary(folder / exe(host, "axcenity-editor"), host)
    check_binary(folder / exe(host, "axcenity-player"), host)
    for os_name in OSES:
        player = folder / "players" / os_name / exe(os_name, "axcenity-player")
        if os_name in players:
            check_binary(player, os_name)
        else:
            need(not player.exists(), f"`{player}` is there but wasn't staged")
            print(f"  players/{os_name}: not in this local package (no {os_name} player given)")
    for name in ("LICENSE", "NOTICE", "INSTALL.txt"):
        need((folder / name).is_file(), f"`{name}` is missing from `{folder}`")


# ------------------------------------------------------- Build Game smoke


def copy_example(into: Path) -> Path:
    project = into / "project"
    shutil.copytree(EXAMPLE, project, ignore=shutil.ignore_patterns(".axcenity"))
    return project


def build_game(editor: Path, os_name: str, work: Path) -> Path:
    """Runs the packaged editor's Build Game for `os_name`; checks the files."""
    out = work / f"game-{os_name}"
    shutil.rmtree(out, ignore_errors=True)
    project = work / "project"
    if not project.exists():
        copy_example(work)
    run([editor, "--build-game", project, out, "--target", os_name])
    if os_name == "windows":
        check_binary(out / "AxcenityPlayer.exe", "windows")
        files = ["game.axgame", "LICENSES.txt"]
    elif os_name == "linux":
        check_binary(out / "AxcenityPlayer", "linux")
        files = ["game.axgame", "LICENSES.txt", "AxcenityPlayer.desktop"]
        check_desktop_entry(out / "AxcenityPlayer.desktop")
    else:
        contents = out / "AxcenityPlayer.app" / "Contents"
        check_binary(contents / "MacOS" / "AxcenityPlayer", "macos")
        files = [
            "AxcenityPlayer.app/Contents/MacOS/game.axgame",
            "AxcenityPlayer.app/Contents/Resources/LICENSES.txt",
        ]
        with open(contents / "Info.plist", "rb") as file:
            info = plistlib.load(file)
        need(info.get("CFBundleExecutable") == "AxcenityPlayer", "Info.plist names another program")
        need(info.get("CFBundlePackageType") == "APPL", "Info.plist isn't an application's")
        if HOST == "macos":
            run(["plutil", "-lint", contents / "Info.plist"])
    for name in files:
        need((out / name).is_file(), f"the build has no `{name}`")
    if HOST == "windows" and os_name != "windows":
        check_zip(out / f"AxcenityPlayer-{os_name}.zip", os_name)
    return out


def check_desktop_entry(path: Path) -> None:
    text = path.read_text()
    need("Exec=sh -c " in text and "%k" in text, "the launcher doesn't find the player beside it")
    if shutil.which("desktop-file-validate"):
        run(["desktop-file-validate", path])


def check_zip(path: Path, os_name: str) -> None:
    """Built on Windows, the zip must mark the player as a program."""
    need(path.is_file(), f"`{path.name}` is missing")
    player = {
        "linux": "AxcenityPlayer",
        "macos": "AxcenityPlayer.app/Contents/MacOS/AxcenityPlayer",
    }[os_name]
    with zipfile.ZipFile(path) as archive:
        mode = archive.getinfo(player).external_attr >> 16
    need(mode & 0o111 == 0o111, f"`{player}` in `{path.name}` isn't marked executable")


def run_game(out: Path, host: str) -> None:
    """Starts the built game for this OS and has it draw 30 frames."""
    if host == "macos":
        player = out / "AxcenityPlayer.app" / "Contents" / "MacOS" / "AxcenityPlayer"
    else:
        player = out / exe(host, "AxcenityPlayer")
    command = [player, "--bench", "30"]
    no_display = not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY"))
    if host == "linux" and no_display:
        if not shutil.which("xvfb-run"):
            raise Skip("no display, and xvfb-run isn't installed")
        command = ["xvfb-run", "-a"] + command
    # From another folder: a built game finds its game.axgame beside itself.
    done = run(command, cwd=tempfile.gettempdir(), capture_output=True, text=True, timeout=900)
    print(done.stdout[-2000:])
    need("first frame" in done.stdout, "the game didn't report its first frame")


# ------------------------------------------------------------ packaging


def make_linux_packages(stage: Path, out: Path, work: Path) -> Path:
    """Builds what the tools here allow; returns the editor to smoke-test."""
    formats = ["tar"]
    if shutil.which("nfpm"):
        formats += ["deb", "rpm", "archlinux"]
    else:
        print("nfpm isn't installed: no .deb, .rpm or .pkg.tar.zst (https://nfpm.goreleaser.com)")
    if shutil.which("appimagetool"):
        formats.append("appimage")
    else:
        print("appimagetool isn't installed: no AppImage")
    run(["bash", ROOT / "packaging/linux/build-packages.sh", VERSION, ROOT, stage, out, " ".join(formats)],
        env={**os.environ, "APPIMAGE_EXTRACT_AND_RUN": "1"})
    stem = f"AxcenityEditor-{VERSION}-linux-x86_64"
    portable = work / "tar"
    with tarfile.open(out / f"{stem}.tar.gz") as archive:
        try:
            archive.extractall(portable, filter="tar")
        except TypeError:  # Python before 3.12 has no filters
            archive.extractall(portable)
    folder = portable / stem
    check_payload(folder, "linux", players_in(stage))
    if (out / f"{stem}.deb").exists() and shutil.which("dpkg-deb"):
        deb = work / "deb"
        run(["dpkg-deb", "-x", out / f"{stem}.deb", deb])
        check_payload(deb / "opt/axcenity", "linux", players_in(stage))
        for link in ("usr/bin/axcenity-editor", "usr/bin/axcenity-player"):
            need(os.readlink(deb / link).startswith("/opt/axcenity/"), f"`/{link}` points elsewhere")
        need((deb / "usr/share/applications/axcenity-editor.desktop").is_file(), "no menu entry")
        if shutil.which("desktop-file-validate"):
            run(["desktop-file-validate", deb / "usr/share/applications/axcenity-editor.desktop"])
    if (out / f"{stem}.rpm").exists() and shutil.which("rpm"):
        listing = run(["rpm", "-qlp", out / f"{stem}.rpm"], capture_output=True, text=True).stdout
        need("/opt/axcenity/axcenity-editor" in listing, "the .rpm has no editor")
    if (out / f"{stem}.AppImage").exists():
        run([out / f"{stem}.AppImage", "--appimage-extract"], cwd=work, capture_output=True)
        check_payload(work / "squashfs-root/usr/bin", "linux", players_in(stage))
    return folder / "axcenity-editor"


def find_makensis() -> str | None:
    found = shutil.which("makensis")
    if found:
        return found
    for base in (os.environ.get("ProgramFiles(x86)"), os.environ.get("ProgramFiles")):
        if base and (Path(base) / "NSIS" / "makensis.exe").is_file():
            return str(Path(base) / "NSIS" / "makensis.exe")
    return None


def make_windows_installer(stage: Path, out: Path) -> Path:
    makensis = find_makensis()
    if not makensis:
        raise Skip("NSIS isn't installed (https://nsis.sourceforge.io, or `choco install nsis`)")
    installer = out / f"AxcenityEditor-{VERSION}-windows-x86_64.exe"
    # NSIS takes /D on Windows and -D elsewhere.
    flag = "/" if HOST == "windows" else "-"
    defines = {
        "VERSION": VERSION,
        "STAGE": stage,
        "OUTFILE": installer,
        "ICON": ROOT / "assets/branding/icon.ico",
        "HEADER": ROOT / "packaging/assets/nsis-header.bmp",
        "SIDEBAR": ROOT / "packaging/assets/nsis-sidebar.bmp",
    }
    run([makensis] + [f"{flag}D{key}={value}" for key, value in defines.items()]
        + [ROOT / "packaging/windows/installer.nsi"])
    check_binary(installer, "windows")
    return installer


def install_windows(installer: Path, work: Path) -> Path:
    """Installs silently into a scratch folder; returns the folder."""
    import winreg  # Windows only

    try:
        winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Axcenity\Editor").Close()
        raise Skip("Axcenity Editor is installed on this account; uninstalling the test copy "
                   "would remove its registration")
    except FileNotFoundError:
        pass
    folder = work / "installed"
    # /D must come last, unquoted.
    run([installer, "/S", f"/D={folder}"])
    return folder


def uninstall_windows(folder: Path) -> None:
    run([folder / "Uninstall.exe", "/S", f"_?={folder}"])
    shutil.rmtree(folder, ignore_errors=True)


def make_macos_dmg(stage: Path, out: Path, work: Path) -> tuple[Path, Path]:
    """Builds and mounts the disk image; returns (mount point, app)."""
    dmg = out / f"AxcenityEditor-{VERSION}-macos.dmg"
    # build-dmg.sh installs dmgbuild in a throwaway venv, which needs the network.
    run(["bash", ROOT / "packaging/macos/build-dmg.sh", VERSION, ROOT, stage, dmg])
    mount = work / "dmg"
    mount.mkdir()
    run(["hdiutil", "attach", "-nobrowse", "-readonly", "-mountpoint", mount, dmg])
    app = mount / "Axcenity Editor.app"
    need(app.is_dir(), "the disk image has no Axcenity Editor.app")
    need((mount / "Applications").is_symlink(), "the disk image has no Applications shortcut")
    with open(app / "Contents/Info.plist", "rb") as file:
        info = plistlib.load(file)
    need(info["CFBundleExecutable"] == "axcenity-editor", "Info.plist names another program")
    need((app / "Contents/Resources/AppIcon.icns").is_file(), "the app has no icon")
    run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", app])
    check_payload(app / "Contents/MacOS", "macos", players_in(stage))
    return mount, app


# ------------------------------------------------------------------ main


def players_in(stage: Path) -> dict:
    return {
        os_name: True
        for os_name in OSES
        if (stage / "players" / os_name / exe(os_name, "axcenity-player")).is_file()
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--profile", default="dist", help="cargo profile (dist, as released, or release)")
    parser.add_argument("--no-build", action="store_true", help="use the binaries already in target/<profile>")
    parser.add_argument("--players", type=Path, help="folder with <os>/axcenity-player[.exe] from other computers")
    parser.add_argument("--install", action="store_true",
                        help="Windows: also install silently into a scratch folder, check it, uninstall")
    parser.add_argument("--keep", action="store_true", help="keep target/package-check/work afterwards")
    args = parser.parse_args()

    os.chdir(ROOT)
    check = Check()
    base = ROOT / "target" / "package-check"
    stage, out, work = base / "stage", base / "out", base / "work"
    for folder in (out, work):
        shutil.rmtree(folder, ignore_errors=True)
        folder.mkdir(parents=True)
    bins = ROOT / "target" / args.profile

    if not args.no_build:
        check.step(f"Build the editor and player ({args.profile})", lambda: run(
            ["cargo", "build", "--profile", args.profile, "--locked", "-p", "axcenity-editor", "-p", "axcenity-player"]))

    def stage_payload() -> None:
        player = bins / exe(HOST, "axcenity-player")
        command = [sys.executable, ROOT / "packaging/stage.py", "--os", HOST,
                   "--editor", bins / exe(HOST, "axcenity-editor"), "--player-native", player,
                   f"--player-{HOST}", player, "--out", stage, "--allow-missing-players"]
        for os_name in OSES:
            other = args.players / os_name / exe(os_name, "axcenity-player") if args.players else None
            if os_name != HOST and other and other.is_file():
                command += [f"--player-{os_name}", other]
        run(command)
        check_payload(stage, HOST, players_in(stage))

    staged = check.step("Stage the payload and check its files", stage_payload)

    editor = None
    cleanup = None
    if staged and HOST == "linux":
        editor = check.step("Linux packages (tar.gz always; .deb .rpm .pkg.tar.zst AppImage with their tools)",
                            lambda: make_linux_packages(stage, out, work))
    elif staged and HOST == "windows":
        installer = check.step("Windows installer (NSIS)", lambda: make_windows_installer(stage, out))
        if installer and args.install:
            folder = check.step("Install silently into a scratch folder", lambda: install_windows(installer, work))
            if folder:
                check.step("Installed files", lambda: check_payload(folder, "windows", players_in(stage)))
                editor = folder / "axcenity-editor.exe"
                cleanup = lambda: check.step("Uninstall silently", lambda: uninstall_windows(folder))  # noqa: E731
    elif staged and HOST == "macos":
        mounted = check.step("macOS disk image (needs the network for dmgbuild)",
                             lambda: make_macos_dmg(stage, out, work))
        if mounted:
            mount, app = mounted
            editor = app / "Contents/MacOS/axcenity-editor"
            cleanup = lambda: check.step("Detach the disk image", lambda: run(["hdiutil", "detach", mount]))  # noqa: E731
    if staged and editor is None:
        # The installer copies the staged files as they are.
        editor = stage / exe(HOST, "axcenity-editor")
        print(f"\n(checking Build Game with the staged editor, `{editor}`)")

    if staged and editor:
        for os_name in OSES:
            if os_name not in players_in(stage):
                check.results.append(f"SKIP  Build Game for {os_name} (no {os_name} player in this package)")
                continue
            built = check.step(f"Build Game for {os_name} with the packaged editor",
                               lambda os_name=os_name: build_game(editor, os_name, work))
            if built and os_name == HOST:
                check.step(f"The built {os_name} game starts and draws frames", lambda: run_game(built, HOST))
    if cleanup:
        cleanup()

    if not args.keep:
        shutil.rmtree(work, ignore_errors=True)
    commit = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    lines = [f"Package check on {platform.system()} {platform.machine()}, commit {commit}, "
             f"profile {args.profile}; packages in {out}"] + check.results
    summary = ROOT / "target" / f"package-check-{HOST}.txt"
    summary.write_text("\n".join(lines) + "\n")
    print("\n" + "\n".join(lines))
    print(f"Saved to {summary}")
    return 1 if check.failed else 0


if __name__ == "__main__":
    sys.exit(main())
