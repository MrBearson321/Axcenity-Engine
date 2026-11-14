"""Builds the Axcenity default humanoid in Blender, from nothing.

Run headless:  blender -b --factory-startup --python build.py -- --out DIR
(or with the `bpy` module: python build.py --out DIR)

Everything is generated: skeleton, body, head and face, hair styles,
outfit, materials, skin weights, face shape keys, LODs. Units are metres,
Blender Z up, the character faces -Y (glTF +Z after export).
"""

import math
import os
import sys

import bpy  # noqa: F811 (bpy must load before bmesh)
import bmesh
import bpy
from mathutils import Matrix, Vector

# ---------------------------------------------------------------------------
# Proportions (metres). Total height 1.75, head 0.27 (6.5 heads).
# ---------------------------------------------------------------------------

HEIGHT = 1.75
HEAD_H = 0.27
CHIN_Z = HEIGHT - HEAD_H  # 1.48
ARM_DOWN = math.radians(45.0)  # relaxed A-pose: arms 45 deg below horizontal


def v(x, y, z):
    return Vector((x, y, z))


def arm_point(side, shoulder, length_along):
    """A point along the A-posed arm from `shoulder`."""
    d = v(side * math.cos(ARM_DOWN), 0.0, -math.sin(ARM_DOWN))
    return shoulder + d * length_along


def skeleton_joints():
    """Bone name -> (head, tail, parent). Unity/VRM humanoid names."""
    j = {}
    j["Root"] = (v(0, 0, 0), v(0, -0.12, 0), None)
    hips = v(0, 0.0, 0.94)
    j["Hips"] = (hips, v(0, 0.0, 1.03), "Root")
    j["Spine"] = (v(0, 0.0, 1.03), v(0, 0.005, 1.15), "Hips")
    j["Chest"] = (v(0, 0.005, 1.15), v(0, 0.01, 1.27), "Spine")
    j["UpperChest"] = (v(0, 0.01, 1.27), v(0, 0.015, 1.41), "Chest")
    j["Neck"] = (v(0, 0.015, 1.41), v(0, 0.005, 1.505), "UpperChest")
    j["Head"] = (v(0, 0.005, 1.505), v(0, 0.005, HEIGHT), "Neck")
    j["Jaw"] = (v(0, -0.0, 1.535), v(0, -0.075, 1.495), "Head")
    for side, s in (("Left", 1), ("Right", -1)):
        eye = v(s * 0.036, -0.082, 1.612)
        j[f"{side}Eye"] = (eye, eye + v(0, -0.03, 0), "Head")
        clav = v(s * 0.022, 0.0, 1.385)
        shoulder = v(s * 0.175, 0.012, 1.395)
        j[f"{side}Shoulder"] = (clav, shoulder, "UpperChest")
        elbow = arm_point(s, shoulder, 0.28)
        wrist = arm_point(s, shoulder, 0.28 + 0.245)
        j[f"{side}UpperArm"] = (shoulder, elbow, f"{side}Shoulder")
        j[f"{side}LowerArm"] = (elbow, wrist, f"{side}UpperArm")
        hand_end = arm_point(s, shoulder, 0.28 + 0.245 + 0.085)
        j[f"{side}Hand"] = (wrist, hand_end, f"{side}LowerArm")
        hip = v(s * 0.092, 0.0, 0.885)
        knee = v(s * 0.098, -0.005, 0.485)
        ankle = v(s * 0.104, 0.02, 0.085)
        toe = v(s * 0.108, -0.135, 0.025)
        j[f"{side}UpperLeg"] = (hip, knee, "Hips")
        j[f"{side}LowerLeg"] = (knee, ankle, f"{side}UpperLeg")
        j[f"{side}Foot"] = (ankle, toe, f"{side}LowerLeg")
        j[f"{side}Toes"] = (toe, toe + v(0, -0.07, 0), f"{side}Foot")
        for name, bones in finger_layout(s, wrist, shoulder).items():
            parent = f"{side}Hand"
            for seg, (a, b) in zip(("Proximal", "Intermediate", "Distal"), bones):
                bone = f"{side}{name}{seg}"
                j[bone] = (a, b, parent)
                parent = bone
    return j


def hand_frame(side, shoulder):
    """Unit vectors for a hand in the A-pose: along (toward fingertips),
    across (thumb side, toward the front), palm normal (toward the body)."""
    along = v(side * math.cos(ARM_DOWN), 0.0, -math.sin(ARM_DOWN))
    across = v(0, -1, 0)  # thumb points forward in a relaxed A-pose
    palm = along.cross(across).normalized() * side  # toward the body
    return along, across, palm


def finger_layout(side, wrist, shoulder):
    """Finger bone segments from the wrist. Slightly big, stylized hands."""
    along, across, palm = hand_frame(side, shoulder)
    fingers = {}
    # (name, knuckle offset across, knuckle distance along, segment lengths, splay)
    spec = [
        ("Index", 0.030, 0.092, (0.040, 0.026, 0.022), 0.10),
        ("Middle", 0.010, 0.096, (0.044, 0.029, 0.023), 0.0),
        ("Ring", -0.010, 0.092, (0.041, 0.027, 0.021), -0.08),
        ("Little", -0.029, 0.083, (0.032, 0.021, 0.019), -0.18),
    ]
    for name, off, dist, segs, splay in spec:
        base = wrist + along * dist + across * off
        direction = (along + across * splay).normalized()
        # A relaxed hand: each segment curls a little toward the palm.
        bones = []
        p = base
        curl = 0.0
        for length in segs:
            curl += 0.16
            d = (direction * math.cos(curl) + palm * math.sin(curl)).normalized()
            q = p + d * length
            bones.append((p, q))
            p = q
        fingers[name] = bones
    # Thumb: from the base of the palm, toward the front and along.
    t0 = wrist + along * 0.022 + across * 0.022 + palm * 0.012
    t_dir = (along * 0.55 + across * 0.75 + palm * 0.35).normalized()
    t_lengths = (0.036, 0.030, 0.026)
    p = t0
    bones = []
    bend = 0.0
    for length in t_lengths:
        q = p + t_dir * length
        bones.append((p, q))
        bend += 0.18
        t_dir = (t_dir + along * 0.25 + palm * 0.1).normalized()
        p = q
    fingers["Thumb"] = bones
    return fingers


# ---------------------------------------------------------------------------
# Mesh helpers
# ---------------------------------------------------------------------------

def lerp(a, b, t):
    return a + (b - a) * t


