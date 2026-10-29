"""The raptor race: a lean, crested, bird-footed traveller (after Nathan's
pick of concept 2, 2026-10-29, in Boundmore's own design). Built with the
shared kit:

    python tools/bm-modelgen/raptor.py OUT_DIR
"""

import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import kit  # noqa: E402  (loads bpy, which provides mathutils)
from mathutils import Vector  # noqa: E402
from kit import along, blob, fuse, join, mask, mottle, smoothstep, spike  # noqa: E402

NAME = "raptor"
# Preview colours (linear); a player picks their own.
SKIN = (0.16, 0.17, 0.19)
BELLY = (0.42, 0.42, 0.44)
MARK = (0.80, 0.80, 0.78)
CLAW = (0.80, 0.52, 0.06)
IRIS = (1.0, 0.62, 0.05)
BULK = 1.0
BUILD = 1.0
CLAWS = 1.0
# Fingers (dx, dy, length) from the palm: three long ones and a thumb.
FINGERS = [(0.035, -0.035, 0.19), (0.0, -0.055, 0.20), (-0.035, -0.035, 0.18), (-0.06, -0.06, 0.10)]
# Toes (dx, forward length): three forward, one back (the hallux).
TOES = [(0.06, 0.20), (0.0, 0.23), (-0.06, 0.20), (0.0, -0.09)]
JOINTS = kit.frame(hunch=0.07, build=BUILD, digitigrade=True)


def joints():
    return JOINTS


def body_colour(co, n):
    ankle_z = JOINTS["ankle.L"][2]
    x, y, z = co
    # The throat and a soft chest, front-facing.
    front = smoothstep(0.15, 0.6, -n.y)
    belly = front * smoothstep(0.92, 1.02, z) * smoothstep(1.50, 1.40, z) * smoothstep(0.19, 0.10, abs(x))
    # Inner thighs, lighter.
    inner = smoothstep(0.2, 0.7, -n.x * (1 if x > 0 else -1))
    belly = max(belly, 0.7 * inner * smoothstep(0.55, 0.7, z) * smoothstep(0.92, 0.82, z))
    # A pale mane over the nape and shoulder blades.
    back = smoothstep(0.0, 0.5, n.y)
    mark = back * smoothstep(1.24, 1.40, z) * smoothstep(0.24, 0.12, abs(x))
    # Scaled legs from just above the ankle, and the feet.
    claw = smoothstep(ankle_z + 0.09, ankle_z + 0.01, z)
    return (belly, mark, claw, mottle(co) * (1 - claw))


def head(J):
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


def crest(J):
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


def mane(J):
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


def tufts(J):
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


def extras(J):
    mn = mane(J)
    out = {"head": crest(J), "chest": [o for o in mn if not o.name.startswith("ridge")],
           "spine": [o for o in mn if o.name.startswith("ridge")]}
    out.update(tufts(J))
    return out


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.endswith(".py")]
    kit.run(sys.modules[__name__], args[0] if args else "out")
