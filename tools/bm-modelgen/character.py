"""Builds Boundmore's trial player character in Blender (run headless with
the `bpy` module): a smooth body grown from a skeleton, a rig, idle and walk
animations, and three colour areas the game tints (body, markings, eyes).

    python tools/bm-modelgen/character.py OUT_DIR

writes OUT_DIR/character.glb and preview renders. Units are blocks (one
Blender metre is one block); the body is 1.9 tall with eyes at 1.75, as
`MoveConfig` says.
"""

import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

OUT = Path(sys.argv[-1] if len(sys.argv) > 1 else "out")
OUT.mkdir(parents=True, exist_ok=True)

# ---- colours (linear RGB); the game replaces these with the player's own ----
BODY = (0.20, 0.42, 0.62, 1.0)
MARK = (0.95, 0.62, 0.20, 1.0)
EYE = (0.05, 0.05, 0.08, 1.0)
IRIS = (0.55, 0.95, 0.85, 1.0)


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def material(name, colour, rough=0.6):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = colour
    bsdf.inputs["Roughness"].default_value = rough
    return m


def skin_mesh(name, joints, edges, radii, subdiv=2):
    """A smooth tube body through `joints`, `radii` (x, y) per joint."""
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([Vector(j) for j in joints], edges, [])
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    skin = obj.modifiers.new("skin", "SKIN")
    skin.use_smooth_shade = True
    for v, r in zip(mesh.skin_vertices[0].data, radii):
        v.radius = r
    mesh.skin_vertices[0].data[0].use_root = True
    sub = obj.modifiers.new("sub", "SUBSURF")
    sub.levels = subdiv
    sub.render_levels = subdiv
    apply_all(obj)
    return obj


def apply_all(obj):
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    for m in list(obj.modifiers):
        bpy.ops.object.modifier_apply(modifier=m.name)
    obj.select_set(False)


def smooth(obj):
    for p in obj.data.polygons:
        p.use_smooth = True


def sphere(name, loc, scale, mat, segments=32):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=segments // 2, location=loc)
    o = bpy.context.active_object
    o.name = name
    o.scale = scale
    bpy.ops.object.transform_apply(scale=True)
    smooth(o)
    o.data.materials.append(mat)
    return o


def cone(name, loc, rot, r, depth, mat):
    bpy.ops.mesh.primitive_cone_add(vertices=16, radius1=r, radius2=r * 0.15, depth=depth, location=loc, rotation=rot)
    o = bpy.context.active_object
    o.name = name
    sub = o.modifiers.new("sub", "SUBSURF")
    sub.levels = 1
    apply_all(o)
    smooth(o)
    o.data.materials.append(mat)
    return o


# The skeleton (x right, y back, z up). Joint names double as bone names.
J = {
    "hips": (0.0, 0.0, 0.92),
    "spine": (0.0, 0.0, 1.10),
    "chest": (0.0, 0.0, 1.30),
    "neck": (0.0, 0.0, 1.42),
    "head": (0.0, 0.0, 1.66),
}
for side, s in (("L", 1), ("R", -1)):
    J[f"shoulder.{side}"] = (0.24 * s, 0.0, 1.33)
    J[f"elbow.{side}"] = (0.34 * s, 0.04, 1.08)
    J[f"wrist.{side}"] = (0.38 * s, -0.02, 0.85)
    J[f"hand.{side}"] = (0.40 * s, -0.05, 0.75)
    J[f"thigh.{side}"] = (0.12 * s, 0.0, 0.86)
    J[f"knee.{side}"] = (0.13 * s, -0.04, 0.48)
    J[f"ankle.{side}"] = (0.13 * s, 0.03, 0.12)
    J[f"toe.{side}"] = (0.14 * s, -0.20, 0.06)


def build_body(skin):
    names = [
        "hips", "spine", "chest", "neck",
        "shoulder.L", "elbow.L", "wrist.L", "hand.L",
        "shoulder.R", "elbow.R", "wrist.R", "hand.R",
        "thigh.L", "knee.L", "ankle.L", "toe.L",
        "thigh.R", "knee.R", "ankle.R", "toe.R",
    ]
    idx = {n: i for i, n in enumerate(names)}
    edges = [
        ("hips", "spine"), ("spine", "chest"), ("chest", "neck"),
        ("chest", "shoulder.L"), ("shoulder.L", "elbow.L"), ("elbow.L", "wrist.L"), ("wrist.L", "hand.L"),
        ("chest", "shoulder.R"), ("shoulder.R", "elbow.R"), ("elbow.R", "wrist.R"), ("wrist.R", "hand.R"),
        ("hips", "thigh.L"), ("thigh.L", "knee.L"), ("knee.L", "ankle.L"), ("ankle.L", "toe.L"),
        ("hips", "thigh.R"), ("thigh.R", "knee.R"), ("knee.R", "ankle.R"), ("ankle.R", "toe.R"),
    ]
    radii = {
        "hips": (0.17, 0.13), "spine": (0.14, 0.11), "chest": (0.21, 0.15), "neck": (0.075, 0.075),
        "shoulder": (0.10, 0.10), "elbow": (0.075, 0.075), "wrist": (0.065, 0.06), "hand": (0.095, 0.065),
        "thigh": (0.115, 0.115), "knee": (0.085, 0.085), "ankle": (0.085, 0.09), "toe": (0.10, 0.07),
    }
    r = [radii[n.split(".")[0]] for n in names]
    body = skin_mesh("body", [J[n] for n in names], [(idx[a], idx[b]) for a, b in edges], r)
    body.data.materials.append(skin)
    return body


