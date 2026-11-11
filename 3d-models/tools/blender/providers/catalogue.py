"""Open repositories as data: JSON files listing direct links (``library.catalogues``).

Adding a repository means adding a file, not code::

    {"format": "asset-catalogue", "version": 1, "source": "Some Repository",
     "assets": [{"id": "oak_01", "name": "Oak", "kind": "model",
                 "url": "https://.../oak_01.zip", "source_page": "https://.../oak_01",
                 "license": "CC0", "authors": ["..."], "tags": ["tree"],
                 "polycount": 40000, "format": "glb", "dimensions_m": [5, 5, 9],
                 "resolution": "2k", "maps": ["base_color", "normal"],
                 "size": 1234, "md5": "..."}]}

``url`` is an https link to a ``.zip``, a model or an image. An entry with an
unsafe link is skipped; a missing licence reads as ``unknown``, which never
passes on its own.
"""

import json
import re
from pathlib import Path

from ..utils import PipelineError, log
from . import asset_downloader as dl
from . import download_guard as guard
from . import license_manager
from .base import HDRI, MODEL, TEXTURE, AssetRecord, map_from_filename

NAME = "catalog"
VERSION = "file"


class Catalogue:
    name = NAME
    kinds = (MODEL, TEXTURE, HDRI)
    formats = ("glb", "gltf", "fbx", "obj", "zip")
    licenses = None
    opt_in = True
    wants_config = True

    def __init__(self, library, offline=False, ttl=86400, cfg=None):
        self.library = library
        self.offline = offline
        self.cfg = cfg

    def _files(self):
        if self.cfg is None:
            return []
        base = Path(self.cfg.sources[-1]).parent if self.cfg.sources else Path.cwd()
        return [(p if p.is_absolute() else base / p) for p in map(Path, self.cfg.get("library.catalogues") or [])]

    def entries(self):
        """Every usable entry: ``{id: entry}``."""
        found = {}
        for path in self._files():
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, ValueError) as exc:
                raise PipelineError(f"Catalogue {path} is unreadable: {exc}") from exc
            for item in data.get("assets") or []:
                if not self._usable(item, path):
                    continue
                if item["id"] in found:
                    log.warning("Catalogue %s repeats asset %s; keeping the first", path.name, item["id"])
                    continue
                found[item["id"]] = {**item, "_source": data.get("source") or path.stem}
        return found

    @staticmethod
    def _usable(item, path):
        if not isinstance(item, dict) or not item.get("id") or not item.get("url"):
            log.warning("Catalogue %s: skipping an entry without id and url", path.name)
            return False
        try:
            guard.check_url(item["url"])
        except PipelineError as exc:
            log.warning("Catalogue %s: skipping %s: %s", path.name, item["id"], exc)
            return False
        return True

    @staticmethod
    def _record(item):
        size = item.get("dimensions_m")
        attributes = dict(item.get("attributes") or {})
        attributes["format"] = str(item.get("format") or "").lower()
        return AssetRecord(
            provider=NAME, id=str(item["id"]), kind=item.get("kind") or MODEL, name=item.get("name") or str(item["id"]),
            url=str(item.get("source_page") or ""), description=str(item.get("description") or ""),
            tags=[str(t) for t in item.get("tags") or []], categories=[str(c) for c in item.get("categories") or []],
            authors=[str(a) for a in item.get("authors") or []],
            license=license_manager.normalize(item.get("license")),
            resolutions=[str(item.get("resolution") or VERSION)], maps=list(item.get("maps") or []),
            physical_size=list(size) if isinstance(size, list) else None,
            attributes=attributes, method=str(item.get("method") or ""), polycount=int(item.get("polycount") or 0),
            thumbnail_url=str(item.get("thumbnail") or ""),
            extra={"download_url": item["url"], "size": item.get("size"), "md5": item.get("md5"),
                   "catalogue": item.get("_source")},
        )

    def search(self, query="", kind=MODEL, limit=30, categories=()):
        words = [w for w in re.findall(r"[a-z0-9]+", query.lower()) if w]
        cats = {c.lower() for c in categories}
        hits = []
        for item in self.entries().values():
            rec = self._record(item)
            if rec.kind != kind or (cats and not cats & {c.lower() for c in rec.categories}):
                continue
            if words and not all(any(v.startswith(w) for v in rec.words()) for w in words):
                continue
            hits.append(rec)
        return hits[:limit]

    def info(self, asset_id, kind=None):
        item = self.entries().get(asset_id)
        if item is None:
            raise PipelineError(f"No catalogue lists {asset_id!r}")
        return self._record(item)

    def download(self, record, resolution=VERSION, fmt=None):
        if self.offline:
            raise PipelineError("Offline: catalogue assets need a download")
        url = guard.check_url(record.extra.get("download_url") or self.info(record.id).extra["download_url"])
        res = record.resolutions[0] if record.resolutions else VERSION
        folder = self.library.asset_dir(NAME, record.kind, record.id, res)
        suffix = guard.url_suffix(url)
        if suffix in guard.ARCHIVE_EXTENSIONS:
            archive = self.library.cache_dir(NAME) / "archives" / f"{re.sub(r'[^A-Za-z0-9_-]+', '_', record.id)}.zip"
            dl.download(url, archive, record.extra.get("size"), record.extra.get("md5"), force=True)
            try:
                files = guard.extract(archive, folder)
            finally:
                archive.unlink(missing_ok=True)
        elif suffix in guard.MODEL_EXTENSIONS + guard.IMAGE_EXTENSIONS:
            dest = folder / re.sub(r"[^A-Za-z0-9_.-]+", "_", Path(url.split("?")[0]).name)
            dl.download(url, dest, record.extra.get("size"), record.extra.get("md5"))
            guard.check_model(dest)
            files = [dest]
        else:
            raise PipelineError(f"Catalogue asset {record.id}: can't take a {suffix or 'typeless'} download")
        if not files:
            raise PipelineError(f"Catalogue asset {record.id}: the download held no usable files")
        maps = {}
        for path in files:
            kind = map_from_filename(path.name) if path.suffix.lower() in guard.IMAGE_EXTENSIONS else None
            if kind:
                maps.setdefault(kind, path)
        return res, folder, files, maps, [url]
