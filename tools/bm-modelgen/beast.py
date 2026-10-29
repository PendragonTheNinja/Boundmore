"""The beast race: a heavy, wolfish brute with a long muzzle, tall ears, a
fur ruff, a dark saddle and a bushy tail (after Nathan's pick of concept 22,
2026-10-29, in Boundmore's own design). Built with the shared kit:

    python tools/bm-modelgen/beast.py OUT_DIR
"""

import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import kit  # noqa: E402  (loads bpy, which provides mathutils)
from mathutils import Vector  # noqa: E402
from kit import along, blob, fuse, join, mask, mottle, smoothstep, spike  # noqa: E402

NAME = "beast"
SKIN = (0.28, 0.28, 0.30)
BELLY = (0.62, 0.59, 0.54)
MARK = (0.07, 0.07, 0.08)
CLAW = (0.05, 0.04, 0.04)
IRIS = (0.35, 0.80, 1.0)
BULK = 1.22
BUILD = 1.14
CLAWS = 1.3
FINGERS = [(0.04, -0.035, 0.15), (0.0, -0.055, 0.16), (-0.04, -0.035, 0.15), (-0.065, -0.05, 0.085)]
# Four short paw toes, no back toe.
TOES = [(0.075, 0.10), (0.025, 0.12), (-0.025, 0.12), (-0.075, 0.10)]
JOINTS = kit.frame(hunch=0.10, build=BUILD, digitigrade=True)

FUR = (0.0, 1.0, 0.0, 0.0)  # dark fur: the marking colour
PALE = (1.0, 0.0, 0.0, 0.0)  # pale fur: the belly colour
GREY = (0.0, 0.0, 0.0, 0.3)  # fur in the skin colour


def joints():
    return JOINTS


def body_colour(co, n):
    x, y, z = co
    front = smoothstep(0.1, 0.6, -n.y)
    belly = front * smoothstep(0.88, 1.0, z) * smoothstep(1.50, 1.40, z) * smoothstep(0.22, 0.12, abs(x))
    inner = smoothstep(0.3, 0.8, -n.x * (1 if x > 0 else -1))
    belly = max(belly, 0.8 * inner * smoothstep(0.5, 0.65, z) * smoothstep(0.92, 0.82, z))
    # A dark saddle over the back and shoulders, and dark lower legs and
    # hands like socks.
    saddle = smoothstep(0.0, 0.5, n.y) * smoothstep(1.0, 1.15, z) * (1 - belly)
    socks = smoothstep(0.30, 0.20, z) + smoothstep(0.80, 0.72, z) * smoothstep(0.28, 0.34, abs(x))
    mark = max(saddle, min(1.0, socks))
    return (belly, mark, 0.0, mottle(co))


def head(J):
    x, y, z = J["skull"]
    parts = [
        blob("skull", (0, y + 0.02, z + 0.13), (0.12, 0.14, 0.11)),
        # The long muzzle, the jaw beneath it, and a heavy brow.
        along("muzzle", (0, y - 0.04, z + 0.12), (0, y - 0.29, z + 0.08), 0.5, 0.06, 0.055, 0.15),
        along("jaw", (0, y - 0.02, z + 0.035), (0, y - 0.25, z + 0.045), 0.5, 0.055, 0.035, 0.13),
        blob("nape", (0, y + 0.08, z + 0.07), (0.10, 0.10, 0.09)),
    ]
    for side, s in (("L", 1), ("R", -1)):
        parts.append(along(f"brow.{side}", (0.05 * s, y - 0.12, z + 0.17), (0.08 * s, y + 0.02, z + 0.19), 0.45, 0.035, 0.025, 0.09))
        parts.append(blob(f"cheek.{side}", (0.085 * s, y - 0.02, z + 0.07), (0.06, 0.08, 0.06)))
    h = fuse(join(parts, "head"), 0.0055, 0.3, 5)

    def colour(co, n):
        # Pale muzzle underside and cheeks; a dark brow and crown.
        belly = smoothstep(0.0, 0.6, -n.z) * smoothstep(z + 0.12, z + 0.05, co.z)
        belly = max(belly, smoothstep(0.08, 0.04, abs(co.x)) * smoothstep(y - 0.15, y - 0.25, co.y) * smoothstep(0.3, -0.2, n.z))
        crown = smoothstep(0.3, 0.8, n.z) * smoothstep(y - 0.06, y + 0.04, co.y)
        return (belly, crown, 0.0, mottle(co))

    mask(h, colour)
    out = [h]
    nose = blob("nose", (0, y - 0.305, z + 0.098), (0.03, 0.025, 0.024), segments=16)
    mask(nose, (0.0, 0.0, 1.0, 0.0))
    out.append(nose)
    for side, s in (("L", 1), ("R", -1)):
        # Tall pointed ears, pale inside.
        ear = spike(f"ear.{side}", (0.075 * s, y + 0.03, z + 0.20), (math.radians(-18), math.radians(22 * s), 0), 0.055, 0.21, -0.15, 0.5, 16, 1.4)
        mask(ear, (0.0, 0.5, 0.0, 0.2))
        inner = spike(f"earin.{side}", (0.075 * s, y + 0.005, z + 0.215), (math.radians(-18), math.radians(22 * s), 0), 0.035, 0.15, -0.15, 0.3, 12, 1.2)
        mask(inner, PALE)
        out += [ear, inner]
        # Fangs at the corners of the mouth.
        fang = spike(f"fang.{side}", (0.03 * s, y - 0.24, z + 0.055), (math.radians(180), 0, 0), 0.008, 0.028, 0.0, 1.0, 8)
        mask(fang, PALE)
        out.append(fang)
    eyes = []
    for side, s in (("L", 1), ("R", -1)):
        a = math.radians(24 * s)
        d = Vector((math.sin(a), -math.cos(a), 0.0))
        c = Vector((0.058 * s, y - 0.105, z + 0.15))
        eyes.append(blob(f"eye.{side}", c, (0.022, 0.015, 0.016), (0, 0, a), 16))
        eyes.append(blob(f"pupil.{side}", c + d * 0.014, (0.007, 0.004, 0.009), (0, 0, a), 8))
    return out, eyes


