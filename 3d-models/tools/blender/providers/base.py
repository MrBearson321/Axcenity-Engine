"""The common asset record every provider normalises its metadata into.

Providers describe assets differently (ambientCG: one zip per resolution and
format, sizes in centimetres; Poly Haven: one file per map, sizes in
millimetres, separate texture/HDRI/model APIs). ``AssetRecord`` is the one
shape the rest of the toolkit reads, so a new provider only has to fill it in.

The provider contract (``providers.register_provider`` adds a class)::

    class MySource:
        name = "mysource"            # the "provider" in "provider:id" keys
        kinds = (MODEL,)             # what it really has: never assume every provider has every kind
        formats = ("glb", "gltf")    # optional: files it delivers
        licenses = ("CC0",)          # optional: None when each asset has its own
        opt_in = True                # optional: left out of searches unless a config names it
        wants_config = True          # optional: the constructor also gets ``cfg=``

        def __init__(self, library, offline=False, ttl=86400): ...
        def search(self, query="", kind=MODEL, limit=30, categories=()): ...   # -> [AssetRecord]
        def info(self, asset_id, kind=None): ...                               # -> AssetRecord
        def download(self, record, resolution, fmt=None): ...
            # -> (resolution, folder, files, {map: path}, source_urls)

``search`` returns ``[]`` for a kind it doesn't have. Failures are
``PipelineError`` (or ``ProviderUnavailable`` when the whole source can't
deliver right now, such as a missing token). Downloaded data is untrusted:
see ``download_guard``.
"""

import re
from dataclasses import asdict, dataclass, field

from ..utils import PipelineError

# Asset kinds. ``sprites`` are 2D image packs (patterns, particles, noise)
# that the toolkit mines for masks; they aren't PBR sets.
TEXTURE, HDRI, MODEL, SPRITES = "texture", "hdri", "model", "sprites"
KINDS = (TEXTURE, HDRI, MODEL, SPRITES)


class ProviderUnavailable(PipelineError):
    """A source can't deliver anything just now (no token, rate limited): don't try its other assets."""


# Normalised PBR map names.
BASE_COLOR, NORMAL, NORMAL_DX, ROUGHNESS, METALLIC = "base_color", "normal", "normal_dx", "roughness", "metallic"
AO, HEIGHT, OPACITY, EMISSION, ARM = "ao", "height", "opacity", "emission", "arm"
MAPS = (BASE_COLOR, NORMAL, NORMAL_DX, ROUGHNESS, METALLIC, AO, HEIGHT, OPACITY, EMISSION, ARM)

# Words in file names (lowercase) and the map they hold. "nor_gl" style
# entries span two words.
MAP_WORDS = {
    "ambientocclusion": AO, "basecolor": BASE_COLOR, "displacement": HEIGHT, "roughness": ROUGHNESS,
    "metalness": METALLIC, "metallic": METALLIC, "normalgl": NORMAL, "normaldx": NORMAL_DX,
    "nor_gl": NORMAL, "nor_dx": NORMAL_DX, "opacity": OPACITY, "emission": EMISSION, "albedo": BASE_COLOR,
    "diffuse": BASE_COLOR, "height": HEIGHT, "normal": NORMAL, "rough": ROUGHNESS, "color": BASE_COLOR,
    "alpha": OPACITY, "metal": METALLIC, "disp": HEIGHT, "diff": BASE_COLOR, "arm": ARM, "ao": AO,
}


def map_from_filename(name):
    """The map a texture file holds, guessed from its name (``None`` if unknown).

    Words are read from the end, where map names sit, so the asset's own name
    (``metal_plate_diff_1k``) doesn't count.
    """
    stem = name.rsplit(".", 1)[0].lower()
    words = [w for w in re.split(r"[^a-z0-9]+", stem) if w]
    for i in range(len(words) - 1, -1, -1):
        if i and f"{words[i - 1]}_{words[i]}" in MAP_WORDS:
            return MAP_WORDS[f"{words[i - 1]}_{words[i]}"]
        if words[i] in MAP_WORDS:
            return MAP_WORDS[words[i]]
    return None


def resolution_px(token):
    """"2k" -> 2048, "512" -> 512 (``0`` when it can't be read)."""
    token = str(token).lower().strip()
    m = re.match(r"^(\d+)\s*k$", token)
    if m:
        return int(m.group(1)) * 1024
    return int(token) if token.isdigit() else 0


def resolution_token(px):
    """2048 -> "2k" (sizes under 1k stay numbers: "512")."""
    px = int(px)
    return f"{px // 1024}k" if px >= 1024 and px % 1024 == 0 else str(px)


def pick_resolution(available, wanted):
    """The largest available resolution not above ``wanted`` (else the smallest)."""
    sizes = sorted({resolution_px(a): a for a in available if resolution_px(a)}.items())
    if not sizes:
        return None
    want = resolution_px(wanted) if not isinstance(wanted, int) else wanted
    fitting = [tok for px, tok in sizes if px <= want]
    return fitting[-1] if fitting else sizes[0][1]


@dataclass
class AssetRecord:
    """One provider asset, described the same way whatever its source."""

    provider: str
    id: str
    kind: str
    name: str = ""
    url: str = ""  # the asset's page on the provider's site
    description: str = ""
    tags: list = field(default_factory=list)
    categories: list = field(default_factory=list)
    authors: list = field(default_factory=list)
    license: str = ""
    resolutions: list = field(default_factory=list)  # tokens: "1k", "2k", ...
    maps: list = field(default_factory=list)  # normalised map names
    physical_size: list = None  # [width, height(, depth)] in metres, when known
    attributes: dict = field(default_factory=dict)  # provider facts: surface use, setting, condition...
    method: str = ""  # how it was made: photogrammetry, procedural, ...
    popularity: float = 0.0  # downloads (provider's own count)
    polycount: int = 0  # models only
    thumbnail_url: str = ""
    extra: dict = field(default_factory=dict)  # provider-specific bits needed to download

    @property
    def key(self):
        return f"{self.provider}:{self.id}"

    def words(self):
        """Every lowercase word that describes the asset (for matching)."""
        text = " ".join([self.id, self.name, self.description, *self.tags, *self.categories,
                         *(str(v) for v in self.attributes.values())])
        return set(re.findall(r"[a-z]+", text.lower().replace("_", " ")))

    def to_dict(self):
        return asdict(self)

    @classmethod
    def from_dict(cls, data):
        known = cls.__dataclass_fields__
        return cls(**{k: v for k, v in data.items() if k in known})


def parse_key(key, default_provider=None):
    """"polyhaven:red_brick_03" -> ("polyhaven", "red_brick_03")."""
    if ":" in key:
        provider, asset_id = key.split(":", 1)
        return provider.strip().lower(), asset_id.strip()
    if default_provider:
        return default_provider, key.strip()
    raise ValueError(f"Asset key {key!r} needs a provider: provider:id")
