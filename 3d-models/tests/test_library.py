"""The asset library manager: search, favourites, dependencies, history, deprecation, packs (no kit file needed)."""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

import blender as pipeline
from blender import asset_index, asset_metadata as am, config, pack_library

PipelineError = pipeline.PipelineError
WALL_SOCKETS = [{"name": "L", "pos": [0, 0, 0], "dir": [-1, 0, 0], "tag": "wall_edge"}]
CATS_HEADER = "# Blender catalogs\n\nVERSION 1\n\n"


def record(pack, name, category="Wall", fp=None, size=2.0, **fields):
    rec = am.new_record(am.make_id(pack, name), name, pack, category, **fields)
    rec["geometry"] = {"fingerprint": fp or name, "triangles": 12, "bounds_min": [0.0, -0.1, 0.0], "bounds_max": [size, 0.1, 3.0]}
    return rec


def asset(pack, short):
    return f"{am.slug(pack)}/{short}"


class Fixture(unittest.TestCase):
    """Two packs in a temporary project: a kit and a town that uses one of its walls."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pipeline_library_")).resolve()
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.write(self.root / "pipeline.config.json", {"metadata": {"packs_directory": "packs"}})
        self.kit_dir, self.town_dir = self.make_pack("kit", "Kit", "KT"), self.make_pack("town", "Town", "TN")
        self.make_pack("_template", "Template", "TP")

        kit = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        wall = kit.put(record("Kit", "SM_KT_Wall_Brick", tags=["brick", "exterior"], description="Red brick wall",
                              sockets=WALL_SOCKETS, license="CC0-1.0", kind="generated",
                              materials=[{"name": "M_Red_Clay", "preset": None, "library": None}],
                              dependencies=["material:M_Red_Clay", "library:polyhaven:red_brick",
                                            "texture:textures/a.png"]))
        self.wall = wall["id"]
        self.new_wall = kit.put(record("Kit", "SM_KT_Wall_Plaster", tags=["exterior"], sockets=WALL_SOCKETS, license="mixed"))["id"]
        self.short_wall = kit.put(record("Kit", "SM_KT_Wall_Short", size=1.0, sockets=WALL_SOCKETS))["id"]
        self.crate = kit.put(record("Kit", "SM_KT_Prop_Crate", "Prop", tags=["Wood"], kind="generated", status="exported"))["id"]
        variant = record("Kit", "SM_KT_Wall_Brick_V1", sockets=WALL_SOCKETS, kind="generated")
        variant["generation"] = {"generator": "wall", "params": {}, "seed": 1, "variation": {"of": self.wall}}
        self.variant = kit.put(variant)["id"]
        bay = kit.put(record("Kit", "SM_KT_Bay", "Module", dependencies=[f"asset:{self.wall}"]))
        self.bay = bay["id"]
        house = record("Kit", "ASM_KT_House_S1", "Structure", tags=["assembly"], kind="generated",
                       dependencies=[f"asset:{self.bay}"])
        house["assembly"] = {"pieces": 2}
        self.house = kit.put(house)["id"]
        kit.save()

        town = asset_index.AssetIndex(self.town_dir / "asset_index.json")
        street = record("Town", "ASM_TN_Street_S1", "Structure", kind="generated", dependencies=[f"asset:{self.wall}"])
        street["assembly"] = {"pieces": 1}
        self.street = town.put(street)["id"]
        self.bench = town.put(record("Town", "SM_TN_Prop_Bench", "Prop", tags=["wood"]))["id"]
        self.lamp = town.put(record("Town", "SM_TN_Prop_Crate", "Prop"))["id"]
        town.save()

        self.write(self.kit_dir / "assemblies" / "named.json", self.spec("Named", {"assets": ["Wall_Brick"]}))
        self.write(self.kit_dir / "assemblies" / "by_category.json", self.spec("ByCategory", {"categories": ["Wall"]}))
        self.write(self.kit_dir / "assemblies" / "excluded.json",
                   self.spec("Excluded", {"categories": ["Wall"], "exclude": ["Wall_Brick"]}))
        self.write(self.kit_dir / "assemblies" / "other.json", self.spec("Other", {"categories": ["Prop"]}))

    def write(self, path, data):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data), encoding="utf-8", newline="\n")

    def make_pack(self, folder, name, code):
        path = self.root / "packs" / folder
        self.write(path / "pipeline.config.json", {"pack": {"name": name, "code": code}})
        return path

    def spec(self, name, allow):
        return {"format": "pipeline-assembly", "format_version": 1, "name": name,
                "slots": [{"id": "a", "at": [0, 0, 0], "allow": allow}]}

    def cfg(self, folder=None):
        return config.load_config(start=folder or self.kit_dir)

    def library(self, folder=None):
        return pack_library.open_packs(self.cfg(folder))

    def names(self, records):
        return [r["name"] for r in records]


class SearchTests(Fixture):
    def test_packs_are_found_and_templates_left_out(self):
        lib = self.library()
        self.assertEqual(sorted(lib.packs), ["kit", "town"])
        self.assertEqual(lib.current, "kit")
        self.assertEqual(lib.problems, [])

    def test_search_is_this_pack_unless_asked(self):
        lib = self.library()
        self.assertEqual(self.names(lib.find(category="Prop")), ["SM_KT_Prop_Crate"])
        self.assertEqual(self.names(lib.find(category="Prop", pack="all")),
                         ["SM_KT_Prop_Crate", "SM_TN_Prop_Bench", "SM_TN_Prop_Crate"])
        self.assertEqual(self.names(lib.find(category="Prop", pack="town")), ["SM_TN_Prop_Bench", "SM_TN_Prop_Crate"])
        self.assertEqual(self.names(lib.find(category="Prop", pack="Town")), ["SM_TN_Prop_Bench", "SM_TN_Prop_Crate"])

    def test_unknown_pack_names_the_ones_there_are(self):
        with self.assertRaises(PipelineError) as ctx:
            self.library().find(pack="nowhere")
        self.assertIn("kit", str(ctx.exception))

    def test_filters(self):
        lib = self.library()
        self.assertEqual(self.names(lib.find(status="exported")), ["SM_KT_Prop_Crate"])
        self.assertEqual(self.names(lib.find(license="CC0-1.0")), ["SM_KT_Wall_Brick"])
        self.assertEqual(self.names(lib.find(license="cc0-1.0")), ["SM_KT_Wall_Brick"])
        self.assertEqual(self.names(lib.find(license="mixed")), ["SM_KT_Wall_Plaster"])
        self.assertEqual(self.names(lib.find(kind="manual", category="Wall")), ["SM_KT_Wall_Plaster", "SM_KT_Wall_Short"])
        self.assertEqual(self.names(lib.find(tag="wood")), ["SM_KT_Prop_Crate"])  # any case
        self.assertEqual(self.names(lib.find(tag=["brick", "exterior"])), ["SM_KT_Wall_Brick"])
        self.assertEqual(self.names(lib.find(tag=["brick", "wood"])), [])

    def test_a_filter_that_can_never_match_is_an_error(self):
        lib = self.library()
        with self.assertRaises(PipelineError):
            lib.find(status="shipped")
        with self.assertRaises(PipelineError):
            lib.find(kind="stolen")

    def test_text_needs_every_word_and_reads_more_than_the_name(self):
        lib = self.library()
        self.assertEqual(self.names(lib.find(text="brick red")), ["SM_KT_Wall_Brick"])  # name + description
        self.assertEqual(self.names(lib.find(text="brick missing")), [])
        self.assertEqual(self.names(lib.find(text="clay")), ["SM_KT_Wall_Brick"])  # a material's name
        self.assertEqual(self.names(lib.find(text="module")), ["SM_KT_Bay"])  # category

    def test_facets_count_the_values_there_are(self):
        lib = self.library()
        facets = lib.facets(lib.records("all"))
        self.assertEqual(facets["pack"], {"kit": 7, "town": 3})
        self.assertEqual(facets["category"]["Prop"], 3)
        self.assertEqual(facets["tags"]["exterior"], 2)
        self.assertEqual(facets["license"]["CC0-1.0"], 1)
        self.assertEqual(list(facets["category"]), sorted(facets["category"]))

    def test_names_resolve_to_ids(self):
        lib = self.library()
        self.assertEqual(lib.resolve(self.wall), self.wall)
        self.assertEqual(lib.resolve("SM_KT_Wall_Brick"), self.wall)
        self.assertEqual(lib.resolve("SM_TN_Prop_Bench"), self.bench)
        with self.assertRaises(PipelineError) as ctx:
            lib.resolve("SM_KT_Wall_Bric")
        self.assertIn("SM_KT_Wall_Brick", str(ctx.exception))  # the close match

    def test_a_name_in_two_packs_prefers_this_one_and_is_ambiguous_elsewhere(self):
        kit_index = asset_index.AssetIndex(self.town_dir / "asset_index.json")
        kit_index.put(record("Town", "SM_KT_Prop_Crate", "Prop"))
        kit_index.save()
        self.assertEqual(self.library().resolve("SM_KT_Prop_Crate"), self.crate)
        lib = pack_library.open_packs(config.load_config(start=self.root))  # the project root: no current pack
        with self.assertRaises(PipelineError) as ctx:
            lib.resolve("SM_KT_Prop_Crate")
        self.assertIn("town/SM_KT_Prop_Crate", str(ctx.exception))

    def test_an_unreadable_pack_is_reported_and_the_rest_still_work(self):
        (self.town_dir / "asset_index.json").write_text("{ not json", encoding="utf-8")
        lib = self.library()
        self.assertEqual(sorted(lib.packs), ["kit"])
        self.assertIn("asset_index.json", " ".join(lib.problems))
        self.assertTrue(lib.find(category="Wall"))


class FavouriteTests(Fixture):
    def test_favourites_are_kept_in_the_index(self):
        lib = self.library()
        lib.set_favourite(self.wall)
        lib.set_favourite(self.bench)
        again = self.library()
        self.assertEqual(self.names(again.find(favourite=True)), ["SM_KT_Wall_Brick"])
        self.assertEqual(self.names(again.find(favourite=True, pack="all")), ["SM_KT_Wall_Brick", "SM_TN_Prop_Bench"])
        data = json.loads((self.kit_dir / "asset_index.json").read_text(encoding="utf-8"))
        self.assertEqual(data["favourites"], [self.wall])
        self.assertTrue(again.is_favourite(self.wall))
        again.set_favourite(self.wall, False)
        self.assertEqual(self.library().find(favourite=True), [])

    def test_favourites_are_not_content_and_survive_recording_again(self):
        index = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        before = index.get(self.crate)["updated"], index.get(self.crate)["version"], len(index.history(self.crate))
        index.set_favourite(self.crate)
        index.put(record("Kit", "SM_KT_Prop_Crate", "Prop", tags=["Wood"], kind="generated", status="exported"))
        self.assertTrue(index.is_favourite(self.crate))
        self.assertEqual(index.get(self.crate)["version"], before[1])
        self.assertEqual(len(index.history(self.crate)), before[2])

    def test_removing_an_asset_removes_the_favourite(self):
        index = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        index.set_favourite(self.crate)
        index.remove(self.crate)
        self.assertEqual(index.favourites(), [])

    def test_unknown_assets_can_not_be_favourites(self):
        index = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        with self.assertRaises(PipelineError):
            index.set_favourite("kit/Gone")


class DependencyTests(Fixture):
    def test_what_an_asset_uses_by_type(self):
        view = self.library().dependency_view(self.wall)
        self.assertEqual(view["uses"]["material"], ["M_Red_Clay"])
        self.assertEqual(view["uses"]["library"], ["polyhaven:red_brick"])
        self.assertEqual(view["uses"]["texture"], ["textures/a.png"])
        self.assertEqual(view["uses"]["asset"], [])
        self.assertEqual(view["missing"], [])

    def test_what_uses_an_asset_direct_and_through_others_across_packs(self):
        view = self.library().dependency_view(self.wall)
        self.assertEqual(view["used_by"]["direct"], [self.bay, self.street])
        self.assertEqual(view["used_by"]["indirect"], [self.house])
        self.assertEqual(view["variants"], [self.variant])

    def test_uses_follows_the_chain(self):
        view = self.library().dependency_view(self.house)
        self.assertEqual(view["uses"]["asset"], [self.bay])
        self.assertEqual(view["uses_all"], [self.bay, self.wall])
        self.assertEqual(self.library().dependency_view(self.variant)["variant_of"], self.wall)

    def test_a_cycle_ends(self):
        index = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        wall = index.get(self.wall)
        wall["dependencies"] = [*wall["dependencies"], f"asset:{self.house}"]
        index.save()
        view = self.library().dependency_view(self.wall)
        self.assertIn(self.house, view["uses_all"])
        self.assertNotIn(self.wall, view["uses_all"])
        self.assertNotIn(self.wall, view["used_by"]["direct"] + view["used_by"]["indirect"])

    def test_a_reference_nothing_holds_is_missing(self):
        index = asset_index.AssetIndex(self.town_dir / "asset_index.json")
        index.get(self.bench)["dependencies"] = ["asset:kit/Gone"]
        index.save()
        self.assertEqual(self.library().dependency_view(self.bench)["missing"], ["kit/Gone"])

    def test_another_packs_asset_counts_as_found_only_with_the_other_packs(self):
        index = asset_index.AssetIndex(self.town_dir / "asset_index.json")
        self.assertIn(self.street, index.missing_dependencies())
        lib = self.library(self.town_dir)
        self.assertEqual(index.missing_dependencies(elsewhere=lambda ref: lib.get(ref) is not None), {})


class HistoryTests(Fixture):
    def test_each_row_says_what_changed_newest_first(self):
        index = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        index.set_status(self.short_wall, "failed")
        index.put(record("Kit", "SM_KT_Wall_Short", size=1.0, fp="changed", sockets=WALL_SOCKETS))
        rows = index.history_view(self.short_wall)
        self.assertEqual([r["change"] for r in rows], ["content", "status", "created"])
        self.assertEqual([r["version"] for r in rows], [2, 1, 1])
        self.assertEqual(rows[1]["status"], "failed")
        self.assertEqual(rows[1]["previous_status"], "draft")

    def test_the_library_finds_history_in_any_pack(self):
        self.assertEqual(self.library().history("SM_TN_Prop_Bench")[0]["change"], "created")

    def test_a_deprecation_says_why_in_the_history(self):
        lib = self.library()
        lib.deprecate(self.wall, self.new_wall, "cracks at the join")
        row = self.library().history(self.wall)[0]
        self.assertEqual(row["status"], "deprecated")
        self.assertIn("cracks at the join", row["note"])
        self.assertIn(self.new_wall, row["note"])


class DeprecationTests(Fixture):
    def test_deprecate_stores_why_and_the_replacement(self):
        result = self.library().deprecate(self.wall, "SM_KT_Wall_Plaster", "cracks")
        stored = self.library().get(self.wall)
        self.assertEqual(stored["status"], "deprecated")
        self.assertEqual(stored["deprecation"]["replaced_by"], self.new_wall)
        self.assertEqual(stored["deprecation"]["reason"], "cracks")
        self.assertEqual(stored["version"], 1)  # not content
        self.assertEqual(result["warnings"], [])
        self.assertEqual([p for p in am.validate(stored) if p.field == "deprecation"], [])

    def test_affected_assemblies(self):
        affected = self.library().affected_assemblies(self.wall)
        self.assertEqual(affected["structures"], [self.house, self.street])
        specs = {s["spec"]: s for s in affected["specs"]}
        self.assertEqual(sorted(specs), ["by_category", "named"])
        self.assertEqual(specs["named"]["named"], ["a"])
        self.assertEqual(specs["named"]["may_pick"], [])
        self.assertEqual(specs["by_category"]["may_pick"], ["a"])
        self.assertEqual(specs["by_category"]["pack"], "kit")

    def test_another_packs_spec_counts_only_when_it_uses_the_full_name(self):
        self.write(self.town_dir / "assemblies" / "short.json", self.spec("Short", {"assets": ["Wall_Brick"]}))
        self.write(self.town_dir / "assemblies" / "category.json", self.spec("Category", {"categories": ["Wall"]}))
        self.write(self.town_dir / "assemblies" / "full.json", self.spec("Full", {"assets": ["SM_KT_Wall_Brick"]}))
        specs = [s for s in self.library().affected_assemblies(self.wall)["specs"] if s["pack"] == "town"]
        self.assertEqual([s["spec"] for s in specs], ["full"])

    def test_a_broken_spec_is_reported_not_fatal(self):
        self.write(self.kit_dir / "assemblies" / "broken.json", {"format": "pipeline-assembly", "slots": [{"id": "a"}]})
        lib = self.library()
        self.assertEqual(len(lib.affected_assemblies(self.wall)["specs"]), 2)
        self.assertIn("broken", " ".join(lib.problems))

    def test_a_deprecation_returns_what_it_affects(self):
        result = self.library().deprecate(self.wall, self.new_wall)
        self.assertEqual(result["affected"]["structures"], [self.house, self.street])

    def test_a_replacement_must_exist_and_be_current(self):
        lib = self.library()
        with self.assertRaises(PipelineError):
            lib.deprecate(self.wall, "kit/Gone")
        with self.assertRaises(PipelineError):
            lib.deprecate(self.wall, self.wall)
        lib.deprecate(self.short_wall)
        with self.assertRaises(PipelineError) as ctx:
            lib.deprecate(self.wall, self.short_wall)
        self.assertIn("deprecated", str(ctx.exception))
        self.assertEqual(self.library().get(self.wall)["status"], "draft")  # nothing was stored

    def test_a_replacement_built_from_the_asset_is_refused(self):
        with self.assertRaises(PipelineError) as ctx:
            self.library().deprecate(self.wall, self.bay)
        self.assertIn("built from", str(ctx.exception))

    def test_a_replacement_that_can_not_swap_is_a_warning(self):
        result = self.library().deprecate(self.wall, self.short_wall)
        self.assertTrue(result["warnings"])
        self.assertEqual(self.library().get(self.wall)["deprecation"]["replaced_by"], self.short_wall)
        other = self.library().deprecate(self.new_wall, self.crate)
        self.assertIn("categories differ", " ".join(other["warnings"]))

    def test_replacements_are_followed_to_the_current_one(self):
        lib = self.library()
        lib.deprecate(self.wall, self.new_wall)
        lib.deprecate(self.new_wall, self.short_wall)
        self.assertEqual(self.library().replacement(self.wall), self.short_wall)
        self.assertEqual(self.library().replacement(self.short_wall), "")

    def test_nothing_but_restore_ends_a_deprecation(self):
        index = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        index.deprecate(self.crate, reason="old")
        index.set_status(self.crate, "exported", exports={"axcenity": {"manifest": "m.json"}})
        rec = index.get(self.crate)
        self.assertEqual(rec["status"], "deprecated")
        self.assertEqual(rec["exports"]["axcenity"]["manifest"], "m.json")  # the rest still lands
        index.set_job(self.crate, {"name": "kit", "state": "failed"}, status="failed")
        self.assertEqual(index.get(self.crate)["status"], "deprecated")
        again = index.put(record("Kit", "SM_KT_Prop_Crate", "Prop", tags=["Wood"], kind="generated", status="failed"))
        self.assertEqual(again["status"], "deprecated")
        self.assertEqual(again["deprecation"]["reason"], "old")

    def test_restore_brings_the_status_back(self):
        lib = self.library()
        lib.deprecate(self.crate, reason="old")
        restored = self.library().restore(self.crate)
        self.assertEqual(restored["status"], "exported")
        self.assertEqual(restored["deprecation"], {})
        self.assertEqual(self.library().get(self.crate)["status"], "exported")

    def test_restore_does_not_vouch_for_changed_content(self):
        index = asset_index.AssetIndex(self.kit_dir / "asset_index.json")
        index.deprecate(self.crate)
        index.put(record("Kit", "SM_KT_Prop_Crate", "Prop", fp="changed", kind="generated"))
        self.assertEqual(index.restore(self.crate)["status"], "generated")

    def test_restore_needs_a_deprecated_asset(self):
        with self.assertRaises(PipelineError):
            self.library().restore(self.crate)

    def test_uses_of_deprecated_assets_are_listed(self):
        self.library().deprecate(self.wall, self.new_wall)
        lib = self.library()
        found = sorted(lib.deprecated_uses(lib.records("all")))
        self.assertEqual(found, [(self.bay, self.wall, self.new_wall), (self.street, self.wall, self.new_wall)])

    def test_a_deprecated_asset_using_another_is_not_listed(self):
        lib = self.library()
        lib.deprecate(self.wall)
        lib.deprecate(self.bay)
        lib = self.library()
        self.assertEqual(sorted(u for u, _, _ in lib.deprecated_uses(lib.records("all"))), [self.house, self.street])  # not bay

    def test_a_malformed_block_is_a_validation_error(self):
        rec = record("Kit", "SM_KT_A", deprecation="soon")
        self.assertIn("deprecation", [p.field for p in am.validate(rec)])
        rec = record("Kit", "SM_KT_A", deprecation={"replaced_by": "no slash"})
        self.assertIn("deprecation", [p.field for p in am.validate(rec)])


class PaletteTests(Fixture):
    def piece(self, rec_id, aliases=()):
        name = rec_id.split("/", 1)[1]
        return {"name": name, "id": rec_id, "aliases": sorted({name, *aliases})}

    def test_deprecated_pieces_leave_and_their_names_pass_on(self):
        lib = self.library()
        lib.deprecate(self.wall, self.new_wall)
        pieces = [self.piece(self.wall, ["Wall_Brick"]), self.piece(self.new_wall), self.piece(self.crate)]
        kept = pack_library.retire_pieces(pieces, self.library())
        self.assertEqual([p["id"] for p in kept], [self.new_wall, self.crate])
        self.assertIn("Wall_Brick", kept[0]["aliases"])
        self.assertIn("SM_KT_Wall_Brick", kept[0]["aliases"])

    def test_without_a_replacement_they_just_leave(self):
        self.library().deprecate(self.wall)
        kept = pack_library.retire_pieces([self.piece(self.wall), self.piece(self.crate)], self.library())
        self.assertEqual([p["id"] for p in kept], [self.crate])

    def test_pieces_with_no_record_stay(self):
        pieces = [self.piece("kit/SM_KT_NotRecorded")]
        self.assertEqual(pack_library.retire_pieces(pieces, self.library()), pieces)


class CatalogTests(Fixture):
    A, B, C = ("aaaaaaaa-0000-4000-8000-000000000001", "bbbbbbbb-0000-4000-8000-000000000002",
               "cccccccc-0000-4000-8000-000000000003")

    def setUp(self):
        super().setUp()
        self.cats(self.kit_dir / "blend", (self.A, "Kit", "Kit"), (self.B, "Kit/Wall", "Kit-Wall"))
        self.cats(self.town_dir / "blend", (self.C, "Town/Prop", "Town-Prop"))
        self.dest = self.root / "packs" / pack_library.CATALOG_FILE

    def cats(self, folder, *rows):
        folder.mkdir(parents=True, exist_ok=True)
        lines = "".join(f"{u}:{p}:{s}\n" for u, p, s in rows)
        (folder / pack_library.CATALOG_FILE).write_text(CATS_HEADER + lines, encoding="utf-8", newline="\n")

    def test_every_packs_catalogs_keep_their_ids(self):
        result = pack_library.merge_catalogs(self.library())
        self.assertEqual(result["path"], self.dest)
        found = pack_library.parse_catalogs(self.dest.read_text(encoding="utf-8"))
        self.assertEqual(found, {self.A: ("Kit", "Kit"), self.B: ("Kit/Wall", "Kit-Wall"), self.C: ("Town/Prop", "Town-Prop")})
        self.assertEqual(result["by_pack"], {"kit": ["Kit", "Kit/Wall"], "town": ["Town/Prop"]})

    def test_the_file_is_blender_format_and_lf(self):
        pack_library.merge_catalogs(self.library())
        raw = self.dest.read_bytes()
        self.assertNotIn(b"\r", raw)
        lines = [l for l in raw.decode("utf-8").splitlines() if l and not l.startswith("#")]
        self.assertEqual(lines[0], "VERSION 1")
        self.assertEqual(lines[1], f"{self.A}:Kit:Kit")

    def test_writing_again_changes_nothing(self):
        self.assertTrue(pack_library.merge_catalogs(self.library())["changed"])
        before = self.dest.read_bytes()
        self.assertFalse(pack_library.merge_catalogs(self.library())["changed"])
        self.assertEqual(self.dest.read_bytes(), before)

    def test_catalogs_already_in_the_shared_file_stay(self):
        own = "dddddddd-0000-4000-8000-000000000004"
        self.dest.write_text(CATS_HEADER + f"{own}:Shared/Mine:Shared-Mine\n", encoding="utf-8", newline="\n")
        pack_library.merge_catalogs(self.library())
        self.assertIn(own, pack_library.parse_catalogs(self.dest.read_text(encoding="utf-8")))

    def test_it_can_go_somewhere_else(self):
        out = self.root / "elsewhere" / "cats.txt"
        self.assertEqual(pack_library.merge_catalogs(self.library(), out)["path"], out)
        self.assertTrue(out.exists())
        self.assertFalse(self.dest.exists())

    def test_it_needs_a_place(self):
        solo = self.root / "solo"
        self.write(solo / "pipeline.config.json", {"pack": {"name": "Solo"}})
        cfg = config.Config(config.deep_merge(config.DEFAULTS, {"pack": {"name": "Solo"}}), [solo / "pipeline.config.json"])
        with self.assertRaises(PipelineError):
            pack_library.merge_catalogs(pack_library.open_packs(cfg))


class IndexCompatibilityTests(Fixture):
    def test_an_index_from_before_favourites_opens(self):
        path = self.kit_dir / "asset_index.json"
        data = json.loads(path.read_text(encoding="utf-8"))
        del data["favourites"]
        for rec in data["assets"].values():
            del rec["deprecation"]
        path.write_text(json.dumps(data), encoding="utf-8", newline="\n")
        index = asset_index.AssetIndex(path)
        self.assertEqual(index.favourites(), [])
        self.assertEqual(index.find(favourite=True), [])
        self.assertIsNone(index.get(self.wall).get("deprecation"))

    def test_recording_an_old_entry_again_changes_nothing(self):
        path = self.kit_dir / "asset_index.json"
        data = json.loads(path.read_text(encoding="utf-8"))
        stamp = "2020-01-01T00:00:00Z"
        rec = data["assets"][self.short_wall]
        del rec["deprecation"]
        rec["updated"] = stamp
        path.write_text(json.dumps(data), encoding="utf-8", newline="\n")
        index = asset_index.AssetIndex(path)
        stored = index.put(record("Kit", "SM_KT_Wall_Short", size=1.0, sockets=WALL_SOCKETS))
        self.assertEqual(stored["updated"], stamp)

    def test_a_single_index_needs_no_packs_folder(self):
        solo = self.root / "solo"
        self.write(solo / "pipeline.config.json", {"pack": {"name": "Solo"}})
        cfg = config.Config(config.deep_merge(config.DEFAULTS, {"pack": {"name": "Solo"}}), [solo / "pipeline.config.json"])
        index = asset_index.AssetIndex(solo / "asset_index.json")
        index.put(record("Solo", "SM_A"))
        lib = pack_library.open_packs(cfg, index)
        self.assertEqual(list(lib.packs), ["solo"])
        self.assertEqual(lib.find()[0]["name"], "SM_A")


if __name__ == "__main__":
    unittest.main()
