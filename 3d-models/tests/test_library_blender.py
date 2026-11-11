"""The asset library manager inside Blender: the sidebar, the records check, the command line."""

import subprocess
import tempfile
from pathlib import Path
from types import SimpleNamespace

import bpy

import blender as pipeline
from blender import asset_index, asset_records as ar, pack_library, ui_library
from helpers import PACK, ROOT, KitTestCase
from test_library import Fixture

ICONS = set(bpy.types.UILayout.bl_rna.functions["label"].parameters["icon"].enum_items.keys())


class FakeLayout:
    """Stands in for a panel's layout, which only exists with a window. Calls that wouldn't draw raise."""

    def __init__(self):
        self.labels, self.operators = [], []

    def column(self, **kwargs):
        return self

    row = box = column

    def separator(self):
        pass

    def _icon(self, kwargs):
        icon = kwargs.get("icon", "NONE")
        assert icon in ICONS, f"no icon {icon}"

    def label(self, text="", **kwargs):
        self._icon(kwargs)
        self.labels.append(text)

    def prop(self, data, name, **kwargs):
        self._icon(kwargs)
        assert name in data.bl_rna.properties, f"no property {name}"

    def operator(self, idname, **kwargs):
        self._icon(kwargs)
        module, name = idname.split(".")
        assert getattr(getattr(bpy.ops, module), name).idname(), idname
        self.operators.append(idname)

    def template_list(self, list_type, list_id, data, prop, active_data, active_prop, **kwargs):
        assert hasattr(bpy.types, list_type), list_type
        assert prop in data.bl_rna.properties and active_prop in active_data.bl_rna.properties


