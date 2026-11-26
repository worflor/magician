"""Magician HUD top-bar test: a hero's portrait the way a player sees it, at true size, among Valve's.

  python hud_topbar.py --out sheet.png --hero NAME --row "LABEL=card.png[,gloat|critical|team2]" [--row ...]
         [--frame capture.png] [--strip X0,Y0,X1,Y1] [--valve DIR] [--stock a,b,c,...] [--json metrics.json]

In a match the portrait is the hero card (heroes.vdata m_strIconHeroCard; m_strIconHeroCardGloat and
m_strIconHeroCardCritical after a kill and at low health), drawn in each top-bar slot by a CitadelHeroBadge of class
TopBar. Every number is the game's own (panorama/layout/hero_badge.xml, styles/hero_badge.css,
styles/citadel_base_styles.css, build 6868); docs/reference/engine.md, "The match HUD", has them with their sources:
  - the badge is 88 x 120 px at 1080p; its image container is 80 % of that width (70 px), and the 280 x 380 card is
    drawn in it with scaling "contain": 70 x 95 px. Panorama scales by screen height, so 1440p is x 4/3.
  - the card is cut by hero_badges/hero_image_mask (square shoulders above, a half circle below) over a disc of the
    team colour (team1Color #D4860B, team2Color #4D75C3), red #B02727 at low health.
  - the card's mips are Box-filtered, so the slot is drawn here with a box filter on premultiplied colour.
Each --row is one line of the sheet: the card in the fourth slot among Valve's cards for other heroes (the same
form: card, gloat or critical), with Valve's own card for --hero first on the line when the game has one. The
sheet draws every line at 1080p and 1440p as the pixels fall, then the 1080p line at 4x (nearest neighbour), so the
same pixels can be looked at. Not drawn: the lit panel of the local player's own slot, the soul bar over its left
edge and the kill-streak colour.

The strip behind the slots is cut from --frame, a capture of the game (1920 x 1080, or scaled from it; --strip is
the box in 1080p pixels, default a run of street and wall clear of the HUD), else a neutral grey ground the sheet
names. Valve's cards and the mask come from --valve (a folder holding the decompiled panorama/images tree); when it
is not given they are decompiled once with Source 2 Viewer into <reference>/work/hud-harness/valve (reference only,
never committed).

Measures (printed, drawn on the sheet, optionally written as JSON), each on the 1080p slot and beside the median of
Valve's cards on the same line: `fill`, the share of the masked slot the card covers (the disc shows through the
rest); `contrast`, the spread of lightness over what it covers (a card that reads as one flat shape at 70 px has
little). They say how a card sits among Valve's; the picture is the judge.
Needs Pillow.
"""
import argparse
import json
import os
import statistics
import subprocess
import sys

from PIL import Image, ImageChops, ImageDraw, ImageStat

TEAM1 = (0xD4, 0x86, 0x0B)
TEAM2 = (0x4D, 0x75, 0xC3)
LOW = (0xB0, 0x27, 0x27)
BADGE_W, BADGE_H = 88, 120          # CitadelHeroBadge
IMG_W = 0.80                        # CitadelHeroBadge .HeroImageContainer width
CARD_W, CARD_H = 280, 380           # the hero card's own size
RES = {"1080p": 1.0, "1440p": 4 / 3}
STRIP_1080 = (40, 150, 680, 282)    # a run of world under the bar in a 1080p capture, clear of the HUD
GROUND = (74, 78, 84)               # the neutral ground without a capture
MASK = "panorama/images/hero_badges/hero_image_mask_psd.png"
CARDS = "panorama/images/heroes"

BG = (18, 20, 24)
TEXT = (200, 204, 210)


def rgba(path):
    return Image.open(path).convert("RGBA")


def area(img, w, h):
    """Box filter down on premultiplied colour, so transparent edges do not bleed dark (the card's mips are Box)."""
    return img.convert("RGBa").resize((w, h), Image.BOX).convert("RGBA")


def slot_size(scale):
    """(badge w, badge h, image w, image h) at `scale` (1 = 1080p)."""
    iw = round(BADGE_W * IMG_W * scale)
    # scaling "contain": the card keeps its aspect inside the container's width
    return round(BADGE_W * scale), round(BADGE_H * scale), iw, round(iw * CARD_H / CARD_W)


def badge(card, mask, scale, colour):
    """One top-bar slot at `scale`: the team disc and the masked card, transparent elsewhere."""
    W, H, iw, ih = slot_size(scale)
    out = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    x0 = (W - iw) // 2
    ImageDraw.Draw(out).ellipse([x0, ih - iw, x0 + iw - 1, ih - 1], fill=colour + (255,))
    c = area(card, iw, ih)
    cut = ImageChops.multiply(c.getchannel("A"), area(mask, iw, ih).getchannel("A"))
    c.putalpha(cut)
    out.alpha_composite(c, (x0, 0))
    return out


