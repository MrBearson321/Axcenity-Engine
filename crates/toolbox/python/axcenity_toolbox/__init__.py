"""The Axcenity Toolbox for Python and Blender.

A thin layer over the engine's Toolbox: anything that touches the network
or writes (search, info, fetch, migrate, credits) runs the
``axcenity-toolbox`` command line, and fast local questions (records,
downloads, maps, licences) read the central asset directory's
``metadata.db`` directly with ``sqlite3``. Plain Python, standard library
only, so it runs inside Blender's bundled Python without compiling
anything.

The command is found from ``AXCENITY_TOOLBOX`` (a path to the binary) or
``PATH``; the asset directory from ``AXCENITY_ASSET_DIR`` or the command's
``where``.
"""

from __future__ import annotations

import json
import os
import shutil
import sqlite3
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

__all__ = [
    "SCHEMA_VERSION",
    "Download",
    "Library",
    "SourceUnavailable",
    "Toolbox",
    "ToolboxError",
    "find_binary",
]

#: The newest metadata.db schema this package reads.
SCHEMA_VERSION = 1

#: Exit codes of the command line.
EXIT_USAGE = 1
EXIT_SOURCE = 2
EXIT_UNAVAILABLE = 3


class ToolboxError(RuntimeError):
    """A Toolbox command failed; the message is safe to show."""


class SourceUnavailable(ToolboxError):
    """A source can't be reached (offline, down, or every source failed).

    The Blender pipeline treats this like its ``ProviderUnavailable``: try no
    more of that source's assets this run.
    """


def find_binary() -> Optional[str]:
    """The ``axcenity-toolbox`` command, or ``None`` when it isn't found."""
    named = os.environ.get("AXCENITY_TOOLBOX", "").strip()
    if named:
        return named if Path(named).is_file() else None
    return shutil.which("axcenity-toolbox")


@dataclass
class Download:
    """One variant of one asset in the asset directory."""

    key: str
    variant: str
    folder: str
    path: Path
    files: list = field(default_factory=list)
    bytes: int = 0
    source_urls: list = field(default_factory=list)
    fetched: int = 0
    notes: list = field(default_factory=list)

    @classmethod
    def from_json(cls, data: dict, root: Optional[Path] = None) -> "Download":
        folder = data.get("folder", "")
        path = data.get("path")
        return cls(
            key=data.get("key", ""),
            variant=data.get("variant", ""),
            folder=folder,
            path=Path(path) if path else (root or Path()).joinpath(*folder.split("/")),
            files=list(data.get("files", [])),
            bytes=int(data.get("bytes", 0)),
            source_urls=list(data.get("source_urls", [])),
            fetched=int(data.get("fetched", 0)),
            notes=list(data.get("notes", [])),
        )

    def file_paths(self) -> list:
        """Every file's full path."""
        return [self.path.joinpath(*f["path"].split("/")) for f in self.files]


class Toolbox:
    """Runs the ``axcenity-toolbox`` command line."""

    def __init__(self, binary: Optional[str] = None, asset_dir: Optional[str] = None,
                 offline: bool = False, timeout: float = 30 * 60):
        self.binary = binary or find_binary()
        if not self.binary:
            raise ToolboxError(
                "axcenity-toolbox wasn't found; set AXCENITY_TOOLBOX to its path"
            )
        self.asset_dir = asset_dir or os.environ.get("AXCENITY_ASSET_DIR") or None
        self.offline = offline
        self.timeout = timeout

    def _run(self, *args: str, as_json: bool = True) -> Any:
        command = [self.binary, *args]
        if self.asset_dir:
            command += ["--dir", str(self.asset_dir)]
        if self.offline:
            command.append("--offline")
        if as_json:
            command.append("--json")
        done = subprocess.run(command, capture_output=True, text=True,
                              encoding="utf-8", timeout=self.timeout)
        if done.returncode == EXIT_UNAVAILABLE:
            raise SourceUnavailable(done.stderr.strip() or "a source is unavailable")
        if done.returncode != 0:
            raise ToolboxError(done.stderr.strip().splitlines()[0] if done.stderr.strip()
                               else f"axcenity-toolbox exited with {done.returncode}")
        return json.loads(done.stdout) if as_json else done.stdout

    def where(self) -> Path:
        """The central asset directory."""
        if self.asset_dir:
            return Path(self.asset_dir)
        return Path(self._run("where", as_json=False).strip())

    def search(self, words: str, kind: str = "texture", provider: Optional[str] = None,
               limit: int = 15) -> list:
        """Records (``asset-source`` dicts) matching ``words``."""
        args = ["search", words, "--kind", kind, "--limit", str(limit)]
        if provider:
            args += ["--provider", provider]
        return self._run(*args)

    def info(self, key: str) -> dict:
        """The full record for ``provider:id``, with its variants."""
        return self._run("info", key)

    def fetch(self, key: str, res: str = "2k", fmt: Optional[str] = None) -> Download:
        """Makes sure a variant is in the asset directory, downloading only
        what's missing, and returns it."""
        args = ["fetch", key, "--res", res]
        if fmt:
            args += ["--format", fmt]
        return Download.from_json(self._run(*args))

    def migrate(self, old_library: str, dry_run: bool = False) -> dict:
        """Copies a Blender ``asset_library`` into the directory where its
        files verify; returns the report (copied, present, skipped)."""
        args = ["migrate", str(old_library)]
        if dry_run:
            args.append("--dry-run")
        return self._run(*args)

    def credits(self, keys: Optional[list] = None, project: Optional[str] = None) -> str:
        """The credits text for some assets, a project, or the library."""
        args = ["credits"]
        if keys:
            args += ["--keys", ",".join(keys)]
        if project:
            args += ["--project", str(project)]
        return self._run(*args, as_json=False)

    def stats(self) -> dict:
        return self._run("stats")


