"""Turns the raw AI images of the texture trial into game textures.

    python tools/texture-ai/cleanup.py [--size 128] [--out DIR] [NAME ...]

For each texture with a raw image in `art/texture-trial/ai-raw/`:

1. crops it square: the whole image becomes one face, as Gemini drew it
   (Nathan, 2026-10-22: the AI's own design and colours, not ours);
2. makes it tile seamlessly: the strip just past each edge is joined into
   the opposite edge along the cut where the two agree best (image
   quilting), so stones and joints carry on across the wrap;
3. downsizes to `--size` pixels (128 by default);
4. with `--nudge` only, moves its colours part of the way toward the
   painted 32's (mean lightness, hue, colourfulness); off by default;
5. variants: where Gemini painted extra patches (`<name>.v1` to `.v3`),
   sets each one's middle into the base along four least-visible cuts in
   the edge band, so every variant has the base's edges and any variant
   sits beside any other without a seam (`--variants` instead cuts four
   smaller faces from one image, cross-fading their edge bands);
6. checks the result still reads when the game recolours it by brightness
   (ADR-0007, `bm-client/src/colour.rs`): its light and shade pattern must
   be about as strong as the painted texture's, with little clipped.

Writes `DIR/blocks/<name>.png` (and `_1` to `_3`), plus a review sheet and
a report in `art/texture-trial/`. Needs Pillow and numpy.

See docs/proposals/textures-128.md.
"""

import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
TRIAL = ROOT / "art" / "texture-trial"
RAW = TRIAL / "ai-raw"
PAINTED = ROOT / "assets" / "textures" / "blocks"

# Textures the game gives variants (bm-texgen `VARIED`).
VARIED = {"stone", "deep_stone", "soil", "turf_top", "sand", "gravel", "snow", "foliage"}

# Working size of the raw image, and how much of it one face covers: all of
# it but the strip that is cross-faded into the opposite edge.
WORK = 1024
OVERLAP = 128
FACE = WORK - OVERLAP
# With --variants: smaller faces, one per quadrant.
VAR_OVERLAP = 64
VAR_FACE = 448

# How far the colours move toward the painted texture's (0 none, 1 all).
NUDGE_HUE = 0.6
NUDGE_LIGHT = 0.5
NUDGE_CHROMA = 0.5

# The edge band variants share with the base, as a share of the face
# (bm-texgen's 3 and 9 pixels of 32).
BAND_SOLID = 3 / 32
BAND_FREE = 9 / 32

# The recolour check: detail (texel luma over face luma) must spread at
# least this share of the painted texture's spread, at most this many times
# it, with at most this share of texels clipped by the shader.
SPREAD_MIN = 0.5
SPREAD_MAX = 2.0
CLIP_MAX = 0.05
DETAIL_MIN, DETAIL_MAX = 0.15, 3.0


# ------------------------------------------------------------ colour maths


def srgb_to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def to_oklab(rgb):
    """sRGB 0..1 (..., 3) to OKLab."""
    lin = srgb_to_linear(rgb)
    m1 = np.array([[0.4122214708, 0.5363325363, 0.0514459929],
                   [0.2119034982, 0.6806995451, 0.1073969566],
                   [0.0883024619, 0.2817188376, 0.6299787005]])
    lms = np.cbrt(lin @ m1.T)
    m2 = np.array([[0.2104542553, 0.7936177850, -0.0040720468],
                   [1.9779984951, -2.4285922050, 0.4505937099],
                   [0.0259040371, 0.7827717662, -0.8086757660]])
    return lms @ m2.T


def from_oklab(lab):
    m2i = np.array([[1.0, 0.3963377774, 0.2158037573],
                    [1.0, -0.1055613458, -0.0638541728],
                    [1.0, -0.0894841775, -1.2914855480]])
    lms = (lab @ m2i.T) ** 3
    m1i = np.array([[4.0767416621, -3.3077115913, 0.2309699292],
                    [-1.2684380046, 2.6097574011, -0.3413193965],
                    [-0.0041960863, -0.7034186147, 1.7076147010]])
    return linear_to_srgb(lms @ m1i.T)


def luma(rgb):
    lin = srgb_to_linear(rgb)
    return lin @ np.array([0.2126, 0.7152, 0.0722])


