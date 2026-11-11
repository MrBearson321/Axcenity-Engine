"""Licences, attribution and terms for downloaded assets.

Each provider declares its licence (``LICENSES``). Every asset in the
library records the licence, authors and source URL it came with, and
``credits`` / ``write_credits`` turn the assets a pack uses into a credits
file to ship with it. ambientCG and Poly Haven publish under CC0, which
needs no attribution, but crediting authors is good practice and the
registry keeps what is needed to do it.

Sources with per-asset licences (Sketchfab, local folders, catalogues) are
checked one asset at a time with ``verify``: only CC0 and public domain pass
by default, CC-BY only when the config allows it and the asset names someone
to credit, and an unknown licence never passes on its own.
"""

import re
from dataclasses import asdict, dataclass
from datetime import date
from pathlib import Path

LICENSES = {
    "CC0": {
        "name": "CC0 1.0 Universal (public domain dedication)",
        "url": "https://creativecommons.org/publicdomain/zero/1.0/",
        "attribution_required": False,
        "commercial_use": True,
    },
    "PD": {
        "name": "Public domain",
        "url": "https://creativecommons.org/publicdomain/mark/1.0/",
        "attribution_required": False,
        "commercial_use": True,
    },
    "CC-BY": {
        "name": "Creative Commons Attribution 4.0",
        "url": "https://creativecommons.org/licenses/by/4.0/",
        "attribution_required": True,
        "commercial_use": True,
    },
}

# Licences we can name but don't count as safe to ship: ``problems`` flags them. NC can't be
# sold, ND can't be modified, SA binds the derivative, and the Sketchfab store licences carry
# restrictions of their own.
UNKNOWN = "unknown"
RESTRICTED_LICENSES = {
    "CC-BY-SA": {"name": "Creative Commons Attribution-ShareAlike 4.0", "url": "https://creativecommons.org/licenses/by-sa/4.0/"},
    "CC-BY-ND": {"name": "Creative Commons Attribution-NoDerivs 4.0", "url": "https://creativecommons.org/licenses/by-nd/4.0/"},
    "CC-BY-NC": {"name": "Creative Commons Attribution-NonCommercial 4.0", "url": "https://creativecommons.org/licenses/by-nc/4.0/"},
    "CC-BY-NC-SA": {"name": "Creative Commons Attribution-NonCommercial-ShareAlike 4.0", "url": "https://creativecommons.org/licenses/by-nc-sa/4.0/"},
    "CC-BY-NC-ND": {"name": "Creative Commons Attribution-NonCommercial-NoDerivs 4.0", "url": "https://creativecommons.org/licenses/by-nc-nd/4.0/"},
    "SKETCHFAB-STANDARD": {"name": "Sketchfab Standard", "url": "https://sketchfab.com/licenses"},
    "SKETCHFAB-FREE-STANDARD": {"name": "Sketchfab Free Standard", "url": "https://sketchfab.com/licenses"},
    "SKETCHFAB-EDITORIAL": {"name": "Sketchfab Editorial", "url": "https://sketchfab.com/licenses"},
}

# ``acquisition.licenses`` in the config. ``allowed``: passes outright. ``allow_attribution``
# adds ``attribution_licenses``, which also need an author to credit.
DEFAULT_POLICY = {"allowed": ["CC0", "PD"], "allow_attribution": False, "attribution_licenses": ["CC-BY"]}

PROVIDER_TERMS = {
    "ambientcg": {
        "license": "CC0",
        "site": "https://ambientcg.com",
        "credit": "ambientCG (ambientcg.com)",
        "notes": "Assets are CC0. The API is public; keep request volume modest.",
    },
    "kenney": {
        "license": "CC0",
        "site": "https://kenney.nl",
        "credit": "Kenney (kenney.nl)",
        "notes": ("Assets are CC0 (each pack page states it). There is no API: packs come through "
                  "the site's own download link, one request per pack, cached in the library."),
    },
    "polyhaven": {
        "license": "CC0",
        "site": "https://polyhaven.com",
        "credit": "Poly Haven (polyhaven.com)",
        "notes": ("Assets are CC0. The API asks for a unique User-Agent and is free for "
                  "non-commercial tools; a commercial product built on the API itself should "
                  "contact Poly Haven (https://polyhaven.com/our-api). Using downloaded assets "
                  "in a game is fine either way."),
    },
    "sketchfab": {
        "site": "https://sketchfab.com",
        "credit": "Sketchfab (sketchfab.com)",
        "notes": ("Each model has its own licence, read from its metadata. Search is public; "
                  "downloads need the user's API token (SKETCHFAB_API_TOKEN)."),
    },
    "local": {
        "site": "",
        "credit": "local files",
        "notes": "Licence from a sidecar file next to the model; without one it is unknown.",
    },
    "catalog": {
        "site": "",
        "credit": "open asset catalogue",
        "notes": "Each catalogue entry states its own licence and source page.",
    },
}


def license_info(code):
    if code in RESTRICTED_LICENSES:
        return {"attribution_required": code.startswith("CC-"), "commercial_use": None, **RESTRICTED_LICENSES[code]}
    return LICENSES.get(code, {"name": code or "unknown", "url": "", "attribution_required": True,
                               "commercial_use": None})


_SKETCHFAB_LICENSES = {"standard": "SKETCHFAB-STANDARD", "st": "SKETCHFAB-STANDARD",
                       "free standard": "SKETCHFAB-FREE-STANDARD", "free-st": "SKETCHFAB-FREE-STANDARD",
                       "editorial": "SKETCHFAB-EDITORIAL", "ed": "SKETCHFAB-EDITORIAL"}


