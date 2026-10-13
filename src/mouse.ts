// Vista de cima do Redragon Storm Pro (M808-KS) — ilustração própria, com áreas clicáveis.

export interface PhysicalButton {
  /** Número exibido (igual ao do manual / software oficial). */
  n: number;
  name: string;
  /** Posição na matriz de 16 slots da EEPROM. */
  slot: number;
  /** Forma clicável no SVG. */
  shape: string;
  /** Onde fica o selo com o número. */
  badge: [number, number];
}

// Formato para destro: curva do polegar à esquerda, lado direito mais cheio, traseira larga.
const BODY = `M128 12 C176 10 206 40 214 100 C222 160 226 250 214 322
  C204 382 162 410 122 408 C80 406 48 382 42 332 C38 288 56 248 52 206
  C48 166 30 124 36 82 C42 38 82 12 128 12 Z`;

// Faixas RGB: uma de cada lado, acompanhando o contorno da frente até a traseira.
const STRIP = "M42 74 C38 122 52 160 57 205 C62 250 47 292 53 336 C58 372 80 394 108 402";
const STRIP_R = "M209 96 C215 150 219 240 210 318 C203 362 184 390 152 402";

const CHANNEL = "M110 26 C110 20 148 20 148 26 L150 248 C150 270 108 270 108 248 Z";

export const PHYSICAL_BUTTONS: PhysicalButton[] = [
  { n: 1, name: "Botão esquerdo", slot: 0, badge: [70, 84],
    shape: `<path d="M106 20 C80 16 48 34 40 80 C35 114 46 152 50 200 L106 200 Z"/>` },
  { n: 2, name: "Botão direito", slot: 1, badge: [188, 84],
    shape: `<path d="M152 20 C182 18 207 44 213 100 C217 140 219 176 219 212 L152 212 Z"/>` },
  { n: 3, name: "Roda (clique)", slot: 2, badge: [129, 18],
    shape: `<rect class="wheel" x="117" y="40" width="24" height="64" rx="12"/>` },
  { n: 4, name: "Lateral (frente)", slot: 4, badge: [16, 214],
    shape: `<path d="M45 194 C43 208 43 222 45 236 L56 235 C54 221 54 208 56 195 Z"/>` },
  { n: 5, name: "Lateral (trás)", slot: 3, badge: [16, 264],
    shape: `<path d="M46 246 C45 260 46 273 49 286 L59 284 C56 271 56 259 57 247 Z"/>` },
  { n: 6, name: "Superior 1", slot: 10, badge: [168, 134],
    shape: `<rect x="117" y="122" width="24" height="24" rx="4"/>` },
  { n: 7, name: "Superior 2", slot: 11, badge: [168, 166],
    shape: `<rect x="117" y="154" width="24" height="24" rx="4"/>` },
  { n: 8, name: "Superior 3", slot: 9, badge: [168, 198],
    shape: `<rect x="117" y="186" width="24" height="24" rx="4"/>` },
];

/** Hexágono "pointy-top" de raio r centrado em (cx, cy). */
function hex(cx: number, cy: number, r: number): string {
  const pts = [0, 1, 2, 3, 4, 5].map((i) => {
    const a = (Math.PI / 3) * i - Math.PI / 2;
    return `${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)}`;
  });
  return `<polygon points="${pts.join(" ")}"/>`;
}

/** x aproximado da faixa RGB numa altura y (para não furar a lateral esquerda). */
function stripX(y: number): number {
  if (y < 205) return 42 + (y - 74) * 0.115;
  if (y < 336) return 57 - (y - 205) * 0.03;
  return 53 + (y - 336) * 0.8;
}

/** x aproximado da faixa direita numa altura y. */
function stripRX(y: number): number {
  if (y < 96) return 230;
  if (y < 220) return 209 + (y - 96) * 0.07;
  if (y < 318) return 218 - (y - 220) * 0.08;
  return 210 - (y - 318) * 0.7;
}

/** Furos grandes: começam no meio dos botões e tomam a traseira; frente e canal central lisos. */
function bigHoles(): string {
  const out: string[] = [];
  const step = 21;
  const dy = step * 0.866;
  for (let row = 0, y = 100; y < 405; row++, y += dy) {
    for (let x = 30 + (row % 2 ? step / 2 : 0); x < 230; x += step) {
      if (x > 100 && x < 158 && y < 275) continue; // canal central
      if (x < stripX(y) + 14) continue; // laterais têm colmeia fina
      if (x > stripRX(y) - 14) continue;
      if (y < 128 && x < 104) continue; // frente do botão esquerdo é lisa
      const fromRear = Math.min(Math.max((y - 100) / 160, 0), 1);
      const edge = Math.min(Math.abs(x - 135) / 95, 1);
      const r = 3.2 + 4.6 * fromRear - 2.2 * edge;
      if (r < 3.2) continue;
      out.push(hex(x, y, r));
    }
  }
  return out.join("");
}

