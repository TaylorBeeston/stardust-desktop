#!/usr/bin/env python3
"""Generate the desk toys' glTF models + procedural textures (needs numpy + Pillow).

Lines in Stardust always render order-independent-transparent, so anything solid is a model.
Usage: tools/gen_models.py   (writes data/io.github.TaylorBeeston.DeskToys/*.glb)

Dimensions mirror the constants in src/bin/*.rs (BOARD_R, POST_H, R, ...): keep them in sync.
"""
import io, json, math, struct, pathlib
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

OUT = pathlib.Path(__file__).resolve().parent.parent / "data" / "io.github.TaylorBeeston.DeskToys"
TAU = 2 * math.pi

# ---------------------------------------------------------------- glb writer
def glb(path, prims):
    """prims: list of dicts {P, N, UV|None, I, mat:{...}, image: PIL.Image|None, emissive: float}"""
    blob = bytearray()
    views, accessors, meshes_prims, materials, images, textures = [], [], [], [], [], []

    def view(data, target=None):
        while len(blob) % 4:
            blob.append(0)
        v = {"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)}
        if target:
            v["target"] = target
        blob.extend(data)
        views.append(v)
        return len(views) - 1

    def acc(data, fmt, count, typ, ctype=5126, target=34962, minmax=None):
        a = {"bufferView": view(data, target), "componentType": ctype, "count": count, "type": typ}
        if minmax:
            a["min"], a["max"] = minmax
        accessors.append(a)
        return len(accessors) - 1

    for p in prims:
        P, N, UV, I = p["P"], p["N"], p.get("UV"), p["I"]
        attrs = {
            "POSITION": acc(b"".join(struct.pack("<3f", *v) for v in P), "3f", len(P), "VEC3",
                            minmax=([min(v[i] for v in P) for i in range(3)], [max(v[i] for v in P) for i in range(3)])),
            "NORMAL": acc(b"".join(struct.pack("<3f", *v) for v in N), "3f", len(N), "VEC3"),
        }
        # baked light (see hemi): multiplied into base color by the renderer, free at runtime
        C = [hemi(n, pt, p.get("grad")) for n, pt in zip(N, P)]
        attrs["COLOR_0"] = acc(b"".join(struct.pack("<4f", c, c, c, 1.0) for c in C), "4f", len(C), "VEC4")
        if UV:
            attrs["TEXCOORD_0"] = acc(b"".join(struct.pack("<2f", *v) for v in UV), "2f", len(UV), "VEC2")
        idx = acc(b"".join(struct.pack("<I", i) for i in I), "I", len(I), "SCALAR", ctype=5125, target=34963)
        m = dict(p["mat"])
        pbr = m.setdefault("pbrMetallicRoughness", {})
        m["doubleSided"] = True
        if p.get("image") is not None:
            buf = io.BytesIO()
            p["image"].convert("RGB").save(buf, "PNG", optimize=True)
            images.append({"bufferView": view(bytes(buf.getvalue())), "mimeType": "image/png"})
            textures.append({"source": len(images) - 1, "sampler": 0})
            pbr["baseColorTexture"] = {"index": len(textures) - 1}
            if p.get("emissive"):
                m["emissiveTexture"] = {"index": len(textures) - 1}
                m["emissiveFactor"] = [p["emissive"]] * 3
        materials.append(m)
        meshes_prims.append({"attributes": attrs, "indices": idx, "material": len(materials) - 1})

    gltf = {
        "asset": {"version": "2.0", "generator": "desk-toys gen_models.py"},
        "scene": 0, "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": path.stem}],
        "meshes": [{"primitives": meshes_prims}],
        "materials": materials,
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": views, "accessors": accessors,
    }
    if images:
        gltf.update(images=images, textures=textures,
                    samplers=[{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}])
    js = json.dumps(gltf, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    while len(blob) % 4:
        blob.append(0)
    body = struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(blob), 0x004E4942) + bytes(blob)
    path.write_bytes(struct.pack("<III", 0x46546C67, 2, 12 + len(body)) + body)
    print(f"wrote {path.name}: {sum(len(p['P']) for p in prims)} verts, {path.stat().st_size // 1024} KiB")

