"""The raptor race, built in Blender (headless, the `bpy` module): a lean,
crested, bird-footed traveller. The fine-tuned first race; the others follow
its method (see races.py for the shared skeleton they all use).

    python tools/bm-modelgen/raptor.py OUT_DIR

writes OUT_DIR/raptor.glb and preview renders.

Colour: one tinted material for the whole creature. Each vertex carries a
mask (the `mask` colour attribute, exported as COLOR_0): red is how much of
the belly colour shows, green the marking colour, blue the claw and scale
colour, alpha the mottling that darkens the skin. The game tints the four
colours per player; the eyes have their own colour.
"""

import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector, noise

args = [a for a in sys.argv[1:] if not a.endswith(".py")]
OUT = Path(args[0] if args else "out")
OUT.mkdir(parents=True, exist_ok=True)

# Preview colours (linear); a player picks their own.
SKIN = (0.16, 0.17, 0.19)
BELLY = (0.42, 0.42, 0.44)
MARK = (0.80, 0.80, 0.78)
CLAW = (0.80, 0.52, 0.06)
IRIS = (1.0, 0.62, 0.05)


# ---- helpers ----------------------------------------------------------------


def smoothstep(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


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
    m = (
        Matrix.Translation(Vector(loc))
        @ Matrix.Rotation(rot[2], 4, "Z")
        @ Matrix.Rotation(rot[1], 4, "Y")
        @ Matrix.Rotation(rot[0], 4, "X")
    )
    obj.data.transform(m)
    return obj


def blob(name, loc, size, rot=(0, 0, 0), segments=24):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=segments // 2)
    o = bpy.context.active_object
    o.name = name
    o.data.transform(Matrix.Diagonal((*size, 1.0)))
    smooth(o)
    return place(o, loc, rot)


def spike(name, loc, rot, r, length, bend=0.0, flat=1.0, segments=12, deep=1.0, curl=2):
    """A tapered blade along +Z curving towards +Y by `bend`; `flat` squashes
    its width in X (a feather blade rather than a horn)."""
    bpy.ops.mesh.primitive_cone_add(vertices=segments, radius1=r, radius2=r * 0.06, depth=length)
    o = bpy.context.active_object
    o.name = name
    o.data.transform(Matrix.Translation((0, 0, length / 2)))
    for v in o.data.vertices:
        t = v.co.z / length
        v.co.x *= flat
        v.co.y = v.co.y * deep + bend * length * t**curl
    sub = o.modifiers.new("sub", "SUBSURF")
    sub.levels = 1
    apply_all(o)
    smooth(o)
    return place(o, loc, rot)


def along(name, a, b, t, width, depth, length, out=(0, 0, 0)):
    a, b = Vector(a), Vector(b)
    c = a.lerp(b, t) + Vector(out)
    o = blob(name, (0, 0, 0), (width, depth, length))
    q = (b - a).to_track_quat("Z", "Y")
    o.data.transform(Matrix.Translation(c) @ q.to_matrix().to_4x4())
    return o


def join(objs, name):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    o = bpy.context.active_object
    o.name = name
    return o


def fuse(obj, voxel, keep, smooth_iter=6):
    """One watertight smooth surface from overlapping parts."""
    rm = obj.modifiers.new("remesh", "REMESH")
    rm.mode = "VOXEL"
    rm.voxel_size = voxel
    sm = obj.modifiers.new("smooth", "CORRECTIVE_SMOOTH")
    sm.iterations = smooth_iter
    sm.use_only_smooth = True
    dec = obj.modifiers.new("decimate", "DECIMATE")
    dec.ratio = keep
    apply_all(obj)
    smooth(obj)
    return obj


def mask(obj, f):
    """Fills the `mask` attribute: `f(position, normal)` gives (belly, mark,
    claw, mottle), or a constant tuple."""
    me = obj.data
    if "mask" not in me.color_attributes:
        me.color_attributes.new("mask", "FLOAT_COLOR", "POINT")
    attr = me.color_attributes["mask"]
    for i, v in enumerate(me.vertices):
        attr.data[i].color = f(v.co, v.normal) if callable(f) else f


def mottle(co):
    """Fine dark speckle and broader blotches, 0 to 1."""
    fine = noise.noise(co * 38.0)
    broad = noise.noise(co * 9.0 + Vector((3.1, 7.7, 1.3)))
    return max(0.0, min(1.0, 0.5 + 0.55 * broad + 0.35 * fine))


# ---- the frame --------------------------------------------------------------

HUNCH = 0.07
J = {
    "hips": (0, 0.0, 0.94),
    "waist": (0, 0.015, 1.10),
    "chest": (0, -0.02 - HUNCH * 0.3, 1.30),
    "neck": (0, -0.05 - HUNCH, 1.45),
    "skull": (0, -0.08 - HUNCH, 1.55),
    "crown": (0, -0.08 - HUNCH, 1.88),
}
for _side, _s in (("L", 1), ("R", -1)):
    J[f"shoulder.{_side}"] = (0.25 * _s, -0.04 - HUNCH * 0.5, 1.40)
    J[f"elbow.{_side}"] = (0.33 * _s, 0.05, 1.09)
    J[f"wrist.{_side}"] = (0.37 * _s, -0.04, 0.80)
    J[f"palm.{_side}"] = (0.39 * _s, -0.07, 0.69)
    J[f"hip.{_side}"] = (0.12 * _s, 0.0, 0.90)
    J[f"knee.{_side}"] = (0.14 * _s, -0.13, 0.58)
    J[f"ankle.{_side}"] = (0.145 * _s, 0.12, 0.24)
    J[f"ball.{_side}"] = (0.15 * _s, -0.03, 0.055)

BONES = [
    ("hips", "hips", "waist", None),
    ("spine", "waist", "chest", "hips"),
    ("chest", "chest", "neck", "spine"),
    ("neck", "neck", "skull", "chest"),
    ("head", "skull", "crown", "neck"),
]
for _side in ("L", "R"):
    BONES += [
        (f"upper_arm.{_side}", f"shoulder.{_side}", f"elbow.{_side}", "chest"),
        (f"forearm.{_side}", f"elbow.{_side}", f"wrist.{_side}", f"upper_arm.{_side}"),
        (f"hand.{_side}", f"wrist.{_side}", f"palm.{_side}", f"forearm.{_side}"),
        (f"thigh.{_side}", f"hip.{_side}", f"knee.{_side}", "hips"),
        (f"shin.{_side}", f"knee.{_side}", f"ankle.{_side}", f"thigh.{_side}"),
        (f"foot.{_side}", f"ankle.{_side}", f"ball.{_side}", f"shin.{_side}"),
    ]

# Fingers (dx, dy, length) from the palm: three long ones and a thumb.
FINGERS = [(0.035, -0.035, 0.19), (0.0, -0.055, 0.20), (-0.035, -0.035, 0.18), (-0.06, -0.06, 0.10)]
# Toes (dx, forward length): three forward, one back (the hallux).
TOES = [(0.06, 0.20), (0.0, 0.23), (-0.06, 0.20), (0.0, -0.09)]


def tip(side, base, d):
    s = 1 if side == "L" else -1
    return Vector((base[0] + d[0] * s, base[1] + d[1], base[2] + d[2]))


def body():
    pts, rad, edges = [], [], []

    def add(p, r):
        pts.append(tuple(p))
        rad.append(r)
        return len(pts) - 1

    def chain(a, *rest):
        for b in rest:
            edges.append((a, b))
            a = b

    hips = add(J["hips"], (0.15, 0.115))
    waist = add(J["waist"], (0.105, 0.09))
    chest = add(J["chest"], (0.19, 0.14))
    upper = add((0, J["chest"][1] - 0.01, 1.39), (0.16, 0.115))
    neck = add(J["neck"], (0.075, 0.075))
    nape = add((0, J["skull"][1] + 0.02, J["skull"][2] + 0.05), (0.068, 0.072))
    chain(hips, waist, chest, upper, neck, nape)
    for side in ("L", "R"):
        sh = add(J[f"shoulder.{side}"], (0.09, 0.085))
        el = add(J[f"elbow.{side}"], (0.055, 0.055))
        wr = add(J[f"wrist.{side}"], (0.048, 0.042))
        palm = add(J[f"palm.{side}"], (0.088, 0.052))
        chain(upper, sh, el, wr, palm)
        p = J[f"palm.{side}"]
        for dx, dy, l in FINGERS:
            knuckle = add(tip(side, p, (dx * 0.35, dy * 0.35, -0.03)), (0.034, 0.032))
            mid = add(tip(side, p, (dx * 0.65, dy * 0.6, -l * 0.55)), (0.03, 0.028))
            end = add(tip(side, p, (dx, dy, -l)), (0.022, 0.022))
            chain(palm, knuckle, mid, end)
        hp = add(J[f"hip.{side}"], (0.10, 0.10))
        kn = add(J[f"knee.{side}"], (0.07, 0.07))
        an = add(J[f"ankle.{side}"], (0.05, 0.05))
        ball = add(J[f"ball.{side}"], (0.06, 0.045))
        chain(hips, hp, kn, an, ball)
        b = J[f"ball.{side}"]
        for dx, l in TOES:
            mid = add(tip(side, b, (dx * 0.5, -l * 0.5, -0.01)), (0.032, 0.026))
            end = add(tip(side, b, (dx, -l, -0.025)), (0.024, 0.02))
            chain(ball, mid, end)

    me = bpy.data.meshes.new("body")
    me.from_pydata([Vector(p) for p in pts], edges, [])
    o = bpy.data.objects.new("body", me)
    bpy.context.collection.objects.link(o)
    sk = o.modifiers.new("skin", "SKIN")
    sk.branch_smoothing = 0.7
    for v, r in zip(me.skin_vertices[0].data, rad):
        v.radius = r
    me.skin_vertices[0].data[0].use_root = True
    sub = o.modifiers.new("sub", "SUBSURF")
    sub.levels = 2
    apply_all(o)

    cy = J["chest"][1]
    parts = [
        o,
        blob("pecs", (0, cy - 0.06, 1.33), (0.18, 0.085, 0.085)),
        blob("lats", (0, cy + 0.05, 1.26), (0.165, 0.09, 0.13)),
        blob("traps", (0, cy + 0.03, 1.42), (0.12, 0.08, 0.055)),
        blob("glutes", (0, 0.06, 0.90), (0.135, 0.085, 0.085)),
    ]
    for side, s in (("L", 1), ("R", -1)):
        sh, el, wr = J[f"shoulder.{side}"], J[f"elbow.{side}"], J[f"wrist.{side}"]
        hp, kn, an = J[f"hip.{side}"], J[f"knee.{side}"], J[f"ankle.{side}"]
        parts += [
            blob(f"delt.{side}", (sh[0] * 1.05, sh[1], sh[2] - 0.01), (0.095, 0.095, 0.085)),
            along(f"bicep.{side}", sh, el, 0.5, 0.068, 0.072, 0.15, (0, -0.01, 0)),
            along(f"tricep.{side}", sh, el, 0.45, 0.06, 0.06, 0.14, (0, 0.02, 0)),
            along(f"forearm.{side}", el, wr, 0.3, 0.066, 0.06, 0.13),
            along(f"thighm.{side}", hp, kn, 0.4, 0.1, 0.105, 0.19, (0, -0.01, 0)),
            along(f"hams.{side}", hp, kn, 0.45, 0.085, 0.08, 0.17, (0, 0.03, 0)),
            # A lean, sinewy shank above the scaled leg.
            along(f"shank.{side}", kn, an, 0.25, 0.06, 0.07, 0.12, (0, 0.015, 0)),
        ]
    body = fuse(join(parts, "body"), 0.0095, 0.22)

    ankle_z = J["ankle.L"][2]

    def colour(co, n):
        x, y, z = co
        # The throat and a soft chest, front-facing.
        front = smoothstep(0.15, 0.6, -n.y)
        belly = front * smoothstep(0.92, 1.02, z) * smoothstep(1.50, 1.40, z) * smoothstep(0.19, 0.10, abs(x))
        # Inner thighs, lighter.
        belly = max(belly, 0.7 * smoothstep(0.2, 0.7, -n.x * (1 if x > 0 else -1)) * smoothstep(0.55, 0.7, z) * smoothstep(0.92, 0.82, z))
        # A pale mane over the nape and shoulder blades.
        back = smoothstep(0.0, 0.5, n.y)
        mark = back * smoothstep(1.24, 1.40, z) * smoothstep(0.24, 0.12, abs(x))
        # Scaled legs from just above the ankle, and the feet.
        claw = smoothstep(ankle_z + 0.09, ankle_z + 0.01, z)
        return (belly, mark, claw, mottle(co) * (1 - claw))

    mask(body, colour)
    return body


def head():
    x, y, z = J["skull"]
    parts = [
        blob("skull", (0, y + 0.02, z + 0.13), (0.10, 0.15, 0.10)),
        # A long snout tapering to the beak.
        along("snout", (0, y - 0.03, z + 0.12), (0, y - 0.24, z + 0.07), 0.45, 0.052, 0.045, 0.13),
        along("jaw", (0, y - 0.02, z + 0.04), (0, y - 0.21, z + 0.04), 0.45, 0.048, 0.03, 0.11),
        blob("nape", (0, y + 0.07, z + 0.07), (0.075, 0.09, 0.08)),
    ]
    for side, s in (("L", 1), ("R", -1)):
        # Sleek brow ridges sweeping back over the eyes.
        parts.append(along(f"brow.{side}", (0.055 * s, y - 0.12, z + 0.17), (0.07 * s, y + 0.04, z + 0.20), 0.45, 0.03, 0.02, 0.10))
    h = fuse(join(parts, "head"), 0.0055, 0.3, 5)

    def colour(co, n):
        # A pale throat and lower jaw.
        belly = smoothstep(0.1, 0.6, -n.z) * smoothstep(z + 0.10, z + 0.04, co.z)
        return (belly, 0.0, 0.0, mottle(co))

    mask(h, colour)
    out = [h]
    # The beak: a hooked sheath over the snout's end, and a smaller lower
    # beak beneath it, in the claw colour.
    # Pre-turn +Y becomes up once the blade is laid forward: `deep` makes
    # the beak taller than wide, a negative bend curls the tip down.
    upper = spike("beak", (0, y - 0.165, z + 0.085), (math.radians(88), 0, 0), 0.05, 0.135, -0.55, 0.75, 24, 1.35, 4)
    lower = spike("beak.lower", (0, y - 0.18, z + 0.045), (math.radians(96), 0, 0), 0.034, 0.08, 0.1, 0.75, 16, 1.1)
    for b in (upper, lower):
        mask(b, (0.0, 0.0, 1.0, 0.0))
        out.append(b)
    eyes = []
    for side, s in (("L", 1), ("R", -1)):
        # Set into the skull under the brow, looking forward and out.
        a = math.radians(27 * s)
        d = Vector((math.sin(a), -math.cos(a), 0.0))
        c = Vector((0.06 * s, y - 0.095, z + 0.145))
        eyes.append(blob(f"eye.{side}", c, (0.024, 0.017, 0.019), (0, 0, a), 16))
        eyes.append(blob(f"pupil.{side}", c + d * 0.016, (0.005, 0.004, 0.015), (0, 0, a), 8))
    return out, eyes


def crest():
    """Swept blades from brow to nape, then a ridge of spines down the back."""
    x, y, z = J["skull"]
    out = []
    blades = [
        (-0.07, 0.205, 50, 0.20), (-0.03, 0.225, 58, 0.28), (0.02, 0.225, 64, 0.34),
        (0.07, 0.205, 70, 0.33), (0.11, 0.17, 78, 0.28), (0.14, 0.12, 86, 0.22),
    ]
    for i, (dy, dz, tilt, length) in enumerate(blades):
        tilt += 10
        out.append(spike(f"crest.{i}", (0, y + dy, z + dz), (math.radians(-tilt), 0, 0), 0.04, length, -0.35, 0.35))
        # A smaller blade either side of the main row.
        if 1 <= i <= 4:
            for s in (1, -1):
                out.append(spike(f"crest.{i}{s}", (0.035 * s, y + dy, z + dz - 0.02), (math.radians(-tilt - 6), math.radians(-14 * s), 0), 0.03, length * 0.7, -0.3, 0.35))
    for o in out:
        mask(o, (0.0, 1.0, 0.0, 0.0))
    return out


def mane():
    """A ruff of feathered spikes round the neck and over the shoulders, and a
    ridge down the spine. Rides the chest."""
    nx, ny, nz = J["neck"]
    out = []
    rings = ((0.09, 0.08, nz + 0.02, 13, 0.17, 40, 125), (0.15, 0.11, 1.38, 11, 0.21, 62, 95))
    for ring, (rx, ry, rz, n, length, out_tilt, reach) in enumerate(rings):
        for i in range(n):
            # Round the back and sides, leaving the throat and chest open.
            a = math.radians(-reach + i * (2 * reach / (n - 1)))
            px, py = math.sin(a) * rx, ny + 0.02 + math.cos(a) * ry
            # Point outwards from the neck and back, drooping down.
            out.append(spike(f"mane.{ring}.{i}", (px, py, rz), (math.radians(-out_tilt * math.cos(a) - 25), math.radians(out_tilt * math.sin(a)), 0), 0.04, length * (0.8 + 0.3 * abs(math.cos(a))), -0.3, 0.4))
    # The ridge: spines down the back, shrinking to the waist.
    for i in range(6):
        t = i / 5
        py = J["chest"][1] + 0.13 + 0.01 * t
        pz = 1.36 - t * 0.30
        out.append(spike(f"ridge.{i}", (0, py, pz), (math.radians(-120 - 10 * t), 0, 0), 0.035 * (1 - 0.5 * t), 0.14 * (1 - 0.55 * t), -0.25, 0.35))
    for o in out:
        mask(o, (0.0, 1.0, 0.0, 0.0))
    return out


def tufts():
    """Feather tufts trailing from the backs of the forearms."""
    out = {}
    for side, s in (("L", 1), ("R", -1)):
        el, wr = Vector(J[f"elbow.{side}"]), Vector(J[f"wrist.{side}"])
        parts = []
        for i in range(3):
            p = el.lerp(wr, 0.15 + 0.12 * i) + Vector((0.035 * s, 0.045, 0))
            parts.append(spike(f"tuft.{side}{i}", p, (math.radians(-150 + 12 * i), math.radians(20 * s), 0), 0.03, 0.13 - 0.02 * i, -0.2, 0.35))
        for o in parts:
            mask(o, (0.0, 1.0, 0.0, 0.0))
        out[f"forearm.{side}"] = parts
    return out


def talons():
    out = {}
    for side, s in (("L", 1), ("R", -1)):
        p = J[f"palm.{side}"]
        hand = []
        for i, (dx, dy, l) in enumerate(FINGERS):
            t = tip(side, p, (dx, dy - 0.005, -l + 0.005))
            hand.append(spike(f"talon.{side}{i}", t, (math.radians(200), 0, 0), 0.019, 0.07, 0.6, 0.8))
        b = J[f"ball.{side}"]
        foot = []
        for i, (dx, l) in enumerate(TOES):
            t = tip(side, b, (dx, -l, -0.025))
            back = l < 0
            rot = (math.radians(-80 if back else 95), 0, 0)
            foot.append(spike(f"toetalon.{side}{i}", t, rot, 0.024, 0.09 if not back else 0.07, -0.5 if back else 0.5, 0.8))
        for o in hand + foot:
            mask(o, (0.0, 0.0, 1.0, 0.0))
        # Claws a shade darker than the scales: the tips.
        out[f"hand.{side}"] = hand
        out[f"foot.{side}"] = foot
    return out


def belt():
    x, y, z = J["hips"]
    bpy.ops.mesh.primitive_torus_add(major_radius=0.165, minor_radius=0.024)
    b = bpy.context.active_object
    b.data.transform(Matrix.Diagonal((1.0, 0.8, 1.7, 1)))
    smooth(b)
    place(b, (0, y, z + 0.05))
    buckle = blob("buckle", (0, y - 0.14, z + 0.05), (0.04, 0.014, 0.035), segments=12)
    flaps = [
        blob("flap.front", (0, y - 0.125, z - 0.12), (0.085, 0.014, 0.14), (math.radians(-6), 0, 0)),
        blob("flap.back", (0, y + 0.125, z - 0.11), (0.10, 0.014, 0.13), (math.radians(8), 0, 0)),
    ]
    return [b] + flaps, [buckle]


# ---- materials --------------------------------------------------------------


def body_material():
    m = bpy.data.materials.new("body")
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.62
    attr = nt.nodes.new("ShaderNodeAttribute")
    attr.attribute_name = "mask"
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(attr.outputs["Color"], sep.inputs["Color"])

    def rgb(c):
        n = nt.nodes.new("ShaderNodeRGB")
        n.outputs[0].default_value = (*c, 1.0)
        return n.outputs[0]

    def mix(a, b, f):
        n = nt.nodes.new("ShaderNodeMix")
        n.data_type = "RGBA"
        nt.links.new(f, n.inputs["Factor"])
        nt.links.new(a, n.inputs[6])
        nt.links.new(b, n.inputs[7])
        return n.outputs[2]

    c = mix(rgb(SKIN), rgb(BELLY), sep.outputs[0])
    c = mix(c, rgb(MARK), sep.outputs[1])
    c = mix(c, rgb(CLAW), sep.outputs[2])
    # Mottling darkens by up to a third.
    dark = mix(c, rgb((0.0, 0.0, 0.0)), attr.outputs["Alpha"])
    shade = nt.nodes.new("ShaderNodeMix")
    shade.data_type = "RGBA"
    shade.inputs["Factor"].default_value = 0.3
    nt.links.new(c, shade.inputs[6])
    nt.links.new(dark, shade.inputs[7])
    nt.links.new(shade.outputs[2], bsdf.inputs["Base Color"])
    return m


def flat_material(name, colour, rough=0.6, metal=0.0, glow=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*colour, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    if glow:
        bsdf.inputs["Emission Color"].default_value = (*colour, 1.0)
        bsdf.inputs["Emission Strength"].default_value = glow
    return m


# ---- rig and animation ------------------------------------------------------


def build_rig():
    arm = bpy.data.armatures.new("rig")
    rig = bpy.data.objects.new("rig", arm)
    bpy.context.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode="EDIT")
    for name, a, b, parent in BONES:
        bone = arm.edit_bones.new(name)
        bone.head, bone.tail = Vector(J[a]), Vector(J[b])
        if parent:
            bone.parent = arm.edit_bones[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return rig


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
            p[f"thigh.{side}"] = (-26 * s * k, 0, 0)
            p[f"shin.{side}"] = (28 if s * k < 0 else 4, 0, 0)
            p[f"foot.{side}"] = (-10 if s * k < 0 else 6, 0, 0)
            p[f"upper_arm.{side}"] = (20 * s * k, 0, 0)
            p[f"forearm.{side}"] = (-18, 0, 0)
        p["spine"] = (3, 0, 5 * s)
        p["neck"] = (-3, 0, -3 * s)
        key(rig, f, p)
    walk.use_fake_user = True
    idle = bpy.data.actions.new("idle")
    rig.animation_data.action = idle
    for f, t in ((1, 0), (41, 1), (81, 0)):
        p = dict(rest)
        p["chest"] = (-2 * t, 0, 0)
        p["head"] = (4 * t, 0, 12 * t)
        for side in ("L", "R"):
            p[f"forearm.{side}"] = (-8 - 4 * t, 0, 0)
        key(rig, f, p)
    idle.use_fake_user = True
    rig.animation_data.action = idle


def build():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    skin = body_material()
    eye = flat_material("eye", IRIS, 0.15, glow=0.6)
    pupil = flat_material("pupil", (0.01, 0.01, 0.01), 0.1)
    cloth = flat_material("cloth", (0.28, 0.16, 0.08), 0.85)
    metal = flat_material("metal", (0.75, 0.6, 0.3), 0.35, 0.8)

    b = body()
    h, eyes = head()
    cr = crest()
    mn = mane()
    tf = tufts()
    tl = talons()
    cl, bk = belt()
    for o in [b] + h + cr + mn + cl + bk + sum(tf.values(), []) + sum(tl.values(), []):
        o.data.materials.append(skin)
    for o in eyes:
        o.data.materials.append(pupil if "pupil" in o.name else eye)
        mask(o, (0.0, 0.0, 0.0, 0.0))
    for o in cl:
        o.data.materials.clear()
        o.data.materials.append(cloth)
        mask(o, (0.0, 0.0, 0.0, 0.0))
    for o in bk:
        o.data.materials.clear()
        o.data.materials.append(metal)
        mask(o, (0.0, 0.0, 0.0, 0.0))

    rig = build_rig()
    bpy.ops.object.select_all(action="DESELECT")
    b.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    groups = {"head": h + cr + eyes, "chest": [o for o in mn if not o.name.startswith("ridge")],
              "spine": [o for o in mn if o.name.startswith("ridge")], "hips": cl + bk}
    groups.update(tf)
    groups.update(tl)
    for bone, parts in groups.items():
        for p in parts:
            vg = p.vertex_groups.new(name=bone)
            vg.add(list(range(len(p.data.vertices))), 1.0, "REPLACE")
    allparts = [b] + [p for ps in groups.values() for p in ps]
    char = join(allparts, "raptor")
    char.data.color_attributes.active_color = char.data.color_attributes["mask"]
    animate(rig)
    tris = sum(len(p.vertices) - 2 for p in char.data.polygons)
    print(f"raptor: {tris} triangles")
    bpy.ops.export_scene.gltf(filepath=str(OUT / "raptor.glb"), export_format="GLB", export_animations=True)
    return rig


# ---- previews ---------------------------------------------------------------


def stage():
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 64
    world = bpy.data.worlds.new("sky")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.42, 0.45, 0.5, 1.0)
    world.node_tree.nodes["Background"].inputs[1].default_value = 0.8
    scene.world = world
    for name, energy, rot, colour in (
        ("key", 3.2, (50, 8, -35), (1.0, 0.95, 0.88)),
        ("rim", 2.2, (60, 0, 150), (0.75, 0.85, 1.0)),
    ):
        sun = bpy.data.objects.new(name, bpy.data.lights.new(name, "SUN"))
        sun.data.energy = energy
        sun.data.color = colour
        sun.rotation_euler = tuple(math.radians(v) for v in rot)
        scene.collection.objects.link(sun)
    bpy.ops.mesh.primitive_circle_add(vertices=48, radius=1.4, fill_type="NGON")
    ground = bpy.context.active_object
    ground.data.materials.append(flat_material("ground", (0.2, 0.22, 0.2), 0.9))
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    return scene, cam


def shoot(scene, cam, name, angle, dist, height, target, lens, size, frame=1):
    scene.frame_set(frame)
    scene.render.resolution_x, scene.render.resolution_y = size
    cam.data.lens = lens
    a = math.radians(angle)
    cam.location = (math.sin(a) * dist, -math.cos(a) * dist, height)
    cam.rotation_euler = (Vector(target) - cam.location).to_track_quat("-Z", "Y").to_euler()
    scene.render.filepath = str(OUT / f"{name}.png")
    bpy.ops.render.render(write_still=True)


rig = build()
scene, cam = stage()
shoot(scene, cam, "raptor-hero", 30, 3.4, 1.3, (0, 0, 0.98), 50, (640, 960))
shoot(scene, cam, "raptor-side", 95, 3.4, 1.2, (0, 0, 0.98), 50, (640, 960))
shoot(scene, cam, "raptor-back", 205, 3.4, 1.4, (0, 0, 0.98), 50, (640, 960))
shoot(scene, cam, "raptor-face", 35, 1.6, 1.7, (0, -0.12, 1.66), 85, (640, 960))
rig.animation_data.action = bpy.data.actions["walk"]
shoot(scene, cam, "raptor-walk1", 70, 3.4, 1.25, (0, 0, 0.98), 50, (640, 960), 1)
shoot(scene, cam, "raptor-walk2", 70, 3.4, 1.25, (0, 0, 0.98), 50, (640, 960), 17)
