"""The default humanoid's starter animations, keyed procedurally in Blender
on the Axcenity Humanoid skeleton (30 frames a second, in place unless
named `_RootMotion`).

Rotations are in each bone's own axes, in degrees. With the skeleton's
bone rolls (see build.py), +X swings a limb forward (and bends an elbow);
a knee bends with -X; +X on a foot lifts the toes; on the spine +X leans
forward and +Y turns toward the character's left. Left-side angles are
mirrored onto the right by `mirror` (Y and Z negated).
"""

import math

import bpy
from mathutils import Euler, Vector

FPS = 30

SIDES = ("Left", "Right")


def mirror(rot):
    x, y, z = rot
    return (x, -y, -z)


class Pose(dict):
    """bone -> (x, y, z) degrees; `hips` is a world offset in metres."""

    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self.hips = Vector()
        self.root = Vector()
        self.root_turn = 0.0

    def add(self, bone, rot):
        a = self.get(bone, (0.0, 0.0, 0.0))
        self[bone] = (a[0] + rot[0], a[1] + rot[1], a[2] + rot[2])
        return self

    def both(self, bone, left_rot, right_rot=None):
        """Sets `Left<bone>` and the mirrored `Right<bone>`."""
        self.add(f"Left{bone}", left_rot)
        self.add(f"Right{bone}", mirror(right_rot if right_rot is not None else left_rot))
        return self

    def side(self, side, bone, rot):
        self.add(f"{side}{bone}", rot if side == "Left" else mirror(rot))
        return self


FINGERS = ("Index", "Middle", "Ring", "Little")


def relaxed(p, curl=14.0, arm_down=36.0, elbow=12.0, weight=1.0):
    """Arms down by the sides from the A-pose, a soft hand."""
    w = weight
    p.both("UpperArm", (4 * w, 0, -arm_down * w))
    p.both("LowerArm", (elbow * w, 0, 0))
    p.both("Hand", (0, 0, 4 * w))
    hand(p, curl * w)
    return p


def hand(p, curl, thumb=None, sides=SIDES):
    for side in sides:
        for f in FINGERS:
            spread = {"Index": 0.85, "Middle": 1.0, "Ring": 1.1, "Little": 1.2}[f]
            p.side(side, f"{f}Proximal", (-curl * spread, 0, 0))
            p.side(side, f"{f}Intermediate", (-curl * 1.2 * spread, 0, 0))
            p.side(side, f"{f}Distal", (-curl * 0.9 * spread, 0, 0))
        t = curl * 0.6 if thumb is None else thumb
        p.side(side, "ThumbProximal", (-t * 0.3, 0, 0))
        p.side(side, "ThumbIntermediate", (-t * 0.6, 0, 0))
        p.side(side, "ThumbDistal", (-t * 0.6, 0, 0))
    return p


def fist(p, sides=SIDES):
    return hand(p, 70, thumb=40, sides=sides)


# ---------------------------------------------------------------------------
# Keying
# ---------------------------------------------------------------------------

def hips_local(arm_obj, world_offset):
    """A world offset as the Hips bone's location (its rest axes)."""
    bone = arm_obj.data.bones["Hips"]
    m = bone.matrix_local.to_3x3()
    return m.inverted() @ world_offset


def root_local(arm_obj, world_offset):
    bone = arm_obj.data.bones["Root"]
    return bone.matrix_local.to_3x3().inverted() @ world_offset


def key_pose(arm_obj, frame, pose, bones):
    for name in bones:
        pb = arm_obj.pose.bones[name]
        pb.rotation_mode = "XYZ"
        rot = pose.get(name, (0.0, 0.0, 0.0))
        pb.rotation_euler = Euler([math.radians(a) for a in rot], "XYZ")
        pb.keyframe_insert("rotation_euler", frame=frame)
    hips = arm_obj.pose.bones["Hips"]
    hips.location = hips_local(arm_obj, pose.hips)
    hips.keyframe_insert("location", frame=frame)
    root = arm_obj.pose.bones["Root"]
    root.location = root_local(arm_obj, pose.root)
    root.keyframe_insert("location", frame=frame)
    root.rotation_mode = "XYZ"
    root.rotation_euler = Euler((0, 0, math.radians(pose.root_turn)), "XYZ")
    root.keyframe_insert("rotation_euler", frame=frame)


