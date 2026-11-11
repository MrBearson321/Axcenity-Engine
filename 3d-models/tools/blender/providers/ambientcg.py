"""ambientCG (https://ambientcg.com): CC0 PBR materials.

Uses the documented v2 JSON API (``/api/v2/full_json``). Each material is
one zip per resolution and format (``Bricks090_2K-JPG.zip``) holding every
map (``_Color``, ``_NormalGL``, ``_NormalDX``, ``_Roughness``,
``_AmbientOcclusion``, ``_Displacement``, ``_Metalness``, ``_Opacity``).
Physical size (``dimensionX``/``Y``) is given in centimetres; 0 means
unknown.
"""

import urllib.parse

from ..utils import PipelineError
from . import asset_downloader as dl
from .base import (AO, BASE_COLOR, EMISSION, HEIGHT, METALLIC, NORMAL, OPACITY, ROUGHNESS, TEXTURE,
                   AssetRecord, map_from_filename, pick_resolution)

NAME = "ambientcg"
API = "https://ambientcg.com/api/v2/full_json"
INCLUDE = "downloadData,tagData,displayData,dimensionsData,mapData"
MAP_NAMES = {"color": BASE_COLOR, "normal": NORMAL, "roughness": ROUGHNESS, "metalness": METALLIC,
             "ambient-occlusion": AO, "displacement": HEIGHT, "opacity": OPACITY, "emission": EMISSION}


class AmbientCG:
    name = NAME
    kinds = (TEXTURE,)

    def __init__(self, library, offline=False, ttl=86400):
        self.library = library
        self.offline = offline
        self.ttl = ttl

    def _get(self, params):
        url = f"{API}?{urllib.parse.urlencode(params)}"
        return dl.get_json(url, self.library.cache_dir(NAME), self.ttl, self.offline)

    def search(self, query="", kind=TEXTURE, limit=30, categories=()):
        if kind != TEXTURE:
            return []
        params = {"type": "Material", "limit": int(limit), "include": INCLUDE, "sort": "Popular"}
        if query:
            params["q"] = query
        if categories:
            params["category"] = ",".join(categories)
        data = self._get(params)
        return [self.record_from_api(a) for a in data.get("foundAssets", [])]

    def info(self, asset_id, kind=TEXTURE):
        data = self._get({"id": asset_id, "include": INCLUDE})
        found = data.get("foundAssets") or []
        if not found:
            raise PipelineError(f"ambientCG has no asset {asset_id!r}")
        return self.record_from_api(found[0])

    @staticmethod
    def record_from_api(a):
        downloads = {}
        folders = a.get("downloadFolders", {}) or {}
        for folder in folders.values():
            for cat in (folder.get("downloadFiletypeCategories") or {}).values():
                for d in cat.get("downloads", []):
                    downloads[d.get("attribute", "")] = {"url": d.get("downloadLink") or d.get("fullDownloadPath"),
                                                         "size": d.get("size"), "file": d.get("fileName")}
        resolutions = sorted({attr.split("-")[0].lower() for attr in downloads if "-" in attr},
                             key=lambda r: int(r.rstrip("k")) if r.rstrip("k").isdigit() else 0)
        dx, dy = a.get("dimensionX") or 0, a.get("dimensionY") or 0
        maps = [MAP_NAMES[m] for m in a.get("maps", []) if m in MAP_NAMES]
        preview = a.get("previewImage") or {}
        return AssetRecord(
            provider=NAME, id=a["assetId"], kind=TEXTURE,
            name=a.get("displayName") or a["assetId"],
            url=a.get("shortLink") or f"https://ambientcg.com/view?id={a['assetId']}",
            description=a.get("description") or "",
            tags=[t for t in a.get("tags", []) if not t.isdigit() and t.lower() != a["assetId"].lower()],
            categories=[c for c in [a.get("displayCategory")] if c],
            authors=["ambientCG"],
            license="CC0",
            resolutions=resolutions,
            maps=maps,
            physical_size=[dx / 100.0, (dy or dx) / 100.0] if dx else None,
            attributes={},
            method=a.get("creationMethod") or "",
            popularity=float(a.get("downloadCount") or 0),
            thumbnail_url=preview.get("256-PNG") or preview.get("128-PNG") or "",
            extra={"downloads": downloads},
        )

    def download(self, record, resolution, fmt="jpg"):
        """Download and unpack one resolution; returns the registry download entry."""
        downloads = record.extra.get("downloads") or self.info(record.id).extra["downloads"]
        res = pick_resolution(record.resolutions or [k.split("-")[0] for k in downloads], resolution)
        options = [f"{res.upper()}-{fmt.upper()}", f"{res.upper()}-JPG", f"{res.upper()}-PNG"]
        attr = next((o for o in options if o in downloads), None)
        if attr is None:
            raise PipelineError(f"ambientCG {record.id}: no {res} download (have {', '.join(downloads)})")
        item = downloads[attr]
        archive = self.library.cache_dir(NAME) / "archives" / item["file"]
        dl.download(item["url"], archive, size=item.get("size"))
        folder = self.library.asset_dir(NAME, TEXTURE, record.id, res)
        files = dl.extract_zip(archive, folder)
        archive.unlink(missing_ok=True)  # the extracted maps are the copy we keep
        maps = {}
        for f in files:
            kind = map_from_filename(f.name)
            if kind and kind not in maps:
                maps[kind] = f
        return res, folder, files, maps, [item["url"]]