# ------------------------------------------------------------------ steps


def load(path, size=None):
    img = Image.open(path).convert("RGB")
    w, h = img.size
    s = min(w, h)
    img = img.crop(((w - s) // 2, (h - s) // 2, (w - s) // 2 + s, (h - s) // 2 + s))
    if size:
        img = img.resize((size, size), Image.LANCZOS)
    return np.asarray(img, dtype=np.float64) / 255.0


def smooth(t):
    t = np.clip(t, 0.0, 1.0)
    return t * t * (3 - 2 * t)


def cut_path(cost):
    """The cheapest top-to-bottom path through `cost` (rows x cols), moving at
    most one column per row: the column per row (image quilting's minimum
    error boundary cut)."""
    rows, cols = cost.shape
    acc = cost.copy()
    for y in range(1, rows):
        prev = acc[y - 1]
        left = np.concatenate([[np.inf], prev[:-1]])
        right = np.concatenate([prev[1:], [np.inf]])
        acc[y] += np.minimum(np.minimum(left, prev), right)
    path = np.zeros(rows, dtype=int)
    path[-1] = int(np.argmin(acc[-1]))
    for y in range(rows - 2, -1, -1):
        x = path[y + 1]
        lo, hi = max(0, x - 1), min(cols, x + 2)
        path[y] = lo + int(np.argmin(acc[y, lo:hi]))
    return path


def join(past, near):
    """Joins the strip just past the far edge (`past`) onto the strip at the
    near edge (`near`), both rows x cols x 3: left of a cut taken from `past`,
    right of it from `near`. The cut follows where the two agree best (dark
    joints between stones), with a 3-pixel feather, so it is invisible where
    a straight fade would ghost."""
    cost = ((past - near) ** 2).sum(-1)
    # Pin the cut away from the strip's ends, so each side keeps its own edge.
    cols = cost.shape[1]
    margin = max(2, cols // 8)
    cost[:, :margin] = 1e3
    cost[:, cols - margin:] = 1e3
    path = cut_path(cost)
    x = np.arange(cols)[None, :]
    w = smooth((x - path[:, None] + 1.5) / 3.0)[..., None]
    return past * (1 - w) + near * w


def seamless(crop, face, overlap):
    """A `face`-wide square from a (face + overlap) crop that tiles: the strip
    past each far edge is joined into the near edge along a minimum-error
    cut, so the wrap-around continues the image instead of cutting it."""
    h = crop[:, :face].copy()
    h[:, :overlap] = join(crop[:, face:face + overlap], crop[:, :overlap])
    t = h[:face].copy()
    t[:overlap] = join(h[face:face + overlap].transpose(1, 0, 2), h[:overlap].transpose(1, 0, 2)).transpose(1, 0, 2)
    return t


def resize(a, size):
    img = Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8))
    return np.asarray(img.resize((size, size), Image.LANCZOS), dtype=np.float64) / 255.0


def painted_faces(name):
    faces = []
    for stem in [name] + [f"{name}_{i}" for i in range(1, 4)]:
        p = PAINTED / f"{stem}.png"
        if p.exists():
            faces.append(np.asarray(Image.open(p).convert("RGB"), dtype=np.float64) / 255.0)
    return faces


def nudge(face, target):
    """Moves `face`'s mean lightness, mean hue and colourfulness part of the
    way toward `target`'s (both sRGB 0..1), keeping its own detail."""
    a, b = to_oklab(face), to_oklab(target)
    am, bm = a.reshape(-1, 3).mean(0), b.reshape(-1, 3).mean(0)
    out = a.copy()
    out[..., 0] += (bm[0] - am[0]) * NUDGE_LIGHT
    out[..., 1:] += (bm[1:] - am[1:]) * NUDGE_HUE
    # Colourfulness: scale chroma around the new mean.
    ac = np.hypot(*(a[..., 1:] - am[1:]).reshape(-1, 2).T).mean() + 1e-6
    bc = np.hypot(*(b[..., 1:] - bm[1:]).reshape(-1, 2).T).mean() + 1e-6
    k = (bc / ac) ** NUDGE_CHROMA
    mean_ab = out[..., 1:].reshape(-1, 2).mean(0)
    out[..., 1:] = mean_ab + (out[..., 1:] - mean_ab) * k
    return np.clip(from_oklab(out), 0.0, 1.0)


def band_mask(n, seed):
    """0 in a band along every edge, 1 in the middle, with a wobbly border so
    the change from base to variant follows no straight line."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:n, 0:n] + 0.5
    d = np.minimum.reduce([x, n - x, y, n - y]) / n
    # Low-frequency wobble that tiles (sums of whole-period sines).
    wob = np.zeros((n, n))
    for _ in range(4):
        fx, fy = rng.integers(1, 4, 2)
        ph = rng.uniform(0, 2 * np.pi)
        wob += np.sin(2 * np.pi * (fx * x + fy * y) / n + ph)
    wob *= 0.02
    return smooth((d + wob - BAND_SOLID) / (BAND_FREE - BAND_SOLID))


def recolour_spread(face):
    """How strongly the face's light and shade pattern survives the game's
    recolouring: the spread of detail (texel luma over face luma), and the
    share of texels the shader would clip."""
    l = luma(face)
    detail = l / max(l.mean(), 0.002)
    # Only the top clips detail away; below DETAIL_MIN a crack just stays dark.
    clipped = np.mean(detail > DETAIL_MAX)
    return float(detail.std()), float(clipped)


def recolour(face, rgb):
    """The shader's recolour (colour.rs `recolour`) for a preview."""
    lin_c = srgb_to_linear(np.array(rgb) / 255.0)
    l = luma(face)
    detail = np.clip(l / max(l.mean(), 0.002), DETAIL_MIN, DETAIL_MAX)
    out = np.maximum(lin_c[None, None, :] * detail[..., None] + 0.012 * (detail[..., None] - 1), 0)
    m = out.max(-1, keepdims=True)
    knee = 0.8
    eased = knee + (1 - knee) * (1 - np.exp(-(m - knee) / (1 - knee)))
    out = np.where(m > knee, out * eased / np.maximum(m, 1e-6), out)
    return linear_to_srgb(out)


# ------------------------------------------------------------------- main


def raw_image(stem):
    """The raw image saved for `stem` (`stone`, `stone.v1`), if any."""
    return next((p for p in sorted(RAW.glob(f"{stem}.*")) if p.suffix != ".json" and p.stem == stem), None)


def side_cut(base, other, d0, d1):
    """Weight (rows x cols) of `other` against `base` across the band
    d0..d1 pixels in from the left edge: 0 outside a cut that runs top to
    bottom where the two agree best, 1 inside it, a 3-pixel feather."""
    n = base.shape[0]
    cost = ((base[:, d0:d1] - other[:, d0:d1]) ** 2).sum(-1)
    margin = max(2, (d1 - d0) // 8)
    cost[:, :margin] = 1e3
    cost[:, -margin:] = 1e3
    path = d0 + cut_path(cost)
    x = np.arange(n)[None, :]
    return smooth((x - path[:, None] + 1.5) / 3.0)


def join_variant(base, other):
    """A variant: `other`'s middle set into `base` along four cuts in the
    edge band, so its edges are the base's and it sits beside any variant
    without a seam, with no cross-faded ghosting."""
    n = base.shape[0]
    d0, d1 = int(n * BAND_SOLID), int(n * BAND_FREE)
    w = np.ones((n, n))
    for k in range(4):
        # Rotate so the side being cut is on the left, cut, rotate back.
        b, o = np.rot90(base, k), np.rot90(other, k)
        w *= np.rot90(side_cut(b, o, d0, d1), -k)
    return base * (1 - w[..., None]) + other * w[..., None]


def clean(name, size, do_nudge=False, variants=False):
    raw_path = raw_image(name)
    if raw_path is None:
        return None
    raw = load(raw_path, WORK)
    target_faces = painted_faces(name)
    target = target_faces[0] if target_faces else None
    if variants and name in VARIED:
        face, overlap, count = VAR_FACE, VAR_OVERLAP, 4
    else:
        face, overlap, count = FACE, OVERLAP, 1
    span = face + overlap
    # One crop per quadrant (just one, the whole image, by default).
    corners = [(0, 0), (WORK - span, 0), (0, WORK - span), (WORK - span, WORK - span)]
    faces = []
    for (cx, cy) in corners[:count]:
        f = resize(seamless(raw[cy:cy + span, cx:cx + span], face, overlap), size)
        if do_nudge and target is not None:
            f = nudge(f, target)
        faces.append(f)
    for i in range(1, count):
        m = band_mask(size, i)[..., None]
        faces[i] = faces[0] * (1 - m) + faces[i] * m
    # Variants Gemini painted as separate patches (`<name>.v1` ...).
    if count == 1:
        for i in range(1, 4):
            vpath = raw_image(f"{name}.v{i}")
            if vpath is None:
                break
            v = resize(seamless(load(vpath, WORK), face, overlap), size)
            if do_nudge and target is not None:
                v = nudge(v, target)
            faces.append(join_variant(faces[0], v))
    spread, clipped = recolour_spread(faces[0])
    painted_spread = recolour_spread(target)[0] if target is not None else spread
    ratio = spread / max(painted_spread, 1e-6)
    ok = SPREAD_MIN <= ratio <= SPREAD_MAX and clipped <= CLIP_MAX
    return {
        "name": name,
        "raw": raw_path,
        "faces": faces,
        "target": target,
        "check": {
            "detail_spread": round(spread, 3),
            "painted_spread": round(painted_spread, 3),
            "ratio": round(ratio, 2),
            "clipped": round(clipped, 4),
            "reads_when_recoloured": bool(ok),
        },
    }


def save(a, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8)).save(path)


def sheet(results, size):
    """One row per texture: the raw image, the painted 32, the cleaned base,
    the base tiled 2 x 2 with its variants, and three recolours."""
    colours = [(147, 43, 42), (40, 96, 160), (200, 170, 60)]
    cell = 2 * size
    rows = []
    for r in results:
        raw = resize(load(r["raw"], WORK), cell)
        painted = np.asarray(
            Image.fromarray((r["target"] * 255).astype(np.uint8)).resize((cell, cell), Image.NEAREST),
            dtype=np.float64) / 255.0 if r["target"] is not None else np.zeros((cell, cell, 3))
        f = r["faces"]
        base = np.asarray(Image.fromarray((f[0] * 255 + 0.5).astype(np.uint8)).resize((cell, cell), Image.NEAREST),
                          dtype=np.float64) / 255.0
        v = lambda i: f[i % len(f)]
        quad = np.concatenate([np.concatenate([v(0), v(1)], 1), np.concatenate([v(2), v(3)], 1)], 0)
        tints = [resize(recolour(f[0], c), cell) for c in colours]
        rows.append(np.concatenate([raw, painted, base, quad] + tints, 1))
    gap = np.full((8, rows[0].shape[1], 3), 0.1)
    out = []
    for row in rows:
        out += [row, gap]
    return np.concatenate(out[:-1], 0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--size", type=int, default=128)
    ap.add_argument("--out", type=Path, default=TRIAL / "ai")
    ap.add_argument("--nudge", action="store_true", help="move colours toward the painted 32s")
    ap.add_argument("--variants", action="store_true", help="cut 3 variants from each varied texture")
    ap.add_argument("names", nargs="*")
    args = ap.parse_args()
    cfg = json.loads((HERE / "prompts.json").read_text())
    names = args.names or list(cfg["textures"])
    results = []
    for name in names:
        r = clean(name, args.size, args.nudge, args.variants)
        if r is None:
            print(f"{name}: no raw image (run generate.py first)", file=sys.stderr)
            continue
        for old in (args.out / "blocks").glob(f"{name}_*.png"):
            old.unlink()
        for i, face in enumerate(r["faces"]):
            stem = name if i == 0 else f"{name}_{i}"
            save(face, args.out / "blocks" / f"{stem}.png")
        c = r["check"]
        verdict = "reads when recoloured" if c["reads_when_recoloured"] else "WEAK when recoloured"
        print(f"{name:<12} {len(r['faces'])} face(s)  detail spread {c['detail_spread']} "
              f"(painted {c['painted_spread']}, x{c['ratio']}), clipped {c['clipped']:.2%}: {verdict}")
        results.append(r)
    if results:
        save(sheet(results, args.size), TRIAL / "ai-sheet.png")
        report = {r["name"]: r["check"] for r in results}
        (TRIAL / "ai-report.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
