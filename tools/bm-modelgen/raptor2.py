"""The raptor race, second build: box modelled, subdivided, painted.

Unlike raptor.py (blobs fused by a voxel remesh), the body here is one
connected low-poly quad cage with planned edge loops (torso rings, limb holes,
the neck entering under the head, knuckles extruded into fingers), smoothed by
a Subdivision Surface with creases where the forms stay crisp. Feathers,
claws, beak and gear are separate faceted pieces. The game copy is the cage
subdivided once; a copy subdivided three times is baked into its normal map
and painted texture.

    python tools/bm-modelgen/raptor2.py OUT_DIR [--preview] [--views=a,b]

Design (head unit H = 0.30 blocks; the body stands about 6 H, hunched):

- head 1.6 H long beak to nape, 0.85 H tall; snout half the head; brow heavy
- shoulders 2.4 H across the deltoids; chest 1.45 H wide, 1.05 H deep
- waist 0.9 H wide (a strong V from the shoulders); hips 1.1 H
- arms long: fingertips at the knee; forearm as thick as the upper arm
- hands 0.85 H long, 0.45 H wide, three fingers and a thumb, all thick
- legs digitigrade: thigh 0.8 H thick, a drumstick calf, a long scaled
  metatarsus, three forward toes 0.6 H long and a back toe
- crest of seven swept blades (longest 1.3 H), a ruff at the neck

The shared skeleton (kit.py's bone names and animations) is fitted to these
joints, so the kit's walk and idle play on it.
"""

import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import kit  # noqa: E402  (loads bpy)
import bpy  # noqa: E402
from cage import Cage, superellipse  # noqa: E402
from mathutils import Vector  # noqa: E402

NAME = "raptor"
V = Vector

# ---- the frame (x left, y back, z up; the raptor faces -y) -------------------

J = {
    "hips": (0, 0.0, 0.95),
    "waist": (0, 0.01, 1.10),
    "chest": (0, -0.04, 1.30),
    "neck": (0, -0.11, 1.47),
    "skull": (0, -0.21, 1.60),
    "crown": (0, -0.21, 1.85),
}
LEFT = {
    "shoulder": (0.26, -0.04, 1.40),
    "elbow": (0.37, 0.07, 1.12),
    "wrist": (0.405, -0.07, 0.88),
    "palm": (0.425, -0.12, 0.78),
    "hip": (0.125, 0.0, 0.92),
    "knee": (0.185, -0.19, 0.60),
    "ankle": (0.18, 0.11, 0.28),
    "ball": (0.185, -0.05, 0.065),
}
for _k, _p in LEFT.items():
    J[f"{_k}.L"] = _p
    J[f"{_k}.R"] = (-_p[0], _p[1], _p[2])


def L(name):
    return V(LEFT[name])


def mir(p, s):
    return V((p[0] * s, p[1], p[2]))


# ---- the body cage --------------------------------------------------------------

# Torso rings, bottom to top: (z, centre y, half width, front depth, back
# depth, squareness).
TORSO = [
    (0.86, 0.015, 0.155, 0.105, 0.130, 2.3),  # 0 crotch: the legs start here
    (0.96, 0.015, 0.165, 0.110, 0.130, 2.4),  # 1 hips
    (1.06, 0.010, 0.132, 0.100, 0.105, 2.4),  # 2 waist, under the belt
    (1.16, -0.005, 0.150, 0.125, 0.115, 2.5),  # 3 lower ribs
    (1.25, -0.025, 0.195, 0.160, 0.130, 2.7),  # 4 chest: arm hole from here
    (1.34, -0.040, 0.215, 0.165, 0.140, 2.7),  # 5 upper chest
    (1.43, -0.055, 0.205, 0.120, 0.140, 2.5),  # 6 shoulder line: hole to here
    (1.49, -0.085, 0.120, 0.085, 0.115, 2.2),  # 7 trapezius, the neck's base
]
TN = 12  # points round the torso; 0 front, 3 left, 6 back, 9 right


PECS = {3: 0.008, 4: 0.022, 5: 0.026, 6: 0.010}


def torso_ring(c, z, cy, w, df, db, p, k=None):
    out = []
    pec = PECS.get(k, 0.0)
    for i in range(TN):
        a = 2 * math.pi * i / TN
        s, co = math.sin(a), math.cos(a)
        x = w * math.copysign(abs(s) ** (2 / p), s)
        d = df if co > 0 else db
        y = cy - d * math.copysign(abs(co) ** (2 / p), co)
        if co > 0:
            # Two pectoral masses either side of a sternum groove.
            aa = min(a, 2 * math.pi - a)
            y -= pec * (math.exp(-(((aa - 0.55) / 0.3) ** 2)) - 0.45 * math.exp(-((aa / 0.2) ** 2)))
        out.append(c.add((x, y, z), "torso"))
    return out


def body():
    c = Cage()
    rings = [torso_ring(c, *r, k=k) for k, r in enumerate(TORSO)]
    holes = {(4, 2), (4, 3), (5, 2), (5, 3), (4, 8), (4, 9), (5, 8), (5, 9)}
    for k in range(len(rings) - 1):
        a, b = rings[k], rings[k + 1]
        for i in range(TN):
            if (k, i) in holes:
                continue
            j = (i + 1) % TN
            c.f.append((a[i], a[j], b[j], b[i]))
    r = rings
    arm_holes = {
        1: [r[4][2], r[4][3], r[4][4], r[5][4], r[6][4], r[6][3], r[6][2], r[5][2]],
        -1: [r[4][10], r[4][9], r[4][8], r[5][8], r[6][8], r[6][9], r[6][10], r[5][10]],
    }
    crotch = c.add((0, 0.02, 0.80), "torso")
    leg_holes = {
        1: [r[0][i] for i in range(0, 7)] + [crotch],
        -1: [r[0][0]] + [r[0][i] for i in range(11, 5, -1)] + [crotch],
    }
    for s in (1, -1):
        arm(c, arm_holes[s], s)
        leg(c, leg_holes[s], s)
    neck_and_head(c, r[-1])
    return c


def tube(c, start, specs, part, s, up=(0, -1, 0)):
    """Lofts rings of `len(start)` points: specs are (centre, axis, w, h, p)
    in left-side space, mirrored by `s`."""
    out = [start]
    n = len(start)
    for centre, axis, w, h, p in specs:
        pts = superellipse(n, w, h, p)
        ring = c.ring(mir(centre, s), mir(axis, s), mir(up, s), pts, part)
        out.append(c.bridge(out[-1], ring))
    return out


def rect(w, t, bow=0.15):
    """Eight points, four across the top and four across the bottom (top row
    left to right, then the bottom row back): knuckles to extrude digits from."""
    xs = (-w, -w / 3, w / 3, w)
    top = [(x, t * (1 + bow * (1 - abs(x) / w))) for x in xs]
    bot = [(x, -t * (1 + bow * (1 - abs(x) / w))) for x in reversed(xs)]
    return top + bot


