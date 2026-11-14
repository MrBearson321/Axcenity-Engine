"""Downloads the luau-lsp that every package ships (`packaging/luau-lsp.toml`).

`stage.py` calls `fetch()` to put the server in the payload's `tools/` folder,
beside its MIT license. The download is checked against the SHA-256 and size
the manifest pins, and kept in a cache folder, so a second stage (or a local
`scripts/check-packages.py` run) doesn't fetch it again.

Run on its own to fill a folder:
    python packaging/fetch_luau_lsp.py --os linux --out some/folder
"""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import stat
import tomllib
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "luau-lsp.toml"
LICENSE = HERE / "luau-lsp-LICENSE.txt"
DEFAULT_CACHE = HERE.parent / "target" / "luau-lsp"


def manifest() -> dict:
    with open(MANIFEST, "rb") as file:
        return tomllib.load(file)


def program_name(os_name: str) -> str:
    return "luau-lsp.exe" if os_name == "windows" else "luau-lsp"


def host_arch() -> str:
    machine = platform.machine().lower()
    return "aarch64" if machine in ("arm64", "aarch64") else "x86_64"


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1 << 16), b""):
            digest.update(block)
    return digest.hexdigest()


def download(os_name: str, arch: str, cache: Path) -> Path:
    """The release zip for `os_name`/`arch`, from `cache` or downloaded."""
    pinned = manifest()
    key = f"{os_name}-{arch}"
    asset = pinned["assets"].get(key)
    if asset is None:
        raise SystemExit(f"packaging/luau-lsp.toml has no luau-lsp for {key}")
    archive = cache / pinned["version"] / asset["file"]
    if archive.is_file() and sha256_of(archive) == asset["sha256"]:
        return archive
    archive.parent.mkdir(parents=True, exist_ok=True)
    url = pinned["url"].format(version=pinned["version"], file=asset["file"])
    print(f"downloading {url}")
    part = archive.with_name(archive.name + ".part")
    request = urllib.request.Request(url, headers={"User-Agent": "axcenity-packaging"})
    with urllib.request.urlopen(request, timeout=60) as response, open(part, "wb") as out:
        while block := response.read(1 << 16):
            out.write(block)
    size = part.stat().st_size
    if size != asset["size"] or sha256_of(part) != asset["sha256"]:
        part.unlink()
        raise SystemExit(f"{url} doesn't match packaging/luau-lsp.toml (size or SHA-256)")
    os.replace(part, archive)
    return archive


def fetch(os_name: str, out: Path, arch: str = "x86_64", cache: Path = DEFAULT_CACHE) -> Path:
    """Puts luau-lsp for `os_name` and its license in `out`; returns the program."""
    # macOS ships one universal binary, listed under both architectures.
    archive = download(os_name, arch, cache)
    name = program_name(os_name)
    out.mkdir(parents=True, exist_ok=True)
    program = out / name
    with zipfile.ZipFile(archive) as zipped:
        program.write_bytes(zipped.read(name))
    program.chmod(program.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    (out / "luau-lsp-LICENSE.txt").write_bytes(LICENSE.read_bytes())
    return program


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--os", required=True, choices=("windows", "macos", "linux"))
    parser.add_argument("--arch", choices=("x86_64", "aarch64"), help="default: this machine's")
    parser.add_argument("--out", required=True)
    parser.add_argument("--cache", default=str(DEFAULT_CACHE))
    arguments = parser.parse_args()
    found = fetch(arguments.os, Path(arguments.out), arguments.arch or host_arch(), Path(arguments.cache))
    print(f"wrote {found}")