def leaf(name, loc, rot, size, mat):
    """A flattened leaf: a fin or a crest plate."""
    # Turned about its own centre, then put in place.
    o = sphere(name, (0, 0, 0), size, mat, 24)
    o.rotation_euler = rot
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    o.data.transform(Matrix.Translation(Vector(loc)))
    return o


def build_head(skin, mark, eye, iris):
    parts = []
    head = sphere("head", (0.0, -0.01, 1.66), (0.25, 0.24, 0.235), skin)
    # A soft brow and cheeks give the face its shape.
    parts.append(sphere("cheeks", (0.0, -0.10, 1.58), (0.20, 0.15, 0.12), skin))
    for side, s in (("L", 1), ("R", -1)):
        # Big, friendly eyes set on the front of the face.
        parts.append(sphere(f"eye.{side}", (0.085 * s, -0.215, 1.69), (0.06, 0.035, 0.07), eye, 24))
        parts.append(sphere(f"iris.{side}", (0.09 * s, -0.245, 1.705), (0.026, 0.01, 0.03), iris, 16))
        # Swept-back ear fins in the marking colour.
        parts.append(leaf(f"fin.{side}", (0.25 * s, 0.07, 1.74), (math.radians(-35), math.radians(-30 * s), math.radians(20 * s)), (0.035, 0.16, 0.07), mark))
    # Three crest plates down the back of the head.
    for i, (y, z, size) in enumerate(((-0.04, 1.89, 0.10), (0.10, 1.85, 0.085), (0.20, 1.75, 0.07))):
        parts.append(leaf(f"crest.{i}", (0.0, y, z), (math.radians(-30 - 25 * i), 0, 0), (0.03, size, size * 0.8), mark))
    return [head] + parts


def markings(body, mark):
    """Paints the marking colour on the forearms, shins and belly by face
    (the game tints the slot)."""
    body.data.materials.append(mark)
    slot = len(body.data.materials) - 1
    for p in body.data.polygons:
        c = p.center
        forearm = abs(c.x) > 0.30 and 0.80 < c.z < 0.98
        shin = 0.14 < c.z < 0.34
        belly = abs(c.x) < 0.10 and c.y < -0.09 and 0.98 < c.z < 1.30
        shoulder = abs(c.x) > 0.24 and c.z > 1.30
        if forearm or shin or belly or shoulder:
            p.material_index = slot


def build_rig():
    arm = bpy.data.armatures.new("rig")
    rig = bpy.data.objects.new("rig", arm)
    bpy.context.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.edit_bones

    def bone(name, head, tail, parent=None):
        b = eb.new(name)
        b.head, b.tail = Vector(head), Vector(tail)
        if parent:
            b.parent = eb[parent]
        return b

    bone("hips", J["hips"], J["spine"])
    bone("spine", J["spine"], J["chest"], "hips")
    bone("chest", J["chest"], J["neck"], "spine")
    bone("neck", J["neck"], (0, 0, 1.50), "chest")
    bone("head", (0, 0, 1.50), (0, 0, 1.95), "neck")
    for side in ("L", "R"):
        bone(f"upper_arm.{side}", J[f"shoulder.{side}"], J[f"elbow.{side}"], "chest")
        bone(f"forearm.{side}", J[f"elbow.{side}"], J[f"wrist.{side}"], f"upper_arm.{side}")
        bone(f"hand.{side}", J[f"wrist.{side}"], J[f"hand.{side}"], f"forearm.{side}")
        bone(f"thigh.{side}", J[f"thigh.{side}"], J[f"knee.{side}"], "hips")
        bone(f"shin.{side}", J[f"knee.{side}"], J[f"ankle.{side}"], f"thigh.{side}")
        bone(f"foot.{side}", J[f"ankle.{side}"], J[f"toe.{side}"], f"shin.{side}")
    bpy.ops.object.mode_set(mode="OBJECT")
    return rig