def digit(c, root, path, sizes, part, up):
    """A finger or toe grown from a quad (`root`, four indices): rings of
    four along `path` (world points), capped; returns the tip and the last
    direction."""
    prev = root
    pts = [c.centroid(root)] + [V(p) for p in path]
    for i, size in enumerate(sizes):
        p = pts[i + 1]
        d = (pts[i + 1] - pts[i]) if i + 2 >= len(pts) else (pts[i + 2] - pts[i])
        sq = [(-size, size), (size, size), (size, -size), (-size, -size)]
        ring = c.ring(p, d, up, sq, part)
        prev = c.bridge(prev, ring)
    d = (pts[-1] - pts[-2]).normalized()
    c.cap(prev)
    return pts[-1], d


def take_face(c, verts):
    """Removes the cage face made of `verts` and returns it as a ring."""
    want = set(verts)
    for i, f in enumerate(c.f):
        if set(f) == want:
            return list(c.f.pop(i))
    raise ValueError("no such face")


def arm(c, hole, s):
    sh, el, wr, pa = L("shoulder"), L("elbow"), L("wrist"), L("palm")
    ua, fa = el - sh, wr - el
    hd = (pa - wr).normalized()
    specs = [
        (sh + V((0.025, 0.0, 0.0)), V((1, 0, -0.7)), 0.100, 0.104, 2.2),  # deltoid cap
        (sh + ua * 0.20 + V((0.020, 0, 0)), ua, 0.100, 0.104, 2.2),  # deltoid
        (sh + ua * 0.50 + V((0.006, -0.008, 0)), ua, 0.082, 0.092, 2.2),  # biceps
        (sh + ua * 0.82, ua, 0.068, 0.072, 2.2),
        (el, ua + fa, 0.062, 0.066, 2.3),  # elbow
        (el + fa * 0.26 + V((0.008, 0, 0)), fa, 0.092, 0.084, 2.4),  # forearm bulk
        (el + fa * 0.60, fa, 0.076, 0.068, 2.4),
        (el + fa * 0.93, fa, 0.054, 0.056, 2.4),  # wrist
    ]
    rings = tube(c, hole, specs, "arm", s)
    # The hand: a box palm, its knuckle end split into three fingers.
    out = mir(V((1, 0, 0)), s)
    p0 = c.ring(mir(wr + hd * 0.04, s), mir(hd, s), out, rect(0.062, 0.034), "hand")
    p1 = c.ring(mir(pa + hd * 0.01, s), mir(hd, s), out, rect(0.072, 0.034), "hand")
    c.bridge(rings[-1], p0)
    c.f += [(p0[i], p0[(i + 1) % 8], p1[(i + 1) % 8], p1[i]) for i in range(8)]
    across = (c.v[p1[3]] - c.v[p1[0]]).normalized()
    inward = -out
    quads = [(p1[0], p1[1], p1[6], p1[7]), (p1[1], p1[2], p1[5], p1[6]), (p1[2], p1[3], p1[4], p1[5])]
    lengths = (0.054, 0.058, 0.050)
    for k, q in enumerate(quads):
        root = c.centroid(q)
        spread = across * (-0.22 + 0.22 * k)
        d1 = (mir(hd, s) + spread).normalized()
        d2 = (d1 * 0.85 + inward * 0.45).normalized()
        d3 = (d1 * 0.45 + inward * 0.9).normalized()
        n = lengths[k]
        j1 = root + d1 * n
        j2 = j1 + d2 * n
        j3 = j2 + d3 * n * 0.7
        tip, d = digit(c, list(q), [j1, j2, j3], (0.027, 0.025, 0.021), "hand", out)
        claw(tip - d * 0.012, d, inward, 0.06, 0.019)
    # The thumb, from the palm's front side near the wrist.
    sides = ([p0[0], p1[0], p1[7], p0[7]], [p0[3], p1[3], p1[4], p0[4]])
    q = take_face(c, min(sides, key=lambda f: c.centroid(f).y))
    root = c.centroid(q)
    fwd = V((0, -1, 0))
    d1 = (fwd * 0.8 + mir(hd, s) * 0.5 + inward * 0.2).normalized()
    d2 = (fwd * 0.3 + mir(hd, s) * 0.7 + inward * 0.5).normalized()
    j1 = root + d1 * 0.048
    j2 = j1 + d2 * 0.046
    tip, d = digit(c, q, [j1, j2], (0.027, 0.023), "hand", out)
    claw(tip - d * 0.012, d, inward, 0.05, 0.018)


def leg(c, hole, s):
    hp, kn, an, ba = L("hip"), L("knee"), L("ankle"), L("ball")
    th, sh, mt = kn - hp, an - kn, ba - an
    specs = [
        (hp + th * 0.10 + V((0.014, 0, 0)), th, 0.118, 0.128, 2.2),  # upper thigh
        (hp + th * 0.42 + V((0.014, -0.016, 0)), th, 0.112, 0.124, 2.2),
        (hp + th * 0.78, th, 0.076, 0.082, 2.2),
        (kn, th + sh, 0.066, 0.070, 2.2),  # knee
        (kn + sh * 0.26 + V((0, 0.02, 0)), sh, 0.080, 0.092, 2.2),  # drumstick
        (kn + sh * 0.6, sh, 0.054, 0.058, 2.2),
        (an, sh + mt, 0.052, 0.058, 2.3),  # the backward heel
        (an + mt * 0.45, mt, 0.044, 0.046, 2.5),  # scaled metatarsus
        (an + mt * 0.84 + V((0, -0.008, 0)), mt + V((0, -0.9, 0)), 0.050, 0.048, 2.5),
    ]
    rings = tube(c, hole, specs, "leg", s)
    # The foot: a box whose front splits into three toes.
    f0 = c.ring(mir(ba + V((0, -0.035, -0.012)), s), V((0, -1, 0)), V((0, 0, 1)), rect(0.074, 0.036, 0.1), "foot")
    n0 = len(c.f)
    c.bridge(rings[-1], f0)
    bridge_faces = c.f[n0:]
    up = V((0, 0, 1))
    # The ring's +X is world +x, so sort the quads inner to outer.
    quads = [(f0[0], f0[1], f0[6], f0[7]), (f0[1], f0[2], f0[5], f0[6]), (f0[2], f0[3], f0[4], f0[5])]
    quads.sort(key=lambda q: abs(c.centroid(q).x))
    for k, q in enumerate(quads):
        root = c.centroid(q)
        ang = math.radians((-24, 0, 26)[k]) * s
        fwd = V((math.sin(ang), -math.cos(ang), 0))
        n = (0.07, 0.078, 0.066)[k]
        j1 = root + fwd * n + V((0, 0, -0.012))
        j2 = j1 + fwd * n + V((0, 0, -0.008))
        j3 = j2 + fwd * n * 0.6 + V((0, 0, -0.004))
        tip, d = digit(c, list(q), [j1, j2, j3], (0.033, 0.031, 0.026), "foot", up)
        claw(tip - d * 0.014, d, -up, 0.075, 0.024)
    # The back toe, from the lowest backward face of the heel.
    back = max(bridge_faces, key=lambda f: c.centroid(f).y - 0.5 * c.centroid(f).z)
    c.f.remove(back)
    root = c.centroid(back)
    bk = V((0.15 * s, 1, -0.35)).normalized()
    j1 = root + bk * 0.045
    j2 = j1 + V((0.05 * s, 1, -0.7)).normalized() * 0.04
    tip, d = digit(c, list(back), [j1, j2], (0.026, 0.022), "foot", up)
    claw(tip - d * 0.01, d, -up, 0.055, 0.019)


