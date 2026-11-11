"""Assembles the files that every installer and archive ships.

The editor looks for the current-OS player beside itself, and other OS players
under `players/<os>/` beside itself. This layout matches that, on every package.

`install.json` says how the copy was installed, so the editor knows whether it
can update itself (`crates/release/src/install.rs`). The stage gets its OS's
main kind; `packaging/linux/build-packages.sh` rewrites it for the AppImage and
the system packages.
"""

from __future__ import annotations

import argparse
import json
import shutil
import stat
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GUIDES = Path(__file__).resolve().parent / "guides"
BRAND = ROOT / "assets" / "branding"
# What each OS's stage is installed as, unless --install-kind says otherwise.
DEFAULT_KIND = {"windows": "windows-setup", "macos": "macos-dmg", "linux": "linux-tarball"}
INSTALL_KINDS = ("windows-setup", "linux-tarball", "appimage", "deb", "rpm", "pacman", "macos-dmg")


def write_install_marker(folder: Path, kind: str) -> None:
    """Writes `install.json` (read by `InstallKind::of_folder`)."""
    text = json.dumps({"schema": 1, "kind": kind}) + "\n"
    (folder / "install.json").write_text(text, encoding="utf-8", newline="\n")


def copy_file(src: Path, dst: Path, executable: bool = False) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    if executable and dst.exists():
        mode = dst.stat().st_mode
        dst.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def stage(args: argparse.Namespace) -> None:
    out = Path(args.out)
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    editor_name = "axcenity-editor.exe" if args.os == "windows" else "axcenity-editor"
    player_name = "axcenity-player.exe" if args.os == "windows" else "axcenity-player"
    copy_file(Path(args.editor), out / editor_name, executable=args.os != "windows")
    copy_file(Path(args.player_native), out / player_name, executable=args.os != "windows")
    if args.updater:
        # Installs in-app updates (`crates/updater`); the editor runs the
        # new release's copy, so every update archive carries one.
        updater_name = "axcenity-updater.exe" if args.os == "windows" else "axcenity-updater"
        copy_file(Path(args.updater), out / updater_name, executable=args.os != "windows")

    players = (
        ("windows", args.player_windows, "axcenity-player.exe"),
        ("macos", args.player_macos, "axcenity-player"),
        ("linux", args.player_linux, "axcenity-player"),
    )
    for os_name, source, name in players:
        if source is None:
            # Only a local check leaves one out (`--allow-missing-players`).
            print(f"no {os_name} player: Build Game > For {os_name} won't work from this package")
            continue
        copy_file(Path(source), out / "players" / os_name / name, executable=os_name != "windows")

    shutil.copy2(ROOT / "LICENSE", out / "LICENSE")
    shutil.copy2(ROOT / "NOTICE", out / "NOTICE")
    guide = GUIDES / f"{args.os}.txt"
    shutil.copy2(guide, out / "INSTALL.txt")
    write_install_marker(out, args.install_kind or DEFAULT_KIND[args.os])

    print(f"staged {args.os} payload in {out}")


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--os", required=True, choices=("windows", "macos", "linux"))
    p.add_argument("--editor", required=True, help="path to the host editor binary")
    p.add_argument("--player-native", required=True, help="path to the host player binary")
    p.add_argument(
        "--updater",
        help="path to the host axcenity-updater (required on Windows and Linux, which update in-app)",
    )
    p.add_argument("--player-windows")
    p.add_argument("--player-macos")
    p.add_argument("--player-linux")
    p.add_argument("--out", required=True, help="directory to write the payload into")
    p.add_argument(
        "--install-kind",
        choices=INSTALL_KINDS,
        help="what install.json records (default: the OS's main package)",
    )
    p.add_argument(
        "--allow-missing-players",
        action="store_true",
        help="leave out other-OS players that weren't given (local checks; releases ship all three)",
    )
    return p


if __name__ == "__main__":
    parser = build_parser()
    arguments = parser.parse_args()
    if arguments.os in ("windows", "linux") and not arguments.updater:
        parser.error("--updater is required on Windows and Linux: their installs update in-app")
    if not arguments.allow_missing_players:
        for flag in ("player_windows", "player_macos", "player_linux"):
            if getattr(arguments, flag) is None:
                parser.error(f"--{flag.replace('_', '-')} is required (or --allow-missing-players)")
    stage(arguments)
