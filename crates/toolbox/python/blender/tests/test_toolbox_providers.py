"""The package-backed ambientCG and Poly Haven providers (``providers/toolbox_backed.py``).

Offline: a stand-in Toolbox answers. Needs the ``axcenity_toolbox`` package
(set AXCENITY_TOOLBOX_PYTHON to the engine's ``crates/toolbox/python``). The
real searches and downloads are LiveProviderTests in test_providers.py, which
go through these classes once ``providers/__init__.py`` wraps them.
"""

import os
import shutil
import tempfile
import unittest
from unittest import mock
from pathlib import Path

from blender import providers
from blender.providers import base as pb
from blender.providers import toolbox_backed as tb
from blender.providers.ambientcg import AmbientCG
from blender.providers.asset_registry import AssetLibrary
from blender.providers.polyhaven import PolyHaven

PACKAGE = tb._package()

BRICK = {
    "provider": "polyhaven", "id": "red_brick_03", "kind": "texture", "name": "Red Brick 03",
    "url": "https://polyhaven.com/a/red_brick_03", "tags": ["brick", "wall"], "categories": ["brick"],
    "authors": ["Rob Tuytel"], "license": "CC0", "license_version": "1.0",
    "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
    "resolutions": ["1k", "2k"], "maps": ["base_color", "normal", "roughness"],
    "physical_size": [1.0, 1.0], "polycount": 0, "popularity": 1200.0,
    "thumbnail_url": "https://cdn.polyhaven.com/asset_img/thumbs/red_brick_03.png",
    "variants": [{"label": "1K JPG", "resolution": "1k", "format": "jpg", "package": "maps", "files": []}],
}


class FakeToolbox:
    """Answers like ``axcenity_toolbox.Toolbox`` without the command line."""

    def __init__(self, root, rows=(), unavailable=False):
        self.root, self.rows, self.unavailable, self.calls = Path(root), list(rows), unavailable, []

    def search(self, words, kind="texture", provider=None, limit=15):
        self.calls.append(("search", words, kind, provider, limit))
        if self.unavailable:
            raise PACKAGE.SourceUnavailable("Poly Haven can't be reached")
        return [r for r in self.rows if r["kind"] == kind and r["provider"] == provider][:limit]

    def info(self, key):
        self.calls.append(("info", key))
        provider, asset_id = key.split(":", 1)
        for row in self.rows:
            if (row["provider"], row["id"]) == (provider, asset_id):
                return row
        raise PACKAGE.ToolboxError("that asset wasn't found")

    def fetch(self, key, res="2k", fmt=None):
        self.calls.append(("fetch", key, res, fmt))
        folder = f"textures/{key.replace(':', '/')}/{res.upper()}-JPG"
        names = ["red_brick_03_diff_1k.jpg", "red_brick_03_nor_gl_1k.jpg", "red_brick_03_rough_1k.jpg"]
        path = self.root.joinpath(*folder.split("/"))
        path.mkdir(parents=True, exist_ok=True)
        for name in names:
            (path / name).write_bytes(b"jpg")
        return PACKAGE.Download.from_json({
            "key": key, "variant": f"{res.upper()} JPG", "folder": folder, "path": str(path),
            "files": [{"path": n, "size": 3} for n in names], "bytes": 9,
            "source_urls": [f"https://dl.polyhaven.org/{n}" for n in names],
        })