class PanelTests(Fixture):
    @classmethod
    def setUpClass(cls):
        pipeline.register()

    @classmethod
    def tearDownClass(cls):
        pipeline.unregister()

    def setUp(self):
        super().setUp()
        scene = bpy.context.scene
        self.settings = scene.pipeline_assets
        scene.pipeline.config_path = str(self.kit_dir / "pipeline.config.json")
        pipeline.set_config_path(scene.pipeline.config_path)
        ui_library._state["library"] = None
        self.addCleanup(self.reset)
        self.search()

    def reset(self):
        scene = bpy.context.scene
        scene.pipeline.config_path = ""
        pipeline.set_config_path(None)
        self.settings.results.clear()
        for name in ("text", "replaced_by", "reason", "notice"):
            setattr(self.settings, name, "")
        self.settings.favourites_only = False
        self.settings.pack = "CURRENT"
        self.settings.category = self.settings.tag = self.settings.status = self.settings.kind = self.settings.license = "ANY"
        ui_library._state["library"] = None

    def search(self):
        self.assertEqual(bpy.ops.pipeline.assets_search(), {"FINISHED"})

    def listed(self):
        return [item.label for item in self.settings.results]

    def pick(self, label):
        self.settings.index = self.listed().index(label)

    def draw(self, panel):
        layout = FakeLayout()
        panel.draw(SimpleNamespace(layout=layout), bpy.context)
        return layout

    def test_the_search_fills_the_list(self):
        self.assertEqual(len(self.listed()), 7)
        self.settings.text = "brick red"
        self.search()
        self.assertEqual(self.listed(), ["SM_KT_Wall_Brick"])

    def test_filters_come_from_what_the_packs_hold(self):
        self.settings.pack = "ALL"
        self.settings.category = "Prop"
        self.search()
        self.assertEqual(self.listed(), ["SM_KT_Prop_Crate", "SM_TN_Prop_Bench", "SM_TN_Prop_Crate"])
        self.settings.pack = "town"
        self.settings.tag = "wood"
        self.search()
        self.assertEqual(self.listed(), ["SM_TN_Prop_Bench"])
        self.settings.pack = "CURRENT"
        self.settings.tag = "ANY"
        self.settings.status = "exported"
        self.search()
        self.assertEqual(self.listed(), ["SM_KT_Prop_Crate"])

    def test_licence_filter(self):
        self.settings.license = "CC0-1.0"
        self.search()
        self.assertEqual(self.listed(), ["SM_KT_Wall_Brick"])

    def test_favourite_toggles_and_filters(self):
        self.pick("SM_KT_Prop_Crate")
        self.assertEqual(bpy.ops.pipeline.assets_favourite(), {"FINISHED"})
        self.assertTrue(self.settings.results[self.settings.index].favourite)
        self.settings.favourites_only = True
        self.search()
        self.assertEqual(self.listed(), ["SM_KT_Prop_Crate"])
        self.settings.index = 0
        bpy.ops.pipeline.assets_favourite()
        self.search()
        self.assertEqual(self.listed(), [])

    def test_nothing_picked_is_reported(self):
        self.settings.results.clear()
        with self.assertRaises(RuntimeError):
            bpy.ops.pipeline.assets_favourite()

    def test_deprecate_and_restore_from_the_panel(self):
        self.pick("SM_KT_Wall_Brick")
        self.settings.replaced_by = "SM_KT_Wall_Plaster"
        self.settings.reason = "cracks"
        self.assertEqual(bpy.ops.pipeline.assets_deprecate(), {"FINISHED"})
        stored = pack_library.open_packs(self.cfg()).get(self.wall)
        self.assertEqual((stored["status"], stored["deprecation"]["replaced_by"]), ("deprecated", self.new_wall))
        self.assertEqual(self.settings.results[self.settings.index].status, "deprecated")
        self.assertEqual(bpy.ops.pipeline.assets_restore(), {"FINISHED"})
        self.assertEqual(pack_library.open_packs(self.cfg()).get(self.wall)["status"], "draft")

    def test_a_bad_replacement_is_reported_and_changes_nothing(self):
        self.pick("SM_KT_Wall_Brick")
        self.settings.replaced_by = "no such asset"
        with self.assertRaises(RuntimeError):
            bpy.ops.pipeline.assets_deprecate()
        self.assertEqual(pack_library.open_packs(self.cfg()).get(self.wall)["status"], "draft")

    def test_catalogs_operator(self):
        self.assertEqual(bpy.ops.pipeline.assets_catalogs(), {"FINISHED"})
        self.assertTrue((self.root / "packs" / pack_library.CATALOG_FILE).exists())

    def test_every_panel_draws(self):
        self.pick("SM_KT_Wall_Brick")
        search = self.draw(ui_library.PIPELINE_PT_assets)
        self.assertIn("pipeline.assets_search", search.operators)
        details = self.draw(ui_library.PIPELINE_PT_asset_details)
        self.assertIn(self.wall, details.labels)
        self.assertIn("pipeline.assets_favourite", details.operators)
        depends = self.draw(ui_library.PIPELINE_PT_asset_dependencies)
        self.assertIn("Used by (2)", depends.labels)
        self.assertIn(f"   {self.street}", depends.labels)
        self.assertIn("Used through those (1)", depends.labels)
        self.assertIn("Materials (1)", depends.labels)
        history = self.draw(ui_library.PIPELINE_PT_asset_history)
        self.assertTrue(history.labels[0].startswith("v1 draft, created"))
        retire = self.draw(ui_library.PIPELINE_PT_asset_deprecate)
        self.assertIn("pipeline.assets_deprecate", retire.operators)
        self.assertIn("Built structures (2)", retire.labels)
        self.assertIn("Specs naming it (1)", retire.labels)
        self.assertIn("Specs that may pick it (1)", retire.labels)

    def test_a_deprecated_asset_offers_restore(self):
        self.pick("SM_KT_Wall_Brick")
        self.settings.replaced_by = "SM_KT_Wall_Plaster"
        bpy.ops.pipeline.assets_deprecate()
        retire = self.draw(ui_library.PIPELINE_PT_asset_deprecate)
        self.assertEqual(retire.operators, ["pipeline.assets_restore"])
        self.assertIn(f"Replaced by {self.new_wall}", retire.labels)

    def test_a_long_list_is_cut_short(self):
        layout = FakeLayout()
        ui_library._rows(layout, "Many", [str(i) for i in range(20)])
        self.assertEqual(layout.labels[0], "Many (20)")
        self.assertEqual(layout.labels[-1], "   and 8 more")

    def test_the_list_row_draws(self):
        layout = FakeLayout()
        item = SimpleNamespace(favourite=True, label="A", version=3, status="exported")
        ui_library.PIPELINE_UL_assets.draw_item(None, bpy.context, layout, None, item, 0, None, "", 0)
        self.assertEqual(layout.labels, ["", "A", "v3 exported"])


class CheckTests(Fixture):
    def problems(self, folder):
        cfg = self.cfg(folder)
        return ar.check(asset_index.open_for(cfg), cfg, library=False)

    def test_an_asset_in_another_pack_is_found(self):
        self.assertEqual([p for p in self.problems(self.town_dir) if p.level == "error"], [])

    def test_a_reference_no_pack_holds_is_still_an_error(self):
        index = asset_index.AssetIndex(self.town_dir / "asset_index.json")
        index.get(self.bench)["dependencies"] = ["asset:kit/Gone"]
        index.save()
        self.assertIn("kit/Gone", " ".join(p.message for p in self.problems(self.town_dir) if p.level == "error"))

    def test_using_a_deprecated_asset_is_a_warning_naming_the_replacement(self):
        pack_library.open_packs(self.cfg()).deprecate(self.wall, self.new_wall)
        found = [p for p in self.problems(self.town_dir) if "deprecated" in p.message]
        self.assertEqual([(p.level, p.asset) for p in found], [("warning", self.street)])
        self.assertIn(self.new_wall, found[0].message)