# Rudimentary lighting, baked per vertex. Stardust's default scene light is a flat ambient
# (same from every direction), so without this every surface is exactly its texture: flat and
# glowy. Light comes from straight above (hemisphere: up-facing full, sides ~70%, underside
# ~40%), which stays right however a toy is yawed and while the sign spins about Y.
SKY, GROUND = 1.0, 0.4

def hemi(n, p, grad=None):
    up = 0.5 + 0.5 * n[1]
    shade = GROUND + (SKY - GROUND) * up
    if grad:  # (y0, y1): extra top-to-bottom falloff across a flat face
        y0, y1 = grad
        t = min(max((p[1] - y0) / (y1 - y0), 0.0), 1.0)
        shade *= 0.8 + 0.2 * t
    return shade

def mat(color, metallic=0.0, roughness=0.5, name="m"):
    return {"name": name, "pbrMetallicRoughness": {
        "baseColorFactor": list(color) + ([1.0] if len(color) == 3 else []),
        "metallicFactor": metallic, "roughnessFactor": roughness}}

# ---------------------------------------------------------------- meshes
def cap(radius, z, nz, segs=72, mirror_u=False):
    """Flat disc in XY at height z facing nz, UV = planar projection (u mirrored for the back)."""
    P, N, UV, I = [(0, 0, z)], [(0, 0, nz)], [(0.5, 0.5)], []
    for s in range(segs):
        a = TAU * s / segs
        x, y = math.cos(a), math.sin(a)
        P.append((radius * x, radius * y, z)); N.append((0, 0, nz))
        UV.append((0.5 + (-x if mirror_u else x) / 2, 0.5 - y / 2))
    for s in range(segs):
        a, b = 1 + s, 1 + (s + 1) % segs
        I += (0, a, b) if nz > 0 else (0, b, a)
    return P, N, UV, I

def frustum(r_bottom, r_top, y0, y1, segs=64, caps=True):
    """Solid of revolution about Y (cylinder when radii match)."""
    P, N, UV, I = [], [], [], []
    slope = (r_bottom - r_top) / (y1 - y0)
    for s in range(segs + 1):
        a = TAU * s / segs
        x, z = math.cos(a), math.sin(a)
        n = np.array([x, slope, z]); n /= np.linalg.norm(n)
        P += [(r_bottom * x, y0, r_bottom * z), (r_top * x, y1, r_top * z)]; N += [tuple(n)] * 2
        UV += [(s / segs, 1.0), (s / segs, 0.0)]
    for s in range(segs):
        a = 2 * s
        I += (a, a + 2, a + 1, a + 1, a + 2, a + 3)
    if caps:
        for y, r, ny in ((y0, r_bottom, -1.0), (y1, r_top, 1.0)):
            c = len(P); P.append((0, y, 0)); N.append((0, ny, 0)); UV.append((0.5, 0.02 if ny > 0 else 0.98))
            for s in range(segs):
                a = TAU * s / segs
                P.append((r * math.cos(a), y, r * math.sin(a))); N.append((0, ny, 0)); UV.append((s / segs, 0.02 if ny > 0 else 0.98))
            for s in range(segs):
                i, j = c + 1 + s, c + 1 + (s + 1) % segs
                I += (c, j, i) if ny > 0 else (c, i, j)
    return P, N, UV, I

def torus_xy(major, minor, segs=96, sides=16):
    P, N, UV, I = [], [], [], []
    for s in range(segs):
        a = TAU * s / segs
        cx, cy = math.cos(a), math.sin(a)
        for t in range(sides):
            b = TAU * t / sides
            n = (cx * math.cos(b), cy * math.cos(b), math.sin(b))
            P.append(((major + minor * math.cos(b)) * cx, (major + minor * math.cos(b)) * cy, minor * math.sin(b)))
            # 1 - v: Stardust shows glTF -Z toward the viewer, so put the "sky" half there
            N.append(n); UV.append((s / segs, 1 - t / sides))
    for s in range(segs):
        for t in range(sides):
            a = s * sides + t; b = ((s + 1) % segs) * sides + t
            c = s * sides + (t + 1) % sides; d = ((s + 1) % segs) * sides + (t + 1) % sides
            I += (a, b, c, c, b, d)
    return P, N, UV, I

