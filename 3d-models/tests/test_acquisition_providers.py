"""New asset providers: Sketchfab, local folders, catalogues, and the guards on downloaded data (offline).

Set PIPELINE_LIVE_TESTS=1 to also search the real Sketchfab API (no token needed).
"""

import json
import os
import shutil
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from blender import providers
from blender.config import Config
from blender.providers import download_guard as guard
from blender.providers import sketchfab as sf
from blender.providers.asset_registry import AssetLibrary
from blender.providers.base import MODEL, TEXTURE, AssetRecord
from blender.providers.catalogue import Catalogue
from blender.providers.local_files import LocalFiles
from blender.providers.sketchfab import Sketchfab
from blender.utils import PipelineError

FIXTURES = Path(__file__).resolve().parent / "fixtures"
GLB = b"glTF\x02\x00\x00\x00" + b"\x00" * 32
TOKEN = "sekret-token-value"


class TempCase(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="pipeline_acq_"))
        self.lib = AssetLibrary(self.tmp / "lib")
        self.lib.ensure_layout()
        self.addCleanup(shutil.rmtree, self.tmp, True)


class FakeResponse:
    def __init__(self, payload):
        self.payload = payload

    def read(self):
        return json.dumps(self.payload).encode("utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False


class AcquisitionSketchfabParsingTests(unittest.TestCase):
    def setUp(self):
        self.data = json.loads((FIXTURES / "sketchfab_search.json").read_text(encoding="utf-8"))
        self.rec = Sketchfab.record_from_api(self.data["results"][0])

    def test_record_from_a_real_search_result(self):
        rec = self.rec
        self.assertEqual(rec.provider, "sketchfab")
        self.assertEqual(rec.kind, MODEL)
        self.assertEqual(rec.id, "94bf085f6b5344afb18e45eb8fca5e85")
        self.assertEqual(rec.license, "CC0")
        self.assertEqual(rec.polycount, 150000)
        self.assertIn("Digital Archive", rec.authors[0])
        self.assertIn("fruits", rec.tags)
        self.assertTrue(rec.thumbnail_url.startswith("https://media.sketchfab.com/"))
        self.assertEqual(rec.attributes["texture_count"], 1)
        self.assertEqual(rec.attributes["texture_max_resolution"], 2048)
        self.assertEqual(rec.attributes["format"], "glb")
        self.assertFalse(rec.attributes["animated"])
        self.assertEqual(rec.method, "scan")  # "Scanned with our DISC3D system"
        self.assertEqual(rec.resolutions, [sf.VERSION])

    def test_licence_comes_from_the_model(self):
        model = dict(self.data["results"][0], license={"label": "Standard"})
        self.assertEqual(Sketchfab.record_from_api(model).license, "SKETCHFAB-STANDARD")
        model = dict(self.data["results"][0], license=None)
        self.assertEqual(Sketchfab.record_from_api(model).license, "unknown")

    def test_detail_shape_is_read_too(self):
        model = dict(self.data["results"][0], archives=None, downloadCount=543, textureCount=3, faceCount=900,
                     viewerUrl="https://sketchfab.com/3d-models/x-1")
        rec = Sketchfab.record_from_api(model)
        self.assertEqual(rec.polycount, 900)
        self.assertEqual(rec.popularity, 543)
        self.assertEqual(rec.attributes["texture_count"], 3)
        self.assertEqual(rec.url, "https://sketchfab.com/3d-models/x-1")


class AcquisitionSketchfabTests(TempCase):
    def provider(self, **cfg):
        return Sketchfab(self.lib, cfg=Config(cfg))

    def test_search_asks_only_for_allowed_licences(self):
        calls = []

        def fake(url, *a, **k):
            calls.append(url)
            return {"results": [], "next": None}

        with mock.patch.object(sf.dl, "get_json", fake):
            self.provider().search("oak")
            self.assertEqual(len(calls), 1)
            self.assertIn("license=cc0", calls[0])
            self.assertIn("downloadable=true", calls[0])
            calls.clear()
            self.provider(acquisition={"licenses": {"allow_attribution": True}}).search("oak")
        self.assertEqual(len(calls), 2)  # the API takes one licence per request
        self.assertTrue(any("license=by" in c for c in calls))

    def test_search_follows_cursors_on_the_api_host_only(self):
        data = json.loads((FIXTURES / "sketchfab_search.json").read_text(encoding="utf-8"))
        first = dict(data, next="https://api.sketchfab.com/v3/search?cursor=2")
        evil = dict(data, next="https://evil.test/next")
        answers = [first, evil, {"results": [], "next": None}]
        with mock.patch.object(sf.dl, "get_json", side_effect=answers) as got:
            found = self.provider().search("oak", limit=10)
        self.assertEqual(got.call_count, 2)
        self.assertEqual(len(found), 1)  # the same model twice is one record

    def test_other_kinds_return_nothing(self):
        self.assertEqual(self.provider().search("oak", kind=TEXTURE), [])

    def test_download_without_a_token_says_so(self):
        rec = AssetRecord(provider="sketchfab", id="abc", kind=MODEL)
        env = {k: v for k, v in os.environ.items() if k != sf.ENV_TOKEN}
        with mock.patch.dict(os.environ, env, clear=True):
            with self.assertRaises(PipelineError) as caught:
                self.provider().download(rec)
        self.assertIn(sf.ENV_TOKEN, str(caught.exception))
        self.assertIsInstance(caught.exception, providers.ProviderUnavailable)

    def test_token_goes_only_in_a_header_of_the_api_request(self):
        rec = AssetRecord(provider="sketchfab", id="abc", kind=MODEL, name="Oak", url="https://sketchfab.com/x")
        seen = {}

        def fake_open(request, timeout=None):
            seen["url"] = request.full_url
            seen["auth"] = request.get_header("Authorization")
            return FakeResponse({"glb": {"url": "https://files.example.test/a.glb?sig=1", "size": 40}})

        def fake_download(url, dest, *a, **k):
            seen["download"] = (url, a, k)
            Path(dest).parent.mkdir(parents=True, exist_ok=True)
            Path(dest).write_bytes(GLB)
            return Path(dest), True

        with mock.patch.dict(os.environ, {sf.ENV_TOKEN: TOKEN}), \
                mock.patch.object(sf, "_opener") as opener, mock.patch.object(sf.dl, "download", fake_download):
            opener.open.side_effect = fake_open
            res, folder, files, maps, urls = self.provider().download(rec)
        self.assertEqual(seen["auth"], f"Token {TOKEN}")
        self.assertNotIn(TOKEN, seen["url"])
        self.assertNotIn(TOKEN, json.dumps(seen["download"], default=str))
        self.assertEqual(res, sf.VERSION)
        self.assertEqual(urls, ["https://sketchfab.com/x"])  # the signed link isn't kept
        self.assertEqual(files[0].suffix, ".glb")

    def test_errors_never_contain_the_token(self):
        import urllib.error

        rec = AssetRecord(provider="sketchfab", id="abc", kind=MODEL)
        err = urllib.error.HTTPError("https://api.sketchfab.com", 401, "Unauthorized", {}, None)
        with mock.patch.dict(os.environ, {sf.ENV_TOKEN: TOKEN}), mock.patch.object(sf, "_opener") as opener:
            opener.open.side_effect = err
            with self.assertRaises(providers.ProviderUnavailable) as caught:
                self.provider().download(rec)
        self.assertNotIn(TOKEN, str(caught.exception))

    def test_a_non_https_link_is_refused(self):
        rec = AssetRecord(provider="sketchfab", id="abc", kind=MODEL)
        with mock.patch.dict(os.environ, {sf.ENV_TOKEN: TOKEN}), mock.patch.object(sf, "_opener") as opener:
            opener.open.return_value = FakeResponse({"glb": {"url": "http://x.test/a.glb"}})
            with self.assertRaises(PipelineError):
                self.provider().download(rec)

    def test_a_download_that_is_not_a_glb_is_refused(self):
        rec = AssetRecord(provider="sketchfab", id="abc", kind=MODEL)

        def fake_download(url, dest, *a, **k):
            Path(dest).parent.mkdir(parents=True, exist_ok=True)
            Path(dest).write_bytes(b"<html>nope</html>")
            return Path(dest), True

        with mock.patch.dict(os.environ, {sf.ENV_TOKEN: TOKEN}), mock.patch.object(sf, "_opener") as opener, \
                mock.patch.object(sf.dl, "download", fake_download):
            opener.open.return_value = FakeResponse({"glb": {"url": "https://x.test/a.glb"}})
            with self.assertRaises(PipelineError):
                self.provider().download(rec)
        self.assertEqual(list(self.lib.asset_dir("sketchfab", MODEL, "abc").rglob("*.glb")), [])

    @unittest.skipUnless(os.environ.get("PIPELINE_LIVE_TESTS"), "live network test")
    def test_live_search(self):
        found = self.provider().search("oak tree", limit=5)
        self.assertTrue(found)
        for rec in found:
            self.assertEqual(rec.license, "CC0")
            self.assertTrue(rec.attributes.get("format"))


class AcquisitionLocalFilesTests(TempCase):
    def setUp(self):
        super().setUp()
        self.root = self.tmp / "my_models"
        (self.root / "trees").mkdir(parents=True)
        (self.root / "trees" / "oak_tree.glb").write_bytes(GLB)
        (self.root / "trees" / "pine_tree.glb").write_bytes(GLB)
        (self.root / "trees" / "pine_tree.license.json").write_text(json.dumps(
            {"license": "CC0", "authors": ["Me"], "url": "https://example.test/pine", "tags": ["conifer"]}),
            encoding="utf-8")
        (self.root / "trees" / "secret.blend").write_bytes(b"BLENDER")
        self.cfg = Config({"library": {"local_directories": [str(self.root)]}})

    def provider(self):
        return LocalFiles(self.lib, cfg=self.cfg)

    def test_indexes_models_but_never_blend_files(self):
        found = self.provider().search("", MODEL)
        names = sorted(r.name for r in found)
        self.assertEqual(names, ["oak tree", "pine tree"])

    def test_licence_from_sidecar_else_unknown(self):
        found = {r.name: r for r in self.provider().search("tree", MODEL)}
        self.assertEqual(found["pine tree"].license, "CC0")
        self.assertEqual(found["pine tree"].authors, ["Me"])
        self.assertIn("conifer", found["pine tree"].tags)
        self.assertEqual(found["oak tree"].license, "unknown")

    def test_folder_wide_sidecar(self):
        (self.root / "license.json").write_text('{"license": "by"}', encoding="utf-8")
        found = {r.name: r for r in self.provider().search("oak", MODEL)}
        self.assertEqual(found["oak tree"].license, "CC-BY")

    def test_broken_sidecar_means_unknown(self):
        (self.root / "trees" / "oak_tree.license.json").write_text("{not json", encoding="utf-8")
        found = {r.name: r for r in self.provider().search("oak", MODEL)}
        self.assertEqual(found["oak tree"].license, "unknown")

    def test_search_needs_a_shared_word(self):
        self.assertEqual(self.provider().search("spaceship", MODEL), [])

    def test_download_points_at_the_files_in_place(self):
        rec = self.provider().search("oak", MODEL)[0]
        res, folder, files, maps, urls = self.provider().download(rec)
        self.assertEqual(files, [(self.root / "trees" / "oak_tree.glb").resolve()])
        self.assertEqual(list(self.lib.root.glob("models/local/**/*.glb")), [])
        self.lib.upsert(rec)
        self.lib.add_download(rec.key, res, folder, files, maps, urls)
        self.assertEqual(self.lib.downloaded_resolutions(rec.key), [res])

    def test_gltf_companions_are_included_and_escapes_are_not(self):
        folder = self.root / "barrel"
        folder.mkdir()
        (folder / "tex.png").write_bytes(b"png")
        (folder / "b.bin").write_bytes(b"bin")
        (self.tmp / "outside.png").write_bytes(b"png")
        (folder / "barrel.gltf").write_text(json.dumps({
            "buffers": [{"uri": "b.bin"}, {"uri": "data:application/octet-stream;base64,AA=="}],
            "images": [{"uri": "tex.png"}, {"uri": "../../outside.png"}]}), encoding="utf-8")
        rec = self.provider().search("barrel", MODEL)[0]
        _, _, files, _, _ = self.provider().download(rec)
        self.assertEqual(sorted(f.name for f in files), ["b.bin", "barrel.gltf", "tex.png"])

    def test_no_directories_is_fine(self):
        self.assertEqual(LocalFiles(self.lib, cfg=Config({})).search("oak", MODEL), [])


class AcquisitionCatalogueTests(TempCase):
    def setUp(self):
        super().setUp()
        self.file = self.tmp / "repo.json"
        self.write([
            {"id": "oak", "name": "Oak", "kind": "model", "url": "https://example.test/oak.zip",
             "source_page": "https://example.test/oak", "license": "CC0", "authors": ["A"], "tags": ["tree"],
             "polycount": 5000, "format": "glb", "dimensions_m": [5, 5, 9]},
            {"id": "bad", "name": "Evil tree", "kind": "model", "url": "https://example.test/evil.blend",
             "source_page": "https://example.test/evil", "license": "CC0"},
            {"id": "plain", "name": "Http tree", "kind": "model", "url": "http://example.test/a.glb",
             "source_page": "https://example.test/a", "license": "CC0"},
            {"id": "nolic", "name": "Mystery tree", "kind": "model", "url": "https://example.test/m.glb",
             "source_page": "https://example.test/m"},
            {"id": "tex", "name": "Tree bark", "kind": "texture", "url": "https://example.test/bark_diff.png",
             "source_page": "https://example.test/bark", "license": "CC0"},
        ])
        self.cfg = Config({"library": {"catalogues": [str(self.file)]}})

    def write(self, assets):
        self.file.write_text(json.dumps({"format": "asset-catalogue", "version": 1, "source": "Repo", "assets": assets}),
                             encoding="utf-8")

    def provider(self):
        return Catalogue(self.lib, cfg=self.cfg)

    def test_search_normalises_and_skips_unsafe_entries(self):
        found = {r.id: r for r in self.provider().search("tree", MODEL)}
        self.assertEqual(sorted(found), ["nolic", "oak"])
        self.assertEqual(found["oak"].license, "CC0")
        self.assertEqual(found["oak"].url, "https://example.test/oak")
        self.assertEqual(found["oak"].polycount, 5000)
        self.assertEqual(found["oak"].physical_size, [5, 5, 9])
        self.assertEqual(found["nolic"].license, "unknown")

    def test_kind_filter(self):
        self.assertEqual([r.id for r in self.provider().search("bark", TEXTURE)], ["tex"])

    def test_info(self):
        self.assertEqual(self.provider().info("oak").name, "Oak")
        with self.assertRaises(PipelineError):
            self.provider().info("nope")

    def test_download_unpacks_a_zip_safely(self):
        def fake_download(url, dest, *a, **k):
            Path(dest).parent.mkdir(parents=True, exist_ok=True)
            with zipfile.ZipFile(dest, "w") as zf:
                zf.writestr("model/oak.glb", GLB)
                zf.writestr("model/evil.blend", b"BLENDER")
                zf.writestr("../../escape.glb", GLB)
            return Path(dest), True

        rec = self.provider().info("oak")
        with mock.patch("blender.providers.catalogue.dl.download", fake_download):
            res, folder, files, _, urls = self.provider().download(rec)
        names = sorted(f.name for f in files)
        self.assertEqual(names, ["escape.glb", "oak.glb"])
        for f in files:
            self.assertIn(self.lib.root, f.resolve().parents)
        self.assertFalse(list(self.lib.root.rglob("*.blend")))
        self.assertEqual(urls, ["https://example.test/oak.zip"])


class AcquisitionGuardTests(TempCase):
    def test_refused_urls(self):
        guard.check_url("https://x.test/a.glb")
        for url in ("https://x.test/a.blend", "https://x.test/a.BLEND?x=1", "http://x.test/a.glb",
                    "file:///c:/a.glb", "https://x.test/run.exe"):
            with self.assertRaises(PipelineError, msg=url):
                guard.check_url(url)

    def test_zip_size_limit(self):
        archive = self.tmp / "a.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
            zf.writestr("a.png", b"\x00" * 10000)
        with self.assertRaises(PipelineError):
            guard.extract(archive, self.tmp / "out", max_bytes=100)

    def test_not_a_zip(self):
        archive = self.tmp / "a.zip"
        archive.write_bytes(b"nope")
        with self.assertRaises(PipelineError):
            guard.extract(archive, self.tmp / "out")

    def test_glb_magic(self):
        good, bad = self.tmp / "a.glb", self.tmp / "b.glb"
        good.write_bytes(GLB)
        bad.write_bytes(b"not a glb")
        guard.check_model(good)
        with self.assertRaises(PipelineError):
            guard.check_model(bad)

    def test_gltf_with_escaping_uri_is_refused(self):
        path = self.tmp / "a.gltf"
        for uri in ("../x.bin", "/etc/passwd", "C:/x.bin", "https://evil.test/x.bin", "file:///x"):
            path.write_text(json.dumps({"buffers": [{"uri": uri}]}), encoding="utf-8")
            with self.assertRaises(PipelineError, msg=uri):
                guard.check_model(path)
        path.write_text(json.dumps({"buffers": [{"uri": "x.bin"}], "images": [{"uri": "t%20a.png"}]}), encoding="utf-8")
        guard.check_model(path)
        self.assertEqual(guard.gltf_uris(path), ["x.bin", "t a.png"])


class AcquisitionRegistryTests(TempCase):
    def test_new_providers_are_registered_with_capabilities(self):
        for name in ("sketchfab", "local", "catalog"):
            self.assertIn(name, providers.PROVIDERS)
        caps = providers.capabilities("sketchfab")
        self.assertEqual(caps["kinds"], (MODEL,))
        self.assertIn("glb", caps["formats"])
        self.assertTrue(caps["licenses_per_asset"])
        self.assertEqual(providers.capabilities("polyhaven")["licenses"], ("CC0",))

    def test_existing_defaults_are_unchanged(self):
        self.assertEqual(sorted(providers.enabled_providers(MODEL, Config({}))), ["kenney", "polyhaven"])
        self.assertEqual(providers.enabled_providers(MODEL, Config({"library": {"providers": ["sketchfab"]}})),
                         ["sketchfab"])

    def test_get_provider_hands_config_to_those_that_want_it(self):
        cfg = Config({"library": {"local_directories": ["somewhere"]}})
        self.assertIs(providers.get_provider("local", self.lib, cfg).cfg, cfg)
        self.assertEqual(providers.get_provider("polyhaven", self.lib, cfg).name, "polyhaven")


if __name__ == "__main__":
    unittest.main()