def normalize(text):
    """A licence code from any way of writing it ("CC Attribution", "by", "CC0 1.0"), else ``unknown``."""
    label = re.sub(r"\s+", " ", str(text or "").strip().lower())
    if label in _SKETCHFAB_LICENSES:
        return _SKETCHFAB_LICENSES[label]
    if label.upper() in LICENSES or label.upper() in RESTRICTED_LICENSES:
        return label.upper()
    if re.search(r"\bcc0\b|\bcc[ -]zero\b|publicdomain/zero", label):
        return "CC0"
    if label in ("pd", "public domain", "public-domain", "publicdomain"):
        return "PD"
    words = re.findall(r"[a-z0-9]+", label)
    slug = re.fullmatch(r"by(-nc)?(-sa|-nd)?", label)
    if not (slug or "cc" in words or ("creative" in words and "commons" in words)):
        return UNKNOWN
    if not (slug or "by" in words or "attribution" in words):
        return UNKNOWN
    code = "CC-BY"
    if "nc" in words or "noncommercial" in words:
        code += "-NC"
    if "sa" in words or "sharealike" in words:
        code += "-SA"
    elif "nd" in words or "noderivs" in words or "noderivatives" in words:
        code += "-ND"
    return code


def policy_from_config(cfg):
    """The licence policy: ``DEFAULT_POLICY`` under ``acquisition.licenses``."""
    return {**DEFAULT_POLICY, **(cfg.get("acquisition.licenses", {}) or {})}


@dataclass
class Verdict:
    """Whether one asset's licence lets it be used, and what that use requires."""

    accepted: bool
    code: str
    reason: str
    attribution_required: bool = False
    attribution: str = ""  # the credit line, when one is owed
    needs_review: bool = False  # nobody knows the licence: a person has to look

    def to_dict(self):
        return asdict(self)


def verify(record, policy=None, confirmed=None):
    """Check one asset's own licence against ``policy`` (default: CC0 and public domain).

    ``confirmed`` is a licence a person states for this asset (a per-asset
    override); it is the only way an unknown licence can pass.
    """
    data = record.to_dict() if hasattr(record, "to_dict") else dict(record)
    policy = {**DEFAULT_POLICY, **(policy or {})}
    code = normalize(confirmed or data.get("license"))
    note = f" (confirmed by the user as {code})" if confirmed else ""
    if code == UNKNOWN:
        return Verdict(False, code, "licence unknown: needs a person to confirm it", needs_review=True)
    info = license_info(code)
    attribution = attribution_line({**data, "license": code})
    if code in policy["allowed"]:
        owed = bool(info.get("attribution_required"))
        return Verdict(True, code, f"{code} is allowed{note}", owed, attribution if owed else "")
    if policy["allow_attribution"] and code in policy["attribution_licenses"]:
        if not data.get("authors"):
            return Verdict(False, code, f"{code} needs attribution but the asset names no author")
        return Verdict(True, code, f"{code} is allowed with attribution{note}", True, attribution)
    if code in policy["attribution_licenses"]:
        return Verdict(False, code, f"{code} needs acquisition.licenses.allow_attribution in the config")
    return Verdict(False, code, f"{info['name']} isn't an allowed licence")


def attribution_line(record):
    """One credit line: "Red Brick 03 by Rob Tuytel, Poly Haven (polyhaven.com), CC0"."""
    terms = PROVIDER_TERMS.get(record.get("provider"), {})
    authors = ", ".join(record.get("authors") or []) or "unknown author"
    name = record.get("name") or record.get("id")
    return f"{name} by {authors}, {terms.get('credit', record.get('provider'))}, {record.get('license') or 'licence unknown'}"


def problems(record):
    """Licence problems that should stop an asset being shipped."""
    found = []
    code = record.get("license")
    if not code:
        found.append("no licence recorded")
    elif code not in LICENSES:
        found.append(f"licence {code} isn't one this toolkit knows is safe to ship")
    elif LICENSES[code]["attribution_required"] and not record.get("authors"):
        found.append("attribution required but no author recorded")
    if not record.get("url"):
        found.append("no source URL recorded")
    return found


def credits(records):
    """Markdown credits for the given registry records."""
    lines = ["# Third-party assets", "",
             f"Generated {date.today().isoformat()} from the asset library registry.", ""]
    by_provider = {}
    for rec in sorted(records, key=lambda r: (r.get("provider", ""), r.get("name") or r.get("id"))):
        by_provider.setdefault(rec.get("provider", "unknown"), []).append(rec)
    for provider, recs in by_provider.items():
        terms = PROVIDER_TERMS.get(provider, {})
        per_asset = provider in PROVIDER_TERMS and "license" not in terms
        lic = license_info(terms.get("license", ""))
        heading = ("Licence: per asset, below" if per_asset else
                   f"Licence: [{lic['name']}]({lic['url']})" if lic.get("url") else f"Licence: {lic['name']}")
        lines += [f"## {terms.get('credit', provider)}", "", heading, ""]
        for rec in recs:
            own = f" ({rec.get('license') or 'licence unknown'})" if per_asset else ""
            lines.append(f"- [{rec.get('name') or rec.get('id')}]({rec.get('url')}): "
                         f"{', '.join(rec.get('authors') or []) or 'unknown author'}{own}")
        lines.append("")
    return "\n".join(lines)


def write_credits(records, path):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(credits(records), encoding="utf-8", newline="\n")
    return path