def smoothstep(e0, e1, x):
    t = max(0.0, min(1.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def interp_table(xs, ys, x):
    """Linear interpolation in a sorted table, clamped."""
    if x <= xs[0]:
        return ys[0]
    for i in range(1, len(xs)):
        if x <= xs[i]:
            t = (x - xs[i - 1]) / (xs[i] - xs[i - 1])
            return lerp(ys[i - 1], ys[i], t)
    return ys[-1]


class MeshBuilder:
    """Collects vertices and faces (with a material name per face)."""

    def __init__(self):
        self.verts = []
        self.faces = []
        self.mats = []
        self.tags = []  # per-vertex dict, e.g. region / mirror helpers

    def add_vert(self, p, tag=None):
        self.verts.append(Vector(p))
        self.tags.append(tag or {})
        return len(self.verts) - 1

    def add_face(self, idx, mat):
        self.faces.append(tuple(idx))
        self.mats.append(mat)

    def loft(self, rings, mat, closed=True, cap_start=False, cap_end=False,
             tags=None):
        """Quads between consecutive rings of equal length. Returns the
        vertex index rings."""
        idx_rings = []
        for k, ring in enumerate(rings):
            tag = tags[k] if tags else None
            idx_rings.append([self.add_vert(p, dict(tag) if tag else None) for p in ring])
        n = len(rings[0])
        for k in range(len(idx_rings) - 1):
            a, b = idx_rings[k], idx_rings[k + 1]
            count = n if closed else n - 1
            for i in range(count):
                j = (i + 1) % n
                self.add_face((a[i], a[j], b[j], b[i]), mat)
        if cap_start:
            self.cap(idx_rings[0], mat, flip=True)
        if cap_end:
            self.cap(idx_rings[-1], mat, flip=False)
        return idx_rings

    def cap(self, ring, mat, flip=False, tip=None):
        """Closes a ring with a fan around its centre (or `tip`)."""
        centre = sum((self.verts[i] for i in ring), Vector()) / len(ring)
        if tip is not None:
            centre = Vector(tip)
        c = self.add_vert(centre, dict(self.tags[ring[0]]))
        n = len(ring)
        for i in range(n):
            j = (i + 1) % n
            f = (ring[i], ring[j], c) if not flip else (ring[j], ring[i], c)
            self.add_face(f, mat)
        return c

    def merge(self, other):
        off = len(self.verts)
        self.verts += other.verts
        self.tags += other.tags
        for f, m in zip(other.faces, other.mats):
            self.faces.append(tuple(i + off for i in f))
            self.mats.append(m)
        return off

    def to_object(self, name, collection=None, flat=True):
        me = bpy.data.meshes.new(name)
        me.from_pydata([tuple(p) for p in self.verts], [], self.faces)
        mat_names = []
        for m in self.mats:
            if m not in mat_names:
                mat_names.append(m)
        for m in mat_names:
            me.materials.append(material(m))
        for poly, m in zip(me.polygons, self.mats):
            poly.material_index = mat_names.index(m)
            poly.use_smooth = not flat
        me.validate()
        me.update()
        obj = bpy.data.objects.new(name, me)
        (collection or bpy.context.scene.collection).objects.link(obj)
        obj["tags"] = None
        return obj


def apply_modifiers(obj, mb):
    """Applies `obj`'s modifiers to its mesh and returns a MeshBuilder whose
    vertices are the new mesh's, each tagged like the nearest original
    vertex (so weights can be worked out for what a modifier made)."""
    from mathutils.kdtree import KDTree
    dg = bpy.context.evaluated_depsgraph_get()
    new_mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    old = obj.data
    obj.modifiers.clear()
    obj.data = new_mesh
    new_mesh.name = old.name
    bpy.data.meshes.remove(old)
    tree = KDTree(len(mb.verts))
    for i, p in enumerate(mb.verts):
        tree.insert(p, i)
    tree.balance()
    out = MeshBuilder()
    for vtx in new_mesh.vertices:
        _, i, _ = tree.find(vtx.co)
        out.add_vert(vtx.co.copy(), dict(mb.tags[i]))
    return out


def ring_points(centre, u, w, n, radius, phase=0.0):
    """`n` points around `centre` in the plane of unit vectors u, w.
    radius(angle) -> (ru, rw); angle 0 is +u."""
    pts = []
    for i in range(n):
        a = phase + 2 * math.pi * i / n
        ru, rw = radius(a)
        pts.append(centre + u * (math.cos(a) * ru) + w * (math.sin(a) * rw))
    return pts


def superellipse(rx, ry, p=2.6):
    """Radius function for a rounded-rectangle-like ring."""

    def f(a):
        c, s = math.cos(a), math.sin(a)
        k = (abs(c) ** p + abs(s) ** p) ** (-1.0 / p)
        return (rx * k, ry * k)

    return f


def frame_for(direction):
    """Two unit vectors perpendicular to `direction`."""
    d = direction.normalized()
    up = Vector((0, 0, 1)) if abs(d.z) < 0.9 else Vector((0, 1, 0))
    u = d.cross(up).normalized()
    w = d.cross(u).normalized()
    return u, w


def tube(points, radii, n, mat, cap_start=True, cap_end=True, p=2.0,
         up_hint=None, tags=None):
    """A loft along a polyline; radii are (ru, rw) per point."""
    mb = MeshBuilder()
    rings = []
    for k, c in enumerate(points):
        if k == 0:
            d = points[1] - points[0]
        elif k == len(points) - 1:
            d = points[-1] - points[-2]
        else:
            d = (points[k + 1] - points[k - 1])
        d.normalize()
        if up_hint is not None:
            u = d.cross(up_hint).normalized()
            w = d.cross(u).normalized()
        else:
            u, w = frame_for(d)
        ru, rw = radii[k]
        rings.append(ring_points(c, u, w, n, superellipse(ru, rw, p)))
    mb.loft(rings, mat, cap_start=cap_start, cap_end=cap_end, tags=tags)
    return mb


# ---------------------------------------------------------------------------
# Materials (metallic-roughness, flat colours; customised per material name)
# ---------------------------------------------------------------------------

PALETTE = {
    # name: (sRGB colour, roughness, metalness)
    "Skin": ((0.80, 0.56, 0.42), 0.62, 0.0),
    "Lips": ((0.74, 0.46, 0.38), 0.55, 0.0),
    "Eye": ((0.95, 0.94, 0.92), 0.25, 0.0),
    "Iris": ((0.27, 0.45, 0.62), 0.2, 0.0),
    "Pupil": ((0.03, 0.03, 0.035), 0.2, 0.0),
    "Mouth": ((0.32, 0.12, 0.12), 0.7, 0.0),
    "Teeth": ((0.93, 0.91, 0.86), 0.35, 0.0),
    "Hair": ((0.23, 0.15, 0.10), 0.7, 0.0),
    "Brow": ((0.20, 0.13, 0.09), 0.8, 0.0),
    "Jacket": ((0.26, 0.29, 0.33), 0.82, 0.0),
    "Shirt": ((0.80, 0.78, 0.72), 0.85, 0.0),
    "Trousers": ((0.15, 0.16, 0.19), 0.85, 0.0),
    "Shoes": ((0.20, 0.20, 0.22), 0.6, 0.0),
    "Sole": ((0.86, 0.84, 0.80), 0.7, 0.0),
    "AccentCyan": ((0.10, 0.74, 0.82), 0.5, 0.0),
    "AccentOrange": ((0.96, 0.52, 0.16), 0.5, 0.0),
    "Underwear": ((0.30, 0.33, 0.38), 0.85, 0.0),
}


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def material(name):
    mat = bpy.data.materials.get(name)
    if mat:
        return mat
    colour, rough, metal = PALETTE[name]
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    lin = tuple(srgb_to_linear(c) for c in colour) + (1.0,)
    bsdf.inputs["Base Color"].default_value = lin
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    mat.diffuse_color = lin
    return mat


# ---------------------------------------------------------------------------
# Head
# ---------------------------------------------------------------------------

HEAD_AXIS_Y = 0.008
HEAD_N = 32  # segments around; index 0 faces front (-Y)
PROFILE_ANGLES = [0, 20, 40, 60, 80, 100, 130, 160, 180]
# (z, radius at each PROFILE_ANGLES) from the neck to the crown.
HEAD_ROWS = [
    (1.360, [0.054, 0.054, 0.054, 0.054, 0.055, 0.056, 0.058, 0.060, 0.060]),
    (1.430, [0.051, 0.051, 0.051, 0.051, 0.052, 0.054, 0.057, 0.059, 0.059]),
    (1.465, [0.056, 0.055, 0.054, 0.054, 0.056, 0.058, 0.061, 0.064, 0.064]),
    (1.482, [0.088, 0.082, 0.064, 0.058, 0.059, 0.062, 0.066, 0.069, 0.069]),
    (1.497, [0.098, 0.093, 0.080, 0.070, 0.069, 0.070, 0.073, 0.076, 0.077]),
    (1.518, [0.099, 0.096, 0.087, 0.079, 0.077, 0.078, 0.082, 0.086, 0.087]),
    (1.536, [0.101, 0.098, 0.089, 0.083, 0.082, 0.083, 0.088, 0.093, 0.094]),  # mouth
    (1.558, [0.102, 0.099, 0.092, 0.087, 0.086, 0.087, 0.092, 0.098, 0.099]),
    (1.585, [0.097, 0.097, 0.097, 0.093, 0.090, 0.090, 0.097, 0.103, 0.104]),  # cheekbone
    (1.610, [0.091, 0.089, 0.090, 0.093, 0.091, 0.092, 0.100, 0.106, 0.107]),  # eyes
    (1.636, [0.099, 0.098, 0.096, 0.092, 0.091, 0.093, 0.102, 0.108, 0.109]),  # brow
    (1.665, [0.094, 0.094, 0.092, 0.090, 0.089, 0.092, 0.101, 0.107, 0.108]),
    (1.698, [0.082, 0.082, 0.082, 0.083, 0.083, 0.086, 0.094, 0.099, 0.100]),
    (1.728, [0.061, 0.062, 0.063, 0.065, 0.065, 0.068, 0.073, 0.077, 0.078]),
]
CROWN_Z = 1.750
MOUTH_ROW = 6
MOUTH_HALF_ANGLE = 22.5  # degrees either side of the front


def head_point(z, radii, a_deg):
    r = interp_table(PROFILE_ANGLES, radii, abs(a_deg))
    a = math.radians(a_deg)
    return Vector((math.sin(a) * r, HEAD_AXIS_Y - math.cos(a) * r, z))


def seg_angle(i):
    a = 360.0 * i / HEAD_N
    return a if a <= 180 else a - 360


def build_head():
    """The head and neck skin, with a mouth slit, plus nose and ears.
    Tags: part, row, angle."""
    mb = MeshBuilder()
    rows = []
    for r, (z, radii) in enumerate(HEAD_ROWS):
        ring = []
        for i in range(HEAD_N):
            a = seg_angle(i)
            p = head_point(z, radii, a)
            part = "neck" if r < 3 else "face"
            ring.append(mb.add_vert(p, {"part": part, "row": r, "angle": a}))
        rows.append(ring)
    # The mouth: the mouth row's front vertices are split in two, the upper
    # lip keeping the original and the lower lip a copy, so the lips can part.
    lower = list(rows[MOUTH_ROW])
    for i in range(HEAD_N):
        a = seg_angle(i)
        if abs(a) < MOUTH_HALF_ANGLE - 1e-6:
            p = mb.verts[rows[MOUTH_ROW][i]].copy()
            # At rest the lower lip sits a little back and down, so the
            # mouth reads as a line.
            p += Vector((0, 0.002, -0.0032)) * (1.0 - abs(a) / MOUTH_HALF_ANGLE)
            lower[i] = mb.add_vert(p, {"part": "face", "row": MOUTH_ROW, "angle": a,
                                       "lip": "lower"})
            mb.tags[rows[MOUTH_ROW][i]]["lip"] = "upper"
        elif abs(abs(a) - MOUTH_HALF_ANGLE) < 1e-6:
            mb.tags[rows[MOUTH_ROW][i]]["lip"] = "corner"
    for r in range(len(rows) - 1):
        a_ring = rows[r]
        b_ring = lower if r + 1 == MOUTH_ROW else rows[r + 1]
        for i in range(HEAD_N):
            j = (i + 1) % HEAD_N
            mat = "Skin"
            mb.add_face((a_ring[i], a_ring[j], b_ring[j], b_ring[i]), mat)
    # Crown.
    mb.cap(rows[-1], "Skin", tip=(0, HEAD_AXIS_Y + 0.004, CROWN_Z))
    mb.tags[-1] = {"part": "face", "row": len(rows), "angle": 0.0}
    # Neck bottom stays open (inside the collar / body).
    head = mb
    head.rows = rows
    head.lower_lip = lower

    # Nose: a stylised wedge.
    nose = MeshBuilder()
    y_face = HEAD_AXIS_Y - 0.091
    pts = {
        "bridge": (0.0, y_face + 0.004, 1.628),
        "bridge_l": (0.009, y_face + 0.006, 1.618),
        "bridge_r": (-0.009, y_face + 0.006, 1.618),
        "tip": (0.0, HEAD_AXIS_Y - 0.126, 1.568),
        "tip_l": (0.010, HEAD_AXIS_Y - 0.119, 1.564),
        "tip_r": (-0.010, HEAD_AXIS_Y - 0.119, 1.564),
        "wing_l": (0.020, HEAD_AXIS_Y - 0.103, 1.562),
        "wing_r": (-0.020, HEAD_AXIS_Y - 0.103, 1.562),
        "base": (0.0, HEAD_AXIS_Y - 0.108, 1.556),
        "back": (0.0, HEAD_AXIS_Y - 0.090, 1.556),
    }
    ids = {k: nose.add_vert(p, {"part": "nose"}) for k, p in pts.items()}
    f = lambda *names: tuple(ids[n] for n in names)
    for face in [
        f("bridge", "bridge_l", "tip_l", "tip"),
        f("bridge", "tip", "tip_r", "bridge_r"),
        f("bridge_l", "wing_l", "tip_l"),
        f("bridge_r", "tip_r", "wing_r"),
        f("tip", "tip_l", "base"),
        f("tip", "base", "tip_r"),
        f("tip_l", "wing_l", "base"),
        f("tip_r", "base", "wing_r"),
        f("wing_l", "back", "base"),
        f("wing_r", "base", "back"),
    ]:
        nose.add_face(face, "Skin")
    head.merge(nose)

    # Ears: bevelled plates, tilted back.
    for s in (1, -1):
        ear = MeshBuilder()
        c = Vector((s * 0.088, HEAD_AXIS_Y + 0.016, 1.592))
        outline = [(0.0, 0.034), (0.017, 0.024), (0.020, 0.0), (0.012, -0.024),
                   (0.0, -0.030), (-0.010, -0.012), (-0.010, 0.020)]
        tilt = math.radians(14)
        front, back = [], []
        for (u, w) in outline:
            # u: toward the back of the head, w: up; tilted back by `tilt`.
            uu = u * math.cos(tilt) + w * math.sin(tilt)
            ww = -u * math.sin(tilt) + w * math.cos(tilt)
            p_out = c + Vector((s * 0.016, uu, ww))
            p_in = c + Vector((-s * 0.002, uu * 0.8, ww * 0.85))
            front.append(ear.add_vert(p_out, {"part": "ear"}))
            back.append(ear.add_vert(p_in, {"part": "ear"}))
        n = len(outline)
        for i in range(n):
            j = (i + 1) % n
            face = (front[i], back[i], back[j], front[j])
            ear.add_face(face if s > 0 else face[::-1], "Skin")
        # Outer face with a slight hollow.
        hollow = ear.add_vert(c + Vector((s * 0.011, 0.004, 0.002)), {"part": "ear"})
        for i in range(n):
            j = (i + 1) % n
            face = (front[i], front[j], hollow)
            ear.add_face(face if s > 0 else face[::-1], "Skin")
        head.merge(ear)
    return head


def build_mouth_interior():
    """A dark mouth bag behind the lips, with upper and lower teeth."""
    mb = MeshBuilder()
    yf = HEAD_AXIS_Y - 0.085
    yb = HEAD_AXIS_Y - 0.035
    x0, z0, z1 = 0.034, 1.508, 1.566
    box = [
        (-x0, yf, z0), (x0, yf, z0), (x0, yb, z0), (-x0, yb, z0),
        (-x0, yf, z1), (x0, yf, z1), (x0, yb, z1), (-x0, yb, z1),
    ]
    ids = [mb.add_vert(p, {"part": "mouth", "jaw": p[2] < 1.54}) for p in box]
    # Inside faces (normals inward), front left open.
    for face in [(0, 3, 2, 1), (4, 5, 6, 7), (3, 7, 6, 2), (0, 4, 7, 3), (1, 2, 6, 5)]:
        mb.add_face(tuple(ids[i] for i in face)[::-1], "Mouth")
    # Teeth: shallow arcs.
    for upper in (True, False):
        zt = 1.548 if upper else 1.522
        h = 0.010
        ring_top, ring_bot = [], []
        for k in range(7):
            a = math.radians(-50 + 100 * k / 6)
            r = 0.030
            p = Vector((math.sin(a) * r, HEAD_AXIS_Y - 0.052 - math.cos(a) * r, 0))
            ring_top.append(mb.add_vert(p + Vector((0, 0, zt + h / 2)),
                                        {"part": "teeth", "jaw": not upper}))
            ring_bot.append(mb.add_vert(p + Vector((0, 0, zt - h / 2)),
                                        {"part": "teeth", "jaw": not upper}))
        for k in range(6):
            mb.add_face((ring_bot[k], ring_bot[k + 1], ring_top[k + 1], ring_top[k]), "Teeth")
    return mb


EYE_R = 0.0146


def eye_centre(s):
    return Vector((s * 0.037, HEAD_AXIS_Y - 0.077, 1.611))


def build_eye(s):
    """An eyeball: sclera, iris and pupil, front toward -Y."""
    mb = MeshBuilder()
    c = eye_centre(s)
    lat_n, lon_n = 8, 12
    # Polar angle from the front (-Y): pupil < 0.32 rad < iris < 0.62 rad.
    thetas = [0.0, 0.24, 0.42, 0.62, 0.78, 1.1, 1.6, 2.2, 2.8, math.pi]
    rings = []
    for th in thetas[1:-1]:
        ring = []
        for k in range(lon_n):
            ph = 2 * math.pi * k / lon_n
            d = Vector((math.sin(th) * math.cos(ph), -math.cos(th), math.sin(th) * math.sin(ph)))
            ring.append(mb.add_vert(c + d * EYE_R, {"part": "eye"}))
        rings.append(ring)
    front = mb.add_vert(c + Vector((0, -EYE_R, 0)), {"part": "eye"})
    back = mb.add_vert(c + Vector((0, EYE_R, 0)), {"part": "eye"})
    mats = ["Pupil", "Iris", "Iris", "Eye", "Eye", "Eye", "Eye", "Eye", "Eye"]
    for k in range(lon_n):
        j = (k + 1) % lon_n
        mb.add_face((front, rings[0][k], rings[0][j]), "Pupil")
    for r in range(len(rings) - 1):
        for k in range(lon_n):
            j = (k + 1) % lon_n
            mb.add_face((rings[r][k], rings[r + 1][k], rings[r + 1][j], rings[r][j]), mats[r + 1])
    for k in range(lon_n):
        j = (k + 1) % lon_n
        mb.add_face((rings[-1][k], back, rings[-1][j]), "Eye")
    return mb


def build_lids_and_brows():
    """Upper and lower eyelids (skin shells over each eye) and eyebrows."""
    mb = MeshBuilder()
    for s in (1, -1):
        c = eye_centre(s)
        side = "L" if s > 0 else "R"
        # Upper lid: a shell from the top of the eye down to its open edge.
        for lid, (th0, th1) in (("upper", (0.0, 1.26)), ("lower", (2.25, 2.75))):
            rr = EYE_R * 1.16
            lats = [th0 + (th1 - th0) * t for t in (0.0, 0.5, 1.0)]
            lons = [math.radians(a) for a in (-80, -45, -15, 15, 45, 80)]
            grid = []
            for th in lats:
                row = []
                for ph in lons:
                    # theta from straight up (+Z), phi around toward the front.
                    d = Vector((math.sin(th) * math.sin(ph) * s,
                                -math.sin(th) * math.cos(ph),
                                math.cos(th)))
                    if lid == "upper" and th == lats[-1]:
                        rr_e = rr * 1.02
                    else:
                        rr_e = rr
                    row.append(mb.add_vert(c + d * rr_e,
                                           {"part": "lid", "lid": lid, "side": side,
                                            "edge": th == (lats[-1] if lid == "upper" else lats[0])}))
                grid.append(row)
            for i in range(len(lats) - 1):
                for k in range(len(lons) - 1):
                    face = (grid[i][k], grid[i][k + 1], grid[i + 1][k + 1], grid[i + 1][k])
                    mb.add_face(face if s < 0 else face[::-1], "Skin")
            if lid == "upper":
                # A dark lash line along the lid's edge.
                edge = grid[-1]
                lash = []
                for k, vid in enumerate(edge):
                    p = mb.verts[vid]
                    d = (p - c).normalized()
                    q = c + d * (rr * 1.04) + Vector((0, 0, -0.0022))
                    lash.append(mb.add_vert(q, {"part": "lid", "lid": "upper", "side": side,
                                                 "edge": True}))
                for k in range(len(lons) - 1):
                    face = (edge[k], edge[k + 1], lash[k + 1], lash[k])
                    mb.add_face(face if s < 0 else face[::-1], "Brow")
        # Brow: a chunky tapered bar following the brow ridge.
        pts = []
        for t in (0.0, 0.33, 0.66, 1.0):
            x = s * lerp(0.014, 0.066, t)
            a = math.degrees(math.atan2(abs(x), 0.1))
            z = 1.646 + 0.005 * math.sin(t * math.pi) - 0.004 * t
            p = head_point(z, HEAD_ROWS[10][1], a * s)
            pts.append((p, lerp(0.0075, 0.0045, t)))
        top, bot, top_b, bot_b = [], [], [], []
        for p, h in pts:
            out = Vector((p.x, p.y - HEAD_AXIS_Y, 0)).normalized()
            top.append(mb.add_vert(p + Vector((0, 0, h)) + out * 0.006, {"part": "brow", "side": side}))
            bot.append(mb.add_vert(p - Vector((0, 0, h)) + out * 0.006, {"part": "brow", "side": side}))
            top_b.append(mb.add_vert(p + Vector((0, 0, h)) - out * 0.004, {"part": "brow", "side": side}))
            bot_b.append(mb.add_vert(p - Vector((0, 0, h)) - out * 0.004, {"part": "brow", "side": side}))
        for k in range(len(pts) - 1):
            quads = [
                (bot[k], bot[k + 1], top[k + 1], top[k]),
                (top[k], top[k + 1], top_b[k + 1], top_b[k]),
                (bot_b[k], bot_b[k + 1], bot[k + 1], bot[k]),
            ]
            for q in quads:
                mb.add_face(q if s < 0 else q[::-1], "Brow")
        for ends in ((bot[0], top[0], top_b[0], bot_b[0]), (bot[-1], bot_b[-1], top_b[-1], top[-1])):
            mb.add_face(ends if s < 0 else ends[::-1], "Brow")
    return mb


# ---------------------------------------------------------------------------
# Scene, preview renders
# ---------------------------------------------------------------------------

def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    return scene


def fix_normals(obj):
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()


def look_at(obj, target):
    d = Vector(target) - obj.location
    obj.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()


def render_views(path_prefix, target, distance, views, size=(640, 900), lens=50,
                 samples=24):
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = False
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.film_transparent = False
    world = bpy.data.worlds.new("World") if not scene.world else scene.world
    scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    bg.inputs[0].default_value = (0.23, 0.30, 0.34, 1)
    bg.inputs[1].default_value = 0.6
    if "Key" not in bpy.data.objects:
        for name, loc, energy, size_ in (("Key", (-1.5, -2.5, 3.0), 600, 1.5),
                                         ("Fill", (2.5, -1.5, 1.5), 180, 3.0),
                                         ("Rim", (0.5, 3.0, 2.8), 400, 1.0)):
            light = bpy.data.lights.new(name, "AREA")
            light.energy = energy
            light.size = size_
            lo = bpy.data.objects.new(name, light)
            scene.collection.objects.link(lo)
            lo.location = loc
            look_at(lo, (0, 0, 1.0))
    cam_data = bpy.data.cameras.new("Cam")
    cam_data.lens = lens
    cam = bpy.data.objects.new("Cam", cam_data)
    scene.collection.objects.link(cam)
    scene.camera = cam
    out = []
    for label, yaw in views:
        a = math.radians(yaw)
        cam.location = Vector(target) + Vector((math.sin(a) * distance, -math.cos(a) * distance, 0.0))
        look_at(cam, target)
        scene.render.filepath = f"{path_prefix}_{label}.png"
        bpy.ops.render.render(write_still=True)
        out.append(scene.render.filepath)
    bpy.data.objects.remove(cam)
    return out


# ---------------------------------------------------------------------------
# Hands (skin): a palm loft and a tube per finger, on the finger bones
# ---------------------------------------------------------------------------

def build_hand(side, joints):
    s = 1 if side == "Left" else -1
    shoulder = joints[f"{side}UpperArm"][0]
    wrist = joints[f"{side}Hand"][0]
    along, across, palm = hand_frame(s, shoulder)
    mb = MeshBuilder()
    rings = []
    tags = []
    # (distance along, half width, half thickness, shift toward the thumb)
    table = [(-0.035, 0.026, 0.022, 0.0), (0.0, 0.029, 0.021, 0.0),
             (0.035, 0.040, 0.021, 0.003), (0.070, 0.046, 0.019, 0.002),
             (0.096, 0.045, 0.016, 0.0)]
    for d, hw, ht, sh in table:
        c = wrist + along * d + across * sh
        rings.append(ring_points(c, across, palm, 8, superellipse(hw, ht, 3.0)))
        tags.append({"part": "hand", "side": side})
    mb.loft(rings, "Skin", cap_start=True, cap_end=True, tags=tags)
    for finger in ("Thumb", "Index", "Middle", "Ring", "Little"):
        pts = []
        for seg in ("Proximal", "Intermediate", "Distal"):
            pts.append(joints[f"{side}{finger}{seg}"][0])
        pts.append(joints[f"{side}{finger}Distal"][1])
        back = (pts[1] - pts[0]).normalized()
        pts[0] = pts[0] - back * 0.012
        thick = 0.0140 if finger == "Thumb" else (0.0098 if finger == "Little" else 0.0112)
        radii = [(thick, thick * 0.85), (thick * 0.95, thick * 0.82),
                 (thick * 0.88, thick * 0.78), (thick * 0.7, thick * 0.62)]
        t = tube(pts, radii, 6, "Skin", up_hint=palm,
                 tags=[{"part": "finger", "side": side, "finger": finger}] * 4)
        mb.merge(t)
    return mb


# ---------------------------------------------------------------------------
# Base body (under the clothes): a skin-modifier mannequin
# ---------------------------------------------------------------------------

def build_base_body(joints):
    verts, radii, edges = [], [], []

    def add(p, r, parent=None):
        verts.append(tuple(p))
        radii.append(r)
        i = len(verts) - 1
        if parent is not None:
            edges.append((parent, i))
        return i

    pelvis = add((0, 0.008, 0.90), (0.120, 0.092))
    waist = add((0, 0.004, 1.03), (0.108, 0.082), pelvis)
    chest = add((0, 0.0, 1.18), (0.128, 0.094), waist)
    upper = add((0, 0.006, 1.31), (0.135, 0.088), chest)
    neck = add((0, 0.012, 1.42), (0.050, 0.048), upper)
    add((0, 0.010, 1.47), (0.048, 0.046), neck)
    for side in ("Left", "Right"):
        sh, el = joints[f"{side}UpperArm"][0], joints[f"{side}LowerArm"][0]
        wr = joints[f"{side}Hand"][0]
        a = add(sh, (0.054, 0.054), upper)
        b = add(el, (0.040, 0.038), a)
        add(wr, (0.029, 0.026), b)
        hip, knee = joints[f"{side}UpperLeg"][0], joints[f"{side}LowerLeg"][0]
        ankle, toe = joints[f"{side}Foot"][0], joints[f"{side}Toes"][0]
        h = add(hip, (0.078, 0.078), pelvis)
        k = add(knee, (0.054, 0.054), h)
        an = add(ankle, (0.039, 0.039), k)
        ball = add(toe + Vector((0, 0.01, 0.005)), (0.040, 0.026), an)
        add(toe + Vector((0, -0.06, 0.0)), (0.034, 0.020), ball)
    me = bpy.data.meshes.new("BodySkeleton")
    me.from_pydata(verts, edges, [])
    obj = bpy.data.objects.new("BodySkeleton", me)
    bpy.context.scene.collection.objects.link(obj)
    skin = obj.modifiers.new("Skin", "SKIN")
    skin.branch_smoothing = 0.6
    for i, sv in enumerate(me.skin_vertices[0].data):
        sv.radius = radii[i]
        sv.use_root = i == 0
    sub = obj.modifiers.new("Sub", "SUBSURF")
    sub.levels = 1
    dg = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(dg))
    bpy.data.objects.remove(obj)
    mb = MeshBuilder()
    for vtx in mesh.vertices:
        mb.add_vert(vtx.co.copy(), {"part": "body"})
    for poly in mesh.polygons:
        mb.add_face(tuple(poly.vertices), "Skin")
    bpy.data.meshes.remove(mesh)
    return mb


# ---------------------------------------------------------------------------
# Default outfit
# ---------------------------------------------------------------------------

def open_ring(zc, yc, hw, hd, gap_deg, m, p=2.4, back_bulge=0.0):
    """Points of a torso cross-section from `gap` round the back to -`gap`
    (an open front). Angle 0 is the front (-Y), toward +X positive."""
    f = superellipse(hw, hd, p)
    pts = []
    g = math.radians(gap_deg)
    for i in range(m):
        a = g + (2 * math.pi - 2 * g) * i / (m - 1)
        ru, rw = f(a)  # superellipse param: u along X? map: x=sin, y=-cos
        r_x = ru * abs(math.sin(a)) if False else None
        # Use the superellipse in (sin, cos) space.
        c, s_ = math.cos(a), math.sin(a)
        k = (abs(s_) ** p + abs(c) ** p) ** (-1.0 / p)
        y_scale = hd + (back_bulge if c < 0 else 0.0)
        pts.append(Vector((s_ * hw * k, yc - c * y_scale * k, zc)))
    return pts


def closed_ring(zc, yc, hw, hd, n, p=2.4, back_bulge=0.0):
    pts = []
    for i in range(n):
        a = 2 * math.pi * i / n
        c, s_ = math.cos(a), math.sin(a)
        k = (abs(s_) ** p + abs(c) ** p) ** (-1.0 / p)
        y_scale = hd + (back_bulge if c < 0 else 0.0)
        pts.append(Vector((s_ * hw * k, yc - c * y_scale * k, zc)))
    return pts


# (z, half width, half depth, y centre, front gap degrees)
JACKET_ROWS = [
    (0.885, 0.153, 0.112, 0.004, 14),
    (0.960, 0.150, 0.110, 0.004, 14),
    (1.060, 0.150, 0.110, 0.002, 15),
    (1.160, 0.160, 0.116, 0.000, 17),
    (1.260, 0.170, 0.115, 0.002, 21),
    (1.330, 0.176, 0.108, 0.006, 26),
    (1.375, 0.196, 0.100, 0.010, 32),
    (1.410, 0.188, 0.090, 0.012, 38),
    (1.433, 0.150, 0.080, 0.013, 42),
    (1.446, 0.105, 0.072, 0.013, 45),
    (1.457, 0.077, 0.068, 0.014, 47),
    (1.478, 0.074, 0.068, 0.016, 50),  # collar top
]
SHIRT_ROWS = [
    (0.930, 0.140, 0.102, 0.004),
    (1.060, 0.138, 0.100, 0.002),
    (1.180, 0.150, 0.106, 0.000),
    (1.300, 0.164, 0.102, 0.004),
    (1.390, 0.160, 0.086, 0.010),
    (1.430, 0.080, 0.066, 0.012),
    (1.452, 0.058, 0.054, 0.012),
]


def build_jacket(joints):
    mb = MeshBuilder()
    m = 27
    rings = [open_ring(z, yc, hw, hd, gap, m, back_bulge=0.004)
             for z, hw, hd, yc, gap in JACKET_ROWS]
    tags = [{"part": "jacket"}] * len(rings)
    idx = mb.loft(rings, "Jacket", closed=False, tags=tags)
    # Turn the collar's top row outward a little.
    for i in idx[-1]:
        p = mb.verts[i]
        out = Vector((p.x, p.y - 0.018, 0)).normalized()
        mb.verts[i] = p + out * 0.016 + Vector((0, 0, -0.004))
    # Sleeves.
    for side in ("Left", "Right"):
        s = 1 if side == "Left" else -1
        sh = joints[f"{side}UpperArm"][0]
        el = joints[f"{side}LowerArm"][0]
        wr = joints[f"{side}Hand"][0]
        along = (wr - sh).normalized()
        # Starts inside the jacket's shoulder, so its top doesn't stand up
        # above the shoulder line.
        pts = [sh + v(-s * 0.010, 0, -0.022), sh + along * 0.07,
               el - along * 0.01, el + along * 0.05, wr - along * 0.025,
               wr - along * 0.006]
        radii = [(0.040, 0.042), (0.060, 0.058), (0.053, 0.051), (0.050, 0.048),
                 (0.047, 0.045), (0.048, 0.046)]
        t = tube(pts, radii, 12, "Jacket", cap_start=True, cap_end=False,
                 up_hint=Vector((0, 1, 0)),
                 tags=[{"part": "sleeve", "side": side}] * len(pts))
        mb.merge(t)
        # Cyan accent band at the cuff.
        cuff_pts = [wr - along * 0.026, wr - along * 0.017]
        band = tube(cuff_pts, [(0.0495, 0.0475)] * 2, 12, "AccentCyan",
                    cap_start=False, cap_end=False, up_hint=Vector((0, 1, 0)),
                    tags=[{"part": "sleeve", "side": side}] * 2)
        mb.merge(band)
    # Orange tag on the left chest pocket seam.
    tag = MeshBuilder()
    c = Vector((0.098, -0.120, 1.232))
    corners = [(-0.012, 0.006), (0.012, 0.006), (0.012, -0.006), (-0.012, -0.006)]
    ids = [tag.add_vert(c + Vector((u, -0.002, w)), {"part": "jacket"}) for u, w in corners]
    ids_b = [tag.add_vert(c + Vector((u, 0.004, w)), {"part": "jacket"}) for u, w in corners]
    tag.add_face(ids, "AccentOrange")
    for k in range(4):
        j = (k + 1) % 4
        tag.add_face((ids[j], ids[k], ids_b[k], ids_b[j]), "AccentOrange")
    mb.merge(tag)
    return mb


def build_shirt():
    mb = MeshBuilder()
    rings = [closed_ring(z, yc, hw, hd, 24) for z, hw, hd, yc in SHIRT_ROWS]
    mb.loft(rings, "Shirt", tags=[{"part": "shirt"}] * len(rings))
    return mb


def build_trousers(joints):
    mb = MeshBuilder()
    rows = [(1.035, 0.138, 0.104, 0.006), (0.985, 0.143, 0.106, 0.006),
            (0.900, 0.146, 0.106, 0.008), (0.820, 0.128, 0.098, 0.008)]
    rings = [closed_ring(z, yc, hw, hd, 20) for z, hw, hd, yc in rows]
    mb.loft(rings, "Trousers", cap_end=False, cap_start=True,
            tags=[{"part": "trousers"}] * len(rings))
    # Belt in a darker tone, with a small cyan accent loop at the back.
    belt = [closed_ring(z, 0.006, 0.1405, 0.1065, 20) for z in (1.010, 0.985)]
    mb.loft(belt, "Shoes", tags=[{"part": "trousers"}] * 2)
    for side in ("Left", "Right"):
        hip = joints[f"{side}UpperLeg"][0]
        knee = joints[f"{side}LowerLeg"][0]
        ankle = joints[f"{side}Foot"][0]
        s = 1 if side == "Left" else -1
        pts = [hip + Vector((-s * 0.012, 0.0, 0.02)), lerp(hip, knee, 0.45), knee + Vector((0, -0.006, 0.02)),
               lerp(knee, ankle, 0.55), ankle + Vector((0, -0.004, 0.012))]
        radii = [(0.084, 0.086), (0.072, 0.072), (0.057, 0.058), (0.052, 0.053), (0.050, 0.052)]
        t = tube(pts, radii, 12, "Trousers", cap_start=True, cap_end=False,
                 up_hint=Vector((1, 0, 0)),
                 tags=[{"part": "trousers", "side": side}] * len(pts))
        mb.merge(t)
    return mb


def build_shoes(joints):
    mb = MeshBuilder()
    for side in ("Left", "Right"):
        ankle = joints[f"{side}Foot"][0]
        x = ankle.x
        # (y, half width, top z, bottom z) from the heel to the toe.
        table = [(0.075, 0.034, 0.090, 0.024), (0.050, 0.042, 0.118, 0.022),
                 (0.000, 0.045, 0.118, 0.022), (-0.060, 0.047, 0.090, 0.022),
                 (-0.120, 0.050, 0.070, 0.022), (-0.170, 0.046, 0.058, 0.022),
                 (-0.196, 0.034, 0.048, 0.024)]
        rings = []
        for y, hw, top, bot in table:
            zc, hz = (top + bot) / 2, (top - bot) / 2
            pts = []
            for i in range(10):
                a = 2 * math.pi * i / 10
                c, s_ = math.cos(a), math.sin(a)
                p = 3.0
                k = (abs(c) ** p + abs(s_) ** p) ** (-1.0 / p)
                pts.append(Vector((x + c * hw * k, y, zc + s_ * hz * k)))
            rings.append(pts)
        mb.loft(rings, "Shoes", cap_start=True, cap_end=True,
                tags=[{"part": "shoe", "side": side}] * len(rings))
        # Sole: a light slab under the shoe.
        outline = [(0.080, 0.036), (0.040, 0.046), (-0.020, 0.048), (-0.090, 0.051),
                   (-0.150, 0.050), (-0.195, 0.040), (-0.214, 0.0)]
        loop = []
        for y, hw in outline:
            loop.append((x + hw, y))
        for y, hw in reversed(outline[:-1]):
            loop.append((x - hw, y))
        top = [mb.add_vert((px, py, 0.026), {"part": "shoe", "side": side}) for px, py in loop]
        bot = [mb.add_vert((px, py, 0.0), {"part": "shoe", "side": side}) for px, py in loop]
        n = len(loop)
        for i in range(n):
            j = (i + 1) % n
            mb.add_face((top[i], top[j], bot[j], bot[i]), "Sole")
        mb.add_face(tuple(top), "Sole")
        mb.add_face(tuple(reversed(bot)), "Sole")
        # Cyan heel tab.
        tab = [mb.add_vert((x + dx, 0.081 + dy, z), {"part": "shoe", "side": side})
               for dx, dy, z in ((-0.012, 0.0, 0.09), (0.012, 0.0, 0.09),
                                 (0.012, 0.004, 0.125), (-0.012, 0.004, 0.125))]
        mb.add_face(tuple(tab), "AccentCyan")
    return mb


# ---------------------------------------------------------------------------
# Hair (stylised meshes over the skull)
# ---------------------------------------------------------------------------

def head_surface(z, a_deg):
    """A point on the skull (head profile) at height z and angle a."""
    zs = [r[0] for r in HEAD_ROWS] + [CROWN_Z]
    if z >= HEAD_ROWS[-1][0]:
        t = (z - HEAD_ROWS[-1][0]) / (CROWN_Z - HEAD_ROWS[-1][0])
        p = head_point(HEAD_ROWS[-1][0], HEAD_ROWS[-1][1], a_deg)
        top = Vector((0, HEAD_AXIS_Y, CROWN_Z))
        # Round over the top.
        q = p.lerp(top, t)
        q.z = lerp(HEAD_ROWS[-1][0], CROWN_Z, math.sin(t * math.pi / 2))
        return q
    for k in range(1, len(HEAD_ROWS)):
        if z <= HEAD_ROWS[k][0]:
            z0, r0 = HEAD_ROWS[k - 1]
            z1, r1 = HEAD_ROWS[k]
            t = (z - z0) / (z1 - z0)
            return head_point(z0, r0, a_deg).lerp(head_point(z1, r1, a_deg), t)
    return head_point(*HEAD_ROWS[-1], a_deg)


def hair_cap(hairline, thickness, n=20, rows=7, jag=0.0, mat="Hair", tag="hair",
             length_fn=None, fringe=0.0):
    """A shell over the skull from `hairline(a)` up to the crown.
    thickness(a, t) is the offset at angle a, t from the edge (0) to the
    crown (1). `length_fn(a)` hangs an extra drop below the hairline (long
    hair). Returns a MeshBuilder."""
    mb = MeshBuilder()
    grid_out, grid_in = [], []
    angles = [360.0 * i / n - 180.0 + 180.0 / n for i in range(n)]
    angles = [a if a <= 180 else a - 360 for a in angles]
    for r in range(rows + 1):
        t = r / rows
        ring_o, ring_i = [], []
        for k, a in enumerate(angles):
            z0 = hairline(a)
            if r == 0 and jag:
                z0 -= jag * (1 if k % 2 == 0 else -0.3)
            z = lerp(z0, CROWN_Z, t ** 0.9)
            if t >= 0.999:
                z = CROWN_Z
            p = head_surface(min(z, CROWN_Z), a)
            centre = Vector((0, HEAD_AXIS_Y + 0.004, 1.60))
            out = (p - centre).normalized()
            th = thickness(a, t)
            q = p + out * th
            if fringe and r == 0 and abs(a) < 60:
                # Front locks: alternate tips hang forward over the forehead.
                k_drop = fringe * (1.0 if k % 2 == 0 else 0.35) * smoothstep(60, 20, abs(a))
                q = q + Vector((0, 0, -k_drop)) + out * k_drop * 0.6
            if length_fn and r == 0:
                drop = length_fn(a)
                q = q + Vector((0, 0, -drop)) + out * 0.004 * (drop > 0)
            ring_o.append(mb.add_vert(q, {"part": tag, "t": t, "angle": a}))
            if r == 0:
                ring_i.append(mb.add_vert(p + out * 0.002 + (Vector((0, 0, -length_fn(a))) if length_fn else Vector()),
                                          {"part": tag, "t": 0.0, "angle": a}))
        grid_out.append(ring_o)
        if r == 0:
            grid_in = ring_i
    for r in range(rows):
        for k in range(n):
            j = (k + 1) % n
            mb.add_face((grid_out[r][k], grid_out[r][j], grid_out[r + 1][j], grid_out[r + 1][k]), mat)
    mb.cap(grid_out[-1], mat, tip=(0, HEAD_AXIS_Y + 0.004, CROWN_Z + thickness(0, 1.0)))
    for k in range(n):
        j = (k + 1) % n
        mb.add_face((grid_in[k], grid_in[j], grid_out[0][j], grid_out[0][k]), mat)
    return mb


def hairline_short(a):
    a = abs(a)
    # Front 1.688, temples 1.665, over the ear 1.63, back of the head 1.535.
    return interp_table([0, 25, 50, 75, 95, 120, 150, 180],
                        [1.690, 1.684, 1.664, 1.640, 1.640, 1.585, 1.548, 1.538], a)


def fringe_clumps(angles, length, sweep=1, mat="Hair"):
    """Chunky wedge locks along the front hairline, swept to one side."""
    mb = MeshBuilder()
    for k, a in enumerate(angles):
        z = hairline_short(a) + 0.012
        root = head_surface(z, a)
        centre = Vector((0, HEAD_AXIS_Y, 1.60))
        out = (root - centre).normalized()
        side = Vector((math.cos(math.radians(a)), math.sin(math.radians(a)), 0)) * sweep
        width = 0.024
        l = length * (1.0 - 0.15 * (k % 2))
        base_l = root + out * 0.018 - side * width * 0.5
        base_r = root + out * 0.018 + side * width * 0.5
        base_in = root + out * 0.002
        tip = root + out * 0.012 + Vector((0, 0, -l * 0.55)) + side * l * 0.6
        back = root + out * 0.010 + Vector((0, 0, 0.03)) + side * -0.004
        ids = [mb.add_vert(p, {"part": "hair"}) for p in (base_l, base_r, base_in, tip, back)]
        bl, br, bi, tp, bk = ids
        for f in ((bl, br, tp), (br, bi, tp), (bi, bl, tp), (bk, br, bl), (bk, bi, br), (bk, bl, bi)):
            mb.add_face(f, mat)
    return mb


def build_hair(style):
    if style == "Bald":
        return None
    if style == "Short":
        def thick(a, t):
            front = smoothstep(110, 10, abs(a))
            return 0.010 + 0.016 * t * (0.6 + 0.4 * front) + 0.020 * front * math.sin(min(t * 1.4, 1.0) * math.pi * 0.9)
        return hair_cap(hairline_short, thick, n=22, jag=0.004, fringe=0.016)
    if style == "Medium":
        def hl(a):
            return hairline_short(a) - (0.0 if abs(a) < 70 else 0.03 * smoothstep(70, 150, abs(a)))

        def thick(a, t):
            return 0.016 + 0.012 * t

        def drop(a):
            a = abs(a)
            return 0.0 if a < 75 else 0.05 * smoothstep(75, 140, a)
        return hair_cap(hl, thick, jag=0.009, length_fn=drop)
    if style == "Long":
        def thick(a, t):
            return 0.016 + 0.012 * t

        def drop(a):
            a = abs(a)
            return 0.0 if a < 55 else 0.17 * smoothstep(55, 120, a)
        return hair_cap(hairline_short, thick, jag=0.012, length_fn=drop)
    if style == "Curly":
        def thick(a, t):
            return 0.024 + 0.016 * t

        return hair_cap(lambda a: hairline_short(a) - 0.004, thick, n=24, jag=0.014)
    raise ValueError(style)


# ---------------------------------------------------------------------------
# Skeleton
# ---------------------------------------------------------------------------

def build_armature(joints, name="AxcenityHumanoid"):
    arm = bpy.data.armatures.new(name)
    obj = bpy.data.objects.new(name, arm)
    bpy.context.scene.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    order = list(joints.keys())
    for bone_name in order:
        head, tail, parent = joints[bone_name]
        eb = arm.edit_bones.new(bone_name)
        eb.head = head
        eb.tail = tail
        eb.use_deform = bone_name != "Root"
    for bone_name in order:
        parent = joints[bone_name][2]
        eb = arm.edit_bones[bone_name]
        if parent:
            eb.parent = arm.edit_bones[parent]
        d = (eb.tail - eb.head).normalized()
        if abs(d.y) > 0.7:
            eb.align_roll(Vector((0, 0, 1)))
        elif "Proximal" in bone_name or "Intermediate" in bone_name or "Distal" in bone_name:
            side = 1 if bone_name.startswith("Left") else -1
            _, _, palm = hand_frame(side, Vector())
            eb.align_roll(-palm)
        else:
            eb.align_roll(Vector((0, -1, 0)))
    bpy.ops.object.mode_set(mode="OBJECT")
    obj.select_set(False)
    return obj


# ---------------------------------------------------------------------------
# Skin weights
# ---------------------------------------------------------------------------

def seg_distance(p, a, b):
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / max(ab.length_squared, 1e-12)))
    return (a + ab * t - p).length, t


