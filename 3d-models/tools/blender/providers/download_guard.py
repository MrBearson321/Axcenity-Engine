"""Checks on downloaded data, which is untrusted.

Archives are unpacked through ``asset_downloader.extract_zip`` (image and
model files only, nothing outside the target folder) after a limit on their
unpacked size. ``.blend`` files are never taken from a third party because
they can carry scripts. A glTF may only point at files beside it.
"""

import json
import urllib.parse
import zipfile
from pathlib import Path, PurePosixPath

from ..utils import PipelineError
from . import asset_downloader as dl

MODEL_EXTENSIONS = (".glb", ".gltf", ".fbx", ".obj")
IMAGE_EXTENSIONS = (".png", ".jpg", ".jpeg", ".tga", ".bmp", ".exr", ".hdr", ".tif", ".tiff", ".webp")
ARCHIVE_EXTENSIONS = (".zip",)
EXTRACTED = dl.SAFE_EXTENSIONS | {".tga", ".bmp"}
# Never fetched, whatever a listing says.
REFUSED_EXTENSIONS = (".blend", ".blend1", ".py", ".pyc", ".exe", ".bat", ".cmd", ".ps1", ".sh", ".dll", ".so",
                      ".dylib", ".msi", ".jar", ".scr")
MAX_UNPACKED_BYTES = 4 * 1024 ** 3
MAX_ENTRIES = 5000


def url_suffix(url):
    return PurePosixPath(urllib.parse.urlparse(url).path).suffix.lower()


def check_url(url):
    """Refuse anything but an https link to a file type we're willing to hold."""
    if urllib.parse.urlparse(str(url)).scheme != "https":
        raise PipelineError(f"Refusing a link that isn't https: {url}")
    if url_suffix(url) in REFUSED_EXTENSIONS:
        raise PipelineError(f"Refusing to download a {url_suffix(url)} file: {url}")
    return url


def extract(archive, dest, max_bytes=MAX_UNPACKED_BYTES, max_entries=MAX_ENTRIES):
    """Unpack the image and model files of an archive into ``dest``, folders kept."""
    try:
        with zipfile.ZipFile(archive) as zf:
            infos = zf.infolist()
    except (zipfile.BadZipFile, OSError) as exc:
        raise PipelineError(f"{Path(archive).name} isn't a readable zip: {exc}") from exc
    if len(infos) > max_entries:
        raise PipelineError(f"{Path(archive).name} holds {len(infos)} entries (limit {max_entries})")
    unpacked = sum(i.file_size for i in infos)
    if unpacked > max_bytes:
        raise PipelineError(f"{Path(archive).name} would unpack to {unpacked} bytes (limit {max_bytes})")
    files = dl.extract_zip(archive, dest, EXTRACTED, keep_dirs=True)
    for path in files:
        if path.suffix.lower() in (".glb", ".gltf"):
            check_model(path)
    return files


def unsafe_path(uri):
    """Whether a relative reference inside a model would leave the model's folder."""
    text = str(uri).replace("\\", "/")
    parts = PurePosixPath(text).parts
    return not parts or "://" in text or text.startswith("/") or ":" in parts[0] or ".." in parts


def _read_gltf(path):
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise PipelineError(f"{Path(path).name} isn't valid glTF JSON: {exc}") from exc
    if not isinstance(data, dict):
        raise PipelineError(f"{Path(path).name} isn't valid glTF JSON")
    return data


def gltf_uris(path, skip_unsafe=False):
    """The files a ``.gltf`` names (decoded). An unsafe one is an error, or left out with ``skip_unsafe``."""
    data = _read_gltf(path)
    out = []
    for group in ("buffers", "images"):
        for item in data.get(group) or []:
            uri = str(item.get("uri") or "") if isinstance(item, dict) else ""
            if not uri or uri.startswith("data:"):
                continue
            decoded = urllib.parse.unquote(uri)
            if unsafe_path(decoded):
                if skip_unsafe:
                    continue
                raise PipelineError(f"{Path(path).name} points outside its folder: {uri}")
            out.append(decoded)
    return out


def check_model(path):
    """Refuse a file that isn't the model its extension claims, or a glTF that reaches outside its folder."""
    path = Path(path)
    suffix = path.suffix.lower()
    if suffix in REFUSED_EXTENSIONS:
        raise PipelineError(f"Refusing {path.name}: {suffix} files aren't accepted from third parties")
    if suffix == ".glb":
        with open(path, "rb") as handle:
            if handle.read(4) != b"glTF":
                raise PipelineError(f"{path.name} isn't a binary glTF file")
    elif suffix == ".gltf":
        gltf_uris(path)
    return path
