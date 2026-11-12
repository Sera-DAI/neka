"""Gera as imagens do README (banner, divisor, ícones e pilha) em docs/imagens/.

Identidade "Algodão-Doce", a mesma do Kotsu e da CLI (`neka-cli/src/ui.rs`): noite roxa, rosa,
salmão e dourado, com corações e brilhos. O título vira contorno (Inter Display Bold), então o SVG
não depende de fonte instalada. Os ícones das tecnologias vêm do Simple Icons (CC0) ou foram
copiados do Kotsu. Fontes e licenças: docs/imagens/FONTES.md.

Uso (só em desenvolvimento; não faz parte do programa):
    pip install fonttools
    python scripts/gerar_imagens.py
"""
from __future__ import annotations

import urllib.request
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
SAIDA = RAIZ / "docs" / "imagens"
FONTE_TITULO = "/usr/share/fonts/opentype/inter/InterDisplay-Bold.otf"
SIMPLE_ICONS = "https://raw.githubusercontent.com/simple-icons/simple-icons/13.0.0/icons/{}.svg"

NOITE, ROXO, AMEIXA = "#1d1424", "#33203f", "#5a2a50"
ROSA, ROSA2, SALMAO, OURO, AZUL, VERDE, CLARO = "#ff7fb8", "#ff9ac8", "#ffa28c", "#ffd27a", "#8fb3ff", "#86efc0", "#ffe9f4"
SANS = "'Segoe UI','Nunito','Hiragino Maru Gothic ProN','Yu Gothic UI',system-ui,sans-serif"
MONO = "'Cascadia Code','JetBrains Mono','Consolas',ui-monospace,monospace"

TEXTOS = {
    "pt": ("neka: gerenciador de senhas no terminal", "senhas no terminal, fofas e bem guardadas",
           "um cofre cifrado · sincroniza pelo seu Drive"),
    "en": ("neka: a password manager for your terminal", "passwords in your terminal, cozy and well kept",
           "one encrypted vault · syncs through your Drive"),
}

ESTILO = """
    .e{transform-box:fill-box;transform-origin:center}
    .c1{animation:cint 4.2s ease-in-out infinite}.c2{animation:cint 5.6s ease-in-out infinite 1.1s}
    .c3{animation:cint 3.6s ease-in-out infinite 2.2s}.c4{animation:cint 6.4s ease-in-out infinite .5s}
    @keyframes cint{0%,100%{opacity:.25;transform:scale(.7)}50%{opacity:1;transform:scale(1.15) rotate(45deg)}}
    .flutua{animation:flutua 6s ease-in-out infinite alternate}
    @keyframes flutua{to{transform:translateY(-8px)}}
    .sobe{animation:sobe 7s ease-in infinite}.s2{animation-delay:2.3s}.s3{animation-delay:4.6s}
    @keyframes sobe{0%{transform:translateY(0) scale(.6);opacity:0}15%{opacity:.9}100%{transform:translateY(-190px) scale(1.1);opacity:0}}
    .bate{animation:bate 1.8s ease-in-out infinite}
    @keyframes bate{0%,100%{transform:scale(1)}50%{transform:scale(1.25)}}
    .cursor{animation:pisca 1.1s steps(1) infinite}
    @keyframes pisca{50%{opacity:0}}
    .nome{animation:nome 6s ease-in-out infinite alternate}
    @keyframes nome{from{opacity:.88}to{opacity:1}}
    @media (prefers-reduced-motion:reduce){*{animation:none!important}.nome{opacity:1}}
"""

CORACAO = "M0 -4 C-8 -14 -22 -4 0 14 C22 -4 8 -14 0 -4Z"


def contorno(texto: str, tamanho: float, espaco: float = 0.0) -> tuple[str, float]:
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.ttLib import TTFont

    fonte = TTFont(FONTE_TITULO)
    cmap, glifos = fonte.getBestCmap(), fonte.getGlyphSet()
    escala = tamanho / fonte["head"].unitsPerEm
    x, partes = 0.0, []
    for letra in texto:
        nome = cmap[ord(letra)]
        pen = SVGPathPen(glifos, ntos=lambda v: f"{v:.1f}")
        glifos[nome].draw(TransformPen(pen, (escala, 0, 0, -escala, x, 0)))
        partes.append(pen.getCommands())
        x += glifos[nome].width * escala + espaco
    return " ".join(partes), x - espaco


def brilho(x: float, y: float, r: float, cor: str, classe: str) -> str:
    k = r * 0.25
    return (f'<path d="M{x} {y - r} L{x + k} {y - k} L{x + r} {y} L{x + k} {y + k} L{x} {y + r} '
            f'L{x - k} {y + k} L{x - r} {y} L{x - k} {y - k}Z" fill="{cor}" class="e {classe}"/>')


def coracao(x: float, y: float, escala: float, cor: str, classe: str = "") -> str:
    return f'<g transform="translate({x} {y}) scale({escala})"><path d="{CORACAO}" fill="{cor}" class="{classe}"/></g>'


