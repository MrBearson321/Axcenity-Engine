#!/usr/bin/env python3
"""XB10k: the cross-engine scene's formulas, its checksum, and writers.

    python generate.py check              # print the expected checksum
    python generate.py axcenity <folder>  # write the Axcenity project
    python generate.py roblox <file.rbxmx> # write the Roblox model (scripts only)

The formulas are in README.md; every engine's builder implements the same
ones and prints `XB10K_CHECK` so a wrong scene shows up at once.
"""

import json
import os
import shutil
import sys

SIDE = 95
SPACING = 4
GRID = 8899
SPINNERS = 100
DROPS = 1000
BLINKERS = 97
BLINK_STRIDE = GRID // BLINKERS  # 91

FLOOR = {"name": "Floor", "shape": "Block", "pos": (0.0, -0.5, 0.0),
         "size": (440.0, 1.0, 440.0), "color": (0.35, 0.37, 0.40), "kind": "static"}


def grid_object(i):
    c, r = i % SIDE, i // SIDE
    x = c * SPACING - (SIDE - 1) * SPACING / 2
    z = r * SPACING - (SIDE - 1) * SPACING / 2
    h = 1 + (i * 7 % 5)
    hue = (i % 8) / 8
    spinner = i >= GRID
    return {
        "name": f"S{i - GRID}" if spinner else f"G{i}",
        "shape": "Block" if spinner else ("Block", "Cylinder", "Wedge")[i % 3],
        "pos": (x, h / 2, z),
        "size": (2.0, float(h), 2.0),
        "color": (hue, 1 - hue, 0.5 + hue / 2),
        "kind": "kinematic" if spinner else "static",
    }


def drop_object(j):
    x, y, z = j % 10, j // 100, j // 10 % 10
    return {
        "name": f"D{j}",
        "shape": "Ball" if j % 2 == 0 else "Block",
        "pos": (x * 3 - 13.5, 12 + y * 3, z * 3 - 13.5),
        "size": (1.5, 1.5, 1.5),
        "color": (0.95, 0.6, 0.2),
        "kind": "dynamic",
    }


def objects():
    yield FLOOR
    for i in range(GRID + SPINNERS):
        yield grid_object(i)
    for j in range(DROPS):
        yield drop_object(j)


def checksum():
    count, total = 0, 0.0
    for o in objects():
        count += 1
        total += sum(o["pos"]) + sum(o["size"])
    return count, round(total, 2)


def spinner_speed(k):
    return 30 + k % 7 * 20


def blinker(k):
    """The grid block blinker k watches, and its wait in seconds."""
    return k * BLINK_STRIDE, round(0.1 + (k % 5) * 0.1, 1)


# --- Axcenity ---------------------------------------------------------------

def _part(o):
    props = {
        "Anchored": o["kind"] != "dynamic",
        "Position": list(o["pos"]),
        "Size": list(o["size"]),
        "Shape": o["shape"],
        "Color": [round(v, 6) for v in o["color"]],
    }
    return {"class": "Part", "name": o["name"], "properties": props}


def _folder(name, children):
    return {"class": "Folder", "name": name, "children": children}


SPIN_LUA = """-- XB10k: turns spinner S{k} every Heartbeat.
local RunService = game:GetService("RunService")
local part = workspace.Spinners:FindFirstChild("S{k}")
local angle = 0
RunService.Heartbeat:Connect(function(deltaTime)
    angle = (angle + {speed} * deltaTime) % 360
    part.Orientation = Vector3.new(0, angle, 0)
end)
"""

BLINK_LUA = """-- XB10k: wakes every {wait} s, reads grid block G{block} and recolors it.
local part = workspace.Grid:FindFirstChild("G{block}")
local base = part.Color
local count = 0
while true do
    task.wait({wait})
    count += 1
    local height = part.Position.Y
    if count % 2 == 0 and height > 0 then
        part.Color = base
    else
        part.Color = Color3.new(1, 0.5, 0)
    end
end
"""

DROPS_LUA = """-- XB10k: lifts 100 drops back up every second, so some always fall.
local drops = {}
for j = 0, 999 do
    drops[j + 1] = workspace.Drops:FindFirstChild("D" .. j)
end
local n = 1
while true do
    task.wait(1)
    for _ = 1, 100 do
        local drop = drops[n]
        local x = (n % 10) * 3 - 13.5
        local z = (math.floor(n / 10) % 10) * 3 - 13.5
        drop.Position = Vector3.new(x, 40, z)
        drop.AssemblyLinearVelocity = Vector3.zero
        drop.AssemblyAngularVelocity = Vector3.zero
        n = n % #drops + 1
    end
end
"""

CHECK_LUA = """-- XB10k: prints the scene's checksum, as every engine's builder does.
local count, total = 0, 0
local function add(part)
    count += 1
    local p, s = part.Position, part.Size
    total += p.X + p.Y + p.Z + s.X + s.Y + s.Z
end
add(workspace.Floor)
for _, folder in { workspace.Grid, workspace.Spinners, workspace.Drops } do
    for _, part in folder:GetChildren() do
        add(part)
    end
end
print(string.format("XB10K_CHECK count=%d sum=%.2f", count, total))
"""

CAMERA_LUA = """-- XB10k: the fixed camera every engine uses.
local camera = workspace.CurrentCamera
camera.CameraType = Enum.CameraType.Scriptable
camera.FieldOfView = 70
camera.CFrame = CFrame.lookAt(Vector3.new(-260, 180, -260), Vector3.new(0, 0, 0))
"""


