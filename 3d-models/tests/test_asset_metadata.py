"""Asset records and the pack index (pure Python: no kit file needed)."""

import json
import tempfile
import unittest
from pathlib import Path

import blender as pipeline
from blender import asset_index, asset_metadata as am

PipelineError = pipeline.PipelineError


def record(name="SM_T_Wall_A", category="Wall", fp="g1", **fields):
    rec = am.new_record(am.make_id("Test Pack", name), name, "Test Pack", category, **fields)
    rec["geometry"] = {"fingerprint": fp, "triangles": 12}
    return rec


class MetadataTests(unittest.TestCase):
    def test_ids_are_stable_and_scoped_by_pack(self):
        self.assertEqual(am.make_id("General Store", "SM_GS_Wall_A"), "general_store/SM_GS_Wall_A")
        self.assertEqual(am.make_id("General Store", "SM_GS_Wall_A"), am.make_id("General Store", "SM_GS_Wall_A"))
        self.assertNotEqual(am.make_id("Pack A", "Crate"), am.make_id("Pack B", "Crate"))
        self.assertTrue(am.is_valid_id("general_store/SM_GS_Wall_A"))
        for bad in ("SM_GS_Wall_A", "general store/A", "pack/", "/A", "pack/A B"):
            self.assertFalse(am.is_valid_id(bad), bad)

    def test_new_record_has_every_required_field(self):
        rec = record()
        self.assertEqual(rec["format"], am.FORMAT)
        self.assertEqual(rec["version"], 1)
        self.assertEqual(rec["status"], "draft")
        self.assertEqual(rec["kind"], "manual")
        self.assertEqual(am.validate(rec), [])

    def test_validate_finds_missing_and_bad_values(self):
        rec = record()
        del rec["category"]
        rec["status"] = "shipped"
        rec["kind"] = "stolen"
        rec["id"] = "no slash"
        rec["dependencies"] = ["material:M_Brick", "bogus", "planet:mars"]
        problems = " | ".join(p.message for p in am.validate(rec))
        for word in ("category", "shipped", "stolen", "no slash", "bogus", "planet"):
            self.assertIn(word, problems)
        self.assertNotIn("M_Brick", problems)

    def test_json_round_trip_is_stable(self):
        rec = record(tags=["brick", "wall"], dependencies=["material:M_Brick_Red"])
        text = am.to_json(rec)
        self.assertEqual(am.from_json(text), rec)
        self.assertEqual(am.to_json(am.from_json(text)), text)

    def test_from_json_refuses_other_formats_and_newer_versions(self):
        with self.assertRaises(PipelineError):
            am.from_json(json.dumps({"format": "something-else", "format_version": 1}))
        newer = dict(record(), format_version=am.FORMAT_VERSION + 1)
        with self.assertRaises(PipelineError):
            am.from_json(json.dumps(newer))

    def test_fingerprint_ignores_bookkeeping(self):
        a, b = record(), record()
        b["status"], b["updated"], b["version"] = "exported", "2030-01-01T00:00:00Z", 7
        self.assertEqual(am.fingerprint(a), am.fingerprint(b))
        b["geometry"]["triangles"] = 13
        self.assertNotEqual(am.fingerprint(a), am.fingerprint(b))

    def test_dependencies_parse(self):
        self.assertEqual(am.parse_dependency("library:polyhaven:red_brick_03"), ("library", "polyhaven:red_brick_03"))
        self.assertEqual(am.dependency("asset", "p/A"), "asset:p/A")
        with self.assertRaises(PipelineError):
            am.parse_dependency("nothing")


class IndexTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="pipeline_index_"))
        self.index = asset_index.AssetIndex(self.tmp / "asset_index.json")

    def test_versions_bump_only_when_content_changes(self):
        first = self.index.put(record())
        self.assertEqual(first["version"], 1)
        created = first["created"]
        again = self.index.put(record())
        self.assertEqual(again["version"], 1)
        self.assertEqual(again["created"], created)
        changed = record(fp="g2")
        self.assertEqual(self.index.put(changed)["version"], 2)
        self.assertEqual([h["version"] for h in self.index.history(changed["id"])], [1, 2])

    def test_status_changes_are_kept_in_history(self):
        rec = self.index.put(record())
        self.index.set_status(rec["id"], "validated")
        self.index.set_status(rec["id"], "exported", exports={"axcenity": {"manifest": "m.json"}})
        self.assertEqual([h["status"] for h in self.index.history(rec["id"])], ["draft", "validated", "exported"])
        self.assertEqual(self.index.get(rec["id"])["exports"]["axcenity"]["manifest"], "m.json")
        with self.assertRaises(PipelineError):
            self.index.set_status(rec["id"], "shipped")

    def test_save_and_reload(self):
        self.index.put(record(tags=["x"]))
        self.index.save()
        again = asset_index.AssetIndex(self.tmp / "asset_index.json")
        self.assertEqual(again.ids(), self.index.ids())
        self.assertEqual(again.history(record()["id"]), self.index.history(record()["id"]))
        data = json.loads((self.tmp / "asset_index.json").read_text(encoding="utf-8"))
        self.assertEqual(data["format"], asset_index.FORMAT)

    def test_duplicates_share_geometry(self):
        self.index.put(record("SM_T_Wall_A", fp="same"))
        self.index.put(record("SM_T_Wall_B", fp="same"))
        self.index.put(record("SM_T_Wall_C", fp="other"))
        self.assertEqual(self.index.duplicates(), [["test_pack/SM_T_Wall_A", "test_pack/SM_T_Wall_B"]])

    def test_dependency_order_and_missing(self):
        base = record("SM_T_Base")
        wall = record("SM_T_Wall", dependencies=[am.dependency("asset", base["id"]), "material:M_Brick"])
        house = record("SM_T_House", dependencies=[am.dependency("asset", wall["id"]), "asset:test_pack/Gone"])
        for rec in (house, wall, base):
            self.index.put(rec)
        order = self.index.dependency_order()
        self.assertLess(order.index(base["id"]), order.index(wall["id"]))
        self.assertLess(order.index(wall["id"]), order.index(house["id"]))
        self.assertEqual(self.index.missing_dependencies(), {house["id"]: ["asset:test_pack/Gone"]})
        missing = self.index.missing_dependencies(resolver=lambda kind, ref: ref != "M_Brick")
        self.assertEqual(missing[wall["id"]], ["material:M_Brick"])
        self.assertEqual(self.index.dependents(base["id"]), [house["id"], wall["id"]])

    def test_dependency_cycle_is_named(self):
        a, b = record("SM_T_A"), record("SM_T_B")
        a["dependencies"] = [am.dependency("asset", b["id"])]
        b["dependencies"] = [am.dependency("asset", a["id"])]
        self.index.put(a)
        self.index.put(b)
        with self.assertRaises(PipelineError) as ctx:
            self.index.dependency_order()
        self.assertIn("SM_T_A", str(ctx.exception))

    def test_find(self):
        self.index.put(record("SM_T_Wall_A", tags=["brick"]))
        self.index.put(record("SM_T_Crate", category="Prop", fp="c", kind="generated"))
        self.assertEqual([r["name"] for r in self.index.find(category="Prop")], ["SM_T_Crate"])
        self.assertEqual([r["name"] for r in self.index.find(tag="brick")], ["SM_T_Wall_A"])
        self.assertEqual([r["name"] for r in self.index.find(kind="generated")], ["SM_T_Crate"])
        self.assertEqual([r["name"] for r in self.index.find(text="crate")], ["SM_T_Crate"])

    def test_index_refuses_invalid_records(self):
        bad = record()
        bad["id"] = "nope"
        with self.assertRaises(PipelineError):
            self.index.put(bad)


if __name__ == "__main__":
    unittest.main()
