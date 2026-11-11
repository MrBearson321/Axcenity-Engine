"""Models in folders the user points at (``library.local_directories``).

Indexes ``.glb``, ``.gltf``, ``.fbx`` and ``.obj`` (``.blend`` files are never
read). The licence comes from a sidecar, the first found of
``<model>.license.json``, ``<stem>.license.json`` and a ``license.json`` in the
model's folder or any folder above it inside the listed directory::

    {"license": "CC0", "authors": ["..."], "url": "https://...", "name": "...",
     "tags": ["..."], "description": "...", "dimensions_m": [1, 1, 2]}

Without one the licence is ``unknown``, which never passes on its own. Files
stay where they are: "downloading" records their paths in the registry.
"""

import hashlib
import json
import os
import re
from pathlib import Path

from ..utils import PipelineError, log
from . import download_guard as guard
from . import license_manager
from .base import MODEL, AssetRecord

NAME = "local"
VERSION = "local"
EXTENSIONS = guard.MODEL_EXTENSIONS


class LocalFiles:
    name = NAME
    kinds = (MODEL,)
    formats = ("glb", "gltf", "fbx", "obj")
    licenses = None
    opt_in = True
    wants_config = True

    def __init__(self, library, offline=False, ttl=86400, cfg=None):
        self.library = library
        self.cfg = cfg

    def directories(self):
        """The listed folders that exist. A relative one is under the nearest config file's folder."""
        if self.cfg is None:
            return []
        out = []
        base = Path(self.cfg.sources[-1]).parent if self.cfg.sources else Path.cwd()
        for entry in self.cfg.get("library.local_directories") or []:
            path = Path(os.path.expandvars(os.path.expanduser(str(entry))))
            path = (path if path.is_absolute() else base / path).resolve()
            if path.is_dir():
                out.append(path)
            else:
                log.warning("library.local_directories: %s isn't a folder", path)
        return out

    def _walk(self):
        for root in self.directories():
            for folder, subdirs, files in os.walk(root):
                subdirs[:] = sorted(d for d in subdirs if not d.startswith("."))
                for name in sorted(files):
                    if Path(name).suffix.lower() in EXTENSIONS:
                        yield root, Path(folder) / name

    @staticmethod
    def _asset_id(root, path):
        relative = path.relative_to(root).as_posix()
        digest = hashlib.sha1(f"{root.name}/{relative}".encode()).hexdigest()[:10]
        return f"{re.sub(r'[^A-Za-z0-9]+', '_', path.stem).strip('_')}-{digest}"

    @staticmethod
    def _sidecar(root, path):
        names = [path.parent / f"{path.name}.license.json", path.parent / f"{path.stem}.license.json"]
        folder = path.parent
        while True:
            names.append(folder / "license.json")
            if folder == root:
                break
            folder = folder.parent
        for sidecar in names:
            if not sidecar.is_file():
                continue
            try:
                data = json.loads(sidecar.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                log.warning("Ignoring unreadable licence sidecar %s", sidecar)
                return {}
            return data if isinstance(data, dict) else {}
        return {}

    def _record(self, root, path):
        meta = self._sidecar(root, path)
        words = [w for w in re.split(r"[^A-Za-z0-9]+", path.stem) if w]
        folders = [w for part in path.relative_to(root).parent.parts for w in re.split(r"[^A-Za-z0-9]+", part) if w]
        size = meta.get("dimensions_m")
        return AssetRecord(
            provider=NAME, id=self._asset_id(root, path), kind=MODEL,
            name=meta.get("name") or " ".join(words).lower() or path.stem, url=str(meta.get("url") or ""),
            description=str(meta.get("description") or ""),
            tags=[str(t) for t in meta.get("tags") or []] + [w.lower() for w in folders],
            authors=[str(a) for a in meta.get("authors") or []],
            license=license_manager.normalize(meta.get("license")),
            resolutions=[VERSION], physical_size=list(size) if isinstance(size, list) else None,
            method=str(meta.get("method") or ""),
            attributes={"format": path.suffix.lower().lstrip(".")},
            extra={"path": str(path)},
        )

    def search(self, query="", kind=MODEL, limit=30, categories=()):
        """Models sharing at least one word with ``query``, best match first."""
        if kind != MODEL:
            return []
        wanted = {w for w in re.findall(r"[a-z0-9]+", query.lower()) if w}
        hits = []
        for root, path in self._walk():
            rec = self._record(root, path)
            shared = len(wanted & rec.words())
            if wanted and not shared:
                continue
            hits.append((-shared, str(path), rec))
        hits.sort(key=lambda h: h[:2])
        return [h[2] for h in hits[:limit]]

    def info(self, asset_id, kind=None):
        for root, path in self._walk():
            if self._asset_id(root, path) == asset_id:
                return self._record(root, path)
        raise PipelineError(f"No local model {asset_id!r} in library.local_directories")

    def download(self, record, resolution=VERSION, fmt=None):
        """The model and the files it names, in place."""
        path = Path(record.extra.get("path") or self.info(record.id).extra["path"])
        if not path.is_file():
            raise PipelineError(f"Local model {path} is gone")
        files = [path] + [f for f in self._companions(path) if f.is_file()]
        return VERSION, path.parent, files, {}, []

    @staticmethod
    def _companions(path):
        try:
            if path.suffix.lower() == ".gltf":
                return [path.parent / uri for uri in guard.gltf_uris(path, skip_unsafe=True)]
            if path.suffix.lower() == ".obj":
                return LocalFiles._obj_companions(path)
        except (OSError, PipelineError) as exc:
            log.warning("Couldn't read what %s refers to: %s", path.name, exc)
        return []

    @staticmethod
    def _obj_companions(path):
        out = []
        with open(path, encoding="utf-8", errors="replace") as handle:
            libraries = [line.split(None, 1)[1].strip() for line in handle if line.startswith("mtllib ")]
        for name in libraries:
            if guard.unsafe_path(name):
                continue
            mtl = path.parent / name
            out.append(mtl)
            if mtl.is_file():
                for line in mtl.read_text(encoding="utf-8", errors="replace").splitlines():
                    if line.strip().lower().startswith(("map_", "bump ", "norm ")) and len(line.split()) > 1:
                        texture = line.split()[-1]
                        if not guard.unsafe_path(texture):
                            out.append(path.parent / texture)
        return out