def idw(p, joints, candidates, power=5.0, keep=4):
    ws = {}
    for b in candidates:
        a, t_, _ = joints[b]
        d, _ = seg_distance(p, a, t_)
        ws[b] = 1.0 / max(d, 0.004) ** power
    top = sorted(ws.items(), key=lambda kv: -kv[1])[:keep]
    total = sum(w for _, w in top)
    out = {b: w / total for b, w in top}
    return {b: w for b, w in out.items() if w > 0.02}


TORSO = ["Hips", "Spine", "Chest", "UpperChest"]


def side_of(p):
    return "Left" if p.x >= 0 else "Right"


def arm_share(p, side, joints):
    """How much of the arm's turn cloth at `p` follows: 0 inside the
    shoulder joint, 1 a little way down the upper arm."""
    shoulder, elbow, _ = joints[f"{side}UpperArm"]
    along = (elbow - shoulder).normalized()
    return smoothstep(-0.02, 0.07, (p - shoulder).dot(along))


def mix_weights(a, b, t):
    """`a` × t + `b` × (1 - t), keeping the 4 largest, normalised."""
    out = {}
    for k, w in a.items():
        out[k] = out.get(k, 0.0) + w * t
    for k, w in b.items():
        out[k] = out.get(k, 0.0) + w * (1.0 - t)
    top = sorted(out.items(), key=lambda kv: -kv[1])[:4]
    total = sum(w for _, w in top) or 1.0
    return {k: w / total for k, w in top if w > 0.0}