SPEC_LUA = """describe("XB10k", function()
    it("builds 10,000 objects and the scripts run", function()
        expect(#workspace.Grid:GetChildren()).toBe(8899)
        expect(#workspace.Spinners:GetChildren()).toBe(100)
        expect(#workspace.Drops:GetChildren()).toBe(1000)
        local spinner = workspace.Spinners.S0
        local blink = workspace.Grid.G0
        local base = blink.Color
        Test.step(0.15)
        expect(blink.Color == base).toBe(false)
        Test.step(0.9)
        expect(spinner.Orientation.Y).toBeGreaterThan(20)
        -- The lifter moved drop D0 (n = 1) to height 40 at t = 1 s.
        expect(workspace.Drops.D0.Position.Y).toBeGreaterThan(30)
        Test.step(2)
        expect(workspace.Drops.D199.Position.Y).toBeGreaterThan(-5)
    end)
end)
"""


def lua_scripts():
    files = {}
    for k in range(SPINNERS):
        files[f"spin{k:03}.server.lua"] = SPIN_LUA.format(k=k, speed=spinner_speed(k))
    for k in range(BLINKERS):
        block, wait = blinker(k)
        files[f"blink{k:03}.server.lua"] = BLINK_LUA.format(block=block, wait=wait)
    files["drops.server.lua"] = DROPS_LUA
    files["check.server.lua"] = CHECK_LUA
    files["camera.client.lua"] = CAMERA_LUA
    return files


def write_axcenity(folder):
    os.makedirs(folder, exist_ok=True)
    shutil.rmtree(os.path.join(folder, "scripts"), ignore_errors=True)
    os.makedirs(os.path.join(folder, "scripts"))
    with open(os.path.join(folder, "axcenity.json"), "w") as f:
        json.dump({"name": "XB10k", "format_version": 4}, f, indent=2)
    grid = [_part(grid_object(i)) for i in range(GRID)]
    spinners = [_part(grid_object(i)) for i in range(GRID, GRID + SPINNERS)]
    drops = [_part(drop_object(j)) for j in range(DROPS)]
    world = {
        "format_version": 1,
        "workspace": {"children": [
            _part(FLOOR),
            _folder("Grid", grid),
            _folder("Spinners", spinners),
            _folder("Drops", drops),
        ]},
        # Sun shadows over the whole scene (README: "Sun shadows reach 700 units").
        "lighting": {"properties": {"ShadowDistance": 700}},
        # No character: the other engines have no player either.
        "players": {"properties": {"CharacterAutoLoads": False}},
    }
    with open(os.path.join(folder, "world.json"), "w") as f:
        json.dump(world, f, indent=1)
    for name, source in lua_scripts().items():
        with open(os.path.join(folder, "scripts", name), "w") as f:
            f.write(source)
    # The player's graphics settings live outside the game (per machine), so
    # the run points AXCENITY_SAVE_DIR at <folder>/save: default Automatic
    # quality, with sun shadows over the whole scene.
    save = os.path.join(folder, "save", "XB10k")
    os.makedirs(save, exist_ok=True)
    with open(os.path.join(save, "graphics.json"), "w") as f:
        json.dump({"quality": "High", "automatic": True, "shadow_distance": 700.0}, f, indent=2)
    # A game test that the scene and its scripts work: axcenity-test <folder>
    os.makedirs(os.path.join(folder, "tests"), exist_ok=True)
    with open(os.path.join(folder, "tests", "xb10k.spec.lua"), "w") as f:
        f.write(SPEC_LUA)


# --- Roblox -----------------------------------------------------------------

def _xml_escape(text):
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def write_roblox(path):
    """A model file of Scripts to drop into ServerScriptService: the
    builder and the per-object scripts it clones (see roblox/README.md)."""
    here = os.path.dirname(os.path.abspath(__file__))
    sources = {}
    for name in ("Build.server.lua", "Spin.server.lua", "Blink.server.lua",
                 "Drops.server.lua", "Timer.client.lua"):
        with open(os.path.join(here, "roblox", name)) as f:
            sources[name] = f.read()
    items = []
    ref = 0
    for name, source in sources.items():
        base, kind = name.split(".")[0], name.split(".")[1]
        cls = "LocalScript" if kind == "client" else "Script"
        ref += 1
        disabled = "false" if base in ("Build", "Timer") else "true"
        items.append(
            f'<Item class="{cls}" referent="RBX{ref}"><Properties>'
            f'<string name="Name">XB10k{base}</string>'
            f'<bool name="Disabled">{disabled}</bool>'
            f'<ProtectedString name="Source"><![CDATA[{source}]]></ProtectedString>'
            f'</Properties></Item>')
    xml = ('<roblox xmlns:xmime="http://www.w3.org/2005/05/xmlmime" version="4">'
           '<Item class="Folder" referent="RBX0"><Properties>'
           '<string name="Name">XB10k</string></Properties>'
           + "".join(items) + "</Item></roblox>")
    with open(path, "w") as f:
        f.write(xml)


def main(argv):
    if len(argv) >= 2 and argv[1] == "check":
        count, total = checksum()
        print(f"XB10K_CHECK count={count} sum={total:.2f}")
    elif len(argv) == 3 and argv[1] == "axcenity":
        write_axcenity(argv[2])
        print(f"Wrote the XB10k Axcenity project to {argv[2]}")
    elif len(argv) == 3 and argv[1] == "roblox":
        write_roblox(argv[2])
        print(f"Wrote the XB10k Roblox model to {argv[2]}")
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
