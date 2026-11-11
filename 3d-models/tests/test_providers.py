"""Provider parsing, the asset registry and downloads (offline, from recorded API responses).

Set PIPELINE_LIVE_TESTS=1 to also run LiveProviderTests, which search and
download real assets from ambientCG and Poly Haven into a temporary library.
"""

import json
import os
import shutil
import tempfile
import unittest
import zipfile
from pathlib import Path

from blender import providers, procedural_textures
from blender.providers import asset_downloader as dl
from blender.providers import base as pb
from blender.providers import license_manager
from blender.providers.ambientcg import AmbientCG
from blender.providers.asset_registry import AssetLibrary
from blender.providers.polyhaven import PolyHaven

FIXTURES = Path(__file__).resolve().parent / "fixtures"


class TempLibraryCase(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="pipeline_lib_"))
        self.lib = AssetLibrary(self.tmp)
        self.lib.ensure_layout()

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)


class MapNameTests(unittest.TestCase):
    def test_map_names_are_read_from_the_end(self):
        cases = {
            "Bricks051_1K-JPG_Color.jpg": pb.BASE_COLOR,
            "Bricks051_1K-JPG_NormalGL.jpg": pb.NORMAL,
            "Bricks051_1K-JPG_NormalDX.jpg": pb.NORMAL_DX,
            "Bricks051_1K-JPG_AmbientOcclusion.jpg": pb.AO,
            "Bricks051_1K-JPG_Displacement.jpg": pb.HEIGHT,
            "red_brick_03_nor_gl_1k.jpg": pb.NORMAL,
            "red_brick_03_nor_dx_1k.jpg": pb.NORMAL_DX,
            "metal_plate_diff_1k.jpg": pb.BASE_COLOR,  # "metal" in the asset name doesn't count
            "metal_plate_rough_2k.png": pb.ROUGHNESS,
            "wooden_crate_01_arm_1k.jpg": pb.ARM,
            "Bricks051.png": None,
        }
        for name, expected in cases.items():
            self.assertEqual(pb.map_from_filename(name), expected, name)

    def test_resolutions(self):
        self.assertEqual(pb.resolution_px("2k"), 2048)
        self.assertEqual(pb.resolution_token(512), "512")
        self.assertEqual(pb.resolution_token(4096), "4k")
        self.assertEqual(pb.pick_resolution(["1k", "2k", "4k"], "3k"), "2k")
        self.assertEqual(pb.pick_resolution(["2k", "4k"], "1k"), "2k")  # nothing small enough: smallest


class ParsingTests(unittest.TestCase):
    def test_ambientcg_record(self):
        data = json.loads((FIXTURES / "ambientcg_search.json").read_text(encoding="utf-8"))
        rec = AmbientCG.record_from_api(data["foundAssets"][0])
        self.assertEqual(rec.provider, "ambientcg")
        self.assertEqual(rec.kind, pb.TEXTURE)
        self.assertEqual(rec.license, "CC0")
        self.assertIn(pb.BASE_COLOR, rec.maps)
        self.assertIn(pb.NORMAL, rec.maps)
        self.assertIn("1k", rec.resolutions)
        self.assertAlmostEqual(rec.physical_size[0], 1.1)  # 110 cm
        self.assertIn("1K-JPG", rec.extra["downloads"])

    def test_polyhaven_record(self):
        data = json.loads((FIXTURES / "polyhaven_red_brick_03.json").read_text(encoding="utf-8"))
        rec = PolyHaven.record_from_api("red_brick_03", data["info"], pb.TEXTURE)
        self.assertEqual(rec.physical_size, [1.0, 1.0])  # 1000 mm
        self.assertEqual(rec.url, "https://polyhaven.com/a/red_brick_03")
        self.assertTrue(rec.authors)
        self.assertIn("8k", rec.resolutions)
        self.assertIn("brick", rec.words())

    def test_record_round_trip(self):
        data = json.loads((FIXTURES / "polyhaven_red_brick_03.json").read_text(encoding="utf-8"))
        rec = PolyHaven.record_from_api("red_brick_03", data["info"], pb.TEXTURE)
        again = pb.AssetRecord.from_dict(json.loads(json.dumps(rec.to_dict())))
        self.assertEqual(again, rec)
        self.assertEqual(pb.parse_key("polyhaven:red_brick_03"), ("polyhaven", "red_brick_03"))