def fur(name, base, rot, r, length, colour, bend=-0.55):
    """A soft clump of fur: broad at the root, drooping at the tip."""
    o = spike(name, base, rot, r, length, bend, 0.6, 12, 1.0, 2)
    mask(o, colour)
    return o


def ruff(J):
    """A thick ruff round the neck and over the shoulders, cheek tufts, and a
    dark crest of fur down the nape."""
    nx, ny, nz = J["neck"]
    out = []
    rings = (
        (0.10, 0.09, nz + 0.02, 19, 0.14, 55, 140, PALE),
        (0.16, 0.12, 1.41, 21, 0.17, 70, 125, FUR),
        (0.21, 0.12, 1.33, 13, 0.13, 80, 70, FUR),
    )
    for ring, (rx, ry, rz, count, length, tilt, reach, colour) in enumerate(rings):
        for i in range(count):
            a = math.radians(-reach + i * (2 * reach / (count - 1)))
            px, py = math.sin(a) * rx, ny + 0.03 + math.cos(a) * ry
            out.append(fur(f"ruff.{ring}.{i}", (px, py, rz), (math.radians(-tilt * math.cos(a) - 30), math.radians(tilt * math.sin(a)), 0), 0.065, length * (0.85 + 0.3 * abs(math.cos(a))), colour))
    # The clumps fused into one sculpted mass of fur, lobed at the edge.
    mass = fuse(join(out, "ruff"), 0.007, 0.3, 3)

    def colour(co, n):
        # Pale at the throat, dark over the shoulders.
        pale = smoothstep(ny + 0.02, ny - 0.08, co.y) * smoothstep(1.30, 1.38, co.z)
        return (pale, 1.0 - pale, 0.0, 0.25 * mottle(co))

    mask(mass, colour)
    return [mass]


def cheeks(J):
    x, y, z = J["skull"]
    out = []
    for side, s in (("L", 1), ("R", -1)):
        for i in range(3):
            out.append(fur(f"cheekfur.{side}{i}", (0.10 * s, y + 0.01 + 0.03 * i, z + 0.06 - 0.02 * i), (math.radians(-100 + 15 * i), math.radians(60 * s), 0), 0.03, 0.10 - 0.015 * i, PALE))
    # A dark crest of fur from the crown down the nape.
    for i in range(5):
        out.append(fur(f"crestfur.{i}", (0, y + 0.02 + 0.04 * i, z + 0.22 - 0.035 * i), (math.radians(-75 - 12 * i), 0, 0), 0.035, 0.12 - 0.01 * i, FUR))
    return out


def tufts(J):
    out = {}
    for side, s in (("L", 1), ("R", -1)):
        el, wr = Vector(J[f"elbow.{side}"]), Vector(J[f"wrist.{side}"])
        out[f"forearm.{side}"] = [
            fur(f"armfur.{side}{i}", el.lerp(wr, 0.1 + 0.12 * i) + Vector((0.04 * s, 0.05, 0)), (math.radians(-145 + 12 * i), math.radians(25 * s), 0), 0.035, 0.12 - 0.02 * i, GREY)
            for i in range(3)
        ]
        kn, an = Vector(J[f"knee.{side}"]), Vector(J[f"ankle.{side}"])
        out[f"shin.{side}"] = [
            fur(f"legfur.{side}{i}", kn.lerp(an, 0.25 + 0.2 * i) + Vector((0.02 * s, 0.05, 0)), (math.radians(-150), math.radians(20 * s), 0), 0.03, 0.10, GREY)
            for i in range(2)
        ]
    return out


def tail(J):
    """A bushy tail sweeping down and back from the belt."""
    x, y, z = J["hips"]
    pts = [Vector((0, y + 0.11, z - 0.01)), Vector((0, y + 0.27, z - 0.08)), Vector((0, y + 0.42, z - 0.22)), Vector((0, y + 0.52, z - 0.38)), Vector((0, y + 0.55, z - 0.50))]
    parts = []
    for i, (a, b) in enumerate(zip(pts, pts[1:])):
        w = (0.05, 0.085, 0.095, 0.07)[i]
        parts.append(along(f"tail.{i}", a, b, 0.5, w, w * 0.9, (b - a).length * 0.72))
    t = fuse(join(parts, "tail"), 0.008, 0.35, 4)

    def colour(co, n):
        # Dark along the top, a pale tip.
        tip_ = smoothstep(z - 0.36, z - 0.48, co.z)
        top = smoothstep(0.0, 0.6, n.y) * (1 - tip_)
        return (tip_, top, 0.0, mottle(co))

    mask(t, colour)
    out = [t]
    # Fur along the tail's top and sides.
    for i in range(1, 4):
        a, b = pts[i], pts[i + 1]
        for k, side in enumerate((-1, 0, 1)):
            base = a.lerp(b, 0.3) + Vector((0.04 * side, 0.03, 0))
            out.append(fur(f"tailfur.{i}{k}", base, (math.radians(-160 - 10 * i), math.radians(30 * side), 0), 0.04, 0.10, (0.0, 0.6, 0.0, 0.2)))
    return out


def extras(J):
    out = {"head": cheeks(J), "chest": ruff(J), "hips": tail(J)}
    out.update(tufts(J))
    return out


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.endswith(".py")]
    kit.run(sys.modules[__name__], args[0] if args else "out")