def make_action(arm_obj, name, frames, pose_at, loop=True):
    """Keys `pose_at(t)` (t from 0 to 1 over the clip) at every frame."""
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    arm_obj.animation_data_create()
    arm_obj.animation_data.action = act
    bones = [b.name for b in arm_obj.pose.bones if b.name != "Root"]
    for f in range(frames + 1):
        t = f / frames
        if loop and f == frames:
            t = 0.0
        key_pose(arm_obj, f, pose_at(t), bones)
    for fc in fcurves_of(act):
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    act.frame_range = (0, frames)
    return act


def fcurves_of(act):
    try:
        return list(act.fcurves)
    except AttributeError:
        out = []
        for layer in act.layers:
            for strip in layer.strips:
                for bag in strip.channelbags:
                    out += list(bag.fcurves)
        return out


def osc(t, cycles=1.0, phase=0.0):
    return math.sin(2 * math.pi * (t * cycles + phase))


def ease(x):
    x = max(0.0, min(1.0, x))
    return x * x * (3 - 2 * x)


def blend_pose(a, b, w):
    out = Pose()
    for k in set(a) | set(b):
        va = a.get(k, (0, 0, 0))
        vb = b.get(k, (0, 0, 0))
        out[k] = tuple(x + (y - x) * w for x, y in zip(va, vb))
    out.hips = a.hips.lerp(b.hips, w)
    out.root = a.root.lerp(b.root, w)
    out.root_turn = a.root_turn + (b.root_turn - a.root_turn) * w
    return out


def keyframes(t, keys):
    """Piecewise eased blend through (time, Pose) keys, t in 0..1."""
    for i in range(1, len(keys)):
        if t <= keys[i][0]:
            t0, p0 = keys[i - 1]
            t1, p1 = keys[i]
            return blend_pose(p0, p1, ease((t - t0) / max(t1 - t0, 1e-6)))
    return keys[-1][1]


# ---------------------------------------------------------------------------
# Locomotion
# ---------------------------------------------------------------------------

def idle(t):
    p = relaxed(Pose())
    b = osc(t)
    p.add("Spine", (1.2 * b, 0, 0)).add("Chest", (1.0 * b, 0, 0))
    p.add("UpperChest", (-1.0 * b, 0, 0))
    p.add("Neck", (0.5 * b, 0, 0)).add("Head", (0, 2.5 * osc(t, 1, 0.25), 0))
    p.both("UpperArm", (1.5 * b, 0, -1.0 * b))
    p.both("UpperLeg", (0, 0, 0))
    p.add("LeftUpperLeg", (0, 0, 1.5)).add("RightUpperLeg", (0, 0, -1.5))
    p.hips = Vector((0.004 * osc(t, 1, 0.1), 0, -0.004 + 0.003 * b))
    return p


