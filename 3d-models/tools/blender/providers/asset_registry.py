"""The shared asset library on disk and its searchable registry.

Layout (``library.directory`` in the config, shared by every pack and
project that points at it)::

    asset_library/
        textures/<provider>/<id>/<resolution>/   PBR maps as downloaded
        models/<provider>/<id>/<resolution>/     glTF + textures
        hdri/<provider>/<id>/<resolution>/       .hdr / .exr
        sprites/<provider>/<id>/<version>/       2D image packs (Kenney), folders kept
        materials/imported/      material definitions straight from a source
        materials/customized/    tuned definitions (+ .blend for the Asset Browser)
        materials/procedural/    generated masks (grime, cracks, chips)
        materials/masks/         masks derived from sprite packs (mask_library)
        previews/<provider>/     thumbnails and comparison renders
        metadata/registry.json   one entry per asset (below)
        metadata/<provider>/<id>.json   the same entry, per asset
        cache/<provider>/        cached API responses and archives

A registry entry holds the normalised ``AssetRecord`` (``record``), every
downloaded resolution (``downloads``: files, maps, sizes, source URLs) and
analysis results (``analysis``: preview colour, mean colour per map...).
Downloaded files are never modified: adjustments happen in Blender nodes
and export textures are written elsewhere. Paths in the registry are
relative to the library root, so the library can move.
"""

import json
import os
import re
import shutil
from datetime import datetime, timezone
from pathlib import Path

from ..utils import PipelineError, log
from .base import HDRI, MODEL, SPRITES, TEXTURE, AssetRecord

REGISTRY_VERSION = 1
KIND_FOLDERS = {TEXTURE: "textures", MODEL: "models", HDRI: "hdri", SPRITES: "sprites"}
MATERIAL_KINDS = ("imported", "customized", "procedural", "masks")


def _now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def _atomic_write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(text, encoding="utf-8", newline="\n")
    os.replace(tmp, path)


