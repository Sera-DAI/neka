"""Generates the README images (banner, divider, icons and stack) in docs/images/.

"Cotton Candy" identity, the same as Kotsu and the CLI (`neka-cli/src/ui.rs`): purple night, pink,
salmon and gold, with hearts and sparkles. The title becomes an outline (Inter Display Bold), so the
SVG does not depend on an installed font. The technology icons come from Simple Icons (CC0) or were
copied from Kotsu. Fonts and licenses: docs/images/SOURCES.md.

Usage (development only; not part of the program):
    pip install fonttools
    python scripts/generate_images.py
"""
from __future__ import annotations

import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "docs" / "images"
TITLE_FONT = "/usr/share/fonts/opentype/inter/InterDisplay-Bold.otf"
SIMPLE_ICONS = "https://raw.githubusercontent.com/simple-icons/simple-icons/13.0.0/icons/{}.svg"

NIGHT, PURPLE, PLUM = "#1d1424", "#33203f", "#5a2a50"
PINK, PINK2, SALMON, GOLD, BLUE, GREEN, LIGHT = "#ff7fb8", "#ff9ac8", "#ffa28c", "#ffd27a", "#8fb3ff", "#86efc0", "#ffe9f4"
SANS = "'Segoe UI','Nunito','Hiragino Maru Gothic ProN','Yu Gothic UI',system-ui,sans-serif"
MONO = "'Cascadia Code','JetBrains Mono','Consolas',ui-monospace,monospace"

TEXTS = {
    "en": ("neka: a password manager for your terminal", "passwords in your terminal, cozy and well kept",
           "one encrypted vault · syncs through your Drive"),
}

STYLE = """
    .e{transform-box:fill-box;transform-origin:center}
    .c1{animation:twinkle 4.2s ease-in-out infinite}.c2{animation:twinkle 5.6s ease-in-out infinite 1.1s}
    .c3{animation:twinkle 3.6s ease-in-out infinite 2.2s}.c4{animation:twinkle 6.4s ease-in-out infinite .5s}
    @keyframes twinkle{0%,100%{opacity:.25;transform:scale(.7)}50%{opacity:1;transform:scale(1.15) rotate(45deg)}}
    .float{animation:float 6s ease-in-out infinite alternate}
    @keyframes float{to{transform:translateY(-8px)}}
    .rise{animation:rise 7s ease-in infinite}.s2{animation-delay:2.3s}.s3{animation-delay:4.6s}
    @keyframes rise{0%{transform:translateY(0) scale(.6);opacity:0}15%{opacity:.9}100%{transform:translateY(-190px) scale(1.1);opacity:0}}
    .beat{animation:beat 1.8s ease-in-out infinite}
    @keyframes beat{0%,100%{transform:scale(1)}50%{transform:scale(1.25)}}
    .cursor{animation:blink 1.1s steps(1) infinite}
    @keyframes blink{50%{opacity:0}}
    .name{animation:name 6s ease-in-out infinite alternate}
    @keyframes name{from{opacity:.88}to{opacity:1}}
    @media (prefers-reduced-motion:reduce){*{animation:none!important}.name{opacity:1}}
"""

HEART = "M0 -4 C-8 -14 -22 -4 0 14 C22 -4 8 -14 0 -4Z"


def outline(text: str, size: float, spacing: float = 0.0) -> tuple[str, float]:
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.ttLib import TTFont

    font = TTFont(TITLE_FONT)
    cmap, glyphs = font.getBestCmap(), font.getGlyphSet()
    scale = size / font["head"].unitsPerEm
    x, parts = 0.0, []
    for letter in text:
        name = cmap[ord(letter)]
        pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.1f}")
        glyphs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, x, 0)))
        parts.append(pen.getCommands())
        x += glyphs[name].width * scale + spacing
    return " ".join(parts), x - spacing


def sparkle(x: float, y: float, r: float, color: str, cls: str) -> str:
    k = r * 0.25
    return (f'<path d="M{x} {y - r} L{x + k} {y - k} L{x + r} {y} L{x + k} {y + k} L{x} {y + r} '
            f'L{x - k} {y + k} L{x - r} {y} L{x - k} {y - k}Z" fill="{color}" class="e {cls}"/>')


def heart(x: float, y: float, scale: float, color: str, cls: str = "") -> str:
    return f'<g transform="translate({x} {y}) scale({scale})"><path d="{HEART}" fill="{color}" class="{cls}"/></g>'