def sphere(radius, rings=24, segs=36):
    P, N, UV, I = [], [], [], []
    for r in range(rings + 1):
        th = math.pi * r / rings
        for s in range(segs + 1):
            ph = TAU * s / segs
            n = (math.sin(th) * math.cos(ph), math.cos(th), math.sin(th) * math.sin(ph))
            P.append(tuple(radius * c for c in n)); N.append(n); UV.append((s / segs, r / rings))
    for r in range(rings):
        for s in range(segs):
            a = r * (segs + 1) + s; b = a + segs + 1
            I += (a, b, a + 1, a + 1, b, b + 1)
    return P, N, UV, I

def box(sx, sy, sz, uv_scale=1.0):
    """Axis-aligned box centred on the origin, per-face UVs in metres * uv_scale."""
    P, N, UV, I = [], [], [], []
    hx, hy, hz = sx / 2, sy / 2, sz / 2
    faces = [((1, 0, 0), (0, 0, -1), (0, 1, 0), hx, sz, sy), ((-1, 0, 0), (0, 0, 1), (0, 1, 0), hx, sz, sy),
             ((0, 1, 0), (1, 0, 0), (0, 0, -1), hy, sx, sz), ((0, -1, 0), (1, 0, 0), (0, 0, 1), hy, sx, sz),
             ((0, 0, 1), (1, 0, 0), (0, 1, 0), hz, sx, sy), ((0, 0, -1), (-1, 0, 0), (0, 1, 0), hz, sx, sy)]
    for n, u, v, d, w, h in faces:
        c = len(P)
        for a, b in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            P.append(tuple(n[i] * d + u[i] * a * w / 2 + v[i] * b * h / 2 for i in range(3)))
            N.append(n); UV.append(((a * w / 2 + w / 2) * uv_scale, (b * h / 2 + h / 2) * uv_scale))
        I += (c, c + 1, c + 2, c, c + 2, c + 3)
    return P, N, UV, I

# ---------------------------------------------------------------- textures
S = 1024

def sunburst(c1, c2, rays=16):
    img = Image.new("RGB", (S, S), c1)
    d = ImageDraw.Draw(img)
    for k in range(rays):
        a0, a1 = TAU * k / rays, TAU * (k + 0.5) / rays
        d.polygon([(S / 2, S / 2), (S / 2 + S * math.cos(a0), S / 2 + S * math.sin(a0)),
                   (S / 2 + S * math.cos(a1), S / 2 + S * math.sin(a1))], fill=c2)
    # soft centre glow + darker edge so it reads as a curved, lit face
    yy, xx = np.mgrid[0:S, 0:S]
    r = np.hypot(xx - S / 2, yy - S / 2) / (S / 2)
    arr = np.asarray(img).astype(np.float32)
    shade = (1.25 - 0.45 * np.clip(r, 0, 1) ** 1.5)[..., None]
    return Image.fromarray(np.clip(arr * shade, 0, 255).astype(np.uint8))

def star_points(cx, cy, ro, ri, n=5, rot=-math.pi / 2):
    return [(cx + (ro if k % 2 == 0 else ri) * math.cos(rot + k * math.pi / n),
             cy + (ro if k % 2 == 0 else ri) * math.sin(rot + k * math.pi / n)) for k in range(2 * n)]

def front_face():
    img = sunburst((22, 70, 200), (40, 110, 235))
    d = ImageDraw.Draw(img)
    c = S / 2
    shadow = Image.new("L", (S, S), 0)
    ImageDraw.Draw(shadow).polygon(star_points(c + 14, c + 20, S * 0.40, S * 0.17), fill=170)
    img.paste((10, 25, 80), mask=shadow.filter(ImageFilter.GaussianBlur(18)))
    d.polygon(star_points(c, c, S * 0.42, S * 0.18), fill=(255, 255, 255))
    d.polygon(star_points(c, c, S * 0.37, S * 0.155), fill=(255, 196, 30))
    d.polygon(star_points(c - 8, c - 10, S * 0.24, S * 0.10), fill=(255, 226, 110))  # highlight
    return img

