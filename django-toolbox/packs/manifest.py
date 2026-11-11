"""The pack export: the Option A manifest the engine loads.

build_manifest() is the one builder. The pack page's download and the API's
export both call it, so they always return the same document. It reads only
the Pack, its PackItems and their stored Assets, so nothing private from a
member's library entry (notes, rating, status) can ever reach it, and it
works with every provider down. The shape is locked by a golden file
(tests/golden/pack_manifest.json).
"""

from collections.abc import Mapping
from datetime import UTC, datetime
from typing import Any

from django.utils import timezone

from catalog.models import Asset

from .models import Pack, PackItem, default_variant

MANIFEST_FORMAT = "axcenity-pack"
MANIFEST_VERSION = 1


def is_cc0(license_name: str) -> bool:
    """True for a CC0 license ("CC0", "CC0 1.0"…), which needs no credit."""
    return license_name.strip().upper().startswith("CC0")


def credit_line(asset: Asset) -> str:
    """The asset's attribution, or a plain one built from its title, author and license."""
    if asset.attribution.strip():
        return asset.attribution.strip()
    author = asset.author.strip() or "an unknown author"
    return f'"{asset.title}" by {author}, {asset.license}'


def chosen_variant(item: PackItem) -> Mapping[str, Any] | None:
    """The item's variant, found by its stored label in the Asset's variants.

    A label the Asset no longer lists falls back to the default variant rule;
    an asset with no variants has none.
    """
    variants = item.asset.variants or []
    by_label = {str(variant.get("label", "")): variant for variant in variants}
    if item.preferred_variant in by_label:
        return by_label[item.preferred_variant]
    fallback = default_variant(item.asset.kind, variants)
    return by_label.get(fallback) if variants else None


def manifest_item(item: PackItem) -> dict:
    """One item of the manifest: the asset, its chosen variant's files and its import path."""
    asset = item.asset
    variant = chosen_variant(item)
    files = [
        {"url": file.get("url"), "filename": file.get("filename"), "size": file.get("size")}
        for file in (variant or {}).get("files", [])
    ]
    return {
        "name": asset.title,
        "kind": asset.kind,
        "provider": asset.provider,
        "external_id": asset.external_id,
        "variant": variant.get("label") if variant else None,
        "files": files,
        "total_size": variant.get("total_size") if variant else None,
        "license": asset.license,
        "license_url": asset.license_url,
        "author": asset.author,
        "attribution": asset.attribution,
        "source_url": asset.source_url,
        "import_path": item.import_path or item.default_import_path,
    }


def build_manifest(pack: Pack, *, source_url: str, exported_at: datetime | None = None) -> dict:
    """The pack's export manifest (PRD, "Acceptance criteria: export manifest").

    `source_url` is the absolute URL of the pack's page on the Toolbox.
    `exported_at` defaults to now; tests pass a fixed time. Items come in
    position order, and `credits` holds every non-CC0 item's credit once, in
    item order.
    """
    moment = (exported_at or timezone.now()).astimezone(UTC).replace(microsecond=0)
    items = list(pack.ordered_items())
    credits: list[str] = []
    for item in items:
        if not is_cc0(item.asset.license):
            line = credit_line(item.asset)
            if line not in credits:
                credits.append(line)
    return {
        "format": MANIFEST_FORMAT,
        "version": MANIFEST_VERSION,
        "exported_at": moment.isoformat(),
        "pack": {
            "title": pack.title,
            "slug": pack.slug,
            "author": pack.owner.get_username(),
            "description": pack.description,
            "tags": list(pack.tags.order_by("name").values_list("name", flat=True)),
            "source_url": source_url,
        },
        "items": [manifest_item(item) for item in items],
        "credits": credits,
    }
