"""Generates the raw AI textures for the texture trial through the Gemini API.

    python tools/texture-ai/generate.py [NAME ...]

Textures listed under `variants` also get `<name>.v1` to `.v3`: new
patches of the same material painted with the base as reference.

Reads `prompts.json` beside this file (the shared style prompt plus one line
per texture), asks the image model for each texture named (all of them by
default), and writes the image and what was asked into
`art/texture-trial/ai-raw/<name>.<ext>` and `<name>.json`. A texture with a `base`
(one name or a list) is made from those raw images (copper ore is set into the AI
stone, so the host rock matches).

The key: in a Claude cloud environment the proxy adds it on the way out, so
nothing is needed. Elsewhere set GEMINI_API_KEY. Never commit a key.

See docs/proposals/textures-128.md.
"""

import base64
import datetime
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
RAW = ROOT / "art" / "texture-trial" / "ai-raw"
API = "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
MIME_EXT = {"image/png": "png", "image/jpeg": "jpg", "image/webp": "webp"}


def find_raw(name):
    """The model's original image for `name` (not a healed copy)."""
    for ext in MIME_EXT.values():
        p = RAW / f"{name}.{ext}"
        if p.exists():
            return p
    return None


def call(model, parts, image_size):
    body = {
        "contents": [{"parts": parts}],
        "generationConfig": {
            "responseModalities": ["IMAGE"],
            "imageConfig": {"aspectRatio": "1:1", "imageSize": image_size},
        },
    }
    req = urllib.request.Request(
        API.format(model=model),
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    key = os.environ.get("GEMINI_API_KEY")
    if key:
        req.add_header("x-goog-api-key", key)
    # Busy or rate-limited answers are retried with a growing wait.
    for attempt in range(6):
        try:
            with urllib.request.urlopen(req, timeout=300) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            text = e.read().decode(errors="replace")[:600]
            if e.code in (429, 500, 502, 503, 504) and attempt < 5:
                wait = 10 * 2**attempt
                print(f"  {model}: HTTP {e.code}, retrying in {wait} s", flush=True)
                time.sleep(wait)
                continue
            raise SystemExit(f"{model}: HTTP {e.code}: {text}")
        except (urllib.error.URLError, TimeoutError) as e:
            if attempt < 5:
                wait = 10 * 2**attempt
                print(f"  {model}: {e}, retrying in {wait} s", flush=True)
                time.sleep(wait)
                continue
            raise SystemExit(f"{model}: {e}")


def generate(cfg, name):
    entry = cfg["textures"][name]
    if isinstance(entry, str):
        entry = {"prompt": entry}
    prompt = cfg["style"] + "\n\n" + entry["prompt"]
    bases = entry.get("base", [])
    bases = [bases] if isinstance(bases, str) else bases
    parts = []
    for b in bases:
        path = find_raw(b)
        if path is None:
            raise SystemExit(f"{name} uses {b}, which has no raw image yet")
        parts.append(inline(path))
    parts.append({"text": prompt})
    save_reply(cfg, name, name, prompt, call(cfg["model"], parts, cfg.get("image_size", "1K")), bases)


def inline(path):
    mime = {"png": "image/png", "jpg": "image/jpeg", "webp": "image/webp"}[path.suffix[1:]]
    return {"inlineData": {"mimeType": mime, "data": base64.b64encode(path.read_bytes()).decode()}}


def generate_variant(cfg, name, i):
    """Variant `i` of `name`: a new patch of the same material, painted with
    the base image as the reference (cleanup.py joins its middle into the
    base, so all variants share the base's edges)."""
    base = find_raw(name)
    if base is None:
        raise SystemExit(f"{name} has no raw image to make variants from")
    prompt = cfg["style"] + "\n\n" + cfg["variants"]["prompt"]
    reply = call(cfg["model"], [inline(base), {"text": prompt}], cfg.get("image_size", "1K"))
    save_reply(cfg, name, f"{name}.v{i}", prompt, reply, [name])


def save_reply(cfg, name, stem, prompt, reply, bases):
    out_parts = reply.get("candidates", [{}])[0].get("content", {}).get("parts", [])
    images = [p["inlineData"] for p in out_parts if "inlineData" in p]
    if not images:
        raise SystemExit(f"{name}: no image in the reply: {json.dumps(reply)[:600]}")
    img = images[-1]
    ext = MIME_EXT.get(img["mimeType"], "bin")
    RAW.mkdir(parents=True, exist_ok=True)
    # A new base makes its variants stale; a new variant replaces only itself.
    stale = RAW.glob(f"{name}.*") if stem == name else RAW.glob(f"{stem}.*")
    for old in stale:
        old.unlink()
    path = RAW / f"{stem}.{ext}"
    path.write_bytes(base64.b64decode(img["data"]))
    meta = {
        "texture": stem,
        "model": cfg["model"],
        "image_size": cfg.get("image_size"),
        "made": datetime.date.today().isoformat(),
        "uses": bases,
        "prompt": prompt,
        "model_text": [p["text"] for p in out_parts if "text" in p],
    }
    (RAW / f"{stem}.json").write_text(json.dumps(meta, indent=2) + "\n")
    print(f"{stem}: {path.relative_to(ROOT)}", flush=True)


def main():
    cfg = json.loads((HERE / "prompts.json").read_text())
    names = sys.argv[1:] or list(cfg["textures"])
    # Bases first, so an edit always sees the images it uses.
    names.sort(key=lambda n: isinstance(cfg["textures"].get(n), dict) and "base" in cfg["textures"][n])
    var = cfg.get("variants", {})
    for name in names:
        if name not in cfg["textures"]:
            raise SystemExit(f"no prompt for {name}")
        generate(cfg, name)
        if name in var.get("names", []):
            for i in range(1, var.get("count", 0) + 1):
                generate_variant(cfg, name, i)


if __name__ == "__main__":
    main()
