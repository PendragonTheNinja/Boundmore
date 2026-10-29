"""The finned race: a sleek water-dweller with a fin crest, gill frills,
webbed hands and feet (after Nathan's pick of concept 14, 2026-10-29, in
Boundmore's own design). Built with the shared kit:

    python tools/bm-modelgen/finned.py OUT_DIR
"""

import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import kit  # noqa: E402  (loads bpy, which provides mathutils)
from mathutils import Vector  # noqa: E402
from kit import along, blob, fuse, join, mask, mottle, smoothstep, spike  # noqa: E402

NAME = "finned"
SKIN = (0.07, 0.24, 0.28)
BELLY = (0.50, 0.68, 0.62)
MARK = (0.34, 0.10, 0.48)
CLAW = (0.10, 0.08, 0.13)
IRIS = (0.95, 0.55, 0.06)
BULK = 0.96
BUILD = 0.98
CLAWS = 0.55
FINGERS = [(0.045, -0.035, 0.18), (0.0, -0.055, 0.19), (-0.045, -0.035, 0.17), (-0.07, -0.05, 0.10)]
# Three long webbed toes, no back toe.
TOES = [(0.07, 0.15), (0.0, 0.17), (-0.07, 0.15)]
JOINTS = kit.frame(hunch=0.05, build=BUILD, digitigrade=False)

FIN = (0.0, 1.0, 0.0, 0.0)  # membrane: the marking colour
RAY = (0.0, 0.0, 1.0, 0.0)  # fin rays and nails: the claw colour


def joints():
    return JOINTS


def body_colour(co, n):
    x, y, z = co
    front = smoothstep(0.1, 0.6, -n.y)
    belly = front * smoothstep(0.84, 0.95, z) * smoothstep(1.52, 1.42, z) * smoothstep(0.18, 0.09, abs(x))
    inner = smoothstep(0.3, 0.8, -n.x * (1 if x > 0 else -1))
    belly = max(belly, 0.8 * inner * smoothstep(0.5, 0.65, z) * smoothstep(0.92, 0.82, z))
    belly = max(belly, smoothstep(0.3, 0.8, -n.z) * smoothstep(0.12, 0.06, z))  # soles
    # Tiger bands across the back, shoulders and limbs, broken by noise.
    band = 0.5 + 0.5 * math.sin(z * 58.0 + 6.0 * mottle(co * 0.5))
    backish = smoothstep(-0.1, 0.5, n.y) * (1 - belly)
    mark = smoothstep(0.72, 0.9, band) * backish * smoothstep(0.30, 0.40, z)
    # A dark stripe down the spine.
    mark = max(mark, smoothstep(0.5, 0.8, n.y) * smoothstep(0.06, 0.03, abs(x)) * smoothstep(0.95, 1.05, z))
    return (belly, mark, 0.0, mottle(co))


def fin(name, base, tip_, height, back, rays=4, thickness=0.007):
    """A membrane fin from `base` to `tip_`, rising `height` towards `back`
    (a unit direction), with rays along it."""
    base, tip_, back = Vector(base), Vector(tip_), Vector(back).normalized()
    along_ = tip_ - base
    length = along_.length
    # The membrane: a thin flat ellipsoid, half its height buried.
    o = blob(name, (0, 0, 0), (thickness, length * 0.5, height), segments=24)
    fwd = along_.normalized()
    side = fwd.cross(back).normalized()
    up = side.cross(fwd).normalized()
    from mathutils import Matrix

    m = Matrix((side, fwd, up)).transposed().to_4x4()
    o.data.transform(Matrix.Translation(base.lerp(tip_, 0.5)) @ m)
    mask(o, FIN)
    out = [o]
    for i in range(rays):
        t = (i + 0.5) / rays
        p = base.lerp(tip_, t)
        h = height * math.sqrt(max(0.0, 1 - (2 * t - 1) ** 2)) * 1.05
        if h < 0.02:
            continue
        r = spike(f"{name}.ray{i}", (0, 0, 0), (0, 0, 0), thickness * 1.5, h, 0.0, 1.0, 6)
        r.data.transform(Matrix.Translation(p) @ m @ Matrix.Rotation(math.radians(-12), 4, "X"))
        mask(r, RAY)
        out.append(r)
    return out


