"""Sketchfab (https://sketchfab.com): models from many authors, each with its own licence.

Data API v3 (checked against the live API and its swagger file, 2026-11-08):
``/v3/search?type=models&downloadable=true&license=<slug>`` is public and takes
one licence per request (a repeated ``license`` keeps only the last); slugs are
``cc0``, ``by``, ``by-sa``, ``by-nd``, ``by-nc``, ``by-nc-sa``, ``by-nc-nd``,
``st``, ``ed``. A result's ``license`` has ``uid`` and ``label``; ``archives``
gives each format's face count, texture count and largest texture.

``/v3/models/<uid>/download`` needs the user's token in an ``Authorization:
Token`` header and answers with short-lived links. The token is read from the
environment for each download and never stored, logged or put in a URL, and
the links are not kept in the registry. GLB is preferred (one file, no
archive); a glTF zip is the fallback.
"""

import json
import os
import urllib.error
import urllib.parse
import urllib.request

from ..utils import PipelineError
from . import asset_downloader as dl
from . import download_guard as guard
from . import license_manager
from .base import MODEL, AssetRecord, ProviderUnavailable, map_from_filename

NAME = "sketchfab"
API = "https://api.sketchfab.com/v3"
ENV_TOKEN = "SKETCHFAB_API_TOKEN"
VERSION = "archive"  # one download per model, whatever its container
PAGE_SIZE = 24  # the API's maximum
THUMBNAIL_WIDTH = 512
LICENSE_SLUGS = {"CC0": "cc0", "CC-BY": "by", "CC-BY-SA": "by-sa", "CC-BY-ND": "by-nd", "CC-BY-NC": "by-nc",
                 "CC-BY-NC-SA": "by-nc-sa", "CC-BY-NC-ND": "by-nc-nd", "SKETCHFAB-STANDARD": "st",
                 "SKETCHFAB-EDITORIAL": "ed"}
SCAN_WORDS = {"photogrammetry", "scan", "scanned", "photoscan", "lidar", "scans"}


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """urllib repeats request headers on a redirect, which would hand the token to another host."""

    def redirect_request(self, *args, **kwargs):
        return None


_opener = urllib.request.build_opener(_NoRedirect)