HN = 16  # points round the head; 0 top, 4 left, 8 bottom, 12 right
HEAD_FWD = V((0, -1, -0.10)).normalized()
# Head rings along the snout, back to front: (forward, up, half width, half
# height, squareness, brow, jaw, mouth).
HEAD = [
    (-0.150, 0.080, 0.055, 0.052, 2.0, 0.0, 0.0, 0.0),  # 0 back of the skull
    (-0.110, 0.075, 0.098, 0.096, 2.3, 0.0, 0.0, 0.0),
    (-0.050, 0.065, 0.112, 0.108, 2.5, 0.0, 0.0, 0.0),  # 2 cranium, widest
    (0.010, 0.055, 0.106, 0.106, 2.6, 0.020, 0.016, 0.0),  # 3 brow over the eyes
    (0.075, 0.040, 0.090, 0.094, 2.7, 0.016, 0.016, 0.008),  # 4 front of the eyes
    (0.145, 0.020, 0.072, 0.080, 2.7, 0.0, 0.012, 0.010),  # 5 snout
    (0.215, 0.004, 0.061, 0.068, 2.7, 0.0, 0.008, 0.009),
    (0.285, -0.010, 0.052, 0.058, 2.6, 0.0, 0.004, 0.006),  # 7 under the beak
]


def head_point(f, u):
    sx, sy, sz = J["skull"]
    return V((0, sy, sz)) + HEAD_FWD * f + V((0, 0, u))


def neck_and_head(c, base):
    rings = []
    for f, u, w, h, p, brow, jaw, mouth in HEAD:
        def bulge(a, brow=brow, jaw=jaw, mouth=mouth, w=w):
            # The brow lifts the upper sides, the jaw widens the lower sides,
            # and the mouth line pinches in between them.
            up = math.cos(a)
            side = abs(math.sin(a))
            k = 1.0
            k += brow / w * max(0.0, 1 - abs(up - 0.62) / 0.3) * side
            k += jaw / w * max(0.0, 1 - abs(up + 0.72) / 0.3) * side
            k -= mouth / w * max(0.0, 1 - abs(up + 0.38) / 0.2) * side
            return k

        pts = superellipse(HN, w, h, p, bulge=bulge)
        rings.append(c.ring(head_point(f, u), HEAD_FWD, (0, 0, 1), pts, "head"))
    hole_faces = {(j, i) for j in range(1, 5) for i in (7, 8)}
    for j in range(len(rings) - 1):
        a, b = rings[j], rings[j + 1]
        for i in range(HN):
            if (j, i) in hole_faces:
                continue
            k = (i + 1) % HN
            c.f.append((a[i], a[k], b[k], b[i]))
    c.cap(rings[0])
    c.cap(rings[-1])
    # The brow ridge stays crisp; the mouth line is a firm crease.
    c.crease_path([rings[j][2] for j in range(2, 6)], 0.8)
    c.crease_path([rings[j][14] for j in range(2, 6)], 0.8)
    c.crease_path([rings[j][5] for j in range(4, 8)], 0.7)
    # The mouth line is painted dark along that crease (grad marks it).
    for j in range(4, 8):
        for i in (5, 11):
            c.grad[rings[j][i]] = 1.0
    c.crease_path([rings[j][11] for j in range(4, 8)], 0.7)
    loop = [rings[j][7] for j in range(1, 6)] + [rings[5][8]] + [rings[j][9] for j in range(5, 0, -1)] + [rings[1][8]]
    # The neck: from the trapezius ring up and forward into the hole.
    nc = c.centroid(loop)
    b0 = c.centroid(base)
    neck = []
    for t, w, d in ((0.35, 0.082, 0.090), (0.72, 0.070, 0.088)):
        p = b0.lerp(nc, t)
        pts = []
        for i in range(TN):
            a = 2 * math.pi * i / TN
            pts.append((p.x + w * math.sin(a), p.y - d * math.cos(a), p.z))
        neck.append([c.add(q, "neck") for q in pts])
    c.loft(base, neck + [loop])


# ---- accessories ---------------------------------------------------------------

ACC = []  # every separate piece: (cage, bone)


def blade_path(c, pts, widths, thick, part, up_hint, ridge=0.5):
    """A blade: a diamond section (edge, ridge, edge, ridge) lofted along
    `pts`, ending in a point; the edges creased sharp."""
    rings = []
    n = len(pts) - 1
    for i, p in enumerate(pts[:-1]):
        d = pts[i + 1] - (pts[i - 1] if i else p)
        w, t = widths[i], thick[i]
        rings.append(c.ring(p, d, up_hint, [(-w, 0), (0, t), (w, 0), (0, -t)], part, i / n))
    for a, b in zip(rings, rings[1:]):
        c.f += [(a[i], a[(i + 1) % 4], b[(i + 1) % 4], b[i]) for i in range(4)]
    tip = c.add(pts[-1], part, 1.0)
    r = rings[-1]
    c.f += [(r[i], r[(i + 1) % 4], tip) for i in range(4)]
    c.f.append(tuple(reversed(rings[0])))
    for k in (0, 2):
        c.crease_path([r[k] for r in rings] + [tip], 1.0, sharp=True)
    for k in (1, 3):
        c.crease_path([r[k] for r in rings] + [tip], ridge)
    c.crease_loop(rings[0], 1.0)
    return rings


def claw(base, d, curl, length, r, bone=None):
    c = Cage()
    d = V(d).normalized()
    curl = V(curl)
    curl = (curl - d * curl.dot(d)).normalized()
    pts, ws, ts = [], [], []
    for i in range(5):
        t = i / 4
        pts.append(V(base) + d * length * t + curl * length * 0.45 * t * t)
        ws.append(r * (1 - 0.75 * t) * 0.8)
        ts.append(r * (1 - 0.7 * t))
    blade_path(c, pts, ws, ts, "talon", curl, ridge=0.8)
    ACC.append((c, bone))
    return c


def feather(base, d, bend_dir, flat_n, length, width, bend=0.2, thick=0.3, bone=None):
    """A blade feather from `base` along `d`, curving towards `bend_dir`,
    thin across `flat_n`, keeled down the middle."""
    c = Cage()
    d = V(d).normalized()
    b = V(bend_dir)
    b = (b - d * b.dot(d)).normalized()
    pts, ws, ts = [], [], []
    n = 5
    for i in range(n):
        t = i / (n - 1)
        pts.append(V(base) + d * length * t + b * length * bend * t * t)
        prof = math.sin(math.pi * (0.3 + 0.7 * t)) if t < 1 else 0
        ws.append(width * (0.55 + 0.45 * prof))
        ts.append(width * thick * (1 - 0.5 * t))
    blade_path(c, pts, ws, ts, "feather", V(flat_n), ridge=0.3)
    ACC.append((c, bone))
    return c