def banner(language: str) -> str:
    label, tagline, subtitle = TEXTS[language]
    title, _ = outline("neka", 190, 6)
    list_cmd = "list"
    stars = "".join([
        sparkle(250, 40, 8, GOLD, "c1"), sparkle(520, 40, 6, "#ffffff", "c2"), sparkle(600, 300, 8, "#ffb3c9", "c3"),
        sparkle(440, 250, 5, BLUE, "c4"), sparkle(1210, 50, 9, GOLD, "c3"), sparkle(40, 300, 6, "#ffffff", "c4"),
        sparkle(380, 70, 5, PINK2, "c2")])
    rising = (f'<g transform="translate(470 330) scale(0.6)">{heart(0, 0, 1, PINK, "e rise")}</g>'
             f'<g transform="translate(560 340) scale(0.45)">{heart(0, 0, 1, SALMON, "e rise s2")}</g>'
             f'<g transform="translate(400 340) scale(0.5)">{heart(0, 0, 1, PINK2, "e rise s3")}</g>')
    rows = [("c764800a", "Bank", "ana", "finance"), ("b0cdffc3", "Email", "ana@example", ""), ("94f3f5b5", "Store", "", "shopping")]
    table = ""
    for i, (ident, title_l, username, tag) in enumerate(rows):
        y = 132 + i * 28
        table += (f'<text x="26" y="{y}" font-family="{MONO}" font-size="14" fill="{PINK}">♡</text>'
                   f'<text x="48" y="{y}" font-family="{MONO}" font-size="14" fill="#9a7fa8">{ident}</text>'
                   f'<text x="136" y="{y}" font-family="{MONO}" font-size="14" font-weight="700" fill="{LIGHT}">{title_l}</text>'
                   f'<text x="216" y="{y}" font-family="{MONO}" font-size="14" fill="{BLUE}">{username}</text>'
                   f'<text x="344" y="{y}" font-family="{MONO}" font-size="14" fill="{GOLD}">{("[" + tag + "]") if tag else ""}</text>')
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="360" viewBox="0 0 1280 360" role="img" aria-label="{label}">
  <!-- neka: Cotton Candy night, same as the CLI. Generated by scripts/generate_images.py; the title is an outline. Terminal data is fake. -->
  <defs><style>{STYLE}</style>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{NIGHT}"/><stop offset=".6" stop-color="{PURPLE}"/><stop offset="1" stop-color="{PLUM}"/></linearGradient>
    <radialGradient id="sparkle" cx=".2" cy=".3" r=".7"><stop offset="0" stop-color="{PINK}" stop-opacity=".28"/><stop offset="1" stop-color="{PINK}" stop-opacity="0"/></radialGradient>
    <radialGradient id="glow2" cx=".85" cy=".9" r=".6"><stop offset="0" stop-color="{SALMON}" stop-opacity=".25"/><stop offset="1" stop-color="{SALMON}" stop-opacity="0"/></radialGradient>
    <linearGradient id="thread" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{PINK2}"/><stop offset="1" stop-color="{SALMON}"/></linearGradient>
  </defs>
  <rect width="1280" height="360" rx="28" fill="url(#bg)"/>
  <rect width="1280" height="360" rx="28" fill="url(#sparkle)"/><rect width="1280" height="360" rx="28" fill="url(#glow2)"/>
  <rect x="1.5" y="1.5" width="1277" height="357" rx="27" fill="none" stroke="{PINK}" stroke-opacity=".45" stroke-width="2"/>
  {stars}{rising}
  <g transform="translate(64 214)"><path d="{title}" fill="url(#thread)" class="name"/></g>
  <text x="64" y="282" font-family="{SANS}" font-size="21" font-weight="600" fill="{LIGHT}">{tagline}</text>
  <text x="64" y="312" font-family="{SANS}" font-size="16" fill="{PINK2}">{subtitle}</text>
  <g class="float"><g transform="translate(700 52)">
      <rect width="500" height="256" rx="18" fill="{NIGHT}" stroke="{PINK}" stroke-opacity=".7" stroke-width="2"/>
      <circle cx="24" cy="22" r="6" fill="#ff7a9e"/><circle cx="44" cy="22" r="6" fill="{GOLD}"/><circle cx="64" cy="22" r="6" fill="{GREEN}"/>
      <text x="250" y="27" text-anchor="middle" font-family="{SANS}" font-size="13" fill="#9a7fa8">neka</text>
      <text x="26" y="66" font-family="{MONO}" font-size="14" fill="{PINK}">❯ <tspan fill="{LIGHT}">neka {list_cmd}</tspan></text>
      <text x="26" y="94" font-family="{MONO}" font-size="14" fill="{PINK}">♡ <tspan fill="#9a7fa8">Master password: ••••••••</tspan></text>
      {table}
      <text x="26" y="236" font-family="{MONO}" font-size="14" fill="{GOLD}">(◕‿◕)♡ <tspan fill="#9a7fa8">3 entries</tspan></text>
      <rect x="460" y="222" width="9" height="14" rx="1" fill="{LIGHT}" class="cursor"/>
    </g></g>
