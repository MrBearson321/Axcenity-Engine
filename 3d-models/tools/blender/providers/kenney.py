"""Kenney (https://kenney.nl/assets): CC0 2D packs mined for masks.

Kenney has no API. Each asset page (``/assets/<slug>``) names the pack, its
tags and licence, and downloads go through the page's own "continue without
donating" link: ``/download-link/<hex of "assets/<slug>">`` answers with a
short-lived signed URL to the zip. The zip keeps its folder layout when
unpacked (packs reuse file names across ``Default``/``Double`` folders) and
only images are taken out.

``CATALOG`` lists the packs worth having for surface work, with what they
are likely good for. ``mask_library`` decides per image what each file can
really be used for. Any other Kenney pack can be fetched by its slug
(``kenney:road-textures``); its page is read for name, tags and licence.
Packs whose page doesn't show the CC0 licence are recorded without one, so
the licence check flags them.
"""

import html
import re

from ..utils import PipelineError
from . import asset_downloader as dl
from .base import MODEL, SPRITES, AssetRecord

NAME = "kenney"
SITE = "https://kenney.nl"
VERSION = "pack"  # Kenney packs have one download; the registry stores it under this "resolution"
IMAGE_EXTENSIONS = {".png"}
MODEL_EXTENSIONS = {".glb", ".gltf", ".bin", ".png"}  # a pack's models and the palette texture they share

CATALOG = {
    "development-essentials": {
        "name": "Development Essentials",
        "tags": ["noise", "perlin", "gradient", "checkerboard", "uv", "normal"],
        "uses": ["roughness breakup", "dirt gradients", "UV checking"],
    },
    "pattern-pack": {
        "name": "Pattern Pack",
        "tags": ["pattern", "seamless", "tiles", "bricks", "stripes", "dots", "waves"],
        "uses": ["manufactured surface patterns", "height detail", "decorative overlays"],
    },
    "pattern-pack-lines": {
        "name": "Pattern Pack Lines",
        "tags": ["pattern", "seamless", "lines", "grooves"],
        "uses": ["grooves and panel lines", "brushed directions"],
    },
    "pattern-pack-extra": {
        "name": "Pattern Pack Extra",
        "tags": ["pattern", "seamless"],
        "uses": ["manufactured surface patterns"],
    },
    "particle-pack": {
        "name": "Particle Pack",
        "tags": ["particles", "dirt", "scorch", "smoke", "scratch", "spark", "vfx"],
        "uses": ["stain, speckle and scratch stamps", "density masks"],
    },
    "smoke-particles": {
        "name": "Smoke Particles",
        "tags": ["particles", "smoke", "puff", "vfx"],
        "uses": ["soft stain and water-mark stamps"],
    },
}


# 3D packs, fetched with kind "model". Their GLB files share one palette texture.
MODEL_CATALOG = {
    "nature-kit": {"name": "Nature Kit", "tags": ["nature", "tree", "rock", "foliage", "fence"], "uses": ["rocks, plants, fences"]},
    "car-kit": {"name": "Car Kit", "tags": ["car", "vehicle", "truck"], "uses": ["parked vehicles"]},
    "city-kit-suburban": {"name": "City Kit (Suburban)", "tags": ["building", "house", "suburban", "fence", "tree"], "uses": ["background houses, fences"]},
    "survival-kit": {"name": "Survival Kit", "tags": ["survival", "barrel", "crate", "tools", "sign"], "uses": ["props, signs, tools"]},
}


def download_id(slug):
    """The page's download id: hex of ``assets/<slug>``."""
    return f"assets/{slug}".encode("utf-8").hex()