class AssetLibrary:
    """A folder of downloaded assets plus the registry that describes them."""

    def __init__(self, root):
        self.root = Path(root).resolve()
        self.registry_path = self.root / "metadata" / "registry.json"
        self._data = self._load()

    # -- layout ----------------------------------------------------------

    def ensure_layout(self):
        for folder in ("textures", "models", "hdri", "sprites", "previews", "metadata", "cache",
                       *(f"materials/{k}" for k in MATERIAL_KINDS)):
            (self.root / folder).mkdir(parents=True, exist_ok=True)
        return self.root

    def asset_dir(self, provider, kind, asset_id, resolution=None):
        folder = self.root / KIND_FOLDERS[kind] / provider / _safe(asset_id)
        return folder / resolution if resolution else folder

    def cache_dir(self, provider):
        return self.root / "cache" / provider

    def preview_path(self, provider, asset_id, ext=".png"):
        return self.root / "previews" / provider / f"{_safe(asset_id)}{ext}"

    def materials_dir(self, kind="customized"):
        if kind not in MATERIAL_KINDS:
            raise PipelineError(f"Material folder must be one of {', '.join(MATERIAL_KINDS)}")
        return self.root / "materials" / kind

    def rel(self, path):
        path = Path(path).resolve()
        try:
            return path.relative_to(self.root).as_posix()
        except ValueError:
            return str(path)

    def abs(self, rel):
        p = Path(rel)
        return p if p.is_absolute() else self.root / p

    # -- registry --------------------------------------------------------

    def _load(self):
        if not self.registry_path.exists():
            return {"version": REGISTRY_VERSION, "assets": {}}
        try:
            data = json.loads(self.registry_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise PipelineError(f"Asset registry {self.registry_path} is unreadable: {exc}") from exc
        data.setdefault("assets", {})
        return data

    def reload(self):
        self._data = self._load()

    def save(self):
        _atomic_write(self.registry_path, json.dumps(self._data, indent=1, sort_keys=True))

    def _save_entry(self, key):
        entry = self._data["assets"][key]
        provider, asset_id = key.split(":", 1)
        _atomic_write(self.root / "metadata" / provider / f"{_safe(asset_id)}.json", json.dumps(entry, indent=1))
        self.save()

    def keys(self):
        return sorted(self._data["assets"])

    def get(self, key):
        return self._data["assets"].get(key)

    def record(self, key):
        entry = self.get(key)
        return AssetRecord.from_dict(entry["record"]) if entry else None

    def entries(self, kind=None, provider=None, downloaded=None):
        out = []
        for key in self.keys():
            entry = self._data["assets"][key]
            rec = entry["record"]
            if kind and rec["kind"] != kind:
                continue
            if provider and rec["provider"] != provider:
                continue
            if downloaded is not None and bool(self.downloaded_resolutions(key)) != downloaded:
                continue
            out.append(entry)
        return out

    def upsert(self, record, save=True):
        """Add or refresh an asset's metadata, keeping its downloads."""
        key = record.key
        entry = self._data["assets"].get(key)
        if entry is None:
            entry = {"record": {}, "downloads": {}, "analysis": {}, "added": _now()}
            self._data["assets"][key] = entry
        old = entry["record"]
        new = record.to_dict()
        # A search result can carry less than a full info lookup did; keep what we knew.
        for field_name, value in old.items():
            if new.get(field_name) in (None, "", [], {}, 0) and value not in (None, "", [], {}, 0):
                new[field_name] = value
        entry["record"] = new
        entry["updated"] = _now()
        if save:
            self._save_entry(key)
        return entry

    def set_analysis(self, key, name, value, save=True):
        entry = self._require(key)
        entry.setdefault("analysis", {})[name] = value
        if save:
            self._save_entry(key)

    def analysis(self, key, name, default=None):
        entry = self.get(key)
        return (entry or {}).get("analysis", {}).get(name, default)

    def add_download(self, key, resolution, folder, files, maps=None, source_urls=(), extra=None):
        entry = self._require(key)
        entry["downloads"][resolution] = {
            "resolution": resolution,
            "folder": self.rel(folder),
            "files": sorted(self.rel(f) for f in files),
            "maps": {m: self.rel(p) for m, p in (maps or {}).items()},
            "bytes": sum(Path(f).stat().st_size for f in files if Path(f).exists()),
            "source_urls": list(source_urls),
            "fetched": _now(),
            **(extra or {}),
        }
        self._save_entry(key)
        return entry["downloads"][resolution]

    def download_info(self, key, resolution):
        entry = self.get(key)
        info = (entry or {}).get("downloads", {}).get(resolution)
        return {**info, "resolution": resolution} if info else None

    def downloaded_resolutions(self, key):
        """Resolutions whose files are all still on disk."""
        entry = self.get(key) or {}
        return sorted((res for res, info in entry.get("downloads", {}).items()
                       if info.get("files") and all(self.abs(f).exists() for f in info["files"])),
                      key=lambda r: int(re.sub(r"\D", "", r) or 0) * (1024 if r.endswith("k") else 1))

    def has_download(self, key, resolution):
        return resolution in self.downloaded_resolutions(key)

    def maps(self, key, resolution):
        """``{map: absolute path}`` for a downloaded resolution."""
        info = self.download_info(key, resolution)
        if not info:
            raise PipelineError(f"{key} has no {resolution} download in the library")
        return {m: self.abs(p) for m, p in info.get("maps", {}).items()}

    def remove(self, key, delete_files=False):
        entry = self._data["assets"].pop(key, None)
        if entry is None:
            return False
        if delete_files:
            rec = entry["record"]
            folder = self.asset_dir(rec["provider"], rec["kind"], rec["id"])
            if folder.is_dir() and self.root in folder.resolve().parents:
                shutil.rmtree(folder)
        provider, asset_id = key.split(":", 1)
        (self.root / "metadata" / provider / f"{_safe(asset_id)}.json").unlink(missing_ok=True)
        self.save()
        return True

    def _require(self, key):
        entry = self.get(key)
        if entry is None:
            raise PipelineError(f"{key} isn't in the asset registry")
        return entry

    # -- search ----------------------------------------------------------

    def search(self, query="", kind=None, maps=(), downloaded=None, limit=None):
        """Registry entries matching every word of ``query`` (offline search).

        Ranked by how many query words hit tags/categories/name, then by
        popularity.
        """
        words = [w for w in re.findall(r"[a-z0-9]+", query.lower()) if w]
        scored = []
        for entry in self.entries(kind=kind, downloaded=downloaded):
            rec = AssetRecord.from_dict(entry["record"])
            if maps and not set(maps) <= set(rec.maps):
                continue
            vocab = rec.words() | {rec.id.lower()}
            if not all(any(v.startswith(w) for v in vocab) for w in words):
                continue
            strong = sum(1 for w in words if w in {t.lower() for t in rec.tags + rec.categories} or w in rec.name.lower())
            scored.append((strong, rec.popularity, entry))
        scored.sort(key=lambda s: (-s[0], -s[1]))
        result = [s[2] for s in scored]
        return result[:limit] if limit else result

    # -- material definitions -------------------------------------------

    def save_material(self, name, definition, kind="customized"):
        path = self.materials_dir(kind) / f"{_safe(name)}.json"
        _atomic_write(path, json.dumps({"name": name, "saved": _now(), **definition}, indent=1))
        return path

    def material_definitions(self, kind="customized"):
        folder = self.materials_dir(kind)
        defs = {}
        for path in sorted(folder.glob("*.json")) if folder.is_dir() else []:
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                log.warning("Skipping unreadable material definition %s", path)
                continue
            defs[data.get("name", path.stem)] = data
        return defs

    def stats(self):
        out = {"assets": len(self._data["assets"]), "bytes": 0, "by_kind": {}}
        for entry in self._data["assets"].values():
            kind = entry["record"]["kind"]
            out["by_kind"][kind] = out["by_kind"].get(kind, 0) + 1
            out["bytes"] += sum(d.get("bytes", 0) for d in entry.get("downloads", {}).values())
        return out


def _safe(text):
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", str(text)).strip("._") or "asset"