def weights_for(p, tag, joints):
    part = tag.get("part", "")
    side = tag.get("side") or side_of(p)
    if part == "eye":
        return {f"{side}Eye": 1.0}
    if part in ("hair", "brow", "lid", "nose", "ear"):
        return {"Head": 1.0}
    if part == "teeth" or part == "mouth":
        return {"Jaw": 1.0} if tag.get("jaw") else {"Head": 1.0}
    if part in ("face", "neck"):
        z = p.z
        if z < 1.40:
            w_neck = smoothstep(1.36, 1.43, z)
            return {"UpperChest": 1.0 - w_neck, "Neck": w_neck} if w_neck < 1 else {"Neck": 1.0}
        if z < 1.49:
            w_head = smoothstep(1.455, 1.49, z) if part == "neck" else 1.0
            out = {"Neck": 1.0 - w_head, "Head": w_head}
            return {k: v for k, v in out.items() if v > 0.0}
        # Lower face: the jaw carries the chin and the lower lip.
        a = abs(tag.get("angle", 0.0))
        front = smoothstep(105.0, 55.0, a)
        below = smoothstep(1.545, 1.505, z)
        if tag.get("lip") == "lower":
            below = 1.0
        w_jaw = front * below * 0.95
        if tag.get("lip") in ("upper", "corner"):
            w_jaw = 0.5 * front if tag.get("lip") == "corner" else 0.0
        if w_jaw <= 0.0:
            return {"Head": 1.0}
        return {"Jaw": w_jaw, "Head": 1.0 - w_jaw}
    if part == "hand":
        return idw(p, joints, [f"{side}LowerArm", f"{side}Hand"], power=6)
    if part == "finger":
        f = tag["finger"]
        bones = [f"{side}Hand"] + [f"{side}{f}{s}" for s in ("Proximal", "Intermediate", "Distal")]
        return idw(p, joints, bones, power=6, keep=3)
    if part == "sleeve":
        w = idw(p, joints, [f"{side}UpperArm", f"{side}LowerArm", f"{side}Hand"], power=5)
        # Near the shoulder the sleeve follows the arm only as far as it
        # is past the joint: cloth above or inside the joint stays with
        # the torso, so lowering the arm doesn't lift it into a peak.
        arm = arm_share(p, side, joints)
        return mix_weights(w, {f"{side}Shoulder": 0.7, "UpperChest": 0.3}, arm)
    if part in ("jacket", "shirt"):
        cands = TORSO + ["Neck", "LeftShoulder", "RightShoulder"]
        w = idw(p, joints, cands, power=4)
        if p.z > 1.30:
            arm = arm_share(p, side, joints)
            if arm > 0.0:
                w = mix_weights({f"{side}UpperArm": 1.0}, w, arm)
        return w
    if part == "trousers":
        if "side" not in tag:
            return idw(p, joints, ["Hips", "Spine", "LeftUpperLeg", "RightUpperLeg"], power=4)
        return idw(p, joints, ["Hips", f"{side}UpperLeg", f"{side}LowerLeg", f"{side}Foot"], power=5)
    if part == "shoe":
        return idw(p, joints, [f"{side}LowerLeg", f"{side}Foot", f"{side}Toes"], power=6, keep=3)
    if part == "body":
        if p.z > 1.43 and abs(p.x) < 0.07:
            return {"Neck": 1.0} if p.z < 1.47 else {"Head": 1.0}
        cands = TORSO + ["Neck"]
        if abs(p.x) > 0.02:
            cands += [f"{side}Shoulder", f"{side}UpperArm", f"{side}LowerArm", f"{side}Hand",
                      f"{side}UpperLeg", f"{side}LowerLeg", f"{side}Foot", f"{side}Toes"]
        else:
            cands += ["LeftUpperLeg", "RightUpperLeg"]
        return idw(p, joints, cands, power=5)
    return {"Hips": 1.0}