def crest():
    sx, sy, sz = J["skull"]
    # Main blades from the brow back over the skull, longest in the middle.
    rows = [
        (-0.05, 0.10, 44, 0.20, 0.026),
        (-0.01, 0.12, 36, 0.30, 0.030),
        (0.04, 0.12, 28, 0.40, 0.033),
        (0.09, 0.11, 22, 0.44, 0.034),
        (0.13, 0.09, 16, 0.40, 0.032),
        (0.16, 0.06, 10, 0.32, 0.030),
        (0.18, 0.03, 4, 0.24, 0.026),
    ]
    for i, (dy, dz, rise, length, width) in enumerate(rows):
        a = math.radians(rise)
        d = V((0, math.cos(a), math.sin(a)))
        base = V((0, sy + dy, sz + dz))
        feather(base, d, V((0, 1, -0.4)), V((1, 0, 0)), length, width, bend=0.22, thick=0.4, bone="head")
        if 1 <= i <= 5:
            for s in (1, -1):
                d2 = V((0.6 * s, math.cos(a), math.sin(a) + 0.2)).normalized()
                feather(base + V((0.055 * s, 0.01, -0.035)), d2, V((0.6 * s, 1, -0.5)), V((1, 0, 0.8 * s)), length * 0.75, width * 0.95, bend=0.3, bone="head")


def mane():
    nx, ny, nz = J["neck"]
    # A ruff round the back and sides of the neck, drooping over the shoulders.
    layers = ((1.58, 0.085, 0.10, 13, 0.20, 140, "neck"), (1.48, 0.15, 0.14, 15, 0.25, 128, "chest"))
    for ring, (rz, rx, ry, n, length, reach, bone) in enumerate(layers):
        for i in range(n):
            a = math.radians(-reach + i * 2 * reach / (n - 1))
            base = V((math.sin(a) * rx, ny + 0.03 + math.cos(a) * ry, rz))
            outw = V((math.sin(a), math.cos(a), 0))
            d = (outw * 0.75 + V((0, 0.5, -0.7))).normalized()
            k = 0.6 + 0.5 * max(0.0, math.cos(a))
            feather(base, d, V((0, 0.2, -1)), outw * 0.6 + V((0, 0, 0.8)), length * k, 0.04, bend=0.25, bone=bone)
    # Spines down the back.
    for i in range(5):
        t = i / 4
        base = V((0, 0.11 + 0.01 * t, 1.36 - 0.25 * t))
        d = V((0, 1, -0.5 - 0.2 * t)).normalized()
        feather(base, d, V((0, 0, 1)), V((1, 0, 0)), 0.15 * (1 - 0.45 * t), 0.04 * (1 - 0.4 * t), bend=0.3, bone="spine")


def tufts():
    for s in (1, -1):
        el, wr = L("elbow"), L("wrist")
        for i in range(3):
            p = el.lerp(wr, 0.12 + 0.17 * i) + V((0.035, 0.05, 0))
            d = V((0.4, 1, 0.35 - 0.2 * i)).normalized()
            side = "L" if s > 0 else "R"
            feather(mir(p, s), mir(d, s), V((0, 0, 1)), mir(V((1, -0.3, 0)), s), 0.14 - 0.02 * i, 0.034, bend=0.3, bone=f"forearm.{side}")


def ball(c, centre, gaze, up, r, part, rings=6, n=8, flat=1.0):
    """A closed ellipsoid cage looking along `gaze`."""
    gaze = V(gaze).normalized()
    out = []
    for k in range(1, rings):
        th = math.pi * k / rings
        p = V(centre) + gaze * r[2] * math.cos(th)
        pts = superellipse(n, r[0] * math.sin(th), r[1] * math.sin(th), 2.0)
        out.append(c.ring(p, gaze, up, pts, part))
    for a, b in zip(out, out[1:]):
        c.bridge(a, b, align=False)
    c.cap(out[0], V(centre) + gaze * r[2])
    c.cap(out[-1], V(centre) - gaze * r[2])
    return out


EYES = []


def brows():
    """Heavy brow ridges: creased wedges over the eyes, low at the front and
    swept up and back, which gives the raptor its scowl."""
    for s in (1, -1):
        c = Cage()
        a = head_point(0.115, 0.074) + V((0.058 * s, 0, 0))
        b = head_point(0.035, 0.098) + V((0.090 * s, 0, 0))
        e = head_point(-0.05, 0.110) + V((0.085 * s, 0, 0))
        pts = [a, a.lerp(b, 0.5), b, b.lerp(e, 0.6), e]
        blade_path(c, pts, [0.018, 0.026, 0.026, 0.020], [0.016, 0.022, 0.020, 0.014], "brow", V((s * 0.4, 0, 1)), ridge=0.9)
        ACC.append((c, "head"))


def eyes(high=False):
    f, u = 0.045, 0.058
    for s in (1, -1):
        c = Cage()
        centre = head_point(f, u) + V((0.086 * s, 0, 0))
        gaze = V((0.68 * s, -0.72, 0.04))
        ball(c, centre, gaze, (0, 0, 1), (0.030, 0.028, 0.032), "eye", 12 if high else 6, 24 if high else 12)
        EYES.append((centre, gaze.normalized()))
        ACC.append((c, "head"))


def beak():
    """The upper beak, a hooked sheath over the snout's end, and the lower
    beak under it; their cutting edges and bases are crisp."""
    c = Cage()
    # (forward, up, half width, half height) along the upper beak.
    steps = [(0.235, 0.008, 0.066, 0.070), (0.275, 0.000, 0.060, 0.064), (0.315, -0.008, 0.050, 0.056), (0.350, -0.020, 0.036, 0.046), (0.372, -0.045, 0.022, 0.036)]
    shape = [(0, 1.0), (0.68, 0.84), (1.0, 0.25), (0.92, -0.45), (0, -0.2), (-0.92, -0.45), (-1.0, 0.25), (-0.68, 0.84)]
    rings = []
    for i, (f, u, w, h) in enumerate(steps):
        axis = HEAD_FWD if i < 3 else (HEAD_FWD + V((0, 0, -0.6 * (i - 2)))).normalized()
        pts = [(x * w, y * h) for x, y in shape]
        rings.append(c.ring(head_point(f, u), axis, (0, 0, 1), pts, "beak", i / len(steps)))
    for a, b in zip(rings, rings[1:]):
        c.bridge(a, b, align=False)
    tip = c.add(head_point(0.372, -0.090), "beak", 1.0)
    r = rings[-1]
    c.f += [(r[i], r[(i + 1) % 8], tip) for i in range(8)]
    c.f.append(tuple(reversed(rings[0])))
    c.crease_loop(rings[0], 1.0)
    for k in (3, 5):
        c.crease_path([r[k] for r in rings] + [tip], 1.0, sharp=True)
    c.crease_path([r[0] for r in rings], 0.5)
    ACC.append((c, "head"))
    # The lower beak.
    c = Cage()
    steps = [(0.245, -0.040, 0.050, 0.030), (0.285, -0.044, 0.042, 0.026), (0.320, -0.050, 0.032, 0.022)]
    shape = [(0, 0.3), (0.8, 0.6), (1.0, 0.0), (0.7, -0.75), (0, -1.0), (-0.7, -0.75), (-1.0, 0.0), (-0.8, 0.6)]
    rings = []
    for f, u, w, h in steps:
        pts = [(x * w, y * h) for x, y in shape]
        rings.append(c.ring(head_point(f, u), HEAD_FWD, (0, 0, 1), pts, "beak"))
    for a, b in zip(rings, rings[1:]):
        c.bridge(a, b, align=False)
    tip = c.add(head_point(0.350, -0.052), "beak")
    r = rings[-1]
    c.f += [(r[i], r[(i + 1) % 8], tip) for i in range(8)]
    c.f.append(tuple(reversed(rings[0])))
    c.crease_loop(rings[0], 1.0)
    for k in (1, 7):
        c.crease_path([r[k] for r in rings] + [tip], 1.0, sharp=True)
    ACC.append((c, "head"))


