"""Tests for the axcenity_toolbox package. Standard library only:

    python -m unittest discover -s crates/toolbox/python/tests

The command-line tests use a stand-in script; the database tests use the
real ``axcenity-toolbox`` when it's built (``AXCENITY_TOOLBOX``, else
``target/debug``), and are skipped otherwise.
"""

from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import axcenity_toolbox as tb  # noqa: E402

REPO = HERE.parents[3]


def real_binary():
    named = os.environ.get("AXCENITY_TOOLBOX")
    if named and Path(named).is_file():
        return named
    for name in ("axcenity-toolbox", "axcenity-toolbox.exe"):
        path = REPO / "target" / "debug" / name
        if path.is_file():
            return str(path)
    return None


def fake_binary(folder: Path, exit_code: int = 0, stdout: str = "", stderr: str = "") -> str:
    """A stand-in command that records its arguments and answers as told."""
    script = folder / "fake_toolbox.py"
    script.write_text(textwrap.dedent(f"""
        import json, sys
        with open({str(folder / 'args.json')!r}, 'w') as f:
            json.dump(sys.argv[1:], f)
        sys.stdout.write({stdout!r})
        sys.stderr.write({stderr!r})
        sys.exit({exit_code})
    """))
    if os.name == "nt":
        launcher = folder / "fake_toolbox.cmd"
        launcher.write_text(f'@"{sys.executable}" "{script}" %*\n')
    else:
        launcher = folder / "fake_toolbox"
        launcher.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{script}" "$@"\n')
        launcher.chmod(0o755)
    return str(launcher)