def skin_object(obj, mb, joints, armature):
    groups = {}
    for i, (p, tag) in enumerate(zip(mb.verts, mb.tags)):
        for bone, w in weights_for(p, tag, joints).items():
            if bone not in groups:
                groups[bone] = obj.vertex_groups.new(name=bone)
            groups[bone].add([i], w, "REPLACE")
    obj.parent = armature
    mod = obj.modifiers.new("Armature", "ARMATURE")
    mod.object = armature


# ---------------------------------------------------------------------------
# Face shape keys (Axcenity Humanoid Facial Profile names)
# ---------------------------------------------------------------------------

def rotate_about(p, centre, axis, angle):
    return centre + Matrix.Rotation(angle, 3, axis) @ (p - centre)


def face_shapes(mb):
    """name -> list of per-vertex offsets (Vector) for the head mesh."""
    verts, tags = mb.verts, mb.tags
    n = len(verts)
    shapes = {}

    def new():
        return [Vector() for _ in range(n)]

    corner = {s: head_point(HEAD_ROWS[MOUTH_ROW][0], HEAD_ROWS[MOUTH_ROW][1], s * MOUTH_HALF_ANGLE)
              for s in (1, -1)}
    mouth_c = head_point(HEAD_ROWS[MOUTH_ROW][0], HEAD_ROWS[MOUTH_ROW][1], 0.0)

    def near(p, c, r):
        return smoothstep(r, 0.0, (p - c).length)

    def is_face(t):
        return t.get("part") in ("face",)

    def side_ok(p, s):
        return s is None or (p.x * s > -0.004)

    # Eyelids.
    def lid_shape(s, upper_angle, lower_angle):
        d = new()
        c = eye_centre(s)
        side = "L" if s > 0 else "R"
        for i, (p, t) in enumerate(zip(verts, tags)):
            if t.get("part") != "lid" or t.get("side") != side:
                continue
            ang = upper_angle if t.get("lid") == "upper" else lower_angle
            # Turning about +X moves the front of the lid down.
            q = rotate_about(p, c, Vector((1, 0, 0)), ang)
            d[i] = q - p
        return d

    for s, sfx in ((1, "Left"), (-1, "Right")):
        shapes[f"Blink_{sfx}"] = lid_shape(s, math.radians(52), math.radians(-8))
        shapes[f"Eye_Closed_{sfx}"] = lid_shape(s, math.radians(46), math.radians(-14))
        sq = lid_shape(s, math.radians(10), math.radians(-18))
        shapes[f"Eye_Squint_{sfx}"] = sq
        shapes[f"Eye_Wide_{sfx}"] = lid_shape(s, math.radians(-16), math.radians(6))

    # Brows.
    for s, sfx in ((1, "Left"), (-1, "Right")):
        side = "L" if s > 0 else "R"
        raise_, lower, inward = new(), new(), new()
        for i, (p, t) in enumerate(zip(verts, tags)):
            if not side_ok(p, s):
                continue
            if t.get("part") == "brow" and t.get("side") == side:
                inner = smoothstep(0.066, 0.014, abs(p.x))
                raise_[i] = Vector((0, 0, 0.0075))
                lower[i] = Vector((-s * 0.002 * inner, -0.001, -0.0055))
                inward[i] = Vector((-s * 0.003 * inner, 0, 0.006 * inner - 0.001 * (1 - inner)))
            elif is_face(t) and 1.625 < p.z < 1.70 and abs(t.get("angle", 99)) < 60:
                w = smoothstep(0.70, 1.64, p.z) * smoothstep(1.70, 1.66, p.z)
                w *= smoothstep(0.075, 0.03, abs(p.x - s * 0.04))
                raise_[i] = Vector((0, 0, 0.005 * w))
                lower[i] = Vector((0, 0, -0.003 * w))
        shapes[f"Brow_Raise_{sfx}"] = raise_
        shapes[f"Brow_Lower_{sfx}"] = lower
        shapes[f"Brow_Inward_{sfx}"] = inward

    # Mouth.
    def mouth_region(p, t):
        if t.get("part") not in ("face",):
            return 0.0
        return near(p, mouth_c, 0.06)

    def lip_kind(t):
        return t.get("lip")

    for s, sfx in ((1, "Left"), (-1, "Right")):
        smile, frown, pull, cheek, sneer = new(), new(), new(), new(), new()
        ulr, lll = new(), new()
        for i, (p, t) in enumerate(zip(verts, tags)):
            if t.get("part") in ("face",):
                w = near(p, corner[s], 0.032)
                if w > 0:
                    smile[i] = Vector((s * 0.009, 0.006, 0.010)) * w
                    frown[i] = Vector((s * 0.002, 0.001, -0.0075)) * w
                    pull[i] = Vector((s * 0.008, 0.004, 0.0015)) * w
                cw = near(p, head_point(1.582, HEAD_ROWS[8][1], s * 38), 0.03)
                if cw > 0:
                    cheek[i] = Vector((0, -0.002, 0.006)) * cw
                    smile[i] += Vector((0, -0.0015, 0.003)) * cw
                if lip_kind(t) == "upper" and p.x * s > -0.002:
                    ulr[i] = Vector((0, -0.001, 0.0045)) * smoothstep(0.035, 0.0, abs(p.x - s * 0.012))
                if lip_kind(t) == "lower" and p.x * s > -0.002:
                    lll[i] = Vector((0, -0.001, -0.0055)) * smoothstep(0.035, 0.0, abs(p.x - s * 0.012))
            if t.get("part") == "nose" and p.x * s > 0.004:
                sneer[i] = Vector((s * 0.001, 0.0, 0.004))
            if t.get("part") == "lid" and t.get("lid") == "lower" and t.get("side") == ("L" if s > 0 else "R"):
                cheek[i] = Vector((0, -0.0005, 0.0025))
        for i, (p, t) in enumerate(zip(verts, tags)):
            if lip_kind(t) == "upper" and p.x * s > 0:
                sneer[i] += Vector((0, 0, 0.002)) * smoothstep(0.03, 0.005, abs(p.x))
        shapes[f"Mouth_Smile_{sfx}"] = smile
        shapes[f"Mouth_Frown_{sfx}"] = frown
        shapes[f"Lip_Corner_Pull_{sfx}"] = pull
        shapes[f"Cheek_Raise_{sfx}"] = cheek
        shapes[f"Nose_Sneer_{sfx}"] = sneer
        shapes[f"Upper_Lip_Raise_{sfx}"] = ulr
        shapes[f"Lower_Lip_Lower_{sfx}"] = lll

    open_, wide, narrow, pucker, mleft, mright = new(), new(), new(), new(), new(), new()
    ulr, lll, puff = new(), new(), new()
    jaw_open, jaw_fwd, jaw_l, jaw_r = new(), new(), new(), new()
    jaw_pivot = Vector((0, HEAD_AXIS_Y - 0.005, 1.565))
    for i, (p, t) in enumerate(zip(verts, tags)):
        part = t.get("part")
        lip = lip_kind(t)
        region = mouth_region(p, t)
        jawish = (part == "face" and (lip == "lower" or (p.z < 1.53 and abs(t.get("angle", 99)) < 100))) \
            or (part in ("teeth", "mouth") and t.get("jaw"))
        jw = 1.0
        if part == "face" and lip != "lower":
            jw = smoothstep(1.535, 1.50, p.z) * smoothstep(105, 50, abs(t.get("angle", 0)))
        if lip == "upper":
            open_[i] = Vector((0, 0, 0.0035)) * smoothstep(0.04, 0.0, abs(p.x))
            ulr[i] = Vector((0, -0.001, 0.0045)) * smoothstep(0.04, 0.0, abs(p.x))
        if jawish:
            open_[i] += Vector((0, 0.002, -0.011)) * jw * (1.0 if lip == "lower" or part != "face" else 0.7)
            jaw_fwd[i] = Vector((0, -0.007, 0.0)) * jw
            jaw_l[i] = Vector((0.007, 0, 0)) * jw
            jaw_r[i] = Vector((-0.007, 0, 0)) * jw
        if lip == "lower":
            lll[i] = Vector((0, -0.001, -0.0055)) * smoothstep(0.04, 0.0, abs(p.x))
        if part == "face" and region > 0:
            sx = 1 if p.x >= 0 else -1
            cw = near(p, corner[sx], 0.03)
            wide[i] = Vector((sx * 0.008, 0.002, 0.0)) * cw
            narrow[i] = Vector((-sx * 0.0065, -0.004, 0.0)) * cw
            pucker[i] = Vector((-sx * 0.008 * cw, -0.008 * region, 0.0))
            jaw_open[i] = Vector((-sx * 0.003 * cw, -0.001 * region, 0.0))
            mleft[i] = Vector((0.010, 0.001, 0.0)) * region
            mright[i] = Vector((-0.010, 0.001, 0.0)) * region
        if part == "face" and 1.50 < p.z < 1.585 and 25 < abs(t.get("angle", 0)) < 95:
            sx = 1 if p.x >= 0 else -1
            w = near(p, head_point(1.545, HEAD_ROWS[MOUTH_ROW][1], sx * 55), 0.04)
            puff[i] = Vector((sx * 0.011, -0.004, 0.0)) * w
    shapes["Mouth_Open"] = open_
    shapes["Mouth_Wide"] = wide
    shapes["Mouth_Narrow"] = narrow
    shapes["Mouth_Pucker"] = pucker
    shapes["Mouth_Left"] = mleft
    shapes["Mouth_Right"] = mright
    shapes["Upper_Lip_Raise"] = ulr
    shapes["Lower_Lip_Lower"] = lll
    shapes["Cheek_Puff"] = puff
    shapes["Jaw_Open"] = jaw_open  # corrective; the Jaw bone opens the mouth
    shapes["Jaw_Forward"] = jaw_fwd
    shapes["Jaw_Left"] = jaw_l
    shapes["Jaw_Right"] = jaw_r
    return shapes