def measure(card, mask):
    """fill and contrast of a card on the 1080p slot (see the module text)."""
    _, _, iw, ih = slot_size(1.0)
    c = area(card, iw, ih)
    m = area(mask, iw, ih).getchannel("A")
    cover = ImageChops.multiply(c.getchannel("A"), m)
    slot = ImageStat.Stat(m).sum[0]
    fill = ImageStat.Stat(cover).sum[0] / slot if slot else 0.0
    covered = cover.point(lambda v: 255 if v >= 128 else 0)
    if not covered.getbbox():
        return {"fill": round(fill, 3), "contrast": 0.0}
    light = c.convert("RGB").convert("L")
    return {"fill": round(fill, 3), "contrast": round(ImageStat.Stat(light, covered).stddev[0] / 255, 3)}


def strip_of(frame, box):
    """The world behind the slots: the box (in 1080p pixels) cut from a capture of any size, or the neutral ground."""
    if frame is None:
        return Image.new("RGB", (box[2] - box[0], box[3] - box[1]), GROUND)
    k = frame.size[1] / 1080
    return frame.crop(tuple(round(v * k) for v in box)).convert("RGB")


def line(cards, mask, scale, strip):
    """A run of slots on the strip. cards: list of (image, colour)."""
    W = round(BADGE_W * scale)
    canvas = strip.resize((W * len(cards) + round(24 * scale), round(132 * scale)), Image.BICUBIC).convert("RGBA")
    x = round(12 * scale)
    for img, colour in cards:
        canvas.alpha_composite(badge(img, mask, scale, colour), (x, round(4 * scale)))
        x += W
    return canvas


def valve_card(valve, name, kind):
    """Valve's card of one form for a hero image name (`viscous`), or None when the game has none."""
    for k in (kind, "card"):
        p = os.path.join(valve, CARDS, f"{name}_{k}_psd.png")
        if os.path.exists(p):
            return rgba(p)
    return None


def valve_names(valve):
    """Every hero image name with a card in the decompiled tree, sorted."""
    d = os.path.join(valve, CARDS)
    if not os.path.isdir(d):
        return []
    return sorted(f[: -len("_card_psd.png")] for f in os.listdir(d) if f.endswith("_card_psd.png"))


def ensure_valve(d):
    """Valve's cards and the badge mask, decompiled once into `d` with Source 2 Viewer (reference only)."""
    if os.path.exists(os.path.join(d, MASK)) and valve_names(d):
        return d
    sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
    import gamepaths  # locations as magician resolves them
    pak = os.path.join(gamepaths.game(), "citadel", "pak01_dir.vpk")
    os.makedirs(d, exist_ok=True)
    for f in ("panorama/images/heroes/", "panorama/images/hero_badges/hero_image_mask"):
        subprocess.run([gamepaths.vrf(), "-i", pak, "-f", f, "-d", "-o", d], capture_output=True, cwd=d)
    if not os.path.exists(os.path.join(d, MASK)):
        raise SystemExit(f"hud_topbar: Valve's badge mask did not decompile into {d}; is Source 2 Viewer set (magician doctor)?")
    return d


def parse_row(spec):
    """`label=card.png[,gloat|critical|team2]` -> (label, path, kind, colour)."""
    if "=" not in spec:
        raise SystemExit(f"hud_topbar: --row {spec!r} is not LABEL=card.png[,gloat|critical|team2]")
    label, rest = spec.split("=", 1)
    parts = rest.split(",")
    flags = set(parts[1:])
    unknown = flags - {"gloat", "critical", "team2"}
    if unknown:
        raise SystemExit(f"hud_topbar: --row {label}: unknown {', '.join(sorted(unknown))} (gloat, critical, team2)")
    kind = "card_critical" if "critical" in flags else "card_gloat" if "gloat" in flags else "card"
    colour = LOW if "critical" in flags else TEAM2 if "team2" in flags else TEAM1
    return label, parts[0], kind, colour