class CommandLine(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.folder = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def args(self):
        return json.loads((self.folder / "args.json").read_text())

    def test_search_passes_its_options_and_reads_records(self):
        records = [{"provider": "polyhaven", "id": "red_brick_03", "kind": "texture"}]
        box = tb.Toolbox(binary=fake_binary(self.folder, stdout=json.dumps(records)),
                         asset_dir="D:/Assets")
        self.assertEqual(box.search("red brick", provider="polyhaven", limit=5), records)
        self.assertEqual(self.args(), ["search", "red brick", "--kind", "texture", "--limit", "5",
                                       "--provider", "polyhaven", "--dir", "D:/Assets", "--json"])

    def test_fetch_returns_the_download_with_full_paths(self):
        answer = {"key": "ambientcg:Rock030", "variant": "2K-JPG",
                  "folder": "textures/ambientcg/Rock030/2K-JPG",
                  "path": str(self.folder / "lib" / "textures" / "ambientcg" / "Rock030" / "2K-JPG"),
                  "files": [{"path": "Rock030_2K-JPG_Color.jpg", "size": 3}],
                  "bytes": 3, "notes": ["No PNG here, so JPG."]}
        box = tb.Toolbox(binary=fake_binary(self.folder, stdout=json.dumps(answer)), offline=True)
        download = box.fetch("ambientcg:Rock030", res="2k", fmt="png")
        self.assertEqual(download.variant, "2K-JPG")
        self.assertEqual(download.notes, ["No PNG here, so JPG."])
        self.assertEqual(download.file_paths()[0].name, "Rock030_2K-JPG_Color.jpg")
        self.assertIn("--offline", self.args())

    def test_exit_codes_become_errors(self):
        box = tb.Toolbox(binary=fake_binary(self.folder, 3, stderr="every source failed\n"))
        with self.assertRaises(tb.SourceUnavailable):
            box.search("x")
        box = tb.Toolbox(binary=fake_binary(self.folder, 2, stderr="axcenity-toolbox: nope\n"))
        with self.assertRaisesRegex(tb.ToolboxError, "nope"):
            box.info("polyhaven:x")

    def test_a_missing_command_is_reported(self):
        old = os.environ.get("AXCENITY_TOOLBOX")
        os.environ["AXCENITY_TOOLBOX"] = str(self.folder / "missing")
        try:
            self.assertIsNone(tb.find_binary())
            with self.assertRaises(tb.ToolboxError):
                tb.Toolbox()
        finally:
            if old is None:
                del os.environ["AXCENITY_TOOLBOX"]
            else:
                os.environ["AXCENITY_TOOLBOX"] = old


class MapRoles(unittest.TestCase):
    def test_roles_come_from_ambientcg_and_poly_haven_names(self):
        download = tb.Download(key="k", variant="v", folder="f", path=Path("/lib"), files=[
            {"path": "Asphalt026C_2K-JPG_Color.jpg"},
            {"path": "Asphalt026C_2K-JPG_NormalDX.jpg"},
            {"path": "Asphalt026C_2K-JPG_NormalGL.jpg"},
            {"path": "Asphalt026C_2K-JPG_AmbientOcclusion.jpg"},
            {"path": "red_brick_03_rough_2k.jpg"},
            {"path": "red_brick_03_disp_2k.png"},
        ])
        roles = tb.map_roles(download)
        self.assertEqual(roles["base_color"].name, "Asphalt026C_2K-JPG_Color.jpg")
        self.assertEqual(roles["normal"].name, "Asphalt026C_2K-JPG_NormalGL.jpg")
        self.assertEqual(roles["normal_dx"].name, "Asphalt026C_2K-JPG_NormalDX.jpg")
        self.assertEqual(roles["ao"].name, "Asphalt026C_2K-JPG_AmbientOcclusion.jpg")
        self.assertEqual(roles["roughness"].name, "red_brick_03_rough_2k.jpg")
        self.assertEqual(roles["height"].name, "red_brick_03_disp_2k.png")


@unittest.skipUnless(real_binary(), "axcenity-toolbox isn't built")
class RealDatabase(unittest.TestCase):
    """A database the Rust command line wrote, read here."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        old = root / "asset_library"
        maps = old / "textures" / "ambientcg" / "Rock030" / "2k"
        maps.mkdir(parents=True)
        (maps / "Rock030_2K-JPG_Color.jpg").write_bytes(b"colour")
        (maps / "Rock030_2K-JPG_NormalGL.jpg").write_bytes(b"nrm")
        (old / "metadata").mkdir()
        (old / "metadata" / "registry.json").write_text(json.dumps({"version": 1, "assets": {
            "ambientcg:Rock030": {
                "record": {"provider": "ambientcg", "id": "Rock030", "kind": "texture",
                           "name": "Rock 030", "license": "CC0", "license_version": "1.0",
                           "authors": ["ambientCG"]},
                "downloads": {"2k": {"folder": "textures/ambientcg/Rock030/2k", "bytes": 9,
                                     "files": ["textures/ambientcg/Rock030/2k/Rock030_2K-JPG_Color.jpg",
                                               "textures/ambientcg/Rock030/2k/Rock030_2K-JPG_NormalGL.jpg"]}}},
        }}))
        self.dir = root / "Assets"
        self.box = tb.Toolbox(binary=real_binary(), asset_dir=str(self.dir))
        self.report = self.box.migrate(str(old))

    def tearDown(self):
        self.tmp.cleanup()

    def test_a_migrated_library_reads_back_in_python(self):
        self.assertEqual(len(self.report["copied"]), 1)
        with tb.Library(str(self.dir)) as library:
            record = library.record("ambientcg:Rock030")
            self.assertEqual(record["name"], "Rock 030")
            self.assertEqual([r["id"] for r in library.records(kind="texture", downloaded=True)],
                             ["Rock030"])
            self.assertEqual(library.licence("ambientcg:Rock030")["license"], "CC0")
            download = library.downloads("ambientcg:Rock030")[0]
            self.assertEqual(download.bytes, 9)
            self.assertTrue(all(path.is_file() for path in download.file_paths()))
            maps = library.maps("ambientcg:Rock030")
            self.assertEqual(sorted(maps), ["base_color", "normal"])
        self.assertIn("Rock 030", self.box.credits())
        self.assertEqual(self.box.stats()["downloads"], 1)

    def test_a_newer_schema_is_refused(self):
        conn = sqlite3.connect(self.dir / "metadata.db")
        conn.execute("INSERT INTO schema_version (version) VALUES (99)")
        conn.commit()
        conn.close()
        with self.assertRaises(tb.ToolboxError):
            tb.Library(str(self.dir))


if __name__ == "__main__":
    unittest.main()