def back_face():
    img = sunburst((215, 45, 40), (240, 95, 45))
    d = ImageDraw.Draw(img)
    c, R = S / 2, S * 0.34
    d.ellipse([c - R - 14, c - R - 14, c + R + 14, c + R + 14], fill=(255, 255, 255))
    d.ellipse([c - R, c - R, c + R, c + R], fill=(255, 205, 40))
    d.ellipse([c - R * 0.8, c - R * 0.85, c + R * 0.3, c - R * 0.1], fill=(255, 225, 110))  # highlight
    for ex in (-0.36, 0.36):
        d.ellipse([c + ex * R - R * 0.11, c - R * 0.42, c + ex * R + R * 0.11, c - R * 0.04], fill=(40, 25, 20))
        d.ellipse([c + ex * R - R * 0.03, c - R * 0.36, c + ex * R + R * 0.05, c - R * 0.24], fill=(255, 255, 255))
    d.arc([c - R * 0.62, c - R * 0.55, c + R * 0.62, c + R * 0.62], 20, 160, fill=(40, 25, 20), width=int(S * 0.035))
    return img

def wood(w=1024, h=512, seed=7):
    rng = np.random.default_rng(seed)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    # low-frequency warp, then rings across the plank
    noise = np.zeros((h, w), np.float32)
    for f, a in ((1.3, 14), (2.7, 6), (6.1, 2)):
        ph = rng.uniform(0, TAU, 4)
        noise += a * np.sin(xx / w * TAU * f + ph[0] + yy / h * 2.0 * ph[1])
    rings = (np.sin((yy + noise) / h * TAU * 26) + 1) / 2
    grain = rng.normal(0, 1, (h, w)).astype(np.float32)
    grain = np.asarray(Image.fromarray(((grain * 20) + 128).clip(0, 255).astype(np.uint8)).resize((w // 8, h)).resize((w, h)), np.float32) / 255
    t = (0.45 * rings ** 4 + 0.35 * grain)[..., None]
    light, dark = np.array([128, 78, 44]), np.array([58, 32, 18])
    return Image.fromarray((light * (1 - t) + dark * t).clip(0, 255).astype(np.uint8))

def studio(tint=(1.0, 1.0, 1.0), w=512, h=256):
    """Fake environment reflection for metal, indexed (u = around, v = top->bottom): bright sky,
    two softboxes, a sharp dark horizon, a warm desk below. There's no reflection probe in a
    typical Stardust scene, so real metallic PBR renders flat; this bakes the look in."""
    v = np.linspace(0, 1, h)[:, None]
    u = np.linspace(0, 1, w)[None, :]
    sky = np.array([0.93, 0.96, 1.0]) * (1.0 - 0.35 * v / 0.5)[..., None]
    desk = np.array([0.42, 0.30, 0.22]) * (0.9 - 0.6 * (v - 0.5) / 0.5)[..., None]
    img = np.where((v < 0.5)[..., None], sky, desk) * np.ones((h, w, 1))
    horizon = np.exp(-((v - 0.5) / 0.025) ** 2)[..., None]
    img = img * (1 - 0.85 * horizon)
    for cu, cv, su, sv in ((0.15, 0.25, 0.05, 0.08), (0.62, 0.18, 0.08, 0.05), (0.85, 0.32, 0.03, 0.12)):
        du = np.minimum(abs(u - cu), 1 - abs(u - cu))
        box = np.exp(-((du / su) ** 8 + ((v - cv) / sv) ** 8))
        img = img + box[..., None] * 0.9
    img = np.clip(img * np.array(tint), 0, 1)
    return Image.fromarray((img * 255).astype(np.uint8))

def uvless(P, N, UV, I, tex=None, emissive=0.0):
    d = {"P": P, "N": N, "I": I}
    if tex is not None:
        d.update(UV=UV, image=tex, emissive=emissive)
    return d

# ---------------------------------------------------------------- models
# baked-reflection "metals": dielectric + texture + a little glow so they read in any lighting
# fairly rough on purpose: the shine is baked into the textures, and low roughness made the
# flat ambient light paint a white Fresnel sheen on every grazing edge
CHROME = mat((1, 1, 1), 0.0, 0.7, "chrome")
GOLD = mat((1, 1, 1), 0.0, 0.7, "gold")
# toned down: full-white sky + glow read as glaring in the headset
CHROME_TEX = studio((0.85, 0.86, 0.92))
GOLD_TEX = studio((1.0, 0.72, 0.25))
chrome = lambda geo: {**uvless(*geo, tex=CHROME_TEX), "mat": CHROME}
gold = lambda geo: {**uvless(*geo, tex=GOLD_TEX), "mat": GOLD}
# rods/posts: UV u runs around them, so the studio's bands must vary along u
def rod_tex(w=256, h=16):
    """Brushed steel for thin rods, varying around the rod (u): mid grey with two highlight
    streaks and one soft dark band, so every side reads as metal (the studio map put a dark
    "desk" half on whichever side faced you)."""
    u = np.linspace(0, 1, w)
    g = 0.62 + 0.0 * u
    for c, width, amp in ((0.12, 0.035, 0.35), (0.55, 0.06, 0.2), (0.8, 0.12, -0.22)):
        d = np.minimum(abs(u - c), 1 - abs(u - c))
        g = g + amp * np.exp(-(d / width) ** 2)
    rgb = np.clip(g[None, :, None] * np.array([0.95, 0.96, 1.0]), 0, 1) * np.ones((h, 1, 1))
    return Image.fromarray((rgb * 255).astype(np.uint8))

ROD_TEX = rod_tex()
chrome_rod = lambda geo: {**uvless(*geo, tex=ROD_TEX), "mat": CHROME}

def spin_sign():
    BOARD_R, T, POST_H = 0.06, 0.006, 0.14
    r = BOARD_R - 0.003
    side_P, side_N, side_UV, side_I = frustum(r, r, -T / 2, T / 2, caps=False)
    # rotate the wall from Y-axis to Z-axis (disc lies in XY)
    side_P = [(x, -z, y) for x, y, z in side_P]; side_N = [(x, -z, y) for x, y, z in side_N]
    fP, fN, fUV, fI = cap(r, T / 2, 1.0)
    bP, bN, bUV, bI = cap(r, -T / 2, -1.0, mirror_u=True)
    face = lambda img: mat((1, 1, 1), 0.0, 0.75, "face")
    glb(OUT / "sign_board.glb", [
        # Stardust shows a model's glTF -Z side toward the client's +Z (verified: the star cap
        # at glTF +Z appeared on the back), so the front art goes on the -Z cap, u-mirrored.
        {"P": bP, "N": bN, "UV": bUV, "I": bI, "mat": face(0), "image": front_face(), "emissive": 0.06, "grad": (-r, r)},
        {"P": fP, "N": fN, "UV": fUV, "I": fI, "mat": face(0), "image": back_face(), "emissive": 0.06, "grad": (-r, r)},
        gold((side_P, side_N, side_UV, side_I)),
        gold(torus_xy(BOARD_R - 0.001, 0.0045)),
    ])
    base = frustum(0.042, 0.034, 0.0, 0.011)
    glb(OUT / "sign_stand.glb", [
        {**uvless(*base, tex=studio((0.12, 0.2, 0.55))), "mat": mat((1, 1, 1), 0.0, 0.7, "navy gloss")},
        chrome_rod(frustum(0.008, 0.006, 0.011, 0.02)),
        chrome_rod(frustum(0.0042, 0.0042, 0.011, POST_H + 0.004)),
        gold([[(x, y + POST_H + 0.004, z) for x, y, z in g] if k == 0 else g
              for k, g in enumerate(sphere(0.0065, 12, 18))]),
    ])

def cradle():
    WIDTH, BAR_Z, PIVOT_Y, R = 0.30, 0.028, 0.15, 0.012
    glb(OUT / "ball.glb", [chrome(sphere(R))])
    # unit rod along +Y from 0..1, radius 1: scaled per segment at runtime
    glb(OUT / "rod.glb", [chrome_rod(frustum(1.0, 1.0, 0.0, 1.0, segs=24))])
    bP, bN, bUV, bI = box(WIDTH + 0.03, 0.014, 2 * BAR_Z + 0.04, uv_scale=3.0)
    bP = [(x, y - 0.007, z) for x, y, z in bP]
    feet = []
    for sx in (-1, 1):
        for sz in (-1, 1):
            P, N, UV, I = frustum(0.008, 0.007, -0.017, -0.014)
            feet.append({**uvless([(x + sx * (WIDTH / 2 - 0.0), y, z + sz * (BAR_Z + 0.01)) for x, y, z in P], N, UV, I),
                         "mat": mat((0.05, 0.05, 0.05), 0.0, 0.8, "rubber")})
    glb(OUT / "cradle_base.glb", [
        {"P": bP, "N": bN, "UV": bUV, "I": bI, "mat": mat((1, 1, 1), 0.0, 0.75, "walnut"), "image": wood()},
        *feet,
    ])

if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    spin_sign()
    cradle()