def band(c, z, h, grow, t, part, n=16):
    """A strap round the torso at height `z`: rings of four (a flat
    rectangle) following the torso's section, `grow` out from the skin."""
    lo = max(k for k in range(len(TORSO)) if TORSO[k][0] <= z)
    a, b = TORSO[lo], TORSO[lo + 1]
    f = (z - a[0]) / (b[0] - a[0])
    zz, cy, w, df, db, p = [a[i] + (b[i] - a[i]) * f for i in range(6)]
    loop = []
    for i in range(n):
        ang = 2 * math.pi * i / n
        sn, co = math.sin(ang), math.cos(ang)
        x = w * math.copysign(abs(sn) ** (2 / p), sn)
        d = df if co > 0 else db
        y = cy - d * math.copysign(abs(co) ** (2 / p), co)
        centre = V((x, cy, z))
        outv = V((x, y - cy, 0))
        rr = outv.length
        outv.normalize()
        base = V((0, cy, z)) + outv * (rr + grow)
        loop.append((base, outv))
    rings = []
    for i, (base, outv) in enumerate(loop):
        nxt = loop[(i + 1) % n][0] - loop[i - 1][0]
        pts = [(-t, -h), (t, -h), (t, h), (-t, h)]
        rings.append(c.ring(base, nxt, (0, 0, 1), pts, part))
    for i in range(n):
        c.bridge(rings[i], rings[(i + 1) % n], align=False)
    for k in range(4):
        c.crease_loop([r[k] for r in rings], 0.9)
    return loop


def plate(c, centre, axis, up, w, t, part, crease=0.9):
    """A box (rect section of 8 points) between two rings, creased."""
    a = c.ring(V(centre) - V(axis) * 0.5, axis, up, rect(w, t, 0.0), part)
    b = c.ring(V(centre) + V(axis) * 0.5, axis, up, rect(w, t, 0.0), part)
    c.bridge(a, b, align=False)
    c.f.append(tuple(reversed(a)))
    c.f.append(tuple(b))
    c.crease_loop(a, crease)
    c.crease_loop(b, crease)


def flap(c, top, down, out, w, length, t, part, sway=0.0):
    """A hanging cloth flap: rings of eight (four across) down `down`,
    pointed at the bottom, bulging along `out`."""
    rings = []
    for k in range(5):
        f = k / 4
        p = V(top) + V(down) * length * f + V(out) * (0.02 * math.sin(math.pi * f) + sway * f * f)
        ww = w * (1 - 0.12 * f)
        pts = rect(ww, t, 0.0)
        ring = c.ring(p, down, out, pts, part, f)
        if k == 4:
            # A notched hem: the middle hangs lower.
            for i in (1, 2, 5, 6):
                c.v[ring[i]] += V(down) * length * 0.07
        rings.append(ring)
    for a, b in zip(rings, rings[1:]):
        c.bridge(a, b, align=False)
    c.f.append(tuple(reversed(rings[0])))
    c.f += [(rings[-1][i], rings[-1][i + 1], rings[-1][6 - i], rings[-1][7 - i]) for i in range(3)]
    c.crease_loop(rings[-1], 0.8)
    c.crease_path([r[0] for r in rings], 0.8)
    c.crease_path([r[3] for r in rings], 0.8)
    c.crease_path([r[4] for r in rings], 0.8)
    c.crease_path([r[7] for r in rings], 0.8)


def gear():
    c = Cage()
    loop = band(c, 1.00, 0.032, 0.012, 0.012, "leather")
    ACC.append((c, None))
    # The buckle, a brass plate on the front.
    fb = loop[0]
    c = Cage()
    plate(c, fb[0] + fb[1] * 0.018, fb[1] * 0.014, (0, 0, 1), 0.040, 0.034, "brass")
    ACC.append((c, "hips"))
    # The loincloth, front and back.
    c = Cage()
    front = V((0, -0.115, 0.975))
    flap(c, front, V((0, -0.15, -1)).normalized(), V((0, -1, 0)), 0.068, 0.31, 0.009, "cloth")
    back = V((0, 0.145, 0.975))
    flap(c, back, V((0, 0.2, -1)).normalized(), V((0, 1, 0)), 0.10, 0.30, 0.009, "cloth", sway=0.02)
    ACC.append((c, "hips"))
    # A pouch on the right hip.
    c = Cage()
    side = loop[12]
    plate(c, side[0] + side[1] * 0.04 + V((0, 0, -0.045)), side[1] * 0.045, (0, 0, 1), 0.05, 0.055, "leather", 0.6)
    ACC.append((c, "hips"))


def build_cages(high=False):
    ACC.clear()
    EYES.clear()
    c = body()
    beak()
    eyes(high)
    brows()
    crest()
    mane()
    tufts()
    gear()
    # A bigger head reads better at a distance: everything on the head
    # grows about the skull.
    pivot = V(J["skull"])
    for i, part in enumerate(c.part):
        if part == "head":
            c.v[i] = pivot + (c.v[i] - pivot) * HEAD_SCALE
    for cg, bone in ACC:
        if bone == "head":
            cg.v = [pivot + (v - pivot) * HEAD_SCALE for v in cg.v]
    EYES[:] = [(pivot + (ce - pivot) * HEAD_SCALE, g) for ce, g in EYES]
    return c


HEAD_SCALE = 1.12


# ---- paint -------------------------------------------------------------------------

PARTS = sorted(
    ["arm", "beak", "brass", "brow", "cloth", "eye", "feather", "foot", "hand", "head", "leather", "leg", "neck", "talon", "torso"]
)
GEAR = {"brass", "cloth", "leather"}

# Preview colours (linear); a player picks their own skin, belly, marking and
# claw colours, which the game mixes by the paint texture's weights.
SKIN = (0.045, 0.075, 0.130)
BELLY = (0.66, 0.56, 0.40)
MARK = (0.78, 0.84, 0.92)
MARK_DEEP = (0.22, 0.32, 0.50)
CLAW = (0.90, 0.56, 0.06)
BEAK_TIP = (0.55, 0.22, 0.03)
TALON = (0.022, 0.020, 0.024)
TALON_BASE = (0.10, 0.09, 0.08)
LEATHER = (0.15, 0.065, 0.025)
CLOTH = (0.32, 0.09, 0.04)
BRASS = (0.80, 0.52, 0.16)
IRIS = (1.0, 0.62, 0.05)
IRIS_RIM = (0.45, 0.10, 0.01)


