"""Packs a staged payload (`packaging/stage.py`) into the update archive the
editor's updater installs: `AxcenityEditor-<version>-windows-x86_64-update.zip`.

The files sit at the archive's root, exactly as the installer would put them
on disk. Entries are sorted and dated from SOURCE_DATE_EPOCH (or 1980), so the
same payload always gives the same bytes.

    python3 packaging/update-zip.py dist/stage dist/out/AxcenityEditor-0.2.0-alpha.1-windows-x86_64-update.zip
"""

from __future__ import annotations

import os
import stat
import sys
import time
import zipfile
from pathlib import Path


def entries(stage: Path) -> list[Path]:
    """Every file under `stage`, sorted. Links and other special files are
    refused: the updater won't extract them either."""
    files = []
    for path in sorted(stage.rglob("*")):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise SystemExit(f"{path} is a link or special file; payloads hold only plain files")
        if path.is_file():
            files.append(path)
    return files


def pack(stage: Path, out: Path) -> None:
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "315532800"))
    # Zip dates start in 1980 and have two-second steps.
    when = time.gmtime(max(epoch, 315532800))[:6]
    out.parent.mkdir(parents=True, exist_ok=True)
    files = entries(stage)
    if not (stage / "install.json").is_file():
        raise SystemExit(f"{stage} has no install.json; stage it with packaging/stage.py")
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in files:
            name = path.relative_to(stage).as_posix()
            info = zipfile.ZipInfo(name, date_time=when)
            mode = 0o755 if os.access(path, os.X_OK) or name.endswith(".exe") else 0o644
            info.external_attr = (stat.S_IFREG | mode) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            with path.open("rb") as source, archive.open(info, "w") as target:
                while chunk := source.read(1 << 20):
                    target.write(chunk)
    print(f"wrote {out} ({len(files)} files)")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    pack(Path(sys.argv[1]), Path(sys.argv[2]))
