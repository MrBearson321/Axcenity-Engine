"""External asset sources (ambientCG, Poly Haven, Kenney) and the shared asset library.

Nothing here needs Blender, so it runs from plain Python too
(``tools/library.py``). The rest of the toolkit uses three calls::

    from blender import providers
    records = providers.search("red brick", kind="texture")   # every provider, normalised
    record, info = providers.fetch(records[0], "2k")         # download once, then reuse
    lib = providers.open_library()                           # registry: search, maps, licences

A new source is a class with ``name``, ``kinds``, ``search``, ``info`` and
``download`` added with ``register_provider``; ``base.py`` has the contract.
Sketchfab, local folders and catalogues are opt-in: a search reaches them
only when ``library.providers`` names them (``acquisition`` does by itself).
"""

import os
from pathlib import Path

from ..utils import PipelineError, log
from . import asset_downloader, license_manager
from .ambientcg import AmbientCG
from .asset_registry import AssetLibrary
from .base import (HDRI, KINDS, MODEL, SPRITES, TEXTURE, AssetRecord, ProviderUnavailable, parse_key,  # noqa: F401
                   pick_resolution)
from .catalogue import Catalogue
from .kenney import Kenney
from .local_files import LocalFiles
from .polyhaven import PolyHaven
from .sketchfab import Sketchfab

ENV_LIBRARY = "PIPELINE_ASSET_LIBRARY"

PROVIDERS = {AmbientCG.name: AmbientCG, PolyHaven.name: PolyHaven, Kenney.name: Kenney,
             Sketchfab.name: Sketchfab, LocalFiles.name: LocalFiles, Catalogue.name: Catalogue}
_libraries = {}


def register_provider(cls):
    PROVIDERS[cls.name] = cls
    return cls


def _cfg(cfg):
    if cfg is not None:
        return cfg
    from ..config import get_config

    return get_config()


def library_root(cfg=None):
    """The asset library folder: $PIPELINE_ASSET_LIBRARY, else ``library.directory``."""
    env = os.environ.get(ENV_LIBRARY)
    if env:
        return Path(env).expanduser().resolve()
    path = _cfg(cfg).path("library.directory")
    if path is None:
        raise PipelineError("Set library.directory in pipeline.config.json (or $PIPELINE_ASSET_LIBRARY)")
    return path


def open_library(cfg=None, root=None):
    root = Path(root).resolve() if root else library_root(cfg)
    lib = _libraries.get(root)
    if lib is None:
        lib = _libraries[root] = AssetLibrary(root)
        lib.ensure_layout()
    else:
        lib.reload()
    return lib


def get_provider(name, library=None, cfg=None):
    cfg = _cfg(cfg)
    cls = PROVIDERS.get(name)
    if cls is None:
        raise PipelineError(f"Unknown asset provider {name!r} (have: {', '.join(PROVIDERS)})")
    extra = {"cfg": cfg} if getattr(cls, "wants_config", False) else {}
    return cls(library or open_library(cfg), offline=bool(cfg.get("library.offline", False)),
               ttl=float(cfg.get("library.cache_ttl_hours", 24)) * 3600, **extra)


def capabilities(name):
    """What a provider really offers. ``licenses_per_asset``: each asset states its own."""
    cls = PROVIDERS.get(name)
    if cls is None:
        raise PipelineError(f"Unknown asset provider {name!r} (have: {', '.join(PROVIDERS)})")
    fixed = license_manager.PROVIDER_TERMS.get(name, {}).get("license")
    declared = getattr(cls, "licenses", None)
    return {"name": name, "kinds": tuple(cls.kinds), "formats": tuple(getattr(cls, "formats", ())),
            "licenses": tuple(declared) if declared else ((fixed,) if fixed else None),
            "licenses_per_asset": not (declared or fixed), "opt_in": bool(getattr(cls, "opt_in", False))}


def enabled_providers(kind=TEXTURE, cfg=None):
    cfg = _cfg(cfg)
    names = cfg.get("library.providers") or [n for n, c in PROVIDERS.items() if not getattr(c, "opt_in", False)]
    return [n for n in names if n in PROVIDERS and kind in PROVIDERS[n].kinds]


def search(query="", kind=TEXTURE, providers=None, limit=30, categories=(), cfg=None, library=None):
    """Search providers (online, cached) and return normalised ``AssetRecord``s.

    A provider that fails (offline, API change) is logged and skipped, so one
    source being down doesn't stop the others.
    """
    cfg = _cfg(cfg)
    library = library or open_library(cfg)
    results = []
    for name in providers or enabled_providers(kind, cfg):
        try:
            results += get_provider(name, library, cfg).search(query, kind=kind, limit=limit, categories=categories)
        except PipelineError as exc:
            log.warning("%s search failed: %s", name, exc)
    return results


def resolve(key_or_record, kind=None, cfg=None, library=None):
    """An ``AssetRecord`` from a record, a ``provider:id`` key, or the registry."""
    if isinstance(key_or_record, AssetRecord):
        return key_or_record
    library = library or open_library(cfg)
    known = library.record(key_or_record) if ":" in key_or_record else None
    if known:
        return known
    provider, asset_id = parse_key(key_or_record)
    return get_provider(provider, library, cfg).info(asset_id, kind)


def fetch(key_or_record, resolution=None, kind=None, cfg=None, library=None, force=False):
    """Make sure an asset is in the library at a resolution; download only if needed.

    Returns ``(record, download_info)``. An asset already downloaded at that
    resolution (files still on disk) isn't downloaded again; the provider's
    MD5/size checks skip files that are already complete.
    """
    cfg = _cfg(cfg)
    library = library or open_library(cfg)
    record = resolve(key_or_record, kind, cfg, library)
    resolution = resolution or cfg.get("library.default_resolution", "1k")
    if record.resolutions:
        resolution = pick_resolution(record.resolutions, resolution) or (
            record.resolutions[0] if len(record.resolutions) == 1 else resolution)
    if not force and library.has_download(record.key, resolution):
        library.upsert(record, save=False)
        return record, library.download_info(record.key, resolution)
    provider = get_provider(record.provider, library, cfg)
    if not record.maps and hasattr(provider, "info"):
        try:
            record = provider.info(record.id, record.kind)
        except PipelineError as exc:
            log.debug("Couldn't refresh %s before download: %s", record.key, exc)
    res, folder, files, maps, urls = provider.download(record, resolution, fmt=cfg.get("library.format", "jpg"))
    library.upsert(record)
    info = library.add_download(record.key, res, folder, files, maps, urls)
    for problem in license_manager.problems(library.get(record.key)["record"]):
        log.warning("%s: %s", record.key, problem)
    return record, info


def fetch_preview(record, cfg=None, library=None):
    """Download (once) the provider's thumbnail; returns its path or ``None``."""
    library = library or open_library(cfg)
    if not record.thumbnail_url:
        return None
    dest = library.preview_path(record.provider, record.id, ".png")
    try:
        asset_downloader.download(record.thumbnail_url, dest)
    except PipelineError as exc:
        log.warning("No preview for %s: %s", record.key, exc)
        return None
    return dest


def refresh(key, cfg=None, library=None):
    """Re-read an asset's metadata from its provider without downloading files."""
    cfg = _cfg(cfg)
    library = library or open_library(cfg)
    provider, asset_id = parse_key(key)
    old = library.record(key)
    record = get_provider(provider, library, cfg).info(asset_id, old.kind if old else None)
    library.upsert(record)
    return record
