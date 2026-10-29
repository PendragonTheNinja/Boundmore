"""Box modelling in code: a low-poly quad cage built ring by ring, with planned
edge loops, holes where limbs join, caps, and creases where a form should stay
crisp under a Subdivision Surface. Used by raptor2.py.

A ring is a list of vertex indices forming a closed loop. `bridge` joins two
rings of the same length with quads, choosing the rotation and direction that
lines them up best, so a limb can start from any hole without bookkeeping.
"""

import math

import bmesh
import bpy
from mathutils import Vector


def frame(t, up):
    """An orthonormal basis for a ring whose axis is `t`: X across, Y towards
    `up` (projected into the ring's plane), Z along the axis."""
    z = Vector(t).normalized()
    u = Vector(up)
    y = (u - z * u.dot(z))
    if y.length < 1e-6:
        y = Vector((0, 0, 1)) if abs(z.z) < 0.9 else Vector((0, 1, 0))
        y = y - z * y.dot(z)
    y.normalize()
    x = y.cross(z)
    return x, y, z


def superellipse(n, w, h, p=2.0, start=0.0, bulge=None):
    """`n` points round an ellipse of half-width `w` (X) and half-height `h`
    (Y); `p` above 2 squares it off. Point 0 is at +Y (the top), going towards
    +X. `bulge(angle)` scales the radius per point."""
    pts = []
    for i in range(n):
        a = start + 2 * math.pi * i / n
        s, c = math.sin(a), math.cos(a)
        x = w * math.copysign(abs(s) ** (2 / p), s)
        y = h * math.copysign(abs(c) ** (2 / p), c)
        k = bulge(a) if bulge else 1.0
        pts.append((x * k, y * k))
    return pts


class Cage:
    def __init__(self):
        self.v = []
        self.f = []
        self.crease = {}
        self.sharp = set()
        self.part = []  # per vertex: the part it belongs to
        self.grad = []  # per vertex: 0 at a piece's root to 1 at its tip

    # ---- building ---------------------------------------------------------

    def add(self, p, part="", grad=0.0):
        self.v.append(Vector(p))
        self.part.append(part)
        self.grad.append(grad)
        return len(self.v) - 1

    def ring(self, centre, axis, up, pts, part="", grad=0.0):
        """A ring of 2D points laid in the plane through `centre` across
        `axis`, with +Y of the points towards `up`."""
        x, y, _ = frame(axis, up)
        c = Vector(centre)
        return [self.add(c + x * a + y * b, part, grad) for a, b in pts]

    def centroid(self, r):
        return sum((self.v[i] for i in r), Vector()) / len(r)

    def align(self, a, b):
        """`b` reordered (rotated, maybe reversed) to pair best with `a`."""
        n = len(a)
        ca, cb = self.centroid(a), self.centroid(b)
        da = [(self.v[i] - ca).normalized() for i in a]
        db = [(self.v[i] - cb).normalized() for i in b]
        best, out = None, b
        for d in (1, -1):
            for s in range(n):
                idx = [(s + d * i) % n for i in range(n)]
                cost = sum((da[i] - db[idx[i]]).length for i in range(n))
                if best is None or cost < best:
                    best, out = cost, [b[k] for k in idx]
        return out

    def bridge(self, a, b, align=True):
        assert len(a) == len(b), (len(a), len(b))
        if align:
            b = self.align(a, b)
        n = len(a)
        for i in range(n):
            j = (i + 1) % n
            self.f.append((a[i], a[j], b[j], b[i]))
        return b

    def loft(self, start, rings, align=True):
        """Bridges `start` to each ring in turn; returns every ring."""
        out = [start]
        for r in rings:
            out.append(self.bridge(out[-1], r, align))
        return out

    def cap(self, r, centre=None, part=None, push=0.0):
        """Closes an even ring with quads round a new centre vertex."""
        n = len(r)
        if n == 4:
            self.f.append(tuple(r))
            return None
        c = self.centroid(r) if centre is None else Vector(centre)
        if push:
            # Push the centre out along the ring's normal (away from where
            # the ring came from is the caller's job; push follows `centre`).
            c = c + push
        k = self.add(c, part if part is not None else self.part[r[0]], self.grad[r[0]])
        for i in range(0, n, 2):
            self.f.append((r[i - 1], r[i], r[(i + 1) % n], k))
        return k

    def crease_loop(self, r, value=1.0, closed=True, sharp=False):
        n = len(r)
        for i in range(n if closed else n - 1):
            e = frozenset((r[i], r[(i + 1) % n]))
            self.crease[e] = max(self.crease.get(e, 0), value)
            if sharp:
                self.sharp.add(e)

    def crease_path(self, path, value=1.0, sharp=False):
        self.crease_loop(path, value, closed=False, sharp=sharp)

    # ---- output -----------------------------------------------------------

    def build(self, name, names=None):
        """The cage as a mesh object; `names` lists every part name (shared
        across objects so their `part` ids agree once joined)."""
        used = sorted({i for f in self.f for i in f})
        remap = {o: n for n, o in enumerate(used)}
        me = bpy.data.meshes.new(name)
        me.from_pydata([self.v[i] for i in used], [], [tuple(remap[i] for i in f) for f in self.f])
        me.validate()
        bm = bmesh.new()
        bm.from_mesh(me)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(me)
        bm.free()
        # recalc keeps face order, so self.f still lines up with polygons.
        assert len(me.polygons) == len(self.f)
        attr = me.attributes.new("crease_edge", "FLOAT", "EDGE")
        sharp = me.attributes.new("sharp_edge", "BOOLEAN", "EDGE")
        for e in me.edges:
            a, b = used[e.vertices[0]], used[e.vertices[1]]
            k = frozenset((a, b))
            attr.data[e.index].value = self.crease.get(k, 0.0)
            sharp.data[e.index].value = k in self.sharp
        for p in me.polygons:
            p.use_smooth = True
        # A face belongs to the part most of its corners belong to (ties go
        # to the newest corner, the limb growing out of a hole). Kept per
        # face, so subdivision never blends two parts.
        parts = me.attributes.new("part", "INT", "FACE")
        names = names if names is not None else sorted(set(self.part))
        for p, f in zip(me.polygons, self.f):
            votes = {}
            for i in f:
                votes[self.part[i]] = votes.get(self.part[i], 0) + 1
            best = max(votes.values())
            winner = [self.part[i] for i in sorted(f, reverse=True) if votes[self.part[i]] == best][0]
            parts.data[p.index].value = names.index(winner)
        g = me.attributes.new("grad", "FLOAT", "POINT")
        for n, o in enumerate(used):
            g.data[n].value = self.grad[o]
        o = bpy.data.objects.new(name, me)
        bpy.context.collection.objects.link(o)
        o["parts"] = names
        return o