def lerp3(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def marks(part, co, n):
    """The marking pattern (0 to 1): a stripe back from each eye and bands
    on the forearms and thighs."""
    ss = kit.smoothstep
    s = 1 if co.x >= 0 else -1
    if part == "head" and EYES:
        centre, _ = EYES[0 if s > 0 else 1]
        r = co - centre
        along = r.y
        perp = r.z - 0.22 * r.y + 0.006
        return ss(0.020, 0.010, abs(perp)) * ss(0.0, 0.035, along) * ss(0.20, 0.13, along) * ss(0.2, 0.5, abs(n.x))
    if part == "arm":
        el, wr = mir(L("elbow"), s), mir(L("wrist"), s)
        t = (co - el).dot(wr - el) / (wr - el).length_squared
        back = ss(-0.1, 0.4, n.y + 0.4 * n.x * s)
        band = ss(0.3, 0.9, math.sin((t - 0.12) * math.pi * 3.2)) if 0.08 < t < 0.85 else 0.0
        return band * back
    if part == "leg":
        hp, kn = mir(L("hip"), s), mir(L("knee"), s)
        t = (co - hp).dot(kn - hp) / (kn - hp).length_squared
        outer = ss(0.1, 0.6, n.x * s)
        band = ss(0.35, 0.92, math.sin((t - 0.18) * math.pi * 3.0)) if 0.15 < t < 0.85 else 0.0
        return band * outer
    return 0.0


def paint(part, co, n, g):
    """(colour, mask, fx) for a point: the colour as painted for the
    preview; the tint weights (belly, marking, claw); and fx: scales,
    mottling, darkness."""
    ss = kit.smoothstep
    if part == "talon":
        return lerp3(TALON_BASE, TALON, ss(0.0, 0.6, g)), (0, 0, 1), (0, 0, 0.85)
    if part == "feather":
        return lerp3(MARK_DEEP, MARK, ss(0.15, 0.85, g)), (0, 1, 0), (0, 0.2, 0.35 * (1 - ss(0.0, 0.5, g)))
    if part == "leather":
        return LEATHER, (0, 0, 0), (0, 0.6, 0)
    if part == "cloth":
        hem = ss(0.80, 0.95, g)
        return lerp3(CLOTH, (0.09, 0.03, 0.015), hem), (0, 0, 0), (0, 0.5, 0)
    if part == "brass":
        return BRASS, (0, 0, 0), (0, 0, 0)
    if part == "beak":
        return lerp3(CLAW, BEAK_TIP, ss(0.45, 1.0, g)), (0, 0, 1), (0, 0, 0)
    if part == "eye":
        for centre, gaze in EYES:
            d = co - centre
            if d.length < 0.05:
                f = d.normalized().dot(gaze)
                right = gaze.cross(V((0, 0, 1))).normalized()
                u, v = d.dot(right), d.z
                if f > 0.6 and abs(v) < 0.024 and abs(u) < 0.0055 * (1 - (v / 0.024) ** 2):
                    return (0.005, 0.004, 0.004), (0, 0, 0), (0, 0, 0)
                if f > 0.45:
                    # Bright round the pupil, burnt at the rim.
                    return lerp3(IRIS, IRIS_RIM, ss(0.75, 0.5, f)), (0, 0, 0), (0, 0, 0)
                return (0.03, 0.02, 0.015), (0, 0, 0), (0, 0, 0)
        return IRIS, (0, 0, 0), (0, 0, 0)
    ankle = J["ankle.L"][2]
    scale = 1.0 if part == "foot" else (ss(ankle + 0.0, ankle - 0.05, co.z) if part == "leg" else 0.0)
    belly = 0.0
    if part in ("torso", "neck"):
        width = 0.05 + 0.07 * ss(1.05, 1.32, co.z)
        belly = ss(0.2, 0.8, -n.y) * ss(width + 0.06, width - 0.01, abs(co.x)) * ss(0.98, 1.12, co.z)
    if part == "head":
        belly = ss(0.0, 0.5, -n.z) * ss(J["skull"][2] + 0.0, J["skull"][2] - 0.05, co.z)
        if g > 0.0:
            mouth = ss(0.55, 0.85, g)
            return lerp3(SKIN, (0.01, 0.01, 0.02), mouth), (belly * (1 - mouth), 0, 0), (0, 0, mouth * 0.8)
    m = marks(part, co, n) * 0.5 * (1 - belly)
    col = lerp3(SKIN, BELLY, belly)
    col = lerp3(col, MARK, m)
    col = lerp3(col, CLAW, scale)
    return col, (belly * (1 - scale), m * (1 - scale), scale), (scale, (1 - scale) * (1 - 0.6 * belly), 0)


def subdivide(o, levels):
    m = o.modifiers.new("sub", "SUBSURF")
    m.levels = levels
    m.render_levels = levels
    m.use_creases = True
    m.boundary_smooth = "ALL"
    kit.apply_all(o)
    return o


def paint_mesh(o):
    """Fills the corner attributes `col`, `mask` and `fx` from `paint`."""
    me = o.data
    pa = me.attributes["part"]
    gr = me.attributes["grad"]
    import numpy as np

    names = ("col", "mask", "fx")
    attrs = [me.color_attributes.new(n, "FLOAT_COLOR", "CORNER") for n in names]
    nl = len(me.loops)
    out = [np.ones((nl, 4), dtype=np.float32) for _ in names]
    loop_v = np.empty(nl, dtype=np.int32)
    me.loops.foreach_get("vertex_index", loop_v)
    fparts = np.empty(len(me.polygons), dtype=np.int32)
    pa.data.foreach_get("value", fparts)
    starts = np.empty(len(me.polygons), dtype=np.int32)
    totals = np.empty(len(me.polygons), dtype=np.int32)
    me.polygons.foreach_get("loop_start", starts)
    me.polygons.foreach_get("loop_total", totals)
    vs = me.vertices
    grads = np.empty(len(vs), dtype=np.float32)
    gr.data.foreach_get("value", grads)
    cache = {}
    for fi in range(len(starts)):
        part = int(fparts[fi])
        for li in range(starts[fi], starts[fi] + totals[fi]):
            vi = int(loop_v[li])
            key = (vi, part)
            val = cache.get(key)
            if val is None:
                v = vs[vi]
                val = cache[key] = paint(PARTS[part], v.co, v.normal, float(grads[vi]))
            for k in range(3):
                out[k][li, :3] = val[k]
    for a, o in zip(attrs, out):
        a.data.foreach_set("color", o.ravel())


def preview_material():
    m = bpy.data.materials.new("preview")
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.6
    a = nt.nodes.new("ShaderNodeAttribute")
    a.attribute_name = "col"
    nt.links.new(a.outputs["Color"], bsdf.inputs["Base Color"])
    return m


def pieces(level, high=False):
    """Every cage built and subdivided `level` times: [(object, bone)]."""
    c = build_cages(high)
    out = [(c.build("body", PARTS), "body")]
    for i, (cg, bone) in enumerate(ACC):
        out.append((cg.build(f"acc{i}", PARTS), bone))
    # Blades (feathers, talons) are faceted: one level fewer than the body,
    # none at all in the game copy.
    blades = [False] + [set(cg.part) <= {"feather", "talon"} for cg, _ in ACC]
    for (o, _), blade in zip(out, blades):
        if level - blade > 0:
            subdivide(o, level - blade)
    return out


def build_preview(levels=2):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    mat = preview_material()
    for o, _ in pieces(levels):
        paint_mesh(o)
        o.data.materials.append(mat)


# ---- the game model ----------------------------------------------------------------

TEX = 2048  # baked at this size
OUT_TEX = 1024  # saved at this size (about 3 MB for the three textures)


def tri_count(o):
    return sum(len(p.vertices) - 2 for p in o.data.polygons)


def rig_pieces(parts):
    """The shared skeleton fitted to J; the body weighted automatically,
    each accessory either riding one bone or taking the body's weights
    where it sits."""
    kit.J = J
    kit.BONES = kit.bones()
    rig = kit.build_rig()
    body = parts[0][0]
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    for o, bone in parts[1:]:
        if bone:
            vg = o.vertex_groups.new(name=bone)
            vg.add(list(range(len(o.data.vertices))), 1.0, "REPLACE")
            continue
        dt = o.modifiers.new("dt", "DATA_TRANSFER")
        dt.object = body
        dt.use_vert_data = True
        dt.data_types_verts = {"VGROUP_WEIGHTS"}
        dt.vert_mapping = "POLYINTERP_NEAREST"
        dt.layers_vgroup_select_src = "ALL"
        dt.layers_vgroup_select_dst = "NAME"
        bpy.ops.object.select_all(action="DESELECT")
        bpy.context.view_layer.objects.active = o
        o.select_set(True)
        bpy.ops.object.datalayout_transfer(modifier="dt")
        bpy.ops.object.modifier_apply(modifier="dt")
    char = kit.join([body] + [o for o, _ in parts[1:]], NAME)
    for m in list(char.modifiers):
        if m.type == "ARMATURE":
            m.object = rig
    return char, rig


def unwrap(o):
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.003)
    bpy.ops.uv.pack_islands(rotate=True, margin=0.003)
    bpy.ops.object.mode_set(mode="OBJECT")