def gait(t, speed, stride, lift, arm_swing, lean, bounce, crouch=0.0, dir_turn=0.0,
         backward=False):
    """A walk or run cycle (one cycle = two steps). Left foot forward at
    t = 0."""
    p = relaxed(Pose(), arm_down=38 if speed < 3 else 30, elbow=6 + speed * 8)
    sgn = -1.0 if backward else 1.0
    for side, ph in (("Left", 0.0), ("Right", 0.5)):
        s = osc(t, 1, ph + 0.25)  # +1: leg forward
        c = osc(t, 1, ph)  # >0 while the leg swings forward
        swing = max(0.0, c)
        thigh = sgn * stride * s + crouch * 40 + (12 * swing * (speed > 3))
        knee = -(8 + lift * swing + (6 + speed * 3) * max(0.0, -c) * 0.6) - crouch * 70
        foot = sgn * (8 * max(0.0, s) - 14 * max(0.0, -s) * max(0.0, -c)) + crouch * 30
        p.side(side, "UpperLeg", (thigh, 0, 0))
        p.side(side, "LowerLeg", (knee, 0, 0))
        p.side(side, "Foot", (foot - knee * 0.25 + 4, 0, 0))
        p.side(side, "Toes", (-10 * max(0.0, -s) * max(0.0, -c) * 2, 0, 0))
        # Arms swing against the legs.
        p.side(side, "UpperArm", (-sgn * arm_swing * s, 0, 0))
        p.side(side, "LowerArm", (arm_swing * 0.35 * max(0.0, -s * sgn) + speed * 6, 0, 0))
    p.add("Hips", (0, 6 * osc(t, 1, 0.25) * min(speed, 3) / 1.5, 2.5 * osc(t, 1)))
    p.add("Spine", (lean, -4 * osc(t, 1, 0.25), 0))
    p.add("Chest", (lean * 0.4, -3 * osc(t, 1, 0.25), 0))
    p.add("Neck", (-lean * 0.6, 0, 0)).add("Head", (-lean * 0.3, 2 * osc(t, 1, 0.25), 0))
    p.hips = Vector((0.012 * osc(t, 1), 0, -0.012 - crouch * 0.33 + bounce * osc(t, 2, 0.0)))
    if dir_turn:
        p.root_turn = dir_turn
    return p


WALK_SPEED = 1.45  # m/s the walk is keyed for
RUN_SPEED = 3.6
SPRINT_SPEED = 6.0
CROUCH_SPEED = 0.9


def walk(t):
    return gait(t, WALK_SPEED, 24, 50, 18, 3, 0.012)


def run(t):
    return gait(t, RUN_SPEED, 40, 85, 38, 10, 0.03)


def sprint(t):
    return gait(t, SPRINT_SPEED, 52, 105, 55, 18, 0.04)


def crouch_idle(t):
    p = relaxed(Pose(), arm_down=30, elbow=30)
    b = osc(t)
    p.both("UpperLeg", (62, 0, 4)).both("LowerLeg", (-100, 0, 0)).both("Foot", (38, 0, 0))
    p.add("Spine", (18 + b, 0, 0)).add("Chest", (6, 0, 0)).add("Neck", (-14, 0, 0)).add("Head", (-8, 0, 0))
    p.hips = Vector((0, 0.02, -0.27 + 0.004 * b))
    return p


def crouch_walk(t):
    p = gait(t, CROUCH_SPEED, 22, 40, 10, 18, 0.01, crouch=0.8)
    p.add("Neck", (-8, 0, 0))
    p.hips.z = -0.25 + 0.008 * osc(t, 2)
    return p


def strafe(t, side_sign, speed):
    """Sideways steps toward +X (character's left) when side_sign is 1."""
    p = relaxed(Pose(), arm_down=36)
    amp = 14 if speed < 3 else 22
    for side, ph in (("Left", 0.0), ("Right", 0.5)):
        s = osc(t, 1, ph + 0.25)
        c = max(0.0, osc(t, 1, ph))
        out = (1 if side == "Left" else -1) * side_sign
        p.side(side, "UpperLeg", (6 * c, 0, amp * s * out * (1 if side == "Left" else 1)))
        p.side(side, "LowerLeg", (-(10 + 40 * c * (speed / 1.5) ** 0.5), 0, 0))
        p.side(side, "Foot", (10 * c, 0, 0))
    p.add("Hips", (0, 0, -3 * osc(t, 2)))
    p.add("Spine", (3 + speed, 0, 0))
    p.hips = Vector((0.01 * osc(t, 1), 0, -0.012 + 0.012 * osc(t, 2)))
    return p


# ---------------------------------------------------------------------------
# Jumping
# ---------------------------------------------------------------------------

