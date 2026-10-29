"""The shared kit every race is built with (Blender, headless `bpy`): the
frame and its one skeleton (the same bone names, so one set of animations
plays on every race), a sculpted body grown from it, talons, the belt, the
rig and animations, and the bake from the sculpt to the game model (about
10,000 triangles, a normal map and a painted texture).

A race module gives its name, colours, proportions, head and extras, then
calls `kit.run(race, out_dir)`; see raptor.py.

Colour: one tinted material for the whole creature. Each vertex carries a
mask (the `mask` colour attribute): red is how much of the belly colour
shows, green the marking colour, blue the claw and scale colour, alpha the
mottling. The bake turns that into the painted texture (`<race>.paint.png`:
red, green, blue those weights, alpha the painted shading) beside the normal
map (`<race>.normal.png`). The game tints the four colours per player; the
eyes have their own colour.
"""

import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector, noise

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

# Set by `run` for the race being built.
J = {}
BONES = []
FINGERS = []
TOES = []
R = None
OUT = None


def frame(hunch=0.07, build=1.0, digitigrade=True):
    """Joint positions (x right, y back, z up): lean and slightly hunched,
    broad shouldered, arms to the knees, a small head."""
    w = build
    j = {
        "hips": (0, 0.0, 0.94),
        "waist": (0, 0.015, 1.10),
        "chest": (0, -0.02 - hunch * 0.3, 1.30),
        "neck": (0, -0.05 - hunch, 1.45),
        "skull": (0, -0.08 - hunch, 1.55),
        "crown": (0, -0.08 - hunch, 1.88),
    }
    for side, s in (("L", 1), ("R", -1)):
        j[f"shoulder.{side}"] = (0.25 * w * s, -0.04 - hunch * 0.5, 1.40)
        j[f"elbow.{side}"] = (0.33 * w * s, 0.05, 1.09)
        j[f"wrist.{side}"] = (0.37 * w * s, -0.04, 0.80)
        j[f"palm.{side}"] = (0.39 * w * s, -0.07, 0.69)
        j[f"hip.{side}"] = (0.12 * s, 0.0, 0.90)
        if digitigrade:
            j[f"knee.{side}"] = (0.14 * s, -0.13, 0.58)
            j[f"ankle.{side}"] = (0.145 * s, 0.12, 0.24)
            j[f"ball.{side}"] = (0.15 * s, -0.03, 0.055)
        else:
            j[f"knee.{side}"] = (0.135 * s, -0.06, 0.52)
            j[f"ankle.{side}"] = (0.135 * s, 0.03, 0.11)
            j[f"ball.{side}"] = (0.14 * s, -0.10, 0.045)
    return j


def bones():
    out = [
        ("hips", "hips", "waist", None),
        ("spine", "waist", "chest", "hips"),
        ("chest", "chest", "neck", "spine"),
        ("neck", "neck", "skull", "chest"),
        ("head", "skull", "crown", "neck"),
    ]
    for side in ("L", "R"):
        out += [
            (f"upper_arm.{side}", f"shoulder.{side}", f"elbow.{side}", "chest"),
            (f"forearm.{side}", f"elbow.{side}", f"wrist.{side}", f"upper_arm.{side}"),
            (f"hand.{side}", f"wrist.{side}", f"palm.{side}", f"forearm.{side}"),
            (f"thigh.{side}", f"hip.{side}", f"knee.{side}", "hips"),
            (f"shin.{side}", f"knee.{side}", f"ankle.{side}", f"thigh.{side}"),
            (f"foot.{side}", f"ankle.{side}", f"ball.{side}", f"shin.{side}"),
        ]
    return out


def tip(side, base, d):
    s = 1 if side == "L" else -1
    return Vector((base[0] + d[0] * s, base[1] + d[1], base[2] + d[2]))