class Library:
    """Reads the asset directory's ``metadata.db`` directly.

    Read-only: writes go through the command line so the engine's rules
    (verified downloads, schema migrations) hold.
    """

    def __init__(self, root: Optional[str] = None):
        if root is None:
            root = os.environ.get("AXCENITY_ASSET_DIR") or str(Toolbox().where())
        self.root = Path(root)
        self.db_path = self.root / "metadata.db"
        if not self.db_path.is_file():
            raise ToolboxError(f"there is no metadata.db in {self.root}")
        uri = self.db_path.resolve().as_uri() + "?mode=ro"
        self.conn = sqlite3.connect(uri, uri=True, timeout=10)
        version = self.conn.execute("SELECT MAX(version) FROM schema_version").fetchone()[0]
        if version is None or version > SCHEMA_VERSION:
            self.conn.close()
            raise ToolboxError(
                "the asset database is from a newer Axcenity; update axcenity_toolbox"
            )

    def close(self) -> None:
        self.conn.close()

    def __enter__(self) -> "Library":
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()

    def record(self, key: str) -> Optional[dict]:
        """The stored ``asset-source`` record for ``provider:id``."""
        row = self.conn.execute("SELECT record FROM assets WHERE key = ?", (key,)).fetchone()
        return json.loads(row[0]) if row else None

    def records(self, kind: Optional[str] = None, downloaded: bool = False) -> list:
        """Stored records, by name."""
        sql = "SELECT record FROM assets WHERE (? IS NULL OR kind = ?)"
        if downloaded:
            sql += " AND EXISTS (SELECT 1 FROM downloads d WHERE d.key = assets.key)"
        sql += " ORDER BY name COLLATE NOCASE, key"
        return [json.loads(r[0]) for r in self.conn.execute(sql, (kind, kind))]

    def downloads(self, key: str) -> list:
        """Every download of an asset, newest first."""
        rows = self.conn.execute(
            "SELECT key, variant, folder, files, bytes, source_urls, fetched FROM downloads"
            " WHERE key = ? ORDER BY fetched DESC, variant", (key,))
        return [
            Download.from_json({
                "key": r[0], "variant": r[1], "folder": r[2], "files": json.loads(r[3]),
                "bytes": r[4], "source_urls": json.loads(r[5]), "fetched": r[6],
            }, self.root)
            for r in rows
        ]

    def licence(self, key: str) -> Optional[dict]:
        """The licence code, version, address and authors of an asset."""
        row = self.conn.execute(
            "SELECT license, license_version, license_url, authors, attribution FROM assets"
            " WHERE key = ?", (key,)).fetchone()
        if not row:
            return None
        return {"license": row[0], "license_version": row[1], "license_url": row[2],
                "authors": json.loads(row[3]), "attribution": row[4]}

    def maps(self, key: str, variant: Optional[str] = None) -> dict:
        """A texture download's map files by role (``base_color``,
        ``normal``…), from the record's map names or the files' names."""
        downloads = self.downloads(key)
        if variant is not None:
            downloads = [d for d in downloads if d.variant == variant]
        if not downloads:
            return {}
        return map_roles(downloads[0])


#: Words in a map file's name, and the role they mean. Ordered so the
#: more specific word wins (``nor_dx`` before ``nor``).
_MAP_WORDS = [
    ("normaldx", "normal_dx"), ("nor_dx", "normal_dx"), ("normalgl", "normal"),
    ("nor_gl", "normal"), ("normal", "normal"), ("basecolor", "base_color"),
    ("base_color", "base_color"), ("albedo", "base_color"), ("diffuse", "base_color"),
    ("diff", "base_color"), ("color", "base_color"), ("col", "base_color"),
    ("roughness", "roughness"), ("rough", "roughness"), ("metalness", "metallic"),
    ("metallic", "metallic"), ("metal", "metallic"), ("ambientocclusion", "ao"),
    ("ao", "ao"), ("displacement", "height"), ("height", "height"), ("disp", "height"),
    ("opacity", "opacity"), ("emission", "emission"), ("arm", "arm"),
]


def map_roles(download: Download) -> dict:
    """Each map role in a download and its file's full path."""
    roles: dict = {}
    for f in download.files:
        name = Path(f["path"]).stem.lower()
        tokens = name.replace("-", "_").split("_")
        joined = "".join(tokens)
        for word, role in _MAP_WORDS:
            hit = word in tokens or (("_" in word) and word in name) or (
                len(word) > 4 and joined.endswith(word))
            if hit:
                roles.setdefault(role, download.path.joinpath(*f["path"].split("/")))
                break
    return roles