</svg>
"""


def divider() -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="720" height="30" viewBox="0 0 720 30" role="presentation">
  <defs><style>{STYLE}</style>
    <linearGradient id="d" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{PINK}" stop-opacity="0"/><stop offset="1" stop-color="{PINK}"/></linearGradient>
    <linearGradient id="e" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{SALMON}"/><stop offset="1" stop-color="{SALMON}" stop-opacity="0"/></linearGradient></defs>
  <path d="M0 15H318" stroke="url(#d)" stroke-width="2" stroke-linecap="round" fill="none"/>
  <path d="M402 15H720" stroke="url(#e)" stroke-width="2" stroke-linecap="round" fill="none"/>
  {heart(360, 13, 0.95, PINK, "e beat")}{heart(336, 14, 0.45, SALMON, "e c3")}{heart(384, 14, 0.45, SALMON, "e c4")}
  {sparkle(312, 15, 5, GOLD, "c1")}{sparkle(408, 15, 5, GOLD, "c2")}
</svg>
"""


# Section icons: rounded square in a pink gradient, with a light stroke drawing.
ICONS = {
    "vault": '<rect x="18" y="30" width="28" height="22" rx="6"/><path d="M24 30v-6a8 8 0 0 1 16 0v6"/><path d="M32 39v5"/>',
    "cloud": '<path d="M22 44a9 9 0 0 1-1-17.9A12 12 0 0 1 44 28a8 8 0 0 1-1 16Z"/><path d="M26 36l5 5 8-9"/>',
    "terminal": '<rect x="14" y="16" width="36" height="32" rx="7"/><path d="M22 28l7 6-7 6"/><path d="M34 41h9"/>',
    "import": '<path d="M32 16v24"/><path d="M22 31l10 10 10-10"/><path d="M18 48h28"/>',
    "install": '<path d="M32 14v26"/><path d="M22 31l10 10 10-10"/><rect x="16" y="46" width="32" height="5" rx="2.5"/>',
    "structure": '<rect x="14" y="16" width="36" height="10" rx="4"/><rect x="14" y="30" width="36" height="10" rx="4"/><rect x="14" y="44" width="36" height="6" rx="3"/>',
    "verify": '<circle cx="32" cy="32" r="17"/><path d="M24 33l6 6 11-13"/>',
    "license": '<path d="M20 14h24v36l-12-8-12 8Z"/><path d="M26 24h12M26 31h12"/>',
    "security": '<path d="M32 14l15 6v12c0 9-6 15-15 18-9-3-15-9-15-18V20Z"/><path d="M25 32l5 5 9-10"/>',
}


def icon(name: str, drawing: str) -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64" role="img" aria-label="{name}">
  <defs><linearGradient id="n" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ff9ac8"/><stop offset="1" stop-color="#ff7a9e"/></linearGradient></defs>
  <rect x="3" y="3" width="58" height="58" rx="20" fill="url(#n)"/>
  <rect x="7" y="7" width="50" height="50" rx="16" fill="none" stroke="#fff5fa" stroke-opacity=".55"/>
  <g fill="none" stroke="#fff5fa" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round">{drawing}</g>
</svg>
"""


def stack_icon(name: str, label: str) -> str:
    with urllib.request.urlopen(SIMPLE_ICONS.format(name), timeout=30) as r:
        svg = r.read().decode()
    paths = svg.split("</title>")[1].split("</svg>")[0]
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="56" height="56" viewBox="0 0 56 56" role="img" aria-label="{label}">
  <rect x="1" y="1" width="54" height="54" rx="14" fill="#2a1733" stroke="{PINK}" stroke-opacity=".6" stroke-width="2"/>
  <g transform="translate(14 14) scale(1.1667)" fill="{LIGHT}">{paths}</g>
</svg>
"""


def main() -> None:
    (OUTPUT / "icons").mkdir(parents=True, exist_ok=True)
    (OUTPUT / "stack").mkdir(parents=True, exist_ok=True)
    (OUTPUT / "banner.svg").write_text(banner("en"), encoding="utf-8")
    (OUTPUT / "divider.svg").write_text(divider(), encoding="utf-8")
    for name, drawing in ICONS.items():
        (OUTPUT / "icons" / f"{name}.svg").write_text(icon(name, drawing), encoding="utf-8")
    for name, label in (("rust", "Rust"), ("githubactions", "GitHub Actions")):
        (OUTPUT / "stack" / f"{name}.svg").write_text(stack_icon(name, label), encoding="utf-8")
    # The Claude logo is not in the pinned Simple Icons version: it comes ready-made from Kotsu (same style)
    # and is already versioned in docs/images/stack/claude.svg; this script does not regenerate it.
    print(f"Images generated in {OUTPUT}")


if __name__ == "__main__":
    main()