def body():
    pts, rad, edges = [], [], []
    k, w = R.BULK, R.BUILD

    def add(p, r):
        pts.append(tuple(p))
        rad.append((r[0] * k, r[1] * k))
        return len(pts) - 1

    def chain(a, *rest):
        for b in rest:
            edges.append((a, b))
            a = b

    hips = add(J["hips"], (0.15, 0.115))
    waist = add(J["waist"], (0.105, 0.09))
    chest = add(J["chest"], (0.19 * w, 0.14))
    upper = add((0, J["chest"][1] - 0.01, 1.39), (0.16 * w, 0.115))
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
        blob("pecs", (0, cy - 0.06, 1.33), (0.18 * w * k, 0.085 * k, 0.085 * k)),
        blob("lats", (0, cy + 0.05, 1.26), (0.165 * w * k, 0.09 * k, 0.13 * k)),
        blob("traps", (0, cy + 0.03, 1.42), (0.12 * w * k, 0.08 * k, 0.055 * k)),
        blob("glutes", (0, 0.06, 0.90), (0.135 * k, 0.085 * k, 0.085 * k)),
    ]
    for side, s in (("L", 1), ("R", -1)):
        sh, el, wr = J[f"shoulder.{side}"], J[f"elbow.{side}"], J[f"wrist.{side}"]
        hp, kn, an = J[f"hip.{side}"], J[f"knee.{side}"], J[f"ankle.{side}"]
        parts += [
            blob(f"delt.{side}", (sh[0] * 1.05, sh[1], sh[2] - 0.01), (0.095 * k, 0.095 * k, 0.085 * k)),
            along(f"bicep.{side}", sh, el, 0.5, 0.068 * k, 0.072 * k, 0.15, (0, -0.01, 0)),
            along(f"tricep.{side}", sh, el, 0.45, 0.06 * k, 0.06 * k, 0.14, (0, 0.02, 0)),
            along(f"forearm.{side}", el, wr, 0.3, 0.066 * k, 0.06 * k, 0.13),
            along(f"thighm.{side}", hp, kn, 0.4, 0.1 * k, 0.105 * k, 0.19, (0, -0.01, 0)),
            along(f"hams.{side}", hp, kn, 0.45, 0.085 * k, 0.08 * k, 0.17, (0, 0.03, 0)),
            along(f"shank.{side}", kn, an, 0.25, 0.06 * k, 0.07 * k, 0.12, (0, 0.015, 0)),
        ]
    body = fuse(join(parts, "body"), 0.0095, 0.22)

    colour = R.body_colour
    mask(body, colour)
    return body


def talons():
    out = {}
    for side, s in (("L", 1), ("R", -1)):
        p = J[f"palm.{side}"]
        hand = []
        for i, (dx, dy, l) in enumerate(FINGERS):
            t = tip(side, p, (dx, dy - 0.005, -l + 0.005))
            hand.append(spike(f"talon.{side}{i}", t, (math.radians(200), 0, 0), 0.019 * R.CLAWS, 0.07 * R.CLAWS, 0.6, 0.8))
        b = J[f"ball.{side}"]
        foot = []
        for i, (dx, l) in enumerate(TOES):
            t = tip(side, b, (dx, -l, -0.025))
            back = l < 0
            rot = (math.radians(-80 if back else 95), 0, 0)
            foot.append(spike(f"toetalon.{side}{i}", t, rot, 0.024 * R.CLAWS, (0.09 if not back else 0.07) * R.CLAWS, -0.5 if back else 0.5, 0.8))
        for o in hand + foot:
            mask(o, (0.0, 0.0, 1.0, 0.0))
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

    c = mix(rgb(R.SKIN), rgb(R.BELLY), sep.outputs[0])
    c = mix(c, rgb(R.MARK), sep.outputs[1])
    c = mix(c, rgb(R.CLAW), sep.outputs[2])
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
    eye = flat_material("eye", R.IRIS, 0.15, glow=0.6)
    pupil = flat_material("pupil", (0.01, 0.01, 0.01), 0.1)
    cloth = flat_material("cloth", (0.28, 0.16, 0.08), 0.85)
    metal = flat_material("metal", (0.75, 0.6, 0.3), 0.35, 0.8)

    b = body()
    h, eyes = R.head(J)
    extras = R.extras(J)  # bone -> tinted parts riding it
    tl = talons()
    cl, bk = belt()
    for o in [b] + h + cl + bk + sum(extras.values(), []) + sum(tl.values(), []):
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
    groups = {"head": h + eyes, "hips": cl + bk}
    for bone, parts in list(extras.items()) + list(tl.items()):
        groups.setdefault(bone, []).extend(parts)
    for bone, parts in groups.items():
        for p in parts:
            vg = p.vertex_groups.new(name=bone)
            vg.add(list(range(len(p.data.vertices))), 1.0, "REPLACE")
    allparts = [b] + [p for ps in groups.values() for p in ps]
    char = join(allparts, R.NAME)
    char.data.color_attributes.active_color = char.data.color_attributes["mask"]
    animate(rig)
    low = bake(char, rig)
    bpy.ops.object.select_all(action="DESELECT")
    low.select_set(True)
    rig.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=str(OUT / f"{R.NAME}.glb"), export_format="GLB", export_animations=True, use_selection=True
    )
    return rig