class Kenney:
    name = NAME
    kinds = (SPRITES, MODEL)

    def __init__(self, library, offline=False, ttl=86400):
        self.library = library
        self.offline = offline
        self.ttl = ttl * 7  # pages rarely change

    # -- metadata ----------------------------------------------------------

    def page_url(self, slug):
        return f"{SITE}/assets/{slug}"

    def _page(self, slug):
        return dl.get_text(self.page_url(slug), self.library.cache_dir(NAME), self.ttl, self.offline)

    def search(self, query="", kind=SPRITES, limit=30, categories=()):
        """Search the curated catalogue (and the registry's other Kenney packs)."""
        if kind not in self.kinds:
            return []
        words = [w for w in re.findall(r"[a-z0-9]+", query.lower()) if w]
        found = []
        for slug, meta in (MODEL_CATALOG if kind == MODEL else CATALOG).items():
            vocab = {slug, *slug.split("-"), *meta["tags"], *meta["name"].lower().split()}
            if all(any(v.startswith(w) for v in vocab) for w in words):
                found.append(self._catalog_record(slug, meta, kind))
        return found[:limit]

    def _catalog_record(self, slug, meta, kind=SPRITES):
        known = self.library.record(f"{NAME}:{slug}")
        if known and known.license:
            return known
        return AssetRecord(provider=NAME, id=slug, kind=kind, name=meta["name"], url=self.page_url(slug),
                           description="; ".join(meta.get("uses", [])), tags=list(meta["tags"]),
                           categories=["Textures"], authors=["Kenney"], license="",
                           resolutions=[VERSION], attributes={"uses": meta.get("uses", [])})

    def info(self, slug, kind=SPRITES):
        """Read the asset page: name, tags, category, licence, preview."""
        text = self._page(slug)
        title = re.search(r"<h1[^>]*>(.*?)</h1>", text, re.S) or re.search(r"<title>(.*?)</title>", text, re.S)
        name = html.unescape(re.sub(r"<[^>]+>", "", title.group(1))).strip() if title else slug
        name = re.sub(r"\s*[·|-]\s*Kenney.*$", "", name, flags=re.I) or slug
        tags = sorted(set(re.findall(r"/assets/tag:([a-z0-9-]+)", text)))
        cats = sorted(set(re.findall(r"/assets/category:([A-Za-z0-9 %-]+)", text)))
        cc0 = "publicdomain/zero" in text
        if download_id(slug) not in text:
            raise PipelineError(f"Kenney has no downloadable asset {slug!r} ({self.page_url(slug)})")
        preview = re.search(r"href='(https://kenney\.nl/media/pages/assets/[^']+/preview\.png)'", text)
        kind = MODEL if slug in MODEL_CATALOG else kind
        meta = (MODEL_CATALOG if kind == MODEL else CATALOG).get(slug, {})
        return AssetRecord(
            provider=NAME, id=slug, kind=kind, name=meta.get("name", name), url=self.page_url(slug),
            description="; ".join(meta.get("uses", [])), tags=sorted(set(tags) | set(meta.get("tags", []))),
            categories=cats or ["Textures"], authors=["Kenney"], license="CC0" if cc0 else "",
            resolutions=[VERSION], attributes={"uses": meta.get("uses", [])},
            thumbnail_url=preview.group(1) if preview else "")

    # -- download ------------------------------------------------------------

    def download(self, record, resolution=VERSION, fmt=None):
        """Download and unpack the pack's images or models; returns the registry download entry."""
        if self.offline:
            raise PipelineError("Offline: Kenney packs need a download")
        if not record.license:
            record = self.info(record.id)
        answer = dl.post_json(f"{SITE}/download-link/{download_id(record.id)}",
                              headers={"Referer": self.page_url(record.id)})
        url = answer.get("url")
        if not url or not url.startswith(SITE + "/"):
            raise PipelineError(f"Kenney gave no download link for {record.id} ({answer.get('error', 'no url')})")
        archive = self.library.cache_dir(NAME) / "archives" / f"kenney_{record.id}.zip"
        dl.download(url, archive, force=True)
        models = record.kind == MODEL
        folder = self.library.asset_dir(NAME, MODEL if models else SPRITES, record.id, VERSION)
        files = dl.extract_zip(archive, folder, MODEL_EXTENSIONS if models else IMAGE_EXTENSIONS, keep_dirs=True)
        archive.unlink(missing_ok=True)
        if not files:
            raise PipelineError(f"Kenney {record.id}: the download held no {'models' if models else 'PNG images'}")
        return VERSION, folder, files, {}, [self.page_url(record.id)]