def banner(idioma: str) -> str:
    rotulo, tagline, subtitulo = TEXTOS[idioma]
    titulo, _ = contorno("neka", 190, 6)
    lista = "list" if idioma == "en" else "list"
    estrelas = "".join([
        brilho(250, 40, 8, OURO, "c1"), brilho(520, 40, 6, "#ffffff", "c2"), brilho(600, 300, 8, "#ffb3c9", "c3"),
        brilho(440, 250, 5, AZUL, "c4"), brilho(1210, 50, 9, OURO, "c3"), brilho(40, 300, 6, "#ffffff", "c4"),
        brilho(380, 70, 5, ROSA2, "c2")])
    sobem = (f'<g transform="translate(470 330) scale(0.6)">{coracao(0, 0, 1, ROSA, "e sobe")}</g>'
             f'<g transform="translate(560 340) scale(0.45)">{coracao(0, 0, 1, SALMAO, "e sobe s2")}</g>'
             f'<g transform="translate(400 340) scale(0.5)">{coracao(0, 0, 1, ROSA2, "e sobe s3")}</g>')
    linhas = [("c764800a", "Banco", "ana", "financas"), ("b0cdffc3", "Email", "ana@exemplo", ""), ("94f3f5b5", "Loja", "", "compras")]
    tabela = ""
    for i, (ident, titulo_l, usuario, etiqueta) in enumerate(linhas):
        y = 132 + i * 28
        tabela += (f'<text x="26" y="{y}" font-family="{MONO}" font-size="14" fill="{ROSA}">♡</text>'
                   f'<text x="48" y="{y}" font-family="{MONO}" font-size="14" fill="#9a7fa8">{ident}</text>'
                   f'<text x="136" y="{y}" font-family="{MONO}" font-size="14" font-weight="700" fill="{CLARO}">{titulo_l}</text>'
                   f'<text x="216" y="{y}" font-family="{MONO}" font-size="14" fill="{AZUL}">{usuario}</text>'
                   f'<text x="344" y="{y}" font-family="{MONO}" font-size="14" fill="{OURO}">{("[" + etiqueta + "]") if etiqueta else ""}</text>')
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="360" viewBox="0 0 1280 360" role="img" aria-label="{rotulo}">
  <!-- neka: noite Algodão-Doce, igual à CLI. Gerado por scripts/gerar_imagens.py; o título é contorno. Dados do terminal são fictícios. -->
  <defs><style>{ESTILO}</style>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{NOITE}"/><stop offset=".6" stop-color="{ROXO}"/><stop offset="1" stop-color="{AMEIXA}"/></linearGradient>
    <radialGradient id="brilho" cx=".2" cy=".3" r=".7"><stop offset="0" stop-color="{ROSA}" stop-opacity=".28"/><stop offset="1" stop-color="{ROSA}" stop-opacity="0"/></radialGradient>
    <radialGradient id="brilho2" cx=".85" cy=".9" r=".6"><stop offset="0" stop-color="{SALMAO}" stop-opacity=".25"/><stop offset="1" stop-color="{SALMAO}" stop-opacity="0"/></radialGradient>
    <linearGradient id="fio" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{ROSA2}"/><stop offset="1" stop-color="{SALMAO}"/></linearGradient>
  </defs>
  <rect width="1280" height="360" rx="28" fill="url(#bg)"/>
  <rect width="1280" height="360" rx="28" fill="url(#brilho)"/><rect width="1280" height="360" rx="28" fill="url(#brilho2)"/>
  <rect x="1.5" y="1.5" width="1277" height="357" rx="27" fill="none" stroke="{ROSA}" stroke-opacity=".45" stroke-width="2"/>
  {estrelas}{sobem}
  <g transform="translate(64 214)"><path d="{titulo}" fill="url(#fio)" class="nome"/></g>
  <text x="64" y="282" font-family="{SANS}" font-size="21" font-weight="600" fill="{CLARO}">{tagline}</text>
  <text x="64" y="312" font-family="{SANS}" font-size="16" fill="{ROSA2}">{subtitulo}</text>
  <g class="flutua"><g transform="translate(700 52)">
      <rect width="500" height="256" rx="18" fill="{NOITE}" stroke="{ROSA}" stroke-opacity=".7" stroke-width="2"/>
      <circle cx="24" cy="22" r="6" fill="#ff7a9e"/><circle cx="44" cy="22" r="6" fill="{OURO}"/><circle cx="64" cy="22" r="6" fill="{VERDE}"/>
      <text x="250" y="27" text-anchor="middle" font-family="{SANS}" font-size="13" fill="#9a7fa8">neka</text>
      <text x="26" y="66" font-family="{MONO}" font-size="14" fill="{ROSA}">❯ <tspan fill="{CLARO}">neka {lista}</tspan></text>
      <text x="26" y="94" font-family="{MONO}" font-size="14" fill="{ROSA}">♡ <tspan fill="#9a7fa8">Master password: ••••••••</tspan></text>
      {tabela}
      <text x="26" y="236" font-family="{MONO}" font-size="14" fill="{OURO}">(◕‿◕)♡ <tspan fill="#9a7fa8">3 entries</tspan></text>
      <rect x="460" y="222" width="9" height="14" rx="1" fill="{CLARO}" class="cursor"/>
    </g></g>