# ---- the game model: fewer triangles, the sculpt baked into textures ---------

TEX = 1024
GAME_TRIANGLES = 10000


def triangles(o):
    return sum(len(p.vertices) - 2 for p in o.data.polygons)


def emit_material(name, build):
    """A material that only glows `build(nodes, links)`'s colour: what a bake
    pass reads off the sculpt."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    emit = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(emit.outputs[0], out.inputs[0])
    nt.links.new(build(nt), emit.inputs["Color"])
    return m


def mask_pass(nt):
    attr = nt.nodes.new("ShaderNodeAttribute")
    attr.attribute_name = "mask"
    return attr.outputs["Color"]


def detail_pass(nt):
    """The painted detail as brightness (1 untouched): fine grain, mottling,
    scales where the claw colour shows, streaks where the marking shows."""
    attr = nt.nodes.new("ShaderNodeAttribute")
    attr.attribute_name = "mask"
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(attr.outputs["Color"], sep.inputs["Color"])
    coord = nt.nodes.new("ShaderNodeTexCoord").outputs["Object"]

    def noise(scale, stretch=(1, 1, 1), detail=4.0):
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = stretch
        nt.links.new(coord, mp.inputs["Vector"])
        n = nt.nodes.new("ShaderNodeTexNoise")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        nt.links.new(mp.outputs["Vector"], n.inputs["Vector"])
        return n.outputs["Fac"]

    def remap(x, a, b, lo, hi):
        n = nt.nodes.new("ShaderNodeMapRange")
        n.inputs["From Min"].default_value = a
        n.inputs["From Max"].default_value = b
        n.inputs["To Min"].default_value = lo
        n.inputs["To Max"].default_value = hi
        nt.links.new(x, n.inputs["Value"])
        return n.outputs["Result"]

    def mul(a, b):
        n = nt.nodes.new("ShaderNodeMath")
        n.operation = "MULTIPLY"
        nt.links.new(a, n.inputs[0])
        if isinstance(b, float):
            n.inputs[1].default_value = b
        else:
            nt.links.new(b, n.inputs[1])
        return n.outputs[0]

    def lerp1(f, x):
        """1 where f is 0, x where f is 1."""
        n = nt.nodes.new("ShaderNodeMix")
        n.data_type = "FLOAT"
        nt.links.new(f, n.inputs["Factor"])
        n.inputs[2].default_value = 1.0
        nt.links.new(x, n.inputs[3])
        return n.outputs[0]

    grain = remap(noise(140.0), 0.3, 0.7, 0.8, 1.0)
    blotch = remap(attr.outputs["Alpha"], 0.2, 0.9, 1.0, 0.5)
    # Painted light from above: tops brighter, undersides darker.
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    sepn = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(geo.outputs["Normal"], sepn.inputs[0])
    top = remap(sepn.outputs["Z"], -1.0, 1.0, 0.62, 1.0)
    vor = nt.nodes.new("ShaderNodeTexVoronoi")
    vor.feature = "DISTANCE_TO_EDGE"
    vor.inputs["Scale"].default_value = 85.0
    nt.links.new(coord, vor.inputs["Vector"])
    scales = remap(vor.outputs["Distance"], 0.0, 0.12, 0.35, 1.0)
    streaks = remap(noise(70.0, (1.0, 1.0, 0.12), 2.0), 0.35, 0.65, 0.55, 1.0)
    # Darker spots scattered over the skin (not the belly or scales).
    spots_v = nt.nodes.new("ShaderNodeTexVoronoi")
    spots_v.inputs["Scale"].default_value = 22.0
    spots_v.inputs["Randomness"].default_value = 1.0
    nt.links.new(coord, spots_v.inputs["Vector"])
    spots = remap(spots_v.outputs["Distance"], 0.12, 0.30, 0.62, 1.0)
    skin_only = nt.nodes.new("ShaderNodeMath")
    skin_only.operation = "SUBTRACT"
    skin_only.inputs[0].default_value = 1.0
    nt.links.new(sep.outputs[0], skin_only.inputs[1])
    d = mul(grain, blotch)
    d = mul(d, top)
    d = mul(d, lerp1(skin_only.outputs[0], spots))
    d = mul(d, lerp1(sep.outputs[2], scales))
    d = mul(d, lerp1(sep.outputs[1], streaks))
    c = nt.nodes.new("ShaderNodeCombineColor")
    for i in range(3):
        nt.links.new(d, c.inputs[i])
    return c.outputs[0]


def pixels(img):
    import numpy as np

    a = np.empty(img.size[0] * img.size[1] * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(-1, 4)


def bake(high, rig):
    import numpy as np

    rig.data.pose_position = "REST"
    bpy.context.view_layer.update()
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 16

    low = high.copy()
    low.data = high.data.copy()
    bpy.context.collection.objects.link(low)
    high.name, low.name = f"{R.NAME}.sculpt", R.NAME
    for i, m in enumerate(low.data.materials):
        low.data.materials[i] = m.copy()
    dec = low.modifiers.new("decimate", "DECIMATE")
    dec.ratio = GAME_TRIANGLES / triangles(high)
    bpy.context.view_layer.objects.active = low
    bpy.ops.object.modifier_move_to_index(modifier="decimate", index=0)
    bpy.ops.object.modifier_apply(modifier="decimate")
    print(f"{R.NAME}: {triangles(high)} triangles sculpted, {triangles(low)} in the game model")

    bpy.ops.object.select_all(action="DESELECT")
    low.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.004)
    bpy.ops.object.mode_set(mode="OBJECT")

    images = {}
    for name, colour in (("mask", False), ("detail", False), ("ao", False), ("normal", True)):
        img = bpy.data.images.new(f"{R.NAME}.{name}", TEX, TEX)
        if colour:
            img.colorspace_settings.name = "Non-Color"
        images[name] = img
    # Every material on the game model bakes into the same image.
    targets = []
    for m in low.data.materials:
        node = m.node_tree.nodes.new("ShaderNodeTexImage")
        m.node_tree.nodes.active = node
        targets.append(node)
    originals = list(high.data.materials)
    passes = {"mask": emit_material("pass.mask", mask_pass), "detail": emit_material("pass.detail", detail_pass)}

    def run(name, kind, source=None):
        for t in targets:
            t.image = images[name]
        if source:
            for i in range(len(high.data.materials)):
                high.data.materials[i] = source
        bpy.ops.object.select_all(action="DESELECT")
        high.select_set(True)
        low.select_set(True)
        bpy.context.view_layer.objects.active = low
        bpy.ops.object.bake(
            type=kind, use_selected_to_active=True, cage_extrusion=0.006, max_ray_distance=0.015, margin=6
        )
        for i, m in enumerate(originals):
            high.data.materials[i] = m

    run("mask", "EMIT", passes["mask"])
    run("detail", "EMIT", passes["detail"])
    run("ao", "AO")
    run("normal", "NORMAL")

    # One painted texture: red, green, blue the belly, marking and claw
    # weights; alpha the painted shading (detail times ambient occlusion).
    mask, detail, ao = pixels(images["mask"]), pixels(images["detail"]), pixels(images["ao"])
    paint = bpy.data.images.new(f"{R.NAME}.paint", TEX, TEX, alpha=True)
    out = np.empty_like(mask)
    out[:, :3] = mask[:, :3]
    out[:, 3] = detail[:, 0] * (0.15 + 0.85 * ao[:, 0])
    paint.pixels.foreach_set(out.ravel())
    for img, name in ((paint, "paint"), (images["normal"], "normal")):
        img.filepath_raw = str(OUT / f"{R.NAME}.{name}.png")
        img.file_format = "PNG"
        img.save()

    bpy.data.objects.remove(high)
    for t in targets:
        t.id_data.nodes.remove(t)
    game_material(low, paint, images["normal"])
    rig.data.pose_position = "POSE"
    return low


def game_material(low, paint, normal):
    """How the game draws it: the four tints mixed by the texture's weights,
    shaded by its alpha, with the sculpt's normal map."""
    m = bpy.data.materials.new(f"{R.NAME}.game")
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.6
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = paint
    paint.alpha_mode = "CHANNEL_PACKED"
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(tex.outputs["Color"], sep.inputs["Color"])

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

    c = mix(rgb(R.SKIN), rgb(R.BELLY), sep.outputs[0])
    c = mix(c, rgb(R.MARK), sep.outputs[1])
    c = mix(c, rgb(R.CLAW), sep.outputs[2])
    shade = nt.nodes.new("ShaderNodeMix")
    shade.data_type = "RGBA"
    shade.blend_type = "MULTIPLY"
    shade.inputs["Factor"].default_value = 1.0
    nt.links.new(c, shade.inputs[6])
    nt.links.new(tex.outputs["Alpha"], shade.inputs[7])
    nt.links.new(shade.outputs[2], bsdf.inputs["Base Color"])
    ntex = nt.nodes.new("ShaderNodeTexImage")
    ntex.image = normal
    nmap = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(ntex.outputs["Color"], nmap.inputs["Color"])
    nt.links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    # The tinted surfaces take the game material; eyes, cloth and metal keep
    # theirs (the painted shading is on them too, through the bake).
    for i, old in enumerate(low.data.materials):
        if old.name.startswith("body"):
            low.data.materials[i] = m


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