def jump(t):
    crouch = Pose()
    relaxed(crouch, arm_down=20, elbow=20)
    crouch.both("UpperArm", (-30, 0, 0))
    crouch.both("UpperLeg", (38, 0, 0)).both("LowerLeg", (-70, 0, 0)).both("Foot", (28, 0, 0))
    crouch.add("Spine", (16, 0, 0))
    crouch.hips = Vector((0, 0.02, -0.14))
    up = Pose()
    relaxed(up, arm_down=-10, elbow=25)
    up.both("UpperArm", (60, 0, 0))
    up.both("UpperLeg", (-6, 0, 0)).both("LowerLeg", (-8, 0, 0)).both("Foot", (-30, 0, 0))
    up.add("Spine", (-4, 0, 0)).add("Head", (-6, 0, 0))
    up.hips = Vector((0, 0, 0.0))
    tuck = Pose()
    relaxed(tuck, arm_down=0, elbow=40)
    tuck.both("UpperArm", (40, 0, 0))
    tuck.both("UpperLeg", (50, 0, 0)).both("LowerLeg", (-80, 0, 0)).both("Foot", (10, 0, 0))
    tuck.add("Spine", (6, 0, 0))
    tuck.hips = Vector((0, 0, -0.02))
    return keyframes(t, [(0.0, relaxed(Pose())), (0.35, crouch), (0.6, up), (1.0, tuck)])


def fall(t):
    p = Pose()
    relaxed(p, arm_down=-25 + 6 * osc(t, 2), elbow=30)
    p.both("UpperArm", (20, 0, 0))
    p.side("Left", "UpperLeg", (30 + 8 * osc(t, 1), 0, 4)).side("Right", "UpperLeg", (14 - 8 * osc(t, 1), 0, 4))
    p.both("LowerLeg", (-50, 0, 0)).both("Foot", (-12, 0, 0))
    p.add("Spine", (4, 0, 0)).add("Head", (-8, 0, 0))
    return p


def land(t):
    impact = Pose()
    relaxed(impact, arm_down=10, elbow=30)
    impact.both("UpperArm", (20, 0, 0))
    impact.both("UpperLeg", (55, 0, 3)).both("LowerLeg", (-95, 0, 0)).both("Foot", (38, 0, 0))
    impact.add("Spine", (22, 0, 0)).add("Head", (-10, 0, 0))
    impact.hips = Vector((0, 0.02, -0.24))
    return keyframes(t, [(0.0, fall(0)), (0.18, impact), (1.0, idle(0))])


# ---------------------------------------------------------------------------
# Interactions
# ---------------------------------------------------------------------------

def wave(t):
    base = relaxed(Pose())
    up = relaxed(Pose())
    up.side("Right", "UpperArm", (5, -80, 40))  # raise and turn the right arm
    up.side("Right", "LowerArm", (95, 0, 0))
    hand(up, 0, 0, sides=("Right",))
    keys = [(0.0, base), (0.18, up)]
    for k in range(4):
        w = Pose(up)
        w.side("Right", "LowerArm", (0, 0, 22 if k % 2 == 0 else -22))
        w.side("Right", "Hand", (0, 0, 10 if k % 2 == 0 else -10))
        keys.append((0.26 + 0.12 * k, w))
    keys += [(0.82, up), (1.0, base)]
    p = keyframes(t, keys)
    p.add("Head", (0, -4, -3))
    return p


def point(t):
    base = relaxed(Pose())
    aim = relaxed(Pose())
    aim.side("Right", "UpperArm", (78, 0, 30))
    aim.side("Right", "LowerArm", (8, 0, 0))
    hand(aim, 75, 40, sides=("Right",))
    for seg in ("Proximal", "Intermediate", "Distal"):
        aim.side("Right", f"Index{seg}", (75 * 0.85 * (1.0 if seg == "Proximal" else 1.2), 0, 0))
    aim.add("Spine", (0, -6, 0)).add("Head", (0, -6, 0))
    return keyframes(t, [(0.0, base), (0.25, aim), (0.75, aim), (1.0, base)])