class CliTests(Fixture):
    def run_cli(self, *args):
        tool = Path(pipeline.__file__).parent / "cli.py"
        config = self.kit_dir / "pipeline.config.json"
        proc = subprocess.run([bpy.app.binary_path, "-b", "--factory-startup", "--python", str(tool), "--", "assets", "--config", str(config), *args],
                              capture_output=True, text=True, timeout=300)
        return proc.returncode, proc.stdout

    def test_search_and_views(self):
        code, out = self.run_cli("--text", "brick", "--category", "Wall", "--kind", "generated", "--tag", "brick,exterior")
        self.assertEqual(code, 0, out[-2000:])
        self.assertIn("kit/SM_KT_Wall_Brick\n", out)
        self.assertNotIn("SM_KT_Wall_Plaster", out)
        self.assertIn("1 of 7 asset(s) listed", out)

        code, out = self.run_cli("--pack", "all", "--category", "Prop")
        self.assertIn("town/SM_TN_Prop_Bench", out)

        code, out = self.run_cli("--packs")
        self.assertIn("kit: Kit, 7 asset(s)", out)

        code, out = self.run_cli("--deps", "SM_KT_Wall_Brick")
        self.assertEqual(code, 0, out[-2000:])
        self.assertIn("uses material: M_Red_Clay", out)
        self.assertIn(f"used by: {self.street}", out)
        self.assertIn(f"used through those by: {self.house}", out)
        self.assertIn("spec kit/named: names it in a", out)

        code, out = self.run_cli("--history", "SM_KT_Wall_Brick")
        self.assertRegex(out, r"v1 +draft +created")

    def test_favourite_and_deprecate(self):
        self.run_cli("--favourite", "SM_KT_Prop_Crate")
        code, out = self.run_cli("--favourites")
        self.assertIn("* exported", out)
        self.assertIn("1 of 7", out)

        code, out = self.run_cli("--deprecate", "SM_KT_Wall_Brick", "--dry-run")
        self.assertIn(f"built structure: {self.house}", out)
        self.assertEqual(self.library().get(self.wall)["status"], "draft")

        code, out = self.run_cli("--deprecate", "SM_KT_Wall_Brick", "--replaced-by", "SM_KT_Wall_Short", "--reason", "cracks")
        self.assertEqual(code, 0, out[-2000:])
        self.assertIn(f"replaced by {self.short_wall}", out)
        self.assertIn("won't fit its slots", out)
        self.assertEqual(self.library().get(self.wall)["deprecation"]["reason"], "cracks")

        code, out = self.run_cli("--status", "draft")
        self.assertIn(f"uses deprecated asset {self.wall}; replaced by {self.short_wall}", out)

        code, out = self.run_cli("--restore", "SM_KT_Wall_Brick")
        self.assertIn("draft", out)

    def test_bad_input_says_so(self):
        code, out = self.run_cli("--deps", "SM_KT_Wall_Bric")
        self.assertEqual(code, 2)
        code, out = self.run_cli("--status", "shipped")
        self.assertEqual(code, 2)


class KitCatalogTests(KitTestCase):
    def test_every_marked_asset_keeps_its_catalog_in_the_merged_file(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / pack_library.CATALOG_FILE
            result = pack_library.merge_catalogs(pack_library.open_packs(self.cfg), out)
            merged = pack_library.parse_catalogs(out.read_text(encoding="utf-8"))
        self.assertIn("general_store", result["by_pack"])
        marked = [o for o in bpy.data.objects if o.asset_data is not None and pipeline.utils.is_asset(o)]
        self.assertTrue(marked)
        for obj in marked:
            self.assertIn(obj.asset_data.catalog_id, merged, obj.name)
            self.assertTrue(merged[obj.asset_data.catalog_id][0].startswith("General Store"), obj.name)

    def test_the_real_packs_are_found(self):
        library = pack_library.open_packs(self.cfg)
        self.assertEqual(library.current, "general_store")
        self.assertEqual(library.problems, [])
        self.assertEqual(library.root, ROOT / "packs")
        self.assertEqual(library.packs["general_store"].folder, PACK)

    def test_the_assemblies_a_kit_piece_touches(self):
        library = pack_library.open_packs(self.cfg)
        affected = library.affected_assemblies("SM_GS_Pier_BrickCorner")
        self.assertTrue(affected["structures"])
        self.assertEqual(library.problems, [])