FACE_SHAPE_ORDER = [
    "Blink_Left", "Blink_Right", "Eye_Closed_Left", "Eye_Closed_Right",
    "Eye_Squint_Left", "Eye_Squint_Right", "Eye_Wide_Left", "Eye_Wide_Right",
    "Brow_Raise_Left", "Brow_Raise_Right", "Brow_Lower_Left", "Brow_Lower_Right",
    "Brow_Inward_Left", "Brow_Inward_Right",
    "Mouth_Smile_Left", "Mouth_Smile_Right", "Mouth_Frown_Left", "Mouth_Frown_Right",
    "Mouth_Open", "Mouth_Wide", "Mouth_Narrow", "Mouth_Pucker", "Mouth_Left", "Mouth_Right",
    "Upper_Lip_Raise", "Lower_Lip_Lower", "Upper_Lip_Raise_Left", "Upper_Lip_Raise_Right",
    "Lower_Lip_Lower_Left", "Lower_Lip_Lower_Right", "Lip_Corner_Pull_Left", "Lip_Corner_Pull_Right",
    "Cheek_Raise_Left", "Cheek_Raise_Right", "Cheek_Puff", "Nose_Sneer_Left", "Nose_Sneer_Right",
    "Jaw_Open", "Jaw_Forward", "Jaw_Left", "Jaw_Right",
]