def sheet(rows, valve, hero, stock, strip, ground_named):
    """The sheet and the measures. rows: parsed --row specs."""
    mask = rgba(os.path.join(valve, MASK))
    pad = 10
    pieces, report = [], []
    for label, path, kind, colour in rows:
        ours = rgba(path)
        others = [c for c in (valve_card(valve, s, kind) for s in stock) if c is not None]
        own = valve_card(valve, hero, kind) if hero else None
        cards = ([(own, colour)] if own is not None else []) + [(c, colour) for c in others[:2]] + [(ours, colour)] + [(c, colour) for c in others[2:]]
        m = measure(ours, mask)
        theirs = [measure(c, mask) for c in others]
        med = {k: round(statistics.median(t[k] for t in theirs), 3) for k in ("fill", "contrast")} if theirs else {}
        entry = {"row": label, "card": path, "form": kind, "ours": m, "valve_median": med, "valve_cards": len(others)}
        if own is not None:
            entry["valve_own"] = measure(own, mask)
        report.append(entry)
        l1 = line(cards, mask, RES["1080p"], strip)
        l2 = line(cards, mask, RES["1440p"], strip)
        l4 = l1.resize((l1.size[0] * 4, l1.size[1] * 4), Image.NEAREST)
        slot = (1 if own is not None else 0) + min(2, len(others)) + 1
        where = "Valve's own card in slot 1, " if own is not None else ""
        note = f"{label}: {where}ours in slot {slot}, among {len(others)} of Valve's other heroes; 1080p and 1440p as drawn, the 1080p pixels at 4x below." \
            f"  fill {m['fill']:.2f} contrast {m['contrast']:.2f}" + (f" (Valve's median {med['fill']:.2f} / {med['contrast']:.2f})" if med else "")
        pieces.append((note, l1, l2, l4))
    W = max(max(p[1].size[0] + p[2].size[0] + 3 * pad, p[3].size[0] + 2 * pad) for p in pieces)
    foot = 16 if ground_named else 0
    H = sum(max(p[1].size[1], p[2].size[1]) + p[3].size[1] + 3 * pad + 14 for p in pieces) + pad + foot
    out = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(out)
    y = pad
    for note, l1, l2, l4 in pieces:
        d.text((pad, y), note, fill=TEXT)
        y += 14
        out.paste(l1.convert("RGB"), (pad, y))
        out.paste(l2.convert("RGB"), (2 * pad + l1.size[0], y))
        y += max(l1.size[1], l2.size[1]) + pad
        out.paste(l4.convert("RGB"), (pad, y))
        y += l4.size[1] + 2 * pad
    if ground_named:
        d.text((pad, H - foot), "no --frame: the slots sit on a neutral grey ground, not the game's world", fill=TEXT)
    return out, report


def main(argv=None):
    ap = argparse.ArgumentParser(description="A hero card in the in-match top bar, at true size, among Valve's.")
    ap.add_argument("--out", required=True)
    ap.add_argument("--row", action="append", required=True, help="LABEL=card.png[,gloat|critical|team2]")
    ap.add_argument("--hero", default="", help="the hero's image name (viscous, lash): Valve's own card goes first on each line and is left out of the stock")
    ap.add_argument("--frame", default="", help="a capture of the game; the strip behind the slots is cut from it")
    ap.add_argument("--strip", default=",".join(map(str, STRIP_1080)), help="X0,Y0,X1,Y1 in 1080p pixels")
    ap.add_argument("--valve", default="", help="a folder holding Valve's decompiled panorama/images tree")
    ap.add_argument("--stock", default="", help="Valve's heroes to stand beside ours (image names), default the first six the game has")
    ap.add_argument("--json", default="")
    a = ap.parse_args(argv)
    if a.valve:
        valve = a.valve
    else:
        sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
        import gamepaths
        valve = ensure_valve(gamepaths.work("hud-harness", "valve"))
    if not os.path.exists(os.path.join(valve, MASK)):
        raise SystemExit(f"hud_topbar: no {MASK} under {valve}")
    names = [n for n in valve_names(valve) if n != a.hero]
    stock = [s for s in a.stock.split(",") if s] or names[:6]
    try:
        box = tuple(int(v) for v in a.strip.split(","))
        assert len(box) == 4 and box[2] > box[0] and box[3] > box[1]
    except (ValueError, AssertionError):
        raise SystemExit(f"hud_topbar: --strip {a.strip!r} is not X0,Y0,X1,Y1") from None
    frame = Image.open(a.frame) if a.frame else None
    rows = [parse_row(r) for r in a.row]
    out, report = sheet(rows, valve, a.hero, stock, strip_of(frame, box), frame is None)
    os.makedirs(os.path.dirname(os.path.abspath(a.out)) or ".", exist_ok=True)
    out.save(a.out)
    for r in report:
        med = r["valve_median"]
        print(f"{r['row']}: fill {r['ours']['fill']:.2f} contrast {r['ours']['contrast']:.2f}" + (f"   Valve's median {med['fill']:.2f} / {med['contrast']:.2f} over {r['valve_cards']} cards" if med else ""))
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump({"slot_1080p": list(slot_size(1.0)), "slot_1440p": list(slot_size(4 / 3)), "rows": report}, f, indent=1)
    print("wrote", a.out, out.size)


if __name__ == "__main__":
    main()
