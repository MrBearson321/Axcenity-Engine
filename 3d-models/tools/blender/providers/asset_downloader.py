"""HTTP for the providers: cached JSON requests and verified file downloads.

No Blender needed. API responses are cached on disk (``cache/<provider>/``)
for ``ttl`` seconds and reused when the network is down, so searches work
offline with whatever was fetched before. Downloads go to a ``.part`` file,
are checked against the provider's size and MD5 when it gives them, and
then renamed into place, so a file in the library is always complete. A
file that is already there and matches isn't downloaded again.
"""

import hashlib
import json
import os
import shutil
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

from ..utils import PipelineError, log

USER_AGENT = "AxcenityAssetPipeline/1.0 (Blender asset toolkit)"
TIMEOUT = 60
RETRIES = 2

# Only these files are taken out of downloaded archives.
SAFE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".exr", ".hdr", ".tif", ".tiff", ".webp",
                   ".gltf", ".glb", ".bin", ".fbx", ".obj", ".mtl"}


def _open(url, timeout=TIMEOUT, headers=None):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, **(headers or {})})
    return urllib.request.urlopen(request, timeout=timeout)  # noqa: S310 (https URLs from provider APIs)


def get_text(url, cache_dir=None, ttl=86400, offline=False, headers=None):
    """GET a text document (an HTML page), cached like ``get_json``."""
    cache_file = None
    if cache_dir:
        cache_file = Path(cache_dir) / (hashlib.sha1(url.encode()).hexdigest()[:20] + ".html")
        if cache_file.exists() and (offline or time.time() - cache_file.stat().st_mtime < ttl):
            return cache_file.read_text(encoding="utf-8")
    if offline:
        raise PipelineError(f"Offline and nothing cached for {url}")
    error = None
    for attempt in range(RETRIES + 1):
        try:
            with _open(url, headers=headers) as response:
                text = response.read().decode("utf-8", "replace")
            break
        except (urllib.error.URLError, TimeoutError, ConnectionError) as exc:
            error = exc
            time.sleep(0.5 * (attempt + 1))
    else:
        if cache_file and cache_file.exists():
            log.warning("Request failed (%s); using cached copy of %s", error, url)
            return cache_file.read_text(encoding="utf-8")
        raise PipelineError(f"Could not fetch {url}: {error}")
    if cache_file:
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        tmp = cache_file.with_suffix(".tmp")
        tmp.write_text(text, encoding="utf-8", newline="\n")
        os.replace(tmp, cache_file)
    return text


def post_json(url, headers=None):
    """GET a small JSON answer without caching (signed, short-lived links)."""
    try:
        with _open(url, headers=headers) as response:
            return json.loads(response.read().decode("utf-8"))
    except (urllib.error.URLError, TimeoutError, ConnectionError, json.JSONDecodeError) as exc:
        raise PipelineError(f"Could not fetch {url}: {exc}") from exc


def get_json(url, cache_dir=None, ttl=86400, offline=False):
    """GET a JSON document, using the on-disk cache when it is fresh enough.

    With ``offline`` (or when the request fails) a cached copy of any age is
    used; with no copy at all a ``PipelineError`` is raised.
    """
    cache_file = None
    if cache_dir:
        cache_file = Path(cache_dir) / (hashlib.sha1(url.encode()).hexdigest()[:20] + ".json")
        if cache_file.exists() and (offline or time.time() - cache_file.stat().st_mtime < ttl):
            return _read_cached(cache_file)
    if offline:
        raise PipelineError(f"Offline and nothing cached for {url}")
    error = None
    for attempt in range(RETRIES + 1):
        try:
            with _open(url) as response:
                data = json.loads(response.read().decode("utf-8"))
            break
        except (urllib.error.URLError, TimeoutError, ConnectionError, json.JSONDecodeError) as exc:
            error = exc
            time.sleep(0.5 * (attempt + 1))
    else:
        if cache_file and cache_file.exists():
            log.warning("Request failed (%s); using cached copy of %s", error, url)
            return _read_cached(cache_file)
        raise PipelineError(f"Could not fetch {url}: {error}")
    if cache_file:
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        tmp = cache_file.with_suffix(".tmp")
        tmp.write_text(json.dumps({"url": url, "data": data}), encoding="utf-8", newline="\n")
        os.replace(tmp, cache_file)
    return data


def _read_cached(path):
    return json.loads(path.read_text(encoding="utf-8"))["data"]


def md5_of(path, chunk=1 << 20):
    digest = hashlib.md5()  # noqa: S324 (integrity check against the provider's MD5, not security)
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(chunk), b""):
            digest.update(block)
    return digest.hexdigest()


def is_complete(path, size=None, md5=None):
    """Whether ``path`` exists and matches the expected size and MD5."""
    path = Path(path)
    if not path.is_file():
        return False
    if size and path.stat().st_size != int(size):
        return False
    if md5 and md5_of(path) != md5:
        return False
    return True


def download(url, dest, size=None, md5=None, force=False):
    """Download ``url`` to ``dest`` unless a matching file is already there.

    Returns ``(path, downloaded)``; ``downloaded`` is False when the existing
    file was kept.
    """
    dest = Path(dest)
    if not force and is_complete(dest, size, md5):
        return dest, False
    if not urllib.parse.urlparse(url).scheme.startswith("http"):
        raise PipelineError(f"Refusing to download a non-HTTP URL: {url}")
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_name(dest.name + ".part")
    error = None
    for attempt in range(RETRIES + 1):
        try:
            with _open(url, timeout=TIMEOUT * 4) as response, open(part, "wb") as out:
                shutil.copyfileobj(response, out, 1 << 20)
            if size and part.stat().st_size != int(size):
                raise PipelineError(f"{dest.name}: got {part.stat().st_size} bytes, expected {size}")
            if md5 and md5_of(part) != md5:
                raise PipelineError(f"{dest.name}: MD5 doesn't match the provider's")
            os.replace(part, dest)
            log.info("Downloaded %s (%.1f MB)", dest.name, dest.stat().st_size / 1e6)
            return dest, True
        except (urllib.error.URLError, TimeoutError, ConnectionError, PipelineError) as exc:
            error = exc
            time.sleep(1.0 * (attempt + 1))
    part.unlink(missing_ok=True)
    raise PipelineError(f"Download failed for {url}: {error}")


def extract_zip(archive, dest, extensions=SAFE_EXTENSIONS, keep_dirs=False):
    """Extract an archive's image/model files into ``dest``, safely.

    Entries are written by their base name only (flat), or with
    ``keep_dirs`` under their folders with every ``..``, drive and absolute
    part removed, so nothing can escape ``dest``. Other file types are
    skipped.
    """
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    root = dest.resolve()
    written = []
    with zipfile.ZipFile(archive) as zf:
        for info in zf.infolist():
            if info.is_dir():
                continue
            parts = [p for p in info.filename.replace("\\", "/").split("/") if p not in ("", ".", "..")]
            parts = [p for p in parts if ":" not in p]
            if not parts or Path(parts[-1]).suffix.lower() not in extensions:
                continue
            target = dest.joinpath(*parts) if keep_dirs else dest / parts[-1]
            if root not in target.resolve().parents:
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with zf.open(info) as src, open(target, "wb") as out:
                shutil.copyfileobj(src, out, 1 << 20)
            written.append(target)
    return written