def run(race, out):
    """Builds `race` (a module) into `out`: the game model, its textures and
    preview renders."""
    global J, BONES, FINGERS, TOES, R, OUT
    R, OUT = race, Path(out)
    OUT.mkdir(parents=True, exist_ok=True)
    J = race.joints()
    BONES = bones()
    FINGERS, TOES = race.FINGERS, race.TOES
    rig = build()
    scene, cam = stage()
    n = race.NAME
    shoot(scene, cam, f"{n}-hero", 30, 3.4, 1.3, (0, 0, 0.98), 50, (640, 960))
    shoot(scene, cam, f"{n}-side", 95, 3.4, 1.2, (0, 0, 0.98), 50, (640, 960))
    shoot(scene, cam, f"{n}-back", 205, 3.4, 1.4, (0, 0, 0.98), 50, (640, 960))
    shoot(scene, cam, f"{n}-face", 35, 1.6, 1.7, (0, -0.12, 1.66), 85, (640, 960))
    rig.animation_data.action = bpy.data.actions["walk"]
    shoot(scene, cam, f"{n}-walk1", 70, 3.4, 1.25, (0, 0, 0.98), 50, (640, 960), 1)
    shoot(scene, cam, f"{n}-walk2", 70, 3.4, 1.25, (0, 0, 0.98), 50, (640, 960), 17)