@unittest.skipIf(PACKAGE is None, "set AXCENITY_TOOLBOX_PYTHON to the engine's crates/toolbox/python")
class ToolboxBackedTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="pipeline_toolbox_"))
        self.lib = AssetLibrary(self.tmp / "library")
        self.lib.ensure_layout()
        self.central = self.tmp / "central"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def provider(self, cls=PolyHaven, **fake):
        provider = tb.backed(cls)(self.lib)
        provider._tb = FakeToolbox(self.central, **fake)
        return provider

    def test_a_toolbox_record_reads_as_the_pipelines_record(self):
        rec = tb.record_from_toolbox(BRICK)
        self.assertEqual(rec.key, "polyhaven:red_brick_03")
        self.assertEqual(rec.kind, pb.TEXTURE)
        self.assertEqual(rec.license, "CC0")
        self.assertEqual(rec.physical_size, [1.0, 1.0])
        self.assertIn(pb.NORMAL, rec.maps)
        self.assertEqual(rec.extra["license_version"], "1.0")
        again = pb.AssetRecord.from_dict(rec.to_dict())
        self.assertEqual(again, rec)

    def test_resolutions_come_from_the_variants_when_the_record_has_none(self):
        rec = tb.record_from_toolbox({**BRICK, "resolutions": [], "variants": [
            {"label": "4K JPG", "resolution": "4k"}, {"label": "1K JPG", "resolution": "1k"}]})
        self.assertEqual(rec.resolutions, ["1k", "4k"])

    def test_search_asks_the_toolbox_for_this_provider_only(self):
        provider = self.provider(rows=[BRICK])
        found = provider.search("brick", kind=pb.TEXTURE, limit=5)
        self.assertEqual([r.id for r in found], ["red_brick_03"])
        self.assertEqual(provider._tb.calls, [("search", "brick", pb.TEXTURE, "polyhaven", 5)])
        self.assertEqual(provider.search("brick", kind=pb.TEXTURE, categories=["wood"]), [])

    def test_a_kind_the_source_lacks_is_no_results_without_a_request(self):
        provider = self.provider(cls=AmbientCG, rows=[BRICK])
        self.assertEqual(provider.search("crate", kind=pb.MODEL), [])
        self.assertEqual(provider._tb.calls, [])

    def test_an_unreachable_source_is_provider_unavailable(self):
        provider = self.provider(unavailable=True)
        with self.assertRaises(pb.ProviderUnavailable):
            provider.search("brick", kind=pb.TEXTURE)

    def test_a_missing_asset_is_a_pipeline_error(self):
        with self.assertRaises(providers.PipelineError):
            self.provider().info("nope")

    def test_a_download_lands_in_the_asset_directory_and_registers_there(self):
        provider = self.provider(rows=[BRICK])
        rec = provider.info("red_brick_03")
        res, folder, files, maps, urls = provider.download(rec, "1k")
        self.assertEqual(res, "1k")
        self.assertTrue(str(folder).startswith(str(self.central)))
        self.assertEqual(maps[pb.BASE_COLOR].name, "red_brick_03_diff_1k.jpg")
        self.assertEqual(maps[pb.NORMAL].name, "red_brick_03_nor_gl_1k.jpg")
        self.assertEqual(len(urls), 3)
        self.lib.upsert(rec)
        self.lib.add_download(rec.key, res, folder, files, maps, urls)
        self.assertTrue(self.lib.has_download(rec.key, "1k"))
        self.assertEqual(self.lib.maps(rec.key, "1k")[pb.ROUGHNESS], maps[pb.ROUGHNESS])

    def test_without_the_toolbox_the_python_provider_answers(self):
        provider = tb.backed(PolyHaven)(self.lib, offline=True)
        provider._tb = None
        calls = []
        with mock.patch.object(PolyHaven, "search", lambda self, *a, **k: calls.append(a) or []):
            self.assertEqual(provider.search("brick", kind=pb.TEXTURE), [])
        self.assertEqual(calls, [("brick",)])

    def test_switching_it_off_uses_the_python_provider(self):
        os.environ[tb.ENV_SWITCH] = "0"
        try:
            self.assertIsNone(tb.toolbox())
        finally:
            os.environ.pop(tb.ENV_SWITCH)

    def test_the_registry_uses_the_backed_classes(self):
        self.assertTrue(issubclass(providers.PROVIDERS["polyhaven"], tb.ToolboxBacked))
        self.assertTrue(issubclass(providers.PROVIDERS["ambientcg"], tb.ToolboxBacked))
        self.assertIs(providers.PROVIDERS["polyhaven"].python, PolyHaven)


if __name__ == "__main__":
    unittest.main()
