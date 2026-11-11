"""Search, download and inspect the shared asset library (no Blender needed).

    python tools/library.py search "red brick" [--kind texture|hdri|model] [--provider polyhaven] [--limit 10]
    python tools/library.py fetch polyhaven:red_brick_03 [--res 2k] [--kind texture]
    python tools/library.py list [--kind texture] [--query brick] [--downloaded]
    python tools/library.py info ambientcg:Bricks090
    python tools/library.py refresh ambientcg:Bricks090      metadata only, no downloads
    python tools/library.py credits [--out CREDITS.md] [--keys a:b,c:d]
    python tools/library.py stats
    python tools/library.py masks [--use stain|speckle|scratch|breakup|...] [--force]
        fetch the Kenney sprite packs if missing, sort their images into surface
        masks (or say why one is refused) and list what each use can pick from
    python tools/library.py source fetch [--force]
        CC0 / public-domain reference photos from Wikimedia Commons
    python tools/library.py source variants
        crops and grades of those photos and of every packs/*/reference/views.json

The library folder comes from $PIPELINE_ASSET_LIBRARY or ``library.directory``
in the nearest pipeline.config.json above the current folder (``--config``
picks one). Downloads are skipped when the asset is already in the library.
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from blender import providers  # noqa: E402
from blender.config import load_config  # noqa: E402
from blender.providers import license_manager  # noqa: E402
from blender.utils import PipelineError  # noqa: E402


def _args(argv=None):
    p = argparse.ArgumentParser(prog="library.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("command", choices=["search", "fetch", "list", "info", "refresh", "credits", "stats", "masks", "source"])
    p.add_argument("target", nargs="?", default="", help="query, provider:id, or source fetch|variants")
    p.add_argument("--kind", choices=list(providers.KINDS), help="search defaults to texture")
    p.add_argument("--provider")
    p.add_argument("--res")
    p.add_argument("--limit", type=int, default=15)
    p.add_argument("--query", default="")
    p.add_argument("--downloaded", action="store_true")
    p.add_argument("--keys")
    p.add_argument("--out")
    p.add_argument("--config")
    p.add_argument("--offline", action="store_true")
    p.add_argument("--use")
    p.add_argument("--force", action="store_true")
    return p.parse_args(argv)


def _row(rec):
    size = f"{rec.physical_size[0]:g}x{rec.physical_size[1]:g} m" if rec.physical_size else "size ?"
    return (f"{rec.key:<44} {rec.name[:30]:<30} maps={','.join(rec.maps) or '?':<40} "
            f"res<={rec.resolutions[-1] if rec.resolutions else '?':<4} {size:<12} dl={int(rec.popularity)}")


def main(argv=None):
    args = _args(argv)
    cfg = load_config([args.config] if args.config else None, start=Path.cwd())
    if args.offline:
        cfg.data.setdefault("library", {})["offline"] = True
    lib = providers.open_library(cfg)
    if args.command == "search":
        recs = providers.search(args.target, args.kind or "texture", [args.provider] if args.provider else None,
                                args.limit, cfg=cfg, library=lib)
        for rec in recs[: args.limit * 2]:
            print(_row(rec))
        print(f"{len(recs)} result(s)")
    elif args.command == "fetch":
        record, info = providers.fetch(args.target, args.res, args.kind, cfg=cfg, library=lib)
        print(f"{record.key} at {info['folder']}: {', '.join(sorted(info['maps'])) or 'files'} "
              f"({info['bytes'] / 1e6:.1f} MB)")
    elif args.command == "list":
        entries = lib.search(args.query, args.kind,
                             downloaded=True if args.downloaded else None)
        for entry in entries:
            rec = providers.AssetRecord.from_dict(entry["record"])
            print(f"{_row(rec)}  have={','.join(lib.downloaded_resolutions(rec.key)) or '-'}")
        print(f"{len(entries)} asset(s) in {lib.root}")
    elif args.command == "info":
        entry = lib.get(args.target)
        print(json.dumps(entry or providers.resolve(args.target, cfg=cfg, library=lib).to_dict(), indent=1))
    elif args.command == "refresh":
        print(_row(providers.refresh(args.target, cfg=cfg, library=lib)))
    elif args.command == "credits":
        keys = args.keys.split(",") if args.keys else [k for k in lib.keys() if lib.downloaded_resolutions(k)]
        records = [lib.get(k)["record"] for k in keys if lib.get(k)]
        text = license_manager.credits(records)
        if args.out:
            print(f"Wrote {license_manager.write_credits(records, args.out)}")
        else:
            print(text)
    elif args.command == "masks":
        from blender import mask_library
        from blender.providers import kenney

        for slug in kenney.CATALOG:
            key = f"kenney:{slug}"
            if not lib.downloaded_resolutions(key):
                providers.fetch(key, None, providers.SPRITES, cfg=cfg, library=lib)
        mask_library.analyse_all(lib, args.force, cfg)
        print(mask_library.report(lib, cfg))
        if args.use:
            for c in mask_library.catalog(args.use, lib, cfg)[: args.limit]:
                print(f"  {c['score']:.2f} {c['ref']}")
    elif args.command == "stats":
        print(json.dumps({"root": str(lib.root), **lib.stats()}, indent=1))
    elif args.command == "source":
        from blender import source_imagery

        root = Path(__file__).resolve().parent.parent
        offline = bool(cfg.get("library.offline"))
        if args.target == "fetch":
            lines = source_imagery.fetch(root, force=args.force, offline=offline)
        elif args.target == "variants":
            lines = source_imagery.variants(root)
        else:
            raise PipelineError("source needs fetch or variants")
        for line in lines:
            print(line)
        print(f"{len(lines)} file(s)")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except PipelineError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(2)