def pick_up(t):
    base = relaxed(Pose())
    low = relaxed(Pose(), arm_down=10, elbow=10)
    low.both("UpperArm", (55, 0, 0))
    low.both("UpperLeg", (70, 0, 6)).both("LowerLeg", (-105, 0, 0)).both("Foot", (35, 0, 0))
    low.add("Spine", (30, 0, 0)).add("Chest", (10, 0, 0)).add("Head", (10, 0, 0))
    low.hips = Vector((0, 0.05, -0.30))
    grab = Pose(low)
    grab.hips = low.hips
    fist(grab)
    carry = relaxed(Pose(), arm_down=20, elbow=70)
    carry.both("UpperArm", (25, 0, 0))
    fist(carry)
    return keyframes(t, [(0.0, base), (0.35, low), (0.5, grab), (0.85, carry), (1.0, carry)])


def use_object(t):
    base = relaxed(Pose())
    reach = relaxed(Pose())
    reach.side("Right", "UpperArm", (62, 0, 26))
    reach.side("Right", "LowerArm", (35, 0, 0))
    reach.add("Spine", (6, -8, 0)).add("Head", (8, -4, 0))
    turn = Pose(reach)
    turn.side("Right", "LowerArm", (0, 45, 0))
    hand(turn, 60, 30, sides=("Right",))
    return keyframes(t, [(0.0, base), (0.3, reach), (0.5, turn), (0.65, reach), (0.7, turn), (1.0, base)])


def push(t):
    p = relaxed(Pose(), arm_down=10, elbow=0)
    p.both("UpperArm", (70, 0, 10))
    p.both("LowerArm", (25 + 15 * osc(t), 0, 0))
    p.both("Hand", (-50, 0, 0))
    hand(p, 5, 5)
    q = gait(t, 0.8, 18, 30, 0, 22, 0.006)
    for side in SIDES:
        for b in ("UpperLeg", "LowerLeg", "Foot", "Toes"):
            p[f"{side}{b}"] = q[f"{side}{b}"]
    p.add("Spine", (20, 0, 0)).add("Head", (-10, 0, 0))
    p.hips = q.hips
    return p


def pull(t):
    p = relaxed(Pose(), arm_down=10, elbow=0)
    p.both("UpperArm", (60, 0, 8))
    p.both("LowerArm", (30 + 15 * osc(t), 0, 0))
    fist(p)
    q = gait(t, 0.8, 16, 30, 0, -14, 0.006, backward=True)
    for side in SIDES:
        for b in ("UpperLeg", "LowerLeg", "Foot", "Toes"):
            p[f"{side}{b}"] = q[f"{side}{b}"]
    p.add("Spine", (-12, 0, 0)).add("Head", (8, 0, 0))
    p.hips = q.hips + Vector((0, 0.03, -0.03))
    return p


def interact(t):
    """A short 'press a button / take an item' gesture at chest height."""
    base = relaxed(Pose())
    out = relaxed(Pose())
    out.side("Right", "UpperArm", (55, 0, 22))
    out.side("Right", "LowerArm", (45, 0, 0))
    hand(out, 10, 10, sides=("Right",))
    press = Pose(out)
    press.side("Right", "UpperArm", (8, 0, 0))
    press.side("Right", "LowerArm", (-15, 0, 0))
    return keyframes(t, [(0.0, base), (0.35, out), (0.5, press), (0.62, out), (1.0, base)])


# ---------------------------------------------------------------------------
# Additional
# ---------------------------------------------------------------------------

def swim(t):
    """Breaststroke-ish tread, horizontal-ish body (in place)."""
    p = Pose()
    s = osc(t)
    p.add("Hips", (70, 0, 0))
    p.add("Spine", (-8, 0, 0)).add("Neck", (-30, 0, 0)).add("Head", (-25, 0, 0))
    p.both("UpperArm", (95 + 25 * s, 0, 20 + 40 * max(0.0, -s)))
    p.both("LowerArm", (40 * max(0.0, s) + 10, 0, 0))
    hand(p, 5, 5)
    p.both("UpperLeg", (-60 + 25 * osc(t, 1, 0.3), 0, 10 + 12 * max(0.0, osc(t, 1, 0.3))))
    p.both("LowerLeg", (-60 * max(0.0, osc(t, 1, 0.55)) - 5, 0, 0))
    p.both("Foot", (-40, 0, 0))
    p.hips = Vector((0, 0, -0.05 + 0.02 * s))
    return p


