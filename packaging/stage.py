"""Assembles the files that every installer and archive ships.

The editor looks for the current-OS player beside itself, and other OS players
under `players/<os>/` beside itself. This layout matches that, on every package.
"""

from __future__ import annotations

import argparse
import shutil
import stat
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GUIDES = Path(__file__).resolve().parent / "guides"
BRAND = ROOT / "assets" / "branding"


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

    copy_file(
        Path(args.player_windows),
        out / "players" / "windows" / "axcenity-player.exe",
    )
    copy_file(
        Path(args.player_macos),
        out / "players" / "macos" / "axcenity-player",
        executable=True,
    )
    copy_file(
        Path(args.player_linux),
        out / "players" / "linux" / "axcenity-player",
        executable=True,
    )

    shutil.copy2(ROOT / "LICENSE", out / "LICENSE")
    shutil.copy2(ROOT / "NOTICE", out / "NOTICE")
    guide = GUIDES / f"{args.os}.txt"
    shutil.copy2(guide, out / "INSTALL.txt")

    print(f"staged {args.os} payload in {out}")


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--os", required=True, choices=("windows", "macos", "linux"))
    p.add_argument("--editor", required=True, help="path to the host editor binary")
    p.add_argument("--player-native", required=True, help="path to the host player binary")
    p.add_argument("--player-windows", required=True)
    p.add_argument("--player-macos", required=True)
    p.add_argument("--player-linux", required=True)
    p.add_argument("--out", required=True, help="directory to write the payload into")
    return p


if __name__ == "__main__":
    stage(build_parser().parse_args())
