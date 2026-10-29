"""Builds Boundmore's playable races in Blender (headless, the `bpy` module).

Every race is one shared skeleton (the same bone names, so one set of
animations plays on all of them) dressed in its own body proportions, head,
hands, feet and markings, with the belt every traveller wears.

    python tools/bm-modelgen/races.py OUT_DIR [RACE ...]

writes OUT_DIR/<race>.glb and preview renders. Units are blocks; a body is
about 1.9 tall, as `MoveConfig` says. Colour areas are material slots the
game tints per player: skin, belly, mark, claw, eye, iris, cloth, metal.
"""

import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

args = [a for a in sys.argv[1:] if not a.endswith(".py")]
OUT = Path(args[0] if args else "out")
ONLY = set(args[1:])
OUT.mkdir(parents=True, exist_ok=True)


# ---- small helpers ----------------------------------------------------------


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def material(name, colour, rough=0.6, metal=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*colour, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    return m


def apply_all(obj):
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    for m in list(obj.modifiers):
        bpy.ops.object.modifier_apply(modifier=m.name)
    obj.select_set(False)


def smooth(obj):
    for p in obj.data.polygons:
        p.use_smooth = True


def place(obj, loc, rot=(0, 0, 0)):
    """Turns a mesh built at the origin about its centre, then moves it."""
    m = Matrix.Translation(Vector(loc)) @ Matrix.Rotation(rot[2], 4, "Z") @ Matrix.Rotation(
        rot[1], 4, "Y"
    ) @ Matrix.Rotation(rot[0], 4, "X")
    obj.data.transform(m)
    return obj


def blob(name, loc, size, mat, rot=(0, 0, 0), segments=24):
    """A squashed sphere: heads, muzzles, eyes, plates."""
    bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=segments // 2, location=(0, 0, 0))
    o = bpy.context.active_object
    o.name = name
    o.data.transform(Matrix.Diagonal((*size, 1.0)))
    smooth(o)
    o.data.materials.append(mat)
    return place(o, loc, rot)


def spike(name, loc, rot, r, length, mat, bend=0.0):
    """A tapered horn, blade or claw along +Z, curved by `bend`."""
    bpy.ops.mesh.primitive_cone_add(vertices=12, radius1=r, radius2=r * 0.08, depth=length, location=(0, 0, 0))
    o = bpy.context.active_object
    o.name = name
    o.data.transform(Matrix.Translation((0, 0, length / 2)))
    if bend:
        for v in o.data.vertices:
            t = v.co.z / length
            v.co.y += bend * length * t * t
    sub = o.modifiers.new("sub", "SUBSURF")
    sub.levels = 1
    apply_all(o)
    smooth(o)
    o.data.materials.append(mat)
    return place(o, loc, rot)


def skin_mesh(name, joints, edges, radii, mat, subdiv=2):
    """A smooth body grown along a joint graph (the skin modifier)."""
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([Vector(j) for j in joints], edges, [])
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    skin = obj.modifiers.new("skin", "SKIN")
    skin.use_smooth_shade = True
    skin.branch_smoothing = 0.6
    for v, r in zip(mesh.skin_vertices[0].data, radii):
        v.radius = r
    mesh.skin_vertices[0].data[0].use_root = True
    sub = obj.modifiers.new("sub", "SUBSURF")
    sub.levels = subdiv
    apply_all(obj)
    obj.data.materials.append(mat)
    return obj


# ---- the shared skeleton ----------------------------------------------------

# Bones: (name, head joint, tail joint, parent). Every race has these.
BONES = [
    ("hips", "hips", "waist", None),
    ("spine", "waist", "chest", "hips"),
    ("chest", "chest", "neck", "spine"),
    ("neck", "neck", "skull", "chest"),
    ("head", "skull", "crown", "neck"),
]
for _s in ("L", "R"):
    BONES += [
        (f"upper_arm.{_s}", f"shoulder.{_s}", f"elbow.{_s}", "chest"),
        (f"forearm.{_s}", f"elbow.{_s}", f"wrist.{_s}", f"upper_arm.{_s}"),
        (f"hand.{_s}", f"wrist.{_s}", f"palm.{_s}", f"forearm.{_s}"),
        (f"thigh.{_s}", f"hip.{_s}", f"knee.{_s}", "hips"),
        (f"shin.{_s}", f"knee.{_s}", f"ankle.{_s}", f"thigh.{_s}"),
        (f"foot.{_s}", f"ankle.{_s}", f"ball.{_s}", f"shin.{_s}"),
    ]


def skeleton(r):
    """Joint positions for a race (x right, y back, z up). Lean and slightly
    hunched, broad shouldered, arms to the knees, a small head."""
    w = r["build"]  # shoulder and chest breadth
    hunch = r["hunch"]
    j = {
        "hips": (0, 0.0, 0.94),
        "waist": (0, 0.01, 1.10),
        "chest": (0, -0.02 - hunch * 0.3, 1.30),
        "neck": (0, -0.05 - hunch, 1.45),
        "skull": (0, -0.08 - hunch, 1.55),
        "crown": (0, -0.08 - hunch, 1.88),
    }
    for side, s in (("L", 1), ("R", -1)):
        j[f"shoulder.{side}"] = (0.25 * w * s, -0.04 - hunch * 0.5, 1.40)
        j[f"elbow.{side}"] = (0.33 * w * s, 0.04, 1.08)
        j[f"wrist.{side}"] = (0.37 * w * s, -0.04, 0.78)
        j[f"palm.{side}"] = (0.385 * w * s, -0.06, 0.68)
        j[f"hip.{side}"] = (0.12 * s, 0.0, 0.90)
        if r["legs"] == "digitigrade":
            j[f"knee.{side}"] = (0.14 * s, -0.12, 0.58)
            j[f"ankle.{side}"] = (0.14 * s, 0.12, 0.24)
            j[f"ball.{side}"] = (0.14 * s, -0.04, 0.05)
        else:
            j[f"knee.{side}"] = (0.135 * s, -0.05, 0.50)
            j[f"ankle.{side}"] = (0.135 * s, 0.03, 0.11)
            j[f"ball.{side}"] = (0.135 * s, -0.13, 0.05)
    return j


def body_mesh(r, j, mats):
    """Torso, arms, legs, fingers and toes as one smooth skin."""
    k = r["bulk"]
    pts, rad, edges = [], [], []

    def add(p, rr):
        pts.append(p)
        rad.append((rr[0] * k, rr[1] * k))
        return len(pts) - 1

    def chain(a, *rest):
        for b in rest:
            edges.append((a, b))
            a = b

    hips = add(j["hips"], (0.16, 0.12))
    waist = add(j["waist"], (0.115, 0.095))
    chest = add(j["chest"], (0.20 * r["build"], 0.15))
    upper = add(((0, j["chest"][1] - 0.01, 1.39)), (0.17 * r["build"], 0.12))
    neck = add(j["neck"], (0.07, 0.07))
    nape = add((0, j["skull"][1] + 0.01, j["skull"][2] + 0.06), (0.065, 0.07))
    chain(hips, waist, chest, upper, neck, nape)
    for side, s in (("L", 1), ("R", -1)):
        sh = add(j[f"shoulder.{side}"], (0.095, 0.09))
        el = add(j[f"elbow.{side}"], (0.06, 0.06))
        wr = add(j[f"wrist.{side}"], (0.05, 0.045))
        palm = add(j[f"palm.{side}"], (0.085, 0.05))
        chain(upper, sh, el, wr, palm)
        # Three long fingers and a thumb.
        px, py, pz = j[f"palm.{side}"]
        for i, (dx, dy, dz, l) in enumerate(r["fingers"]):
            mid = add((px + dx * s * 0.5, py + dy * 0.5, pz - l * 0.55), (0.03, 0.03))
            tip = add((px + dx * s, py + dy, pz - l), (0.022, 0.022))
            chain(palm, mid, tip)
        hp = add(j[f"hip.{side}"], (0.11, 0.11))
        kn = add(j[f"knee.{side}"], (0.075, 0.075))
        an = add(j[f"ankle.{side}"], (0.06, 0.06))
        ball = add(j[f"ball.{side}"], r["foot"])
        chain(hips, hp, kn, an, ball)
        bx, by, bz = j[f"ball.{side}"]
        for dx, l in r["toes"]:
            tip = add((bx + dx * s, by - l, 0.03), (0.026, 0.022))
            chain(ball, tip)
    body = skin_mesh("body", pts, edges, rad, mats["skin"])
    # Muscle masses over the frame, then everything fused into one surface
    # (a voxel remesh) and brought down to a game-sized mesh.
    masses = muscles(r, j, mats["skin"])
    bpy.ops.object.select_all(action="DESELECT")
    for o in masses:
        o.select_set(True)
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.join()
    fuse(body, 0.011, 0.25)
    return body


def fuse(obj, voxel, keep):
    """One watertight smooth surface from overlapping parts."""
    rm = obj.modifiers.new("remesh", "REMESH")
    rm.mode = "VOXEL"
    rm.voxel_size = voxel
    sm = obj.modifiers.new("smooth", "CORRECTIVE_SMOOTH")
    sm.iterations = 6
    sm.use_only_smooth = True
    dec = obj.modifiers.new("decimate", "DECIMATE")
    dec.ratio = keep
    apply_all(obj)
    smooth(obj)


def along(name, a, b, t, width, depth, length, mat, out=(0, 0, 0)):
    """A muscle blob laid along the bone from joint `a` to `b`, at `t` of
    the way, pushed `out` from it."""
    a, b = Vector(a), Vector(b)
    c = a.lerp(b, t) + Vector(out)
    o = blob(name, (0, 0, 0), (width, depth, length), mat)
    q = (b - a).to_track_quat("Z", "Y")
    o.data.transform(Matrix.Translation(c) @ q.to_matrix().to_4x4())
    return o


def muscles(r, j, mat):
    k = r["bulk"]
    w = r["build"]
    cy = j["chest"][1]
    out = [
        # Chest and back: a broad wedge narrowing to the waist.
        blob("pecs", (0, cy - 0.06, 1.33), (0.19 * w * k, 0.09 * k, 0.09 * k), mat),
        blob("lats", (0, cy + 0.05, 1.27), (0.17 * w * k, 0.09 * k, 0.13 * k), mat),
        blob("traps", (0, cy + 0.02, 1.42), (0.13 * w * k, 0.08 * k, 0.05 * k), mat),
        blob("glutes", (0, 0.06, 0.90), (0.14 * k, 0.09 * k, 0.09 * k), mat),
    ]
    for side, s in (("L", 1), ("R", -1)):
        sh, el, wr = j[f"shoulder.{side}"], j[f"elbow.{side}"], j[f"wrist.{side}"]
        hp, kn, an = j[f"hip.{side}"], j[f"knee.{side}"], j[f"ankle.{side}"]
        out += [
            blob(f"delt.{side}", (sh[0] * 1.04, sh[1], sh[2] - 0.01), (0.10 * k, 0.10 * k, 0.09 * k), mat),
            along(f"bicep.{side}", sh, el, 0.5, 0.072 * k, 0.075 * k, 0.15, mat),
            along(f"forearm.{side}", el, wr, 0.3, 0.07 * k, 0.065 * k, 0.13, mat),
            along(f"thighm.{side}", hp, kn, 0.4, 0.105 * k, 0.11 * k, 0.19, mat, (0, -0.01, 0)),
            along(f"calf.{side}", kn, an, 0.3, 0.07 * k, 0.08 * k, 0.13, mat, (0, 0.02, 0)),
        ]
    return out


def paint(body, j, r, mats):
    """Belly on the front of the torso and the inner limbs; markings by race."""
    names = ["belly", "mark"]
    for n in names:
        body.data.materials.append(mats[n])
    belly, mark = 1, 2
    for p in body.data.polygons:
        c, n = p.center, p.normal
        front = n.y < -0.45
        if front and abs(c.x) < 0.16 and 0.85 < c.z < 1.42:
            p.material_index = belly
        if r["marks"](c, n):
            p.material_index = mark


# ---- races ------------------------------------------------------------------


def raptor_head(j, m):
    """Narrow skull, short hooked snout, swept crest blades, spiky collar."""
    x, y, z = j["skull"]
    parts = [
        blob("skull", (0, y - 0.01, z + 0.12), (0.12, 0.15, 0.12), m["skin"]),
        blob("snout", (0, y - 0.15, z + 0.08), (0.06, 0.10, 0.055), m["skin"], (math.radians(10), 0, 0)),
        spike("beak", (0, y - 0.22, z + 0.10), (math.radians(120), 0, 0), 0.04, 0.11, m["claw"], 0.5),
        blob("jaw", (0, y - 0.11, z + 0.03), (0.07, 0.08, 0.04), m["belly"]),
    ]
    for side, s in (("L", 1), ("R", -1)):
        parts.append(blob(f"eye.{side}", (0.07 * s, y - 0.11, z + 0.15), (0.03, 0.025, 0.022), m["iris"], (0, 0, math.radians(-25 * s)), 16))
    # Crest: blades sweeping back from the brow.
    for i, (dx, tilt, length) in enumerate(((0.0, 60, 0.30), (0.05, 70, 0.26), (-0.05, 70, 0.26), (0.08, 82, 0.20), (-0.08, 82, 0.20))):
        parts.append(spike(f"crest.{i}", (dx, y - 0.02, z + 0.2), (math.radians(-tilt), math.radians(-dx * 120), 0), 0.035, length, m["mark"], -0.25))
    # Collar of spikes round the neck.
    nx, ny, nz = j["neck"]
    for i in range(9):
        a = math.radians(-100 + i * 25)
        parts.append(spike(f"collar.{i}", (math.sin(a) * 0.09, ny + math.cos(a) * 0.07, nz - 0.03), (math.radians(-35 * math.cos(a)), math.radians(35 * math.sin(a)), 0), 0.03, 0.14, m["mark"], 0.15))
    return parts


def finned_head(j, m):
    """Broad flat head, tall fin crest, side frills, big wide-set eyes."""
    x, y, z = j["skull"]
    parts = [
        blob("skull", (0, y - 0.02, z + 0.11), (0.15, 0.15, 0.11), m["skin"]),
        blob("mouth", (0, y - 0.09, z + 0.06), (0.10, 0.07, 0.035), m["belly"]),
        # The fin crest: a thin plate from brow to nape.
        blob("crest", (0, y + 0.05, z + 0.22), (0.015, 0.17, 0.12), m["mark"], (math.radians(-20), 0, 0)),
    ]
    for side, s in (("L", 1), ("R", -1)):
        parts.append(blob(f"eye.{side}", (0.10 * s, y - 0.10, z + 0.14), (0.035, 0.03, 0.03), m["iris"], segments=16))
        # Frills fanned behind the jaw.
        for i in range(3):
            parts.append(blob(f"frill.{side}{i}", (0.15 * s, y + 0.03 + i * 0.03, z + 0.12 - i * 0.05), (0.012, 0.09, 0.04), m["mark"], (math.radians(10 + i * 15), 0, math.radians(25 * s))))
    return parts


def beast_head(j, m):
    """Heavy skull, long muzzle, tall pointed ears, a fur ruff."""
    x, y, z = j["skull"]
    parts = [
        blob("skull", (0, y, z + 0.13), (0.13, 0.14, 0.12), m["skin"]),
        blob("muzzle", (0, y - 0.17, z + 0.07), (0.065, 0.13, 0.06), m["skin"]),
        blob("nose", (0, y - 0.29, z + 0.09), (0.03, 0.025, 0.022), m["claw"], segments=12),
        blob("chin", (0, y - 0.13, z + 0.01), (0.055, 0.10, 0.03), m["belly"]),
    ]
    for side, s in (("L", 1), ("R", -1)):
        parts.append(blob(f"eye.{side}", (0.06 * s, y - 0.10, z + 0.16), (0.022, 0.02, 0.016), m["iris"], segments=12))
        parts.append(spike(f"ear.{side}", (0.08 * s, y + 0.02, z + 0.22), (math.radians(-15), math.radians(20 * s), 0), 0.05, 0.20, m["skin"]))
        parts.append(spike(f"brow.{side}", (0.06 * s, y - 0.08, z + 0.20), (math.radians(-80), math.radians(20 * s), 0), 0.02, 0.06, m["mark"]))
    # The ruff over the shoulders.
    nx, ny, nz = j["neck"]
    for i in range(11):
        a = math.radians(-120 + i * 24)
        parts.append(spike(f"ruff.{i}", (math.sin(a) * 0.13, ny + math.cos(a) * 0.09 + 0.03, nz - 0.05), (math.radians(-60 * math.cos(a) - 20), math.radians(55 * math.sin(a)), 0), 0.05, 0.16, m["mark"], -0.2))
    return parts


def claws(j, r, m):
    """Claws on every finger and toe tip."""
    out = []
    for side, s in (("L", 1), ("R", -1)):
        px, py, pz = j[f"palm.{side}"]
        for i, (dx, dy, dz, l) in enumerate(r["fingers"]):
            out.append(spike(f"fclaw.{side}{i}", (px + dx * s, py + dy, pz - l), (math.radians(180), 0, 0), 0.016, 0.05, m["claw"], 0.3))
        bx, by, bz = j[f"ball.{side}"]
        for i, (dx, l) in enumerate(r["toes"]):
            out.append(spike(f"tclaw.{side}{i}", (bx + dx * s, by - l - 0.02, 0.03), (math.radians(95), 0, 0), 0.02, r["claw"], m["claw"], 0.3))
    return out


def belt(j, m):
    """The belt and loincloth every traveller wears."""
    x, y, z = j["hips"]
    bpy.ops.mesh.primitive_torus_add(major_radius=0.17, minor_radius=0.025, location=(0, 0, 0))
    b = bpy.context.active_object
    b.data.transform(Matrix.Diagonal((1.0, 0.8, 1.6, 1)))
    smooth(b)
    b.data.materials.append(m["cloth"])
    place(b, (0, y, z + 0.04))
    buckle = blob("buckle", (0, y - 0.145, z + 0.04), (0.04, 0.015, 0.035), m["metal"], segments=12)
    flaps = [
        blob("flap.front", (0, y - 0.13, z - 0.11), (0.09, 0.015, 0.13), m["cloth"]),
        blob("flap.back", (0, y + 0.13, z - 0.10), (0.10, 0.015, 0.12), m["cloth"]),
    ]
    return [b, buckle] + flaps


def _raptor_marks(c, n):
    return (c.z < 0.30) or (abs(c.x) > 0.33 and c.z < 0.86)


def _finned_marks(c, n):
    # A darker back stripe and banded forearms.
    back = n.y > 0.5 and abs(c.x) < 0.07 and c.z > 0.95
    bands = abs(c.x) > 0.30 and (0.84 < c.z < 0.88 or 0.92 < c.z < 0.96)
    return back or bands


def _beast_marks(c, n):
    return n.y > 0.3 and c.z > 0.95 and abs(c.x) < 0.2


RACES = {
    "raptor": dict(
        build=1.0, bulk=0.92, hunch=0.06, legs="digitigrade",
        fingers=[(0.03, -0.03, 0, 0.17), (0.0, -0.05, 0, 0.18), (-0.03, -0.03, 0, 0.16), (-0.05, -0.05, 0, 0.09)],
        toes=[(0.05, 0.16), (0.0, 0.18), (-0.05, 0.16)], foot=(0.05, 0.04), claw=0.08,
        head=raptor_head, marks=_raptor_marks,
        colours=dict(skin=(0.30, 0.31, 0.34), belly=(0.62, 0.62, 0.64), mark=(0.86, 0.86, 0.84),
                     claw=(0.85, 0.62, 0.12), iris=(0.95, 0.75, 0.10)),
    ),
    "finned": dict(
        build=1.0, bulk=1.0, hunch=0.05, legs="plantigrade",
        fingers=[(0.04, -0.03, 0, 0.16), (0.0, -0.05, 0, 0.17), (-0.04, -0.03, 0, 0.15), (-0.06, -0.05, 0, 0.08)],
        toes=[(0.05, 0.12), (0.0, 0.14), (-0.05, 0.12)], foot=(0.07, 0.04), claw=0.04,
        head=finned_head, marks=_finned_marks,
        colours=dict(skin=(0.14, 0.32, 0.36), belly=(0.55, 0.70, 0.66), mark=(0.45, 0.20, 0.55),
                     claw=(0.20, 0.18, 0.22), iris=(0.95, 0.85, 0.30)),
    ),
    "beast": dict(
        build=1.18, bulk=1.2, hunch=0.09, legs="digitigrade",
        fingers=[(0.04, -0.03, 0, 0.13), (0.0, -0.05, 0, 0.14), (-0.04, -0.03, 0, 0.13), (-0.06, -0.05, 0, 0.07)],
        toes=[(0.05, 0.10), (0.0, 0.12), (-0.05, 0.10)], foot=(0.07, 0.05), claw=0.05,
        head=beast_head, marks=_beast_marks,
        colours=dict(skin=(0.42, 0.42, 0.44), belly=(0.70, 0.68, 0.64), mark=(0.22, 0.22, 0.25),
                     claw=(0.12, 0.11, 0.11), iris=(0.40, 0.85, 0.95)),
    ),
}


# ---- rig and animation ------------------------------------------------------


def build_rig(j):
    arm = bpy.data.armatures.new("rig")
    rig = bpy.data.objects.new("rig", arm)
    bpy.context.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.edit_bones
    for name, a, b, parent in BONES:
        bone = eb.new(name)
        bone.head, bone.tail = Vector(j[a]), Vector(j[b])
        if parent:
            bone.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return rig


def rigid(rig, parts, bone):
    for p in parts:
        vg = p.vertex_groups.new(name=bone)
        vg.add(list(range(len(p.data.vertices))), 1.0, "REPLACE")


def bind(rig, body, groups):
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    bpy.ops.object.select_all(action="DESELECT")
    for bone, parts in groups.items():
        rigid(rig, parts, bone)
        for p in parts:
            p.select_set(True)
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.join()
    return body


def key(rig, frame, poses):
    for bone, (x, y, z) in poses.items():
        pb = rig.pose.bones[bone]
        pb.rotation_mode = "XYZ"
        pb.rotation_euler = tuple(math.radians(v) for v in (x, y, z))
        pb.keyframe_insert("rotation_euler", frame=frame)


def animate(rig):
    rig.animation_data_create()
    rest = {b.name: (0, 0, 0) for b in rig.pose.bones}
    walk = bpy.data.actions.new("walk")
    rig.animation_data.action = walk
    for f, ph in ((1, 0), (9, 1), (17, 2), (25, 3), (33, 0)):
        s = [1, 0, -1, 0][ph]
        p = dict(rest)
        for side, k in (("L", 1), ("R", -1)):
            p[f"thigh.{side}"] = (-28 * s * k, 0, 0)
            p[f"shin.{side}"] = (30 if s * k < 0 else 6, 0, 0)
            p[f"upper_arm.{side}"] = (22 * s * k, 0, 0)
            p[f"forearm.{side}"] = (-15, 0, 0)
        p["spine"] = (3, 0, 5 * s)
        key(rig, f, p)
    walk.use_fake_user = True
    idle = bpy.data.actions.new("idle")
    rig.animation_data.action = idle
    for f, t in ((1, 0), (41, 1), (81, 0)):
        p = dict(rest)
        p["chest"] = (-2 * t, 0, 0)
        p["head"] = (3 * t, 0, 10 * t)
        key(rig, f, p)
    idle.use_fake_user = True
    rig.animation_data.action = walk


# ---- building and showing ---------------------------------------------------


def build(name, r):
    clear()
    m = {k: material(k, v, 0.55) for k, v in r["colours"].items()}
    m["cloth"] = material("cloth", (0.30, 0.18, 0.10), 0.85)
    m["metal"] = material("metal", (0.75, 0.62, 0.35), 0.35, 0.8)
    j = skeleton(r)
    body = body_mesh(r, j, m)
    paint(body, j, r, m)
    head = r["head"](j, m)
    hands = claws(j, r, m)
    waist = belt(j, m)
    rig = build_rig(j)
    # Claws ride the hands and feet they grow from.
    by_bone = {"head": [p for p in head if not p.name.startswith(("collar", "ruff"))],
               "chest": [p for p in head if p.name.startswith(("collar", "ruff"))],
               "hips": waist}
    for p in hands:
        side = p.name[6]
        bone = f"hand.{side}" if p.name.startswith("f") else f"foot.{side}"
        by_bone.setdefault(bone, []).append(p)
    body = bind(rig, body, by_bone)
    body.name = name
    animate(rig)
    tris = sum(len(p.vertices) - 2 for p in body.data.polygons)
    print(f"{name}: {tris} triangles")
    bpy.ops.export_scene.gltf(filepath=str(OUT / f"{name}.glb"), export_format="GLB", export_animations=True)
    return rig


def stage():
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 48
    scene.render.resolution_x = 520
    scene.render.resolution_y = 720
    world = bpy.data.worlds.new("sky")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.5, 0.5, 0.52, 1.0)
    scene.world = world
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.0
    sun.rotation_euler = (math.radians(45), math.radians(10), math.radians(-35))
    scene.collection.objects.link(sun)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.lens = 55
    scene.collection.objects.link(cam)
    scene.camera = cam
    return scene, cam


def shoot(scene, cam, name, angle, frame):
    scene.frame_set(frame)
    a = math.radians(angle)
    cam.location = (math.sin(a) * 3.3, -math.cos(a) * 3.3, 1.25)
    cam.rotation_euler = (Vector((0, 0, 0.95)) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scene.render.filepath = str(OUT / f"{name}.png")
    bpy.ops.render.render(write_still=True)


for race, spec in RACES.items():
    if ONLY and race not in ONLY:
        continue
    rig = build(race, spec)
    rig.animation_data.action = bpy.data.actions["idle"]
    scene, cam = stage()
    shoot(scene, cam, f"{race}-front", 25, 1)
    shoot(scene, cam, f"{race}-side", 100, 1)
