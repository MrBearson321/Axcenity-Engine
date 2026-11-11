"""Poly Haven (https://polyhaven.com): CC0 textures, HDRIs and models.

Uses the public API (https://api.polyhaven.com): ``/assets?t=<type>`` lists
every asset of a type with its metadata (cached and searched locally),
``/info/<id>`` describes one and ``/files/<id>`` lists every file with its
size and MD5. Textures download one file per map (``Diffuse``, ``nor_gl``,
``Rough``, ``AO``, ``Displacement``, ``arm``...), HDRIs one ``.hdr``, models
a ``.gltf`` plus the ``.bin`` and textures it includes. Sizes
(``dimensions``) are in millimetres. The API asks for a unique User-Agent
(see ``asset_downloader.USER_AGENT``).
"""

import re
from pathlib import PurePosixPath

from ..utils import PipelineError
from . import asset_downloader as dl
from .base import (AO, ARM, BASE_COLOR, EMISSION, HDRI, HEIGHT, METALLIC, MODEL, NORMAL, NORMAL_DX, OPACITY,
                   ROUGHNESS, TEXTURE, AssetRecord, map_from_filename, pick_resolution)

NAME = "polyhaven"
API = "https://api.polyhaven.com"
TYPES = {TEXTURE: "textures", HDRI: "hdris", MODEL: "models"}
TYPE_IDS = {0: HDRI, 1: TEXTURE, 2: MODEL}
FILE_MAPS = {"Diffuse": BASE_COLOR, "nor_gl": NORMAL, "nor_dx": NORMAL_DX, "Rough": ROUGHNESS, "Metal": METALLIC,
             "AO": AO, "Displacement": HEIGHT, "arm": ARM, "Alpha": OPACITY, "Opacity": OPACITY,
             "Emission": EMISSION, "Emissive": EMISSION}
STANDARD_RESOLUTIONS = ("1k", "2k", "4k", "8k", "16k")
# Maps worth downloading for a texture, in order. arm duplicates AO/Rough/Metal,
# nor_dx duplicates nor_gl; they're fetched only when the separate map is missing.
TEXTURE_FETCH = (BASE_COLOR, NORMAL, ROUGHNESS, METALLIC, AO, HEIGHT, OPACITY, EMISSION)