def attr_out(nt, name):
    a = nt.nodes.new("ShaderNodeAttribute")
    a.attribute_name = name
    return a.outputs["Color"]


def detail_pass(nt):
    """Painted detail as brightness (red): broad mottling and spots where fx
    green says, scales where fx red says; green carries fx's darkness."""
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(attr_out(nt, "fx"), sep.inputs["Color"])
    coord = nt.nodes.new("ShaderNodeTexCoord").outputs["Object"]

    def node(kind, **inputs):
        n = nt.nodes.new(kind)
        for k, v in inputs.items():
            if isinstance(v, (int, float, tuple)):
                n.inputs[k].default_value = v
            else:
                nt.links.new(v, n.inputs[k])
        return n

    def remap(x, a, b, lo, hi):
        return node("ShaderNodeMapRange", **{"Value": x, "From Min": a, "From Max": b, "To Min": lo, "To Max": hi}).outputs["Result"]

    def mixf(f, a, b):
        n = nt.nodes.new("ShaderNodeMix")
        n.data_type = "FLOAT"
        nt.links.new(f, n.inputs["Factor"])
        for i, v in ((2, a), (3, b)):
            if isinstance(v, float):
                n.inputs[i].default_value = v
            else:
                nt.links.new(v, n.inputs[i])
        return n.outputs[0]

    def mul(a, b):
        n = node("ShaderNodeMath")
        n.operation = "MULTIPLY"
        nt.links.new(a, n.inputs[0])
        nt.links.new(b, n.inputs[1])
        return n.outputs[0]

    blot = node("ShaderNodeTexNoise", Vector=coord, Scale=11.0, Detail=3.0)
    mottle = remap(blot.outputs["Fac"], 0.35, 0.68, 0.80, 1.06)
    sp = node("ShaderNodeTexVoronoi", Vector=coord, Scale=34.0)
    spots = remap(sp.outputs["Distance"], 0.06, 0.20, 0.80, 1.0)
    vor = node("ShaderNodeTexVoronoi", Vector=coord, Scale=120.0)
    vor.feature = "DISTANCE_TO_EDGE"
    scales = remap(vor.outputs["Distance"], 0.0, 0.10, 0.62, 1.04)
    grain_n = node("ShaderNodeTexNoise", Vector=coord, Scale=90.0, Detail=2.0)
    grain = remap(grain_n.outputs["Fac"], 0.3, 0.7, 0.95, 1.03)
    d = mul(mixf(sep.outputs[1], 1.0, mottle), mixf(sep.outputs[1], 1.0, spots))
    d = mul(d, mixf(sep.outputs[0], 1.0, scales))
    d = mul(d, grain)
    c = node("ShaderNodeCombineColor")
    nt.links.new(d, c.inputs[0])
    nt.links.new(sep.outputs[2], c.inputs[1])
    return c.outputs[0]


def geo_pass(nt):
    """Red: pointiness (edges above a half, creases below); green: how much
    the surface faces up; blue: height up the body."""
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    sn = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(geo.outputs["Normal"], sn.inputs[0])
    sp = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(geo.outputs["Position"], sp.inputs[0])
    up = nt.nodes.new("ShaderNodeMapRange")
    nt.links.new(sn.outputs["Z"], up.inputs["Value"])
    up.inputs["From Min"].default_value = -1.0
    h = nt.nodes.new("ShaderNodeMapRange")
    nt.links.new(sp.outputs["Z"], h.inputs["Value"])
    h.inputs["From Max"].default_value = 1.9
    c = nt.nodes.new("ShaderNodeCombineColor")
    nt.links.new(geo.outputs["Pointiness"], c.inputs[0])
    nt.links.new(up.outputs["Result"], c.inputs[1])
    nt.links.new(h.outputs["Result"], c.inputs[2])
    return c.outputs[0]


def ao_pass(nt):
    ao = nt.nodes.new("ShaderNodeAmbientOcclusion")
    ao.only_local = True
    ao.samples = 24
    ao.inputs["Distance"].default_value = 0.10
    return ao.outputs["AO"]


def bake_images(low, high):
    import numpy as np

    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    targets = []
    for m in low.data.materials:
        node = m.node_tree.nodes.new("ShaderNodeTexImage")
        m.node_tree.nodes.active = node
        targets.append(node)
    passes = {
        "col": lambda nt: attr_out(nt, "col"),
        "mask": lambda nt: attr_out(nt, "mask"),
        "detail": detail_pass,
        "geo": geo_pass,
        "ao": ao_pass,
    }
    out = {}
    for name in list(passes) + ["normal"]:
        img = bpy.data.images.new(f"{NAME}.{name}", TEX, TEX, float_buffer=True)
        img.colorspace_settings.name = "Non-Color"
        for t in targets:
            t.image = img
        if name == "normal":
            kind = "NORMAL"
        else:
            kind = "EMIT"
            mat = kit.emit_material(f"pass.{name}", passes[name])
            high.data.materials.clear()
            high.data.materials.append(mat)
        scene.cycles.samples = 16 if name == "ao" else 4
        bpy.ops.object.select_all(action="DESELECT")
        high.select_set(True)
        low.select_set(True)
        bpy.context.view_layer.objects.active = low
        bpy.ops.object.bake(type=kind, use_selected_to_active=True, cage_extrusion=0.01, max_ray_distance=0.04, margin=8)
        a = np.empty(TEX * TEX * 4, dtype=np.float32)
        img.pixels.foreach_get(a)
        out[name] = a.reshape(TEX, TEX, 4)
        print("baked", name, flush=True)
    for t in targets:
        t.id_data.nodes.remove(t)
    return out