class RegistryTests(TempLibraryCase):
    def _record(self, asset_id="brick_a", **kw):
        return pb.AssetRecord(provider="polyhaven", id=asset_id, kind=pb.TEXTURE, name=asset_id.replace("_", " "),
                              url=f"https://polyhaven.com/a/{asset_id}", tags=["brick", "wall"], license="CC0",
                              authors=["Someone"], resolutions=["1k", "2k"], maps=[pb.BASE_COLOR], **kw)

    def _download(self, rec, res="1k"):
        folder = self.lib.asset_dir(rec.provider, rec.kind, rec.id, res)
        folder.mkdir(parents=True)
        f = folder / f"{rec.id}_diff_{res}.jpg"
        f.write_bytes(b"jpg")
        self.lib.upsert(rec)
        return self.lib.add_download(rec.key, res, folder, [f], {pb.BASE_COLOR: f}, ["https://example/x.jpg"])

    def test_downloads_are_tracked_and_survive_a_reload(self):
        rec = self._record()
        self._download(rec)
        again = AssetLibrary(self.tmp)
        self.assertTrue(again.has_download(rec.key, "1k"))
        self.assertFalse(again.has_download(rec.key, "2k"))
        self.assertEqual(again.maps(rec.key, "1k")[pb.BASE_COLOR].name, "brick_a_diff_1k.jpg")
        self.assertTrue((self.tmp / "metadata" / "polyhaven" / "brick_a.json").exists())
        info = again.download_info(rec.key, "1k")
        self.assertEqual(info["resolution"], "1k")
        self.assertFalse(Path(info["folder"]).is_absolute())  # paths are relative: the library can move

    def test_deleted_files_mean_not_downloaded(self):
        rec = self._record()
        info = self._download(rec)
        (self.tmp / info["files"][0]).unlink()
        self.assertFalse(self.lib.has_download(rec.key, "1k"))

    def test_refreshing_metadata_keeps_downloads_and_known_fields(self):
        rec = self._record(physical_size=[1.0, 1.0])
        self._download(rec)
        sparse = pb.AssetRecord(provider="polyhaven", id="brick_a", kind=pb.TEXTURE, name="Brick A", popularity=99)
        self.lib.upsert(sparse)
        entry = self.lib.get(rec.key)
        self.assertEqual(entry["record"]["physical_size"], [1.0, 1.0])
        self.assertEqual(entry["record"]["popularity"], 99)
        self.assertIn("1k", entry["downloads"])

    def test_offline_search(self):
        self.lib.upsert(self._record("red_brick"))
        self.lib.upsert(self._record("plank_floor", ))
        self.lib.get("polyhaven:plank_floor")["record"]["tags"] = ["wood", "planks"]
        self.assertEqual([e["record"]["id"] for e in self.lib.search("red brick")], ["red_brick"])
        self.assertEqual(len(self.lib.search("")), 2)

    def test_credits_and_licence_checks(self):
        rec = self._record()
        self.lib.upsert(rec)
        text = license_manager.credits([self.lib.get(rec.key)["record"]])
        self.assertIn("CC0", text)
        self.assertIn("Someone", text)
        self.assertEqual(license_manager.problems(self.lib.get(rec.key)["record"]), [])
        self.assertTrue(license_manager.problems({"provider": "x", "license": "GPL"}))