def add_shape_keys(obj, mb):
    shapes = face_shapes(mb)
    obj.shape_key_add(name="Basis", from_mix=False)
    for name in FACE_SHAPE_ORDER:
        key = obj.shape_key_add(name=name, from_mix=False)
        key.value = 0.0  # (new keys start fully on in recent Blender)
        for i, d in enumerate(shapes[name]):
            if d.length_squared:
                key.data[i].co = mb.verts[i] + d
    obj.data.shape_keys.use_relative = True
    return shapes


# ---------------------------------------------------------------------------
# Levels of detail
# ---------------------------------------------------------------------------

# The engine reads a file's own `_LOD1` and `_LOD2` copies (two levels).
LOD_RATIOS = {1: 0.5, 2: 0.22}


def make_lods(obj, levels=(1, 2)):
    """Simplified copies named <name>_LOD1.. (collapse decimation, no shape
    keys: faces simplify to their rest expression at a distance)."""
    lods = []
    for level in levels:
        copy = obj.copy()
        copy.data = obj.data.copy()
        copy.name = f"{obj.name}_LOD{level}"
        copy.data.name = copy.name
        bpy.context.scene.collection.objects.link(copy)
        if copy.data.shape_keys:
            copy.shape_key_clear()
        for mod in list(copy.modifiers):
            if mod.type == "ARMATURE":
                continue
            copy.modifiers.remove(mod)
        dec = copy.modifiers.new("Decimate", "DECIMATE")
        dec.decimate_type = "COLLAPSE"
        dec.ratio = LOD_RATIOS[level]
        dec.use_collapse_triangulate = True
        # Apply the decimation only (the armature stays live).
        bpy.context.view_layer.objects.active = copy
        bpy.ops.object.modifier_move_to_index(modifier="Decimate", index=0)
        bpy.ops.object.modifier_apply(modifier="Decimate")
        lods.append(copy)
    return lods