def climb(t):
    """Climbing a ladder or wall, in place: hands and feet alternate."""
    p = Pose()
    for side, ph in (("Left", 0.0), ("Right", 0.5)):
        s = osc(t, 1, ph)
        p.side(side, "UpperArm", (120 + 30 * s, 0, 18))
        p.side(side, "LowerArm", (55 - 40 * s, 0, 0))
        hand(p, 55 if s < 0 else 20, 30, sides=(side,))
        p.side(side, "UpperLeg", (45 + 35 * s, 0, 8))
        p.side(side, "LowerLeg", (-(55 + 45 * s), 0, 0))
        p.side(side, "Foot", (10, 0, 0))
    p.add("Spine", (6, 0, 0)).add("Head", (-14, 0, 0))
    p.hips = Vector((0, 0.06, -0.05 + 0.03 * osc(t, 2)))
    return p


def sit(t):
    """Seated on a chair-height seat (0.45 m), loop with a breath."""
    p = relaxed(Pose(), arm_down=24, elbow=55)
    p.both("UpperArm", (22, 0, 0))
    p.both("UpperLeg", (88, 0, 4)).both("LowerLeg", (-88, 0, 0)).both("Foot", (0, 0, 0))
    b = osc(t)
    p.add("Spine", (-2 + b, 0, 0)).add("Chest", (2, 0, 0)).add("Head", (2, 2 * osc(t, 1, 0.3), 0))
    p.hips = Vector((0, 0.13, -0.44))
    return p


def emote_cheer(t):
    base = relaxed(Pose())
    up = Pose()
    up.both("UpperArm", (20, 0, 95))
    up.both("LowerArm", (10, 0, 0))
    fist(up)
    up.add("Spine", (-6, 0, 0)).add("Head", (-14, 0, 0))
    hop = Pose(up)
    hop.both("UpperLeg", (10, 0, 0)).both("LowerLeg", (-25, 0, 0))
    hop.hips = Vector((0, 0, 0.05))
    dip = Pose(up)
    dip.both("UpperLeg", (25, 0, 0)).both("LowerLeg", (-45, 0, 0)).both("Foot", (20, 0, 0))
    dip.hips = Vector((0, 0, -0.07))
    return keyframes(t, [(0.0, base), (0.2, dip), (0.35, hop), (0.5, dip), (0.65, hop), (0.8, up), (1.0, base)])


def emote_clap(t):
    p = relaxed(Pose(), arm_down=12, elbow=80)
    open_ = 18 * (0.5 + 0.5 * osc(t, 4))
    p.both("UpperArm", (45, -10, -open_ * 0.2))
    p.both("LowerArm", (0, 35 + open_, 0))
    hand(p, 4, 4)
    p.add("Head", (6 * osc(t, 4, 0.25) * 0.3, 0, 0))
    return p


def emote_shrug(t):
    base = relaxed(Pose())
    up = relaxed(Pose(), arm_down=30, elbow=80)
    up.both("Shoulder", (0, 0, 14))
    up.both("LowerArm", (0, -40, 0))
    hand(up, 2, 2)
    up.add("Head", (-4, 0, 8)).add("Neck", (0, 0, 4))
    return keyframes(t, [(0.0, base), (0.3, up), (0.7, up), (1.0, base)])