class Sketchfab:
    name = NAME
    kinds = (MODEL,)
    formats = ("glb", "gltf")
    licenses = None  # each model has its own
    opt_in = True
    wants_config = True

    def __init__(self, library, offline=False, ttl=86400, cfg=None):
        self.library = library
        self.offline = offline
        self.ttl = ttl
        self.cfg = cfg

    # -- metadata ----------------------------------------------------------

    def _get(self, url):
        return dl.get_json(url, self.library.cache_dir(NAME), self.ttl, self.offline)

    def _license_slugs(self):
        policy = license_manager.policy_from_config(self.cfg) if self.cfg else license_manager.DEFAULT_POLICY
        codes = list(policy["allowed"])
        if policy["allow_attribution"]:
            codes += policy["attribution_licenses"]
        return [LICENSE_SLUGS[c] for c in dict.fromkeys(codes) if c in LICENSE_SLUGS]

    def search(self, query="", kind=MODEL, limit=30, categories=(), licenses=None):
        """Downloadable models for ``query``, one API query per licence the policy allows."""
        if kind != MODEL:
            return []
        found = {}
        for slug in licenses or self._license_slugs():
            params = [("type", "models"), ("downloadable", "true"), ("count", PAGE_SIZE), ("license", slug)]
            params += [("q", query)] if query else []
            params += [("categories", c) for c in categories]
            url = f"{API}/search?{urllib.parse.urlencode(params)}"
            taken = 0
            while url and taken < limit:
                data = self._get(url)
                for model in data.get("results") or []:
                    if model.get("isAgeRestricted"):
                        continue
                    rec = self.record_from_api(model)
                    found.setdefault(rec.key, rec)
                    taken += 1
                    if taken >= limit:
                        break
                url = data.get("next") if str(data.get("next") or "").startswith(API + "/") else None
        return list(found.values())

    def info(self, asset_id, kind=None):
        if not asset_id or not asset_id.isalnum():
            raise PipelineError(f"Sketchfab has no model {asset_id!r}")
        data = self._get(f"{API}/models/{asset_id}")
        if not isinstance(data, dict) or "uid" not in data:
            raise PipelineError(f"Sketchfab has no model {asset_id!r}")
        return self.record_from_api(data)

    @staticmethod
    def record_from_api(model):
        uid = model.get("uid", "")
        archives = model.get("archives") or {}
        archive = archives.get("glb") or archives.get("gltf") or {}
        fmt = "glb" if archives.get("glb") or not archives else "gltf"
        user = model.get("user") or {}
        author = user.get("displayName") or user.get("username")
        images = (model.get("thumbnails") or {}).get("images") or []
        thumb = min(images, key=lambda i: abs((i.get("width") or 0) - THUMBNAIL_WIDTH), default={})
        license_data = model.get("license") or {}
        tags = [t.get("slug") or t.get("name") for t in model.get("tags") or [] if isinstance(t, dict)]
        categories = [c.get("name") for c in model.get("categories") or [] if isinstance(c, dict) and c.get("name")]
        description = model.get("description") or ""
        words = set(" ".join([description, *tags]).lower().replace("-", " ").split())
        attributes = {
            "format": fmt,
            "animated": bool(model.get("animationCount")),
            "vertex_count": archive.get("vertexCount") or model.get("vertexCount") or 0,
            "texture_count": archive.get("textureCount") or model.get("textureCount") or 0,
            "texture_max_resolution": archive.get("textureMaxResolution") or 0,
            "material_count": model.get("materialCount") or 0,
            "archive_bytes": archive.get("size") or 0,
            "pbr_type": model.get("pbrType") or "",
            "staff_picked": bool(model.get("staffpickedAt")),
            "likes": model.get("likeCount") or 0,
            "published": model.get("publishedAt") or "",
            "license_label": license_data.get("label") or "",
        }
        return AssetRecord(
            provider=NAME, id=uid, kind=MODEL, name=model.get("name") or uid,
            url=model.get("viewerUrl") or f"https://sketchfab.com/3d-models/{uid}",
            description=description, tags=[t for t in tags if t], categories=categories,
            authors=[author] if author else [],
            license=license_manager.normalize(license_data.get("label")),
            resolutions=[VERSION], attributes=attributes,
            method="scan" if words & SCAN_WORDS else "",
            popularity=float(model.get("downloadCount") or model.get("likeCount") or 0),
            polycount=int(archive.get("faceCount") or model.get("faceCount") or 0),
            thumbnail_url=thumb.get("url") or "",
        )

    # -- download ----------------------------------------------------------

    def _links(self, uid):
        token = os.environ.get(ENV_TOKEN, "").strip()
        if not token:
            raise ProviderUnavailable(f"Sketchfab downloads need an API token: set {ENV_TOKEN} (it comes from your "
                                      "Sketchfab account's password and API settings). Search works without one.")
        request = urllib.request.Request(f"{API}/models/{urllib.parse.quote(str(uid), safe='')}/download",
                                         headers={"User-Agent": dl.USER_AGENT, "Authorization": f"Token {token}"})
        try:
            with _opener.open(request, timeout=dl.TIMEOUT) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as exc:
            if exc.code in (401, 403):
                raise ProviderUnavailable(f"Sketchfab refused the token in {ENV_TOKEN} (HTTP {exc.code})") from None
            if exc.code == 429:
                raise ProviderUnavailable("Sketchfab is rate limiting downloads (HTTP 429)") from None
            raise PipelineError(f"Sketchfab can't give a download for {uid} (HTTP {exc.code})") from None
        except (urllib.error.URLError, TimeoutError, ConnectionError, ValueError) as exc:
            raise PipelineError(f"Could not reach Sketchfab for {uid}: {type(exc).__name__}") from None

    def download(self, record, resolution=VERSION, fmt=None):
        """Fetch the model's GLB (else its glTF zip); returns the registry download entry."""
        if self.offline:
            raise PipelineError("Offline: Sketchfab models need a download")
        links = self._links(record.id)
        folder = self.library.asset_dir(NAME, MODEL, record.id, VERSION)
        page = [record.url] if record.url else []
        link = (links.get("glb") or {}).get("url")
        if link:
            dest = folder / f"{record.id}.glb"
            self._fetch(link, dest, record.id)
            try:
                guard.check_model(dest)
            except PipelineError:
                dest.unlink(missing_ok=True)
                raise
            return VERSION, folder, [dest], {}, page
        link = (links.get("gltf") or {}).get("url")
        if not link:
            raise PipelineError(f"Sketchfab offered no glTF download for {record.id}")
        archive = self.library.cache_dir(NAME) / "archives" / f"{record.id}.zip"
        self._fetch(link, archive, record.id)
        try:
            files = guard.extract(archive, folder)
        finally:
            archive.unlink(missing_ok=True)
        if not any(f.suffix.lower() == ".gltf" for f in files):
            raise PipelineError(f"Sketchfab {record.id}: the archive held no glTF")
        maps = {}
        for path in files:
            kind = map_from_filename(path.name) if path.suffix.lower() in guard.IMAGE_EXTENSIONS else None
            if kind:
                maps.setdefault(kind, path)
        return VERSION, folder, files, maps, page

    @staticmethod
    def _fetch(link, dest, uid):
        try:
            guard.check_url(link)
            dl.download(link, dest, force=True)
        except PipelineError:
            # The message would hold the signed link.
            raise PipelineError(f"Sketchfab {uid}: the download failed or its link was refused") from None