def tri_count(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


# ---------------------------------------------------------------------------
# Assembly and export
# ---------------------------------------------------------------------------

HAIR_STYLES = ["Short", "Medium", "Long", "Curly", "Bald"]


def build_character(with_lods=True):
    reset_scene()
    joints = skeleton_joints()
    armature = build_armature(joints)
    parts = {}

    def add(name, mb, flat=True, shapes=False, thickness=0.0):
        obj = mb.to_object(name, flat=flat)
        fix_normals(obj)
        if thickness:
            sol = obj.modifiers.new("Solidify", "SOLIDIFY")
            sol.thickness = thickness
            sol.offset = -1.0
            sol.use_even_offset = True
            sol.use_rim = True
            mb = apply_modifiers(obj, mb)
        skin_object(obj, mb, joints, armature)
        if shapes:
            add_shape_keys(obj, mb)
        parts[name] = (obj, mb)
        return obj

    head = build_head()
    head.merge(build_lids_and_brows())
    head.merge(build_mouth_interior())
    add("Head", head, shapes=True)
    eyes = build_eye(1)
    eyes.merge(build_eye(-1))
    for i, p in enumerate(eyes.verts):
        eyes.tags[i]["side"] = "Left" if p.x > 0 else "Right"
    add("Eyes", eyes, flat=False)
    hands = build_hand("Left", joints)
    hands.merge(build_hand("Right", joints))
    add("Hands", hands)
    add("Jacket", build_jacket(joints), thickness=0.007)
    add("Shirt", build_shirt())
    add("Trousers", build_trousers(joints))
    add("Shoes", build_shoes(joints))
    for style in HAIR_STYLES:
        mb = build_hair(style)
        if mb:
            add(f"Hair_{style}", mb)
    add("Body", build_base_body(joints))
    lods = {}
    if with_lods:
        for name, (obj, _) in parts.items():
            if name == "Eyes":
                lods[name] = make_lods(obj, levels=(1,))
            else:
                lods[name] = make_lods(obj)
    return armature, joints, parts, lods


DEFAULT_PARTS = ["Head", "Eyes", "Hands", "Hair_Short", "Jacket", "Shirt", "Trousers", "Shoes"]


def export_glb(path, armature, objects):
    bpy.ops.object.select_all(action="DESELECT")
    armature.select_set(True)
    for o in objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.export_scene.gltf(
        filepath=path, export_format="GLB", use_selection=True,
        export_skins=True, export_morph=True, export_morph_normal=False,
        # Dense shape keys: with sparse ones Blender writes a shape that
        # leaves a primitive still as an accessor with no data, which the
        # `gltf` crate refuses ("bufferView: Missing data").
        export_try_sparse_sk=False,
        export_animations=False, export_yup=True, export_def_bones=False,
        export_leaf_bone=False, export_tangents=False)


def export_animations(path, armature):
    """The starter animations, one glTF animation per clip, with the
    skeleton and no meshes."""
    import animations
    animations.build_actions(armature)
    bpy.ops.object.select_all(action="DESELECT")
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.export_scene.gltf(
        filepath=path, export_format="GLB", use_selection=True,
        export_skins=True, export_morph=False, export_animations=True,
        export_animation_mode="ACTIONS", export_force_sampling=True,
        export_optimize_animation_size=True, export_anim_slide_to_zero=True,
        export_yup=True, export_def_bones=False, export_leaf_bone=False)


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    armature, joints, parts, lods = build_character()
    # The whole default character is one mesh for one MeshPart, so it has
    # no file LODs: the engine pairs a file's LOD copies with the full mesh
    # by material, which several parts share (AccentCyan, Skin), so it
    # simplifies this one itself. HumanoidParts.glb keeps each part's
    # `_LOD1`/`_LOD2`, used when a MeshPart picks that part
    # (`HumanoidParts.glb#Jacket` brings `Jacket_LOD1` and `Jacket_LOD2`).
    default = [parts[name][0] for name in DEFAULT_PARTS]
    export_glb(os.path.join(out_dir, "Humanoid.glb"), armature, default)
    everything = [o for o, _ in parts.values()] + [o for l in lods.values() for o in l]
    export_glb(os.path.join(out_dir, "HumanoidParts.glb"), armature, everything)
    export_animations(os.path.join(out_dir, "HumanoidAnimations.glb"), armature)
    report = {name: tri_count(obj) for name, (obj, _) in parts.items()}
    for name, objs in lods.items():
        for o in objs:
            report[o.name] = tri_count(o)
    return report


if __name__ == "__main__":
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else sys.argv[1:]
    out = argv[argv.index("--out") + 1] if "--out" in argv else os.path.dirname(os.path.abspath(__file__))
    rep = main(out)
    for k in sorted(rep):
        print(f"{k:24s} {rep[k]:6d}")