def emote_dance(t):
    p = relaxed(Pose(), arm_down=10, elbow=70)
    s, c = osc(t, 2), osc(t, 2, 0.25)
    p.both("UpperArm", (30 + 20 * s, 0, 0))
    p.side("Left", "LowerArm", (60 + 30 * c, 0, 0)).side("Right", "LowerArm", (60 - 30 * c, 0, 0))
    fist(p)
    p.add("Hips", (0, 10 * s, 6 * c)).add("Spine", (4, -8 * s, -4 * c)).add("Head", (6 * abs(c), 6 * s, 0))
    p.side("Left", "UpperLeg", (15 * max(0.0, s), 0, 0)).side("Left", "LowerLeg", (-30 * max(0.0, s), 0, 0))
    p.side("Right", "UpperLeg", (15 * max(0.0, -s), 0, 0)).side("Right", "LowerLeg", (-30 * max(0.0, -s), 0, 0))
    p.hips = Vector((0.03 * c, 0, -0.03 + 0.025 * abs(s)))
    return p


def hit_react(t):
    base = idle(0)
    hit = relaxed(Pose(), arm_down=20, elbow=35)
    hit.add("Spine", (-14, 10, 4)).add("Chest", (-8, 6, 0)).add("Head", (-18, 10, 6))
    hit.both("UpperArm", (25, 0, 0))
    hit.side("Left", "UpperLeg", (-8, 0, 0)).side("Right", "UpperLeg", (12, 0, 0))
    hit.both("LowerLeg", (-15, 0, 0))
    hit.hips = Vector((0, 0.05, -0.02))
    return keyframes(t, [(0.0, base), (0.12, hit), (0.35, hit), (1.0, base)])


def root_motion(fn, speed, frames):
    """`fn` with the Root carried forward at `speed` m/s over the clip."""
    dur = frames / FPS

    def at(t):
        p = fn(t)
        p.root = Vector((0, -speed * dur * t, 0))
        return p
    return at


# name: (function, frames, loops)
CLIPS = [
    ("Idle", idle, 90, True),
    ("Walk", walk, 32, True),
    ("Run", run, 22, True),
    ("Sprint", sprint, 18, True),
    ("WalkBackward", lambda t: gait(1 - t, WALK_SPEED, 20, 40, 14, -2, 0.01, backward=False), 34, True),
    ("WalkLeft", lambda t: strafe(t, 1, WALK_SPEED), 30, True),
    ("WalkRight", lambda t: strafe(t, -1, WALK_SPEED), 30, True),
    ("RunBackward", lambda t: gait(1 - t, RUN_SPEED, 32, 70, 24, -4, 0.025), 24, True),
    ("RunLeft", lambda t: strafe(t, 1, RUN_SPEED), 22, True),
    ("RunRight", lambda t: strafe(t, -1, RUN_SPEED), 22, True),
    ("Jump", jump, 20, False),
    ("Fall", fall, 30, True),
    ("Land", land, 20, False),
    ("Crouch", crouch_idle, 80, True),
    ("CrouchWalk", crouch_walk, 40, True),
    ("Wave", wave, 75, False),
    ("Point", point, 60, False),
    ("PickUp", pick_up, 50, False),
    ("Use", use_object, 60, False),
    ("Push", push, 40, True),
    ("Pull", pull, 40, True),
    ("Interact", interact, 36, False),
    ("Swim", swim, 50, True),
    ("Climb", climb, 40, True),
    ("Sit", sit, 90, True),
    ("Cheer", emote_cheer, 60, False),
    ("Clap", emote_clap, 40, True),
    ("Shrug", emote_shrug, 45, False),
    ("Dance", emote_dance, 60, True),
    ("HitReact", hit_react, 24, False),
]


def build_actions(arm_obj, only=None):
    acts = []
    bpy.context.scene.render.fps = FPS
    for name, fn, frames, loop in CLIPS:
        if only and name not in only:
            continue
        acts.append(make_action(arm_obj, name, frames, fn, loop))
    # Root-motion versions of the main forward locomotion.
    for name, fn, frames, speed in (("Walk", walk, 32, WALK_SPEED), ("Run", run, 22, RUN_SPEED)):
        if only and name not in only:
            continue
        acts.append(make_action(arm_obj, f"{name}_RootMotion", frames,
                                root_motion(fn, speed, frames), loop=False))
    arm_obj.animation_data.action = None
    for pb in arm_obj.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    return acts