def bind(rig, body, head_parts):
    # The body bends with automatic weights; head pieces ride the head bone.
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    for p in head_parts:
        vg = p.vertex_groups.new(name="head")
        vg.add(list(range(len(p.data.vertices))), 1.0, "REPLACE")
        p.parent = rig
        m = p.modifiers.new("rig", "ARMATURE")
        m.object = rig
    # One mesh: the game draws a player in one call.
    bpy.ops.object.select_all(action="DESELECT")
    for p in head_parts:
        p.select_set(True)
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.join()
    return body


def key(rig, frame, poses):
    for bone, (x, y, z) in poses.items():
        pb = rig.pose.bones[bone]
        pb.rotation_mode = "XYZ"
        pb.rotation_euler = (math.radians(x), math.radians(y), math.radians(z))
        pb.keyframe_insert("rotation_euler", frame=frame)


def animate(rig):
    rig.animation_data_create()
    bones = [b.name for b in rig.pose.bones]
    rest = {b: (0, 0, 0) for b in bones}

    # Walk: one stride in 32 frames (at 30 fps, about a second).
    walk = bpy.data.actions.new("walk")
    rig.animation_data.action = walk
    for f, ph in ((1, 0), (9, 1), (17, 2), (25, 3), (33, 0)):
        s = [1, 0, -1, 0][ph]
        p = dict(rest)
        p["thigh.L"] = (-30 * s, 0, 0)
        p["thigh.R"] = (30 * s, 0, 0)
        p["shin.L"] = (35 if s < 0 else 8, 0, 0)
        p["shin.R"] = (35 if s > 0 else 8, 0, 0)
        p["upper_arm.L"] = (25 * s, 0, 0)
        p["upper_arm.R"] = (-25 * s, 0, 0)
        p["forearm.L"] = (-20, 0, 0)
        p["forearm.R"] = (-20, 0, 0)
        p["spine"] = (4, 0, 6 * s)
        key(rig, f, p)
    walk.use_fake_user = True

    # Idle: a slow breath and a look about.
    idle = bpy.data.actions.new("idle")
    rig.animation_data.action = idle
    for f, t in ((1, 0), (31, 1), (61, 0)):
        p = dict(rest)
        p["chest"] = (-3 * t, 0, 0)
        p["head"] = (2 * t, 0, 8 * t)
        p["upper_arm.L"] = (0, 0, 6 * t)
        p["upper_arm.R"] = (0, 0, -6 * t)
        key(rig, f, p)
    idle.use_fake_user = True
    rig.animation_data.action = walk
    return walk, idle


def stage():
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 48
    scene.render.resolution_x = 640
    scene.render.resolution_y = 720
    scene.render.film_transparent = False
    world = bpy.data.worlds.new("sky")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.55, 0.70, 0.90, 1.0)
    world.node_tree.nodes["Background"].inputs[1].default_value = 0.7
    scene.world = world
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.5
    sun.rotation_euler = (math.radians(50), math.radians(10), math.radians(-30))
    scene.collection.objects.link(sun)
    bpy.ops.mesh.primitive_plane_add(size=6)
    ground = bpy.context.active_object
    ground.data.materials.append(material("ground", (0.25, 0.45, 0.18, 1.0), 0.9))
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.lens = 60
    scene.collection.objects.link(cam)
    scene.camera = cam
    return scene, cam


def shoot(scene, cam, name, angle, frame=1):
    scene.frame_set(frame)
    d = 4.2
    a = math.radians(angle)
    cam.location = (math.sin(a) * d, -math.cos(a) * d, 1.35)
    target = Vector((0, 0, 0.95))
    cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    scene.render.filepath = str(OUT / f"{name}.png")
    bpy.ops.render.render(write_still=True)


def main():
    clear()
    skin = material("body", BODY, 0.55)
    mark = material("mark", MARK, 0.5)
    eye = material("eye", EYE, 0.15)
    iris = material("iris", IRIS, 0.2)
    body = build_body(skin)
    markings(body, mark)
    head = build_head(skin, mark, eye, iris)
    rig = build_rig()
    body = bind(rig, body, head)
    body.name = "character"
    animate(rig)
    print("triangles:", sum(len(p.vertices) - 2 for p in body.data.polygons))
    bpy.ops.export_scene.gltf(filepath=str(OUT / "character.glb"), export_format="GLB", export_animations=True)
    scene, cam = stage()
    shoot(scene, cam, "front", 25, 1)
    shoot(scene, cam, "side", 100, 9)
    shoot(scene, cam, "back", 200, 1)


main()