class PolyHaven:
    name = NAME
    kinds = (TEXTURE, HDRI, MODEL)

    def __init__(self, library, offline=False, ttl=86400):
        self.library = library
        self.offline = offline
        self.ttl = ttl

    def _get(self, path):
        return dl.get_json(f"{API}{path}", self.library.cache_dir(NAME), self.ttl, self.offline)

    def catalog(self, kind=TEXTURE):
        """Every asset of a kind: ``{id: api info}`` (one cached request)."""
        if kind not in TYPES:
            raise PipelineError(f"Poly Haven has no {kind} assets")
        return self._get(f"/assets?t={TYPES[kind]}")

    def search(self, query="", kind=TEXTURE, limit=30, categories=()):
        """Assets whose name, tags, categories or description contain every query word."""
        words = [w for w in re.findall(r"[a-z0-9]+", query.lower()) if w]
        cats = {c.lower() for c in categories}
        hits = []
        for asset_id, info in self.catalog(kind).items():
            rec = self.record_from_api(asset_id, info, kind)
            if cats and not cats & {c.lower() for c in rec.categories}:
                continue
            vocab = rec.words() | {asset_id.lower()}
            if words and not all(any(v.startswith(w) for v in vocab) for w in words):
                continue
            strong = sum(1 for w in words if w in {t.lower() for t in rec.tags + rec.categories})
            hits.append((strong, rec.popularity, rec))
        hits.sort(key=lambda h: (-h[0], -h[1]))
        return [h[2] for h in hits[:limit]]

    def info(self, asset_id, kind=None):
        data = self._get(f"/info/{asset_id}")
        if not isinstance(data, dict) or "name" not in data:
            raise PipelineError(f"Poly Haven has no asset {asset_id!r}")
        rec = self.record_from_api(asset_id, data, kind or TYPE_IDS.get(data.get("type"), TEXTURE))
        try:
            files = self.files(asset_id)
        except PipelineError:
            return rec
        rec.maps = sorted({FILE_MAPS[k] for k in files if k in FILE_MAPS})
        res_source = files.get("Diffuse") or files.get("hdri") or files.get("gltf") or {}
        rec.resolutions = _sorted_res(res_source)
        return rec

    def files(self, asset_id):
        return self._get(f"/files/{asset_id}")

    @staticmethod
    def record_from_api(asset_id, info, kind):
        dims = info.get("dimensions") or []
        max_res = (info.get("max_resolution") or [0])[0]
        resolutions = [r for r in STANDARD_RESOLUTIONS if int(r[:-1]) * 1024 <= max_res] or ["1k"]
        attributes = dict(info.get("attributes") or {})
        for key in ("polycount", "texel_density", "evs_cap"):
            if key in info:
                attributes[key] = info[key]
        thumb = info.get("thumbnail_url") or ""
        return AssetRecord(
            provider=NAME, id=asset_id, kind=kind,
            name=info.get("name") or asset_id,
            url=f"https://polyhaven.com/a/{asset_id}",
            description=info.get("description") or "",
            tags=list(info.get("tags") or []),
            categories=list(info.get("categories") or []),
            authors=sorted((info.get("authors") or {}).keys()),
            license="CC0",
            resolutions=resolutions,
            maps=[],
            physical_size=[d / 1000.0 for d in dims] if dims and all(dims) else None,
            attributes=attributes,
            method="scan" if kind != HDRI else "hdri",
            popularity=float(info.get("download_count") or 0),
            polycount=int(info.get("polycount") or 0),
            thumbnail_url=thumb.replace("format=webp", "format=png"),
            extra={"category_path": info.get("category", "")},
        )

    def download(self, record, resolution, fmt="jpg"):
        files = self.files(record.id)
        if record.kind == TEXTURE:
            return self._download_texture(record, files, resolution, fmt)
        if record.kind == HDRI:
            return self._download_hdri(record, files, resolution)
        return self._download_model(record, files, resolution)

    def _download_texture(self, record, files, resolution, fmt):
        res = pick_resolution(_sorted_res(files.get("Diffuse") or {}), resolution) or resolution
        folder = self.library.asset_dir(NAME, TEXTURE, record.id, res)
        by_map = {FILE_MAPS[k]: v for k, v in files.items() if k in FILE_MAPS}
        wanted = [m for m in TEXTURE_FETCH if m in by_map]
        if NORMAL not in by_map and NORMAL_DX in by_map:
            wanted.append(NORMAL_DX)
        if ARM in by_map and not {AO, ROUGHNESS} <= set(by_map):
            wanted.append(ARM)
        maps, written, urls = {}, [], []
        for map_name in wanted:
            item = _pick_format(by_map[map_name].get(res) or {}, (fmt, "jpg", "png", "exr"))
            if not item:
                continue
            dest = folder / PurePosixPath(item["url"]).name
            dl.download(item["url"], dest, item.get("size"), item.get("md5"))
            maps[map_name] = dest
            written.append(dest)
            urls.append(item["url"])
        if BASE_COLOR not in maps:
            raise PipelineError(f"Poly Haven {record.id}: no {res} colour map to download")
        return res, folder, written, maps, urls

    def _download_hdri(self, record, files, resolution):
        options = files.get("hdri") or {}
        res = pick_resolution(_sorted_res(options), resolution)
        item = _pick_format(options.get(res) or {}, ("hdr", "exr"))
        if not item:
            raise PipelineError(f"Poly Haven {record.id}: no {resolution} HDRI file")
        folder = self.library.asset_dir(NAME, HDRI, record.id, res)
        dest = folder / PurePosixPath(item["url"]).name
        dl.download(item["url"], dest, item.get("size"), item.get("md5"))
        return res, folder, [dest], {"environment": dest}, [item["url"]]

    def _download_model(self, record, files, resolution):
        options = files.get("gltf") or {}
        res = pick_resolution(_sorted_res(options), resolution)
        main = (options.get(res) or {}).get("gltf")
        if not main:
            raise PipelineError(f"Poly Haven {record.id}: no {resolution} glTF download")
        folder = self.library.asset_dir(NAME, MODEL, record.id, res)
        entry = folder / PurePosixPath(main["url"]).name
        dl.download(main["url"], entry, main.get("size"), main.get("md5"))
        written, urls, maps = [entry], [main["url"]], {}
        for rel, item in (main.get("include") or {}).items():
            rel_path = PurePosixPath(rel)
            if rel_path.is_absolute() or ".." in rel_path.parts:
                raise PipelineError(f"Poly Haven {record.id}: unsafe include path {rel!r}")
            dest = folder.joinpath(*rel_path.parts)
            dl.download(item["url"], dest, item.get("size"), item.get("md5"))
            written.append(dest)
            urls.append(item["url"])
            kind = map_from_filename(dest.name)
            if kind and dest.suffix.lower() in (".jpg", ".png", ".exr"):
                maps.setdefault(kind, dest)
        return res, folder, written, maps, urls


def _sorted_res(options):
    return sorted((r for r in options if re.match(r"^\d+k$", r)), key=lambda r: int(r[:-1]))


def _pick_format(by_format, order):
    for fmt in order:
        if fmt in by_format:
            return by_format[fmt]
    return None

