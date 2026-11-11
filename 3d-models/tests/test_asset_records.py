"""Asset records built from Blender objects (the General Store kit)."""

import tempfile
from pathlib import Path

import bmesh
import bpy

import blender as pipeline
from blender import asset_index, asset_metadata as am, asset_records as ar
from helpers import PACK, KitTestCase


class RecordTests(KitTestCase):
    def setUp(self):
        super().setUp()
        self.tmp = Path(tempfile.mkdtemp(prefix="pipeline_records_"))
        self.index = asset_index.AssetIndex(self.tmp / "asset_index.json")

    def test_collect_describes_a_kit_wall(self):
        wall = self.asset("Wall_BrickWindow")
        rec = ar.collect(wall, self.cfg)
        self.assertEqual(rec["id"], "general_store/SM_GS_Wall_BrickWindow")
        self.assertEqual(rec["category"], "Wall")
        self.assertEqual(rec["pack"], "General Store")
        self.assertEqual(rec["geometry"]["triangles"], pipeline.utils.triangle_count(wall.data))
        self.assertAlmostEqual(rec["geometry"]["dimensions_m"][0], 2.0, places=3)
        self.assertIn("UVMap", rec["geometry"]["uv_maps"])
        self.assertIn("M_Brick_Red", [m["name"] for m in rec["materials"]])
        self.assertIn("material:M_Brick_Red", rec["dependencies"])
        self.assertEqual(rec["collision"]["type"], "BOX")
        self.assertEqual({s["name"] for s in rec["sockets"]}, {"L", "R", "TOP", "BOTTOM"})
        self.assertTrue(rec["lods"])
        self.assertEqual(rec["kind"], "generated")
        self.assertEqual(rec["generation"]["generator"], "wall")
        self.assertEqual(am.validate(rec), [])
        for key in (s["key"] for s in rec["sources"]):
            self.assertIn(f"library:{key}", rec["dependencies"])

    def test_id_survives_a_rename(self):
        wall = self.asset("Wall_BrickSolid")
        first = ar.assign_id(wall, self.cfg)
        wall.name = "SM_GS_Wall_Renamed"
        self.assertEqual(ar.asset_id(wall, self.cfg), first)
        self.assertEqual(ar.collect(wall, self.cfg)["name"], "SM_GS_Wall_Renamed")

    def test_generation_parameters_make_it_generated(self):
        crate = self.make_cube("SM_GS_Prop_TestCrate", size=1.0)
        pipeline.asset_setup.setup_asset(crate, "Prop", "TestCrate", mark=False, cfg=self.cfg)
        ar.set_generation(crate, "test.crate", {"size": [1, 1, 1], "bevel": 0.02}, seed=7, profile="realistic")
        rec = ar.collect(crate, self.cfg)
        self.assertEqual(rec["kind"], "generated")
        self.assertEqual(rec["generation"]["generator"], "test.crate")
        self.assertEqual(rec["generation"]["params"]["bevel"], 0.02)
        self.assertEqual(rec["generation"]["seed"], 7)

    def test_geometry_edit_bumps_the_version(self):
        wall = self.asset("Wall_BrickSolid")
        ar.record([wall], self.cfg, index=self.index, save=False)
        self.assertEqual(self.index.get(ar.asset_id(wall, self.cfg))["version"], 1)
        ar.record([wall], self.cfg, index=self.index, save=False)
        self.assertEqual(self.index.get(ar.asset_id(wall, self.cfg))["version"], 1)
        bm = bmesh.new()
        bm.from_mesh(wall.data)
        bm.verts.ensure_lookup_table()
        bm.verts[0].co.z += 0.05
        bm.to_mesh(wall.data)
        bm.free()
        ar.record([wall], self.cfg, index=self.index, save=False)
        self.assertEqual(self.index.get(ar.asset_id(wall, self.cfg))["version"], 2)

    def test_record_with_validation_sets_status(self):
        walls = [self.asset("Wall_BrickSolid"), self.asset("Wall_BrickDoor")]
        recs = ar.record(walls, self.cfg, index=self.index, validate=True, save=False)
        self.assertEqual({r["status"] for r in recs}, {"generated"}, "only a QA pass makes an asset validated")
        walls[0].scale.x = 2.0  # a transform error
        recs = ar.record(walls[:1], self.cfg, index=self.index, validate=True, save=False)
        self.assertEqual(recs[0]["status"], "failed")

    def test_record_saves_next_to_the_pack_config(self):
        self.assertEqual(ar.index_path(self.cfg), PACK / "asset_index.json")

    def test_check_reports_duplicates_and_missing_dependencies(self):
        a = self.make_cube("SM_GS_Prop_DupA", size=1.0)
        b = self.make_cube("SM_GS_Prop_DupB", size=1.0)
        for obj in (a, b):
            pipeline.asset_setup.setup_asset(obj, "Prop", obj.name.split("_")[-1], mark=False, cfg=self.cfg)
            pipeline.material_manager.assign_material([obj], "Wood_Weathered", cfg=self.cfg)
        b["pipeline_depends"] = '["asset:general_store/SM_GS_Missing"]'
        ar.record([a, b], self.cfg, index=self.index, save=False)
        messages = " | ".join(p.message for p in ar.check(self.index, self.cfg, library=False))
        self.assertIn("SM_GS_Prop_DupA", messages)
        self.assertIn("SM_GS_Missing", messages)

    def test_whole_kit_records_cleanly(self):
        assets = [o for o in bpy.data.objects if pipeline.utils.is_asset(o)]
        recs = ar.record(assets, self.cfg, index=self.index, save=False)
        self.assertEqual(len(recs), len(assets))
        self.assertEqual(len(set(r["id"] for r in recs)), len(assets))
        self.assertEqual([p for p in ar.check(self.index, self.cfg, library=False) if p.level == "error"], [])


class RecordCliTests(KitTestCase):
    def test_record_and_assets_commands(self):
        import json
        import shutil
        import subprocess

        from helpers import ROOT

        tmp = Path(tempfile.mkdtemp(prefix="pipeline_record_cli_"))
        shutil.copy(ROOT / "pipeline.config.json", tmp / "pipeline.config.json")
        pack = tmp / "pack"
        (pack / "blend").mkdir(parents=True)
        shutil.copy(PACK / "pipeline.config.json", pack / "pipeline.config.json")
        blend = pack / "blend" / self.blend.name
        shutil.copy(self.blend, blend)
        cli = Path(pipeline.__file__).parent / "cli.py"

        def run(*args):
            return subprocess.run([bpy.app.binary_path, "-b", str(blend), "--factory-startup", "--python", str(cli),
                                   "--", *args], capture_output=True, text=True, timeout=300)

        proc = run("record", "--objects", "SM_GS_Wall_BrickDoor,SM_GS_Pier_BrickCorner")
        self.assertEqual(proc.returncode, 0, proc.stdout[-2000:] + proc.stderr[-2000:])
        data = json.loads((pack / "asset_index.json").read_text(encoding="utf-8"))
        self.assertEqual(sorted(data["assets"]), ["general_store/SM_GS_Pier_BrickCorner",
                                                  "general_store/SM_GS_Wall_BrickDoor"])
        self.assertEqual({r["status"] for r in data["assets"].values()}, {"generated"})
        proc = run("assets", "--category", "Wall")
        self.assertEqual(proc.returncode, 0, proc.stdout[-2000:] + proc.stderr[-2000:])
        self.assertIn("general_store/SM_GS_Wall_BrickDoor", proc.stdout)
        self.assertNotIn("SM_GS_Pier_BrickCorner\n", proc.stdout.split("asset(s) listed")[0])