</svg>
"""


def divisor() -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="720" height="30" viewBox="0 0 720 30" role="presentation">
  <defs><style>{ESTILO}</style>
    <linearGradient id="d" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{ROSA}" stop-opacity="0"/><stop offset="1" stop-color="{ROSA}"/></linearGradient>
    <linearGradient id="e" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{SALMAO}"/><stop offset="1" stop-color="{SALMAO}" stop-opacity="0"/></linearGradient></defs>
  <path d="M0 15H318" stroke="url(#d)" stroke-width="2" stroke-linecap="round" fill="none"/>
  <path d="M402 15H720" stroke="url(#e)" stroke-width="2" stroke-linecap="round" fill="none"/>
  {coracao(360, 13, 0.95, ROSA, "e bate")}{coracao(336, 14, 0.45, SALMAO, "e c3")}{coracao(384, 14, 0.45, SALMAO, "e c4")}
  {brilho(312, 15, 5, OURO, "c1")}{brilho(408, 15, 5, OURO, "c2")}
</svg>
"""


# Ícones de seção: quadrado arredondado em degradê rosa, com um desenho de traço claro.
ICONES = {
    "cofre": '<rect x="18" y="30" width="28" height="22" rx="6"/><path d="M24 30v-6a8 8 0 0 1 16 0v6"/><path d="M32 39v5"/>',
    "nuvem": '<path d="M22 44a9 9 0 0 1-1-17.9A12 12 0 0 1 44 28a8 8 0 0 1-1 16Z"/><path d="M26 36l5 5 8-9"/>',
    "terminal": '<rect x="14" y="16" width="36" height="32" rx="7"/><path d="M22 28l7 6-7 6"/><path d="M34 41h9"/>',
    "importar": '<path d="M32 16v24"/><path d="M22 31l10 10 10-10"/><path d="M18 48h28"/>',
    "instalar": '<path d="M32 14v26"/><path d="M22 31l10 10 10-10"/><rect x="16" y="46" width="32" height="5" rx="2.5"/>',
    "estrutura": '<rect x="14" y="16" width="36" height="10" rx="4"/><rect x="14" y="30" width="36" height="10" rx="4"/><rect x="14" y="44" width="36" height="6" rx="3"/>',
    "verificar": '<circle cx="32" cy="32" r="17"/><path d="M24 33l6 6 11-13"/>',
    "licenca": '<path d="M20 14h24v36l-12-8-12 8Z"/><path d="M26 24h12M26 31h12"/>',
    "seguranca": '<path d="M32 14l15 6v12c0 9-6 15-15 18-9-3-15-9-15-18V20Z"/><path d="M25 32l5 5 9-10"/>',
}


def icone(nome: str, desenho: str) -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64" role="img" aria-label="{nome}">
  <defs><linearGradient id="n" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ff9ac8"/><stop offset="1" stop-color="#ff7a9e"/></linearGradient></defs>
  <rect x="3" y="3" width="58" height="58" rx="20" fill="url(#n)"/>
  <rect x="7" y="7" width="50" height="50" rx="16" fill="none" stroke="#fff5fa" stroke-opacity=".55"/>
  <g fill="none" stroke="#fff5fa" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round">{desenho}</g>
</svg>
"""


def pilha(nome: str, rotulo: str) -> str:
    with urllib.request.urlopen(SIMPLE_ICONS.format(nome), timeout=30) as r:
        svg = r.read().decode()
    caminhos = svg.split("</title>")[1].split("</svg>")[0]
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="56" height="56" viewBox="0 0 56 56" role="img" aria-label="{rotulo}">
  <rect x="1" y="1" width="54" height="54" rx="14" fill="#2a1733" stroke="{ROSA}" stroke-opacity=".6" stroke-width="2"/>
  <g transform="translate(14 14) scale(1.1667)" fill="{CLARO}">{caminhos}</g>
</svg>
"""


def main() -> None:
    (SAIDA / "icones").mkdir(parents=True, exist_ok=True)
    (SAIDA / "stack").mkdir(parents=True, exist_ok=True)
    (SAIDA / "banner.svg").write_text(banner("pt"), encoding="utf-8")
    (SAIDA / "banner.en.svg").write_text(banner("en"), encoding="utf-8")
    (SAIDA / "divisor.svg").write_text(divisor(), encoding="utf-8")
    for nome, desenho in ICONES.items():
        (SAIDA / "icones" / f"{nome}.svg").write_text(icone(nome, desenho), encoding="utf-8")
    for nome, rotulo in (("rust", "Rust"), ("githubactions", "GitHub Actions")):
        (SAIDA / "stack" / f"{nome}.svg").write_text(pilha(nome, rotulo), encoding="utf-8")
    # O logotipo do Claude não está na versão fixada do Simple Icons: vem pronto do Kotsu (mesmo estilo)
    # e já fica versionado em docs/imagens/stack/claude.svg; este script não o regenera.
    print(f"Imagens geradas em {SAIDA}")


if __name__ == "__main__":
    main()