/** Colmeia fina das laterais (entre a borda e as faixas RGB). */
function sideMesh(): string {
  const out: string[] = [];
  const step = 6.5;
  for (let row = 0, y = 70; y < 405; row++, y += step * 0.866) {
    for (let x = 20 + (row % 2 ? step / 2 : 0); x < 236; x += step) {
      if (x < stripX(y) - 2 || x > stripRX(y) + 2) out.push(hex(x, y, 2.3));
    }
  }
  return out.join("");
}

/** Banda de piso da roda (chevrons). */
function wheelTread(): string {
  return [50, 58, 66, 74, 82, 90].map((y) => `<path d="M121 ${y + 4} L129 ${y} L137 ${y + 4}"/>`).join("");
}

/** Cor da faixa no desenho: uma cor CSS, "rainbow" ou null (apagada). */
export type StripLook = string | "rainbow" | null;

/**
 * SVG do mouse. `strip` desenha a faixa RGB lateral; `wheel` é a cor da luz da roda
 * (indicador de DPI) ou null se apagada.
 */
export function mouseSvg(strip: StripLook, wheel: string | null): SVGSVGElement {
  const stripColor = strip === null ? "#26292d" : strip === "rainbow" ? "url(#rainbow)" : strip;
  const wheelColor = wheel ?? "#3a3d42";
  const wrap = document.createElement("div");
  wrap.innerHTML = `
<svg viewBox="0 0 250 420" class="mouse-svg" role="group" aria-label="Botões do mouse Storm Pro">
  <defs>
    <linearGradient id="shell" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#2c2f34"/><stop offset=".5" stop-color="#1c1e21"/><stop offset="1" stop-color="#111214"/>
    </linearGradient>
    <radialGradient id="shine" cx=".55" cy=".25" r=".65">
      <stop offset="0" stop-color="#fff" stop-opacity=".10"/><stop offset="1" stop-color="#fff" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="channel" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#16181a"/><stop offset=".5" stop-color="#25282c"/><stop offset="1" stop-color="#16181a"/>
    </linearGradient>
    <linearGradient id="rainbow" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ff3b1f"/><stop offset=".18" stop-color="#ffd000"/>
      <stop offset=".36" stop-color="#3cff3c"/><stop offset=".54" stop-color="#00d5ff"/>
      <stop offset=".72" stop-color="#5a3cff"/><stop offset=".88" stop-color="#ff2bd6"/><stop offset="1" stop-color="#ff2040"/>
    </linearGradient>
    <clipPath id="body-clip"><path d="${BODY}"/></clipPath>
    <filter id="glow" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="3.5"/></filter>
  </defs>

  <ellipse cx="128" cy="402" rx="88" ry="10" fill="#000" opacity=".35"/>

  <path d="${BODY}" fill="url(#shell)" stroke="#3d4147" stroke-width="1.6"/>
  <g clip-path="url(#body-clip)">
    <g fill="#08090a" opacity=".85">${sideMesh()}</g>
    <g fill="#060607" stroke="#2e3136" stroke-width=".6">${bigHoles()}</g>
    <path d="${BODY}" fill="url(#shine)"/>
    <path d="M40 200 L106 200 M152 212 L219 212" stroke="#060607" stroke-width="1.6"/>
  </g>

  ${[STRIP, STRIP_R].map((d) => `
    ${strip !== null ? `<path d="${d}" fill="none" stroke="${stripColor}" stroke-width="9" filter="url(#glow)" opacity=".8"/>` : ""}
    <path d="${d}" fill="none" stroke="${stripColor}" stroke-width="3" stroke-linecap="round"/>`).join("")}

  <path d="${CHANNEL}" fill="url(#channel)" stroke="#060607" stroke-width="1.4"/>

  ${wheel ? `<rect x="114" y="37" width="30" height="70" rx="15" fill="none" stroke="${wheelColor}" stroke-width="3" filter="url(#glow)" opacity=".9"/>` : ""}
  <rect x="114" y="37" width="30" height="70" rx="15" fill="none" stroke="${wheelColor}" stroke-width="1.5"/>

  <g class="hotspots">
    ${PHYSICAL_BUTTONS.map((b) => `
      <g class="hot" data-n="${b.n}" tabindex="0" role="button" aria-label="${b.n} ${b.name}">
        ${b.shape}
        ${b.n === 3 ? `<g class="tread" pointer-events="none">${wheelTread()}</g>` : ""}
        <g class="badge" transform="translate(${b.badge[0]} ${b.badge[1]})">
          <circle r="9"/><text dy="3.5">${b.n}</text>
        </g>
      </g>`).join("")}
  </g>
</svg>`;
  return wrap.firstElementChild as SVGSVGElement;
}