def srgb(x):
    import numpy as np

    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, 12.92 * x, 1.055 * np.power(x, 1 / 2.4) - 0.055)


def compose(b, out_dir):
    """The painted texture and the tint texture from the baked passes:
    occlusion in the crevices, light from above, darker towards the feet,
    shadows leaning blue, a warm highlight on raised edges."""
    import numpy as np

    col = b["col"][..., :3]
    detail = b["detail"][..., 0:1]
    dark = b["detail"][..., 1:2]
    curv = b["geo"][..., 0:1]
    top = b["geo"][..., 1:2]
    h = b["geo"][..., 2:3]
    ao = b["ao"][..., 0:1]
    edge = np.clip((curv - 0.56) * 6.0, 0, 1)
    cavity = np.clip((0.48 - curv) * 7.0, 0, 1)
    occl = 0.22 + 0.78 * np.power(np.clip(ao, 0, 1), 1.5)
    light = 0.66 + 0.50 * top
    feet = 0.74 + 0.26 * np.clip(h / 0.55, 0, 1)
    s = occl * light * feet * (1 - 0.3 * cavity)
    cool = np.array([0.78, 0.84, 1.12], dtype=np.float32)
    tint = cool + (1 - cool) * np.clip(s, 0, 1)
    base = col * detail
    colour = base * s * tint + edge * 0.12 * (0.25 + base) * np.array([1.0, 0.94, 0.82], dtype=np.float32)
    shade = np.clip(s * detail * (1 - 0.75 * dark) + 0.12 * edge, 0, 1)

    def save(name, rgba):
        img = bpy.data.images.new(f"{NAME}.{name}.out", TEX, TEX, alpha=True)
        img.colorspace_settings.name = "Non-Color"
        img.pixels.foreach_set(np.ascontiguousarray(rgba, dtype=np.float32).ravel())
        img.scale(OUT_TEX, OUT_TEX)
        img.filepath_raw = str(out_dir / f"{NAME}.{name}.png")
        img.file_format = "PNG"
        img.save()
        return img

    one = np.ones_like(shade)
    save("color", np.concatenate([srgb(colour), one], axis=-1))
    save("paint", np.concatenate([b["mask"][..., :3], shade], axis=-1))
    n = b["normal"].copy()
    n[..., 3] = 1.0
    save("normal", n)


def game_materials(low, out_dir):
    """Three materials over one texture set: the tinted body, the gear (its
    own colours) and the eyes (glossy). The game tints `raptor.skin` from
    the paint texture; viewers show the painted colour."""
    color = bpy.data.images.load(str(out_dir / f"{NAME}.color.png"))
    normal = bpy.data.images.load(str(out_dir / f"{NAME}.normal.png"))
    normal.colorspace_settings.name = "Non-Color"
    mats = []
    for name, rough in (("skin", 0.62), ("gear", 0.72), ("eye", 0.12)):
        m = bpy.data.materials.new(f"{NAME}.{name}")
        m.use_nodes = True
        nt = m.node_tree
        bsdf = nt.nodes["Principled BSDF"]
        bsdf.inputs["Roughness"].default_value = rough
        tex = nt.nodes.new("ShaderNodeTexImage")
        tex.image = color
        nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
        ntex = nt.nodes.new("ShaderNodeTexImage")
        ntex.image = normal
        nmap = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(ntex.outputs["Color"], nmap.inputs["Color"])
        nt.links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
        mats.append(m)
    low.data.materials.clear()
    for m in mats:
        low.data.materials.append(m)
    pa = low.data.attributes["part"]
    for p in low.data.polygons:
        part = PARTS[pa.data[p.index].value]
        p.material_index = 2 if part == "eye" else (1 if part in GEAR else 0)


def build(out_dir):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    game = pieces(1)
    low, rig = rig_pieces(game)
    print(f"{NAME}: {tri_count(low)} triangles in the game model", flush=True)
    unwrap(low)
    import time

    t = time.time()
    high_parts = pieces(3, high=True)
    print(f"subdivided in {time.time() - t:.0f} s", flush=True)
    high = kit.join([o for o, _ in high_parts], f"{NAME}.high")
    print(f"joined in {time.time() - t:.0f} s", flush=True)
    paint_mesh(high)
    print(f"painted in {time.time() - t:.0f} s", flush=True)
    print(f"{NAME}: {tri_count(high)} triangles baked from", flush=True)
    bake_mat = bpy.data.materials.new("bake")
    bake_mat.use_nodes = True
    low.data.materials.append(bake_mat)
    rig.data.pose_position = "REST"
    bpy.context.view_layer.update()
    baked = bake_images(low, high)
    bpy.data.objects.remove(high)
    compose(baked, out_dir)
    game_materials(low, out_dir)
    rig.data.pose_position = "POSE"
    kit.animate(rig)
    bpy.ops.object.select_all(action="DESELECT")
    low.select_set(True)
    rig.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=str(out_dir / f"{NAME}.glb"),
        export_format="GLB",
        export_animations=True,
        use_selection=True,
        # The textures sit beside the model (raptor.color/paint/normal.png).
        export_image_format="NONE",
    )
    return low, rig


VIEWS = {
    "front": (0, 3.4, 1.15, (0, 0, 0.98), 50),
    "three": (35, 3.4, 1.3, (0, 0, 0.98), 50),
    "side": (90, 3.4, 1.2, (0, 0, 0.98), 50),
    "back": (180, 3.4, 1.4, (0, 0, 0.98), 50),
    "face": (35, 2.4, 1.78, (0, -0.2, 1.68), 85),
    "walk": (60, 3.4, 1.25, (0, 0, 0.98), 50),
}


def render(names, samples=32, prefix=f"{NAME}2"):
    scene, cam = kit.stage()
    scene.cycles.samples = samples
    scene.cycles.use_denoising = True
    for n in names:
        a, dist, h, target, lens = VIEWS[n]
        kit.shoot(scene, cam, f"{prefix}-{n}", a, dist, h, target, lens, (640, 960), 9 if n == "walk" else 1)


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.endswith(".py")]
    out = Path(next((a for a in args if not a.startswith("--")), "out"))
    out.mkdir(parents=True, exist_ok=True)
    kit.OUT = out
    views = [v for v in VIEWS if v != "walk"]
    for a in args:
        if a.startswith("--views="):
            views = a.split("=", 1)[1].split(",")
    if "--preview" in args:
        build_preview()
    else:
        low, rig = build(out)
        rig.animation_data.action = bpy.data.actions["walk" if "walk" in views else "idle"]
    render(views)