def head(J):
    x, y, z = J["skull"]
    parts = [
        # A broad, flat-topped head with a wide mouth.
        blob("skull", (0, y + 0.01, z + 0.12), (0.12, 0.15, 0.09)),
        # A long, flattened face: more salamander than frog.
        along("face", (0, y - 0.04, z + 0.11), (0, y - 0.22, z + 0.07), 0.5, 0.085, 0.05, 0.13),
        along("jaw", (0, y + 0.02, z + 0.03), (0, y - 0.19, z + 0.035), 0.5, 0.085, 0.035, 0.12),
        blob("nape", (0, y + 0.07, z + 0.06), (0.09, 0.09, 0.08)),
    ]
    for side, s in (("L", 1), ("R", -1)):
        # Eye mounds high on the sides of the head.
        parts.append(blob(f"mound.{side}", (0.07 * s, y - 0.09, z + 0.15), (0.045, 0.05, 0.04)))
        parts.append(blob(f"cheek.{side}", (0.10 * s, y - 0.02, z + 0.07), (0.05, 0.07, 0.05)))
    h = fuse(join(parts, "head"), 0.0055, 0.3, 5)

    def colour(co, n):
        # A pale throat and lips, a violet blaze between the eyes.
        belly = smoothstep(0.1, 0.6, -n.z) * smoothstep(z + 0.09, z + 0.03, co.z)
        blaze = smoothstep(0.3, 0.7, n.z) * smoothstep(0.03, 0.01, abs(co.x)) * smoothstep(y - 0.02, y - 0.12, co.y)
        return (belly, blaze, 0.0, mottle(co))

    mask(h, colour)
    out = [h]
    # The mouth: a dark seam across the face.
    eyes = []
    for side, s in (("L", 1), ("R", -1)):
        a = math.radians(34 * s)
        d = Vector((math.sin(a), -math.cos(a), 0.1)).normalized()
        # Half out of the eye mound, looking forward and out.
        c = Vector((0.07 * s, y - 0.09, z + 0.155)) + d * 0.034
        eyes.append(blob(f"eye.{side}", c, (0.027, 0.02, 0.024), (0, 0, a), 16))
        # A round pupil: a creature of dim water.
        eyes.append(blob(f"pupil.{side}", c + d * 0.017, (0.011, 0.005, 0.011), (0, 0, a), 8))
    return out, eyes


def crest(J):
    """A tall fin from brow to nape, and gill frills fanned behind the jaw."""
    x, y, z = J["skull"]
    out = fin("crest", (0, y - 0.10, z + 0.19), (0, y + 0.22, z + 0.10), 0.17, (0, 0.9, 1), 7, 0.008)
    for side, s in (("L", 1), ("R", -1)):
        for i in range(3):
            base = Vector((0.10 * s, y + 0.0 + 0.035 * i, z + 0.13 - 0.045 * i))
            tipp = base + Vector((0.06 * s, 0.16, 0.05 - 0.04 * i))
            out += fin(f"frill.{side}{i}", base, tipp, 0.07 - 0.012 * i, (0.5 * s, 0, 1), 4, 0.007)
    return out


def back_fin(J):
    """A low fin down the spine."""
    cy = J["chest"][1]
    return fin("dorsal", (0, cy + 0.14, 1.40), (0, 0.10, 1.02), 0.06, (0, 1, 0.1), 7)


def limb_fins(J):
    out = {}
    for side, s in (("L", 1), ("R", -1)):
        el, wr = Vector(J[f"elbow.{side}"]), Vector(J[f"wrist.{side}"])
        out[f"forearm.{side}"] = fin(f"armfin.{side}", el + Vector((0.01 * s, 0.04, -0.02)), wr + Vector((0.01 * s, 0.04, 0.05)), 0.045, (0.3 * s, 1, 0), 4)
        kn, an = Vector(J[f"knee.{side}"]), Vector(J[f"ankle.{side}"])
        out[f"shin.{side}"] = fin(f"calffin.{side}", kn + Vector((0, 0.06, -0.06)), an + Vector((0, 0.06, 0.08)), 0.05, (0.2 * s, 1, 0), 3)
    return out


def webbing(J):
    """Membranes between the fingers and between the toes."""
    from mathutils import Matrix

    out = {}
    for side, s in (("L", 1), ("R", -1)):
        p = Vector(J[f"palm.{side}"])
        hand = []
        for (dxa, dya, la), (dxb, dyb, lb) in zip(FINGERS[:2], FINGERS[1:3]):
            a = p + Vector((dxa * s * 0.8, dya * 0.8, -la * 0.65))
            b = p + Vector((dxb * s * 0.8, dyb * 0.8, -lb * 0.65))
            m = blob(f"web.{side}", (0, 0, 0), (0.025, 0.006, 0.06), segments=12)
            d = (b - a)
            ang = math.atan2(d.y, d.x)
            m.data.transform(Matrix.Translation((a + b) / 2 + Vector((0, 0, 0.03))) @ Matrix.Rotation(ang, 4, "Z"))
            mask(m, FIN)
            hand.append(m)
        out[f"hand.{side}"] = hand
        b0 = Vector(J[f"ball.{side}"])
        foot = []
        for (dxa, la), (dxb, lb) in zip(TOES[:-1], TOES[1:]):
            c = b0 + Vector(((dxa + dxb) * 0.5 * s * 0.75, -(la + lb) * 0.5 * 0.7, -0.02))
            m = blob(f"toeweb.{side}", c, (0.032, 0.05, 0.007), segments=12)
            mask(m, FIN)
            foot.append(m)
        out[f"foot.{side}"] = foot
    return out


def extras(J):
    out = {"head": crest(J), "spine": back_fin(J)}
    for part in (limb_fins(J), webbing(J)):
        for bone, objs in part.items():
            out.setdefault(bone, []).extend(objs)
    return out


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.endswith(".py")]
    kit.run(sys.modules[__name__], args[0] if args else "out")