class DownloadTests(TempLibraryCase):
    def test_existing_complete_files_are_not_downloaded(self):
        f = self.tmp / "a.bin"
        f.write_bytes(b"hello")
        md5 = dl.md5_of(f)
        path, downloaded = dl.download("https://invalid.invalid/a.bin", f, size=5, md5=md5)
        self.assertFalse(downloaded)  # no request was made: the host doesn't exist
        self.assertFalse(dl.is_complete(f, size=6))

    def test_cached_json_is_used_offline(self):
        url = "https://api.example.invalid/assets"
        cache = self.tmp / "cache"
        cache.mkdir(exist_ok=True)
        import hashlib

        (cache / (hashlib.sha1(url.encode()).hexdigest()[:20] + ".json")).write_text(
            json.dumps({"url": url, "data": {"x": 1}}), encoding="utf-8")
        self.assertEqual(dl.get_json(url, cache, ttl=0, offline=True), {"x": 1})

    def test_zip_extraction_cannot_escape(self):
        archive = self.tmp / "evil.zip"
        with zipfile.ZipFile(archive, "w") as zf:
            zf.writestr("../../escape_Color.jpg", b"x")
            zf.writestr("/abs/Thing_NormalGL.png", b"y")
            zf.writestr("run.py", b"print('no')")
        out = self.tmp / "out"
        files = dl.extract_zip(archive, out)
        self.assertEqual(sorted(f.name for f in files), ["Thing_NormalGL.png", "escape_Color.jpg"])
        self.assertTrue(all(f.parent == out for f in files))
        self.assertFalse((self.tmp.parent / "escape_Color.jpg").exists())


class MaskTests(unittest.TestCase):
    def test_masks_are_seeded_and_tile(self):
        import numpy as np

        for name in procedural_textures.MASKS:
            a = procedural_textures.generate(name, seed=3, size=128)
            b = procedural_textures.generate(name, seed=3, size=128)
            c = procedural_textures.generate(name, seed=4, size=128)
            self.assertTrue(np.array_equal(a, b), name)
            self.assertFalse(np.array_equal(a, c), name)
            self.assertTrue(0.0 <= a.min() and a.max() <= 1.0)
            # Wrapping around the edge is no bigger a jump than between neighbours inside.
            inner = np.abs(np.diff(a, axis=1)).mean()
            wrap = np.abs(a[:, 0] - a[:, -1]).mean()
            self.assertLess(wrap, inner * 3 + 0.02, name)

    def test_mask_png_is_cached(self):
        tmp = Path(tempfile.mkdtemp())
        try:
            p = procedural_textures.mask_path(tmp, "grime", 1, 64)
            stamp = p.stat().st_mtime
            self.assertEqual(procedural_textures.mask_path(tmp, "grime", 1, 64), p)
            self.assertEqual(p.stat().st_mtime, stamp)
            self.assertEqual(p.read_bytes()[:8], b"\x89PNG\r\n\x1a\n")
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


@unittest.skipUnless(os.environ.get("PIPELINE_LIVE_TESTS"), "set PIPELINE_LIVE_TESTS=1 to hit the real APIs")
class LiveProviderTests(TempLibraryCase):
    """Real searches and downloads (small 1k assets) into a temporary library."""

    def setUp(self):
        super().setUp()
        os.environ[providers.ENV_LIBRARY] = str(self.tmp)

    def tearDown(self):
        os.environ.pop(providers.ENV_LIBRARY, None)
        super().tearDown()

    def test_search_both_providers(self):
        recs = providers.search("brick", pb.TEXTURE, limit=5)
        self.assertEqual({r.provider for r in recs}, {"ambientcg", "polyhaven"})

    def test_fetch_each_provider_once(self):
        for key in ("ambientcg:Bricks051", "polyhaven:red_brick_03"):
            record, info = providers.fetch(key, "1k")
            self.assertIn(pb.BASE_COLOR, info["maps"])
            self.assertIn(pb.NORMAL, info["maps"])
            again = providers.open_library()
            files = [again.abs(f) for f in info["files"]]
            stamps = [f.stat().st_mtime for f in files]
            providers.fetch(key, "1k")  # already there: nothing downloaded again
            self.assertEqual(stamps, [f.stat().st_mtime for f in files])

    def test_fetch_hdri_and_model(self):
        _rec, info = providers.fetch("polyhaven:studio_small_09", "1k", pb.HDRI)
        self.assertIn("environment", info["maps"])
        _rec, info = providers.fetch("polyhaven:wooden_crate_01", "1k", pb.MODEL)
        self.assertTrue(any(f.endswith(".gltf") for f in info["files"]))
