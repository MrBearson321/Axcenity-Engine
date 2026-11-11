"""ambientCG and Poly Haven through the Axcenity Toolbox (the engine's Rust core).

When the ``axcenity-toolbox`` command is found (``AXCENITY_TOOLBOX`` or
``PATH``) and the ``axcenity_toolbox`` package imports (``AXCENITY_TOOLBOX_PYTHON``
names its folder when it isn't installed), ``backed(AmbientCG)`` and
``backed(PolyHaven)`` search, describe and download through it: the same
sources, records and verified downloads as the editor's Toolbox tab, kept in
the central asset directory (``%LOCALAPPDATA%\\Axcenity\\Assets`` or
``AXCENITY_ASSET_DIR``) and its ``metadata.db``. The pipeline's registry then
points at those files (``AssetLibrary.rel`` keeps paths outside the library
absolute), so nothing is downloaded twice.

Without the Toolbox, or with ``PIPELINE_TOOLBOX=0``, each class keeps its own
Python code. That code is retired only once these providers pass the
provider tests on the machine that runs the pipeline (Paul's decision:
conditional migration).
"""

import os
import sys
from pathlib import Path

from ..utils import PipelineError, log
from .base import HDRI, MODEL, AssetRecord, ProviderUnavailable, map_from_filename

ENV_PACKAGE = "AXCENITY_TOOLBOX_PYTHON"
ENV_SWITCH = "PIPELINE_TOOLBOX"

IMAGE_SUFFIXES = (".jpg", ".jpeg", ".png", ".exr", ".tif", ".tiff")
SKY_SUFFIXES = (".hdr", ".exr")
MODEL_SUFFIXES = (".gltf", ".glb", ".fbx")


def _package():
    folder = os.environ.get(ENV_PACKAGE, "").strip()
    if folder and folder not in sys.path:
        sys.path.insert(0, folder)
    try:
        import axcenity_toolbox
    except ImportError:
        return None
    return axcenity_toolbox


def toolbox(offline=False):
    """A ``Toolbox`` when the Rust core is there and not switched off, else ``None``."""
    if os.environ.get(ENV_SWITCH, "").strip().lower() in ("0", "off", "false", "no"):
        return None
    package = _package()
    if package is None or not package.find_binary():
        return None
    return package.Toolbox(offline=offline)


def record_from_toolbox(data):
    """A Toolbox record (``asset-source`` v1, the engine's ``AssetRecord``) as the pipeline's."""
    variants = list(data.get("variants") or [])
    resolutions = list(data.get("resolutions") or [])
    if not resolutions:
        seen = {(v.get("resolution") or "").lower() for v in variants} - {""}
        resolutions = sorted(seen, key=lambda r: int(r.rstrip("k")) if r.rstrip("k").isdigit() else 0)
    return AssetRecord(
        provider=data.get("provider", ""),
        id=data.get("id", ""),
        kind=data.get("kind", ""),
        name=data.get("name") or data.get("id", ""),
        url=data.get("url", ""),
        description=data.get("description", ""),
        tags=list(data.get("tags") or []),
        categories=list(data.get("categories") or []),
        authors=list(data.get("authors") or []),
        license=data.get("license", ""),
        resolutions=resolutions,
        maps=list(data.get("maps") or []),
        physical_size=data.get("physical_size"),
        popularity=float(data.get("popularity") or 0),
        polycount=int(data.get("polycount") or 0),
        thumbnail_url=data.get("thumbnail_url", ""),
        extra={
            "toolbox": True,
            "variants": variants,
            "license_version": data.get("license_version", ""),
            "license_url": data.get("license_url", ""),
            "attribution": data.get("attribution", ""),
        },
    )


def maps_of(kind, files):
    """``{map: path}`` for a download's files, the way the pipeline names maps."""
    maps = {}
    for f in files:
        suffix = f.suffix.lower()
        if kind == HDRI and suffix in SKY_SUFFIXES:
            maps.setdefault("environment", f)
        elif suffix in IMAGE_SUFFIXES:
            role = map_from_filename(f.name)
            if role:
                maps.setdefault(role, f)
    return maps


def resolution_of(label, wanted):
    """``"2K JPG"`` -> ``"2k"`` (the asked-for resolution when the label has none)."""
    first = (label or "").split(" ")[0].lower()
    return first if first.endswith("k") and first[:-1].isdigit() else wanted


class ToolboxBacked:
    """Search, info and download through the Toolbox; the Python class otherwise."""

    def _toolbox(self):
        if not hasattr(self, "_tb"):
            self._tb = toolbox(getattr(self, "offline", False))
            if self._tb is None:
                log.debug("%s: the Axcenity Toolbox isn't available, using the Python provider", self.name)
        return self._tb

    def _call(self, what, *args, **kwargs):
        package = _package()
        try:
            return what(*args, **kwargs)
        except package.SourceUnavailable as exc:
            raise ProviderUnavailable(f"{self.name}: {exc}") from exc
        except package.ToolboxError as exc:
            raise PipelineError(f"{self.name}: {exc}") from exc

    def search(self, query="", kind=None, limit=30, categories=()):
        kind = kind or self.kinds[0]
        tb = self._toolbox()
        if tb is None:
            return super().search(query, kind=kind, limit=limit, categories=categories)
        if kind not in self.kinds:
            return []
        rows = self._call(tb.search, query, kind=kind, provider=self.name, limit=int(limit))
        records = [record_from_toolbox(row) for row in rows]
        if categories:
            wanted = {c.lower() for c in categories}
            records = [r for r in records if wanted & {c.lower() for c in r.categories}]
        return records

    def info(self, asset_id, kind=None):
        tb = self._toolbox()
        if tb is None:
            return super().info(asset_id, kind) if kind else super().info(asset_id)
        return record_from_toolbox(self._call(tb.info, f"{self.name}:{asset_id}"))

    def download(self, record, resolution, fmt="jpg"):
        """Fetches through the Toolbox (verified, kept once in the asset directory).

        Returns ``(resolution, folder, files, maps, source_urls)`` like the Python providers.
        """
        tb = self._toolbox()
        if tb is None:
            return super().download(record, resolution, fmt=fmt)
        # A model's own glTF bundle: with no format, the first variant at that
        # resolution can be a maps-only package (Poly Haven's "1K EXR").
        fmt = {HDRI: None, MODEL: "gltf"}.get(record.kind, fmt)
        got = self._call(tb.fetch, record.key, res=resolution, fmt=fmt)
        files = [Path(f) for f in got.file_paths()]
        if record.kind == MODEL and not any(f.suffix.lower() in MODEL_SUFFIXES for f in files):
            raise PipelineError(f"{record.key}: the download has no model file")
        return resolution_of(got.variant, resolution), got.path, files, maps_of(record.kind, files), got.source_urls


def backed(cls):
    """``cls`` searching and downloading through the Toolbox when it's there."""
    return type(cls.__name__, (ToolboxBacked, cls), {"python": cls, "__module__": cls.__module__})
