import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { PHYSICAL_BUTTONS, mouseSvg, type StripLook } from "./mouse";

// ---- Tipos espelhando src-tauri/src/protocol.rs ----
type RGB = [number, number, number];
interface DpiLevel { dpi: number; color: RGB }
interface LedEffect { mode: number; speed: number; brightness: number }
interface LogoLed extends LedEffect { color: RGB }
interface StripLed { mode: number; color: RGB; speed: number; brightness: number }
interface Button { kind: number; code: number; param: number }
interface Config {
  polling_hz: number;
  level_count: number;
  active_level: number;
  levels: DpiLevel[];
  dpi_led: LedEffect;
  logo_led: LogoLed;
  strip: StripLed;
  buttons: Button[];
}
interface Settings {
  sync_strip_to_dpi: boolean;
  show_osd: boolean;
  auto_switch: boolean;
  default_profile: string | null;
  tray_icon: "logo" | "dpi" | "battery";
}
interface Profile { id: string; name: string; config: Config; apps: string[] }
interface ProfilesView { profiles: Profile[]; active: string | null }
interface DeviceInfo { pid: number; wireless: boolean }
interface DeviceEvent { dpi_level: number | null; polling_hz: number | null }

// ---- Tabelas de domínio ----
const DPI_MIN = 100, DPI_MAX = 16000, DPI_STEP = 100;

const KIND_FACTORY_RESET = 9;

const FUNCTIONS: { label: string; b: Button }[] = [
  { label: "Clique esquerdo", b: { kind: 1, code: 0x01, param: 0 } },
  { label: "Clique direito", b: { kind: 1, code: 0x02, param: 0 } },
  { label: "Clique do meio", b: { kind: 1, code: 0x04, param: 0 } },
  { label: "Voltar", b: { kind: 1, code: 0x08, param: 0 } },
  { label: "Avançar", b: { kind: 1, code: 0x10, param: 0 } },
  { label: "DPI +", b: { kind: 2, code: 0x02, param: 0 } },
  { label: "DPI −", b: { kind: 2, code: 0x03, param: 0 } },
  { label: "DPI em ciclo", b: { kind: 2, code: 0x01, param: 0 } },
  { label: "Trocar polling rate", b: { kind: 7, code: 0, param: 0 } },
  { label: "Ligar/desligar luzes", b: { kind: 8, code: 0, param: 0 } },
  { label: "⚠ Reset de fábrica", b: { kind: KIND_FACTORY_RESET, code: 0, param: 0 } },
  { label: "Desativado", b: { kind: 0, code: 0, param: 0 } },
];

/** Modos da faixa lateral (byte 0 de 0xA0), na ordem do menu. */
const STRIP_MODES = [
  { mode: 0, label: "Onda arco-íris", color: false, speed: true },
  { mode: 3, label: "Onda colorida", color: true, speed: true },
  { mode: 1, label: "Respiração", color: true, speed: true },
  { mode: 2, label: "Fixo", color: true, speed: false },
  { mode: 4, label: "Desligado", color: false, speed: false },
];

/** Luz da roda (0x4C): sempre na cor do nível de DPI ativo. */
const WHEEL_MODES = [
  { mode: 1, label: "Fixa" },
  { mode: 2, label: "Respiração" },
  { mode: 0, label: "Desligada" },
];

// ---- Estado ----
let saved: Config | null = null; // o que está no mouse
let draft: Config | null = null; // o que está na tela
let settings: Settings = { sync_strip_to_dpi: false, show_osd: true, auto_switch: false, default_profile: null, tray_icon: "dpi" };
let profiles: Profile[] = [];
let activeProfile: string | null = null;
let busy = false;

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const sameButton = (a: Button, b: Button) => a.kind === b.kind && a.code === b.code && a.param === b.param;

const toHex = (c: RGB) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");
const fromHex = (h: string): RGB => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as RGB;
const activeColor = (c: Config): RGB => c.levels[c.active_level]?.color ?? [0, 0, 0];

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K, props: Partial<HTMLElementTagNameMap[K]> & Record<string, unknown> = {}, ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  Object.assign(e, props);
  e.append(...children);
  return e;
}

let toastTimer: number | undefined;
function toast(msg: string, kind: "ok" | "err" = "ok") {
  const t = $("#toast");
  t.textContent = msg;
  t.className = `toast show ${kind}`;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (t.className = "toast"), 3500);
}

function setStatus(state: "ok" | "wait" | "err", text: string) {
  $("#status-dot").className = `dot ${state}`;
  $("#status-text").textContent = text;
}

/** Aplica regras derivadas (ex.: faixa sincronizada com a cor do DPI). */
function normalize(c: Config | null) {
  if (c && settings.sync_strip_to_dpi) c.strip.color = activeColor(c);
}

function markDirty() {
  const dirty = !!saved && !!draft && JSON.stringify(saved) !== JSON.stringify(draft);
  $("#dirty-text").textContent = dirty ? "Alterações não aplicadas" : "Sem alterações";
  $("#applybar").classList.toggle("dirty", dirty);
  $<HTMLButtonElement>("#btn-apply").disabled = !dirty || busy;
  $<HTMLButtonElement>("#btn-discard").disabled = !dirty || busy;
}

function update(fn: (c: Config) => void, rerender = false) {
  if (!draft) return;
  fn(draft);
  normalize(draft);
  if (rerender) render();
  markDirty();
}

// ---- DPI ----
function renderDpi(c: Config) {
  const count = $<HTMLSelectElement>("#level-count");
  count.replaceChildren(...Array.from({ length: 8 }, (_, i) => el("option", { value: String(i + 1), textContent: String(i + 1) })));
  count.value = String(c.level_count);
  count.onchange = () => update((d) => {
    d.level_count = Number(count.value);
    d.active_level = Math.min(d.active_level, d.level_count - 1);
  }, true);

  const list = $("#dpi-levels");
  list.replaceChildren(...c.levels.map((lvl, i) => {
    const enabled = i < c.level_count;
    const radio = el("input", { type: "radio", name: "active", checked: i === c.active_level, disabled: !enabled, title: "Nível ativo" });
    radio.onchange = () => update((d) => (d.active_level = i), true);

    const color = el("input", { type: "color", value: toHex(lvl.color), title: "Cor do nível (luz da roda)" });
    color.oninput = () => update((d) => (d.levels[i].color = fromHex(color.value)));

    const slider = el("input", { type: "range", min: String(DPI_MIN), max: String(DPI_MAX), step: String(DPI_STEP), value: String(lvl.dpi) });
    const num = el("input", { type: "number", min: String(DPI_MIN), max: String(DPI_MAX), step: String(DPI_STEP), value: String(lvl.dpi), className: "dpi-num" });
    const setDpi = (v: number) => {
      v = Math.round(Math.min(DPI_MAX, Math.max(DPI_MIN, v || DPI_MIN)) / DPI_STEP) * DPI_STEP;
      slider.value = num.value = String(v);
      update((d) => (d.levels[i].dpi = v));
    };
    slider.oninput = () => setDpi(Number(slider.value));
    num.onchange = () => setDpi(Number(num.value));

    return el("div", { className: `level ${enabled ? "" : "disabled"} ${i === c.active_level ? "active" : ""}` },
      radio, el("span", { className: "idx", textContent: String(i + 1) }), color, slider, num, el("span", { className: "unit", textContent: "DPI" }));
  }));
}

// ---- Botões ----
let selectedButton = 1;

function selectButton(n: number, focusSelect = false) {
  selectedButton = n;
  document.querySelectorAll<SVGGElement>(".mouse-svg .hot").forEach((g) => g.classList.toggle("selected", g.dataset.n === String(n)));
  document.querySelectorAll<HTMLElement>("#button-list .row").forEach((r) => r.classList.toggle("selected", r.dataset.n === String(n)));
  const row = document.querySelector<HTMLElement>(`#button-list .row[data-n="${n}"]`);
  row?.scrollIntoView({ block: "nearest" });
  if (focusSelect) row?.querySelector("select")?.focus();
}

function functionLabel(b: Button): string {
  return FUNCTIONS.find((f) => sameButton(f.b, b))?.label ?? `Tipo ${b.kind} · 0x${b.code.toString(16)}`;
}

function stripLook(c: Config): StripLook {
  if (c.strip.mode === 4) return null;
  if (c.strip.mode === 0) return "rainbow";
  return toHex(c.strip.color);
}

function renderButtons(c: Config) {
  const svg = mouseSvg(stripLook(c), c.dpi_led.mode !== 0 ? toHex(activeColor(c)) : null);

  svg.querySelectorAll<SVGGElement>(".hot").forEach((g) => {
    const n = Number(g.dataset.n);
    const b = PHYSICAL_BUTTONS.find((p) => p.n === n)!;
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = `${b.name}: ${functionLabel(c.buttons[b.slot])}`;
    g.prepend(title);
    g.onclick = () => selectButton(n, true);
    g.onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); selectButton(n, true); } };
    g.onmouseenter = () => document.querySelector(`#button-list .row[data-n="${n}"]`)?.classList.add("hover");
    g.onmouseleave = () => document.querySelector(`#button-list .row[data-n="${n}"]`)?.classList.remove("hover");
  });
  $("#mouse-view").replaceChildren(svg);

  const list = $("#button-list");
  list.replaceChildren(...PHYSICAL_BUTTONS.map(({ n, name, slot }) => {
    const cur = c.buttons[slot];
    const sel = el("select");
    let known = false;
    FUNCTIONS.forEach((f, i) => {
      const match = sameButton(f.b, cur);
      known ||= match;
      sel.append(el("option", { value: String(i), textContent: f.label, selected: match }));
    });
    if (!known) {
      sel.prepend(el("option", {
        value: "keep", selected: true,
        textContent: `Função atual (tipo ${cur.kind}, código 0x${cur.code.toString(16)})`,
      }));
    }
    sel.onchange = async () => {
      if (sel.value === "keep") return;
      const f = FUNCTIONS[Number(sel.value)];
      if (f.b.kind === KIND_FACTORY_RESET) {
        const ok = await ask(
          "Ao ser pressionado, este botão apaga TODA a configuração do mouse (DPIs, cores, botões, polling) e volta ao padrão de fábrica.\n\nAntes de gravar, o app salva um backup automático. Continuar?",
          { title: "Reset de fábrica", kind: "warning", okLabel: "Usar mesmo assim", cancelLabel: "Cancelar" },
        );
        if (!ok) { renderButtons(draft!); return; }
      }
      update((d) => (d.buttons[slot] = { ...f.b }), true);
      selectButton(n);
    };
    sel.onfocus = () => selectButton(n);
    const row = el("div", { className: "row" },
      el("span", { className: "label" }, el("span", { className: "num", textContent: String(n) }), name), sel);
    row.dataset.n = String(n);
    row.onclick = () => selectButton(n);
    return row;
  }));
  selectButton(selectedButton);
}

// ---- Iluminação ----

/** Faixa de bytes de velocidade por tipo de efeito: [mais rápido, mais lento]. Vai além do app oficial. */
const STRIP_SPEED_RANGE: Record<number, [number, number]> = {
  0: [0x01, 0x60], // onda arco-íris (oficial: 0x03..0x2D)
  3: [0x01, 0x60], // onda colorida
  1: [0x10, 0xff], // respiração (oficial: 0x28..0xFF)
};
const speedRange = (mode: number) => STRIP_SPEED_RANGE[mode] ?? STRIP_SPEED_RANGE[0];
const speedToPct = (raw: number, mode: number) => {
  const [fast, slow] = speedRange(mode);
  return Math.round(1 + ((slow - Math.min(slow, Math.max(fast, raw))) / (slow - fast)) * 99);
};
const pctToSpeed = (pct: number, mode: number) => {
  const [fast, slow] = speedRange(mode);
  return Math.round(slow - ((pct - 1) / 99) * (slow - fast));
};
const brightToPct = (raw: number) => Math.max(1, Math.round((raw * 100) / 255));
const pctToBright = (pct: number) => Math.max(1, Math.round((pct * 255) / 100));

function pctSlider(value: number, onChange: (v: number) => void, disabled = false): HTMLElement {
  const input = el("input", { type: "range", min: "1", max: "100", step: "1", value: String(value), disabled });
  const label = el("span", { className: "mono pct", textContent: `${value}%` });
  input.oninput = () => { label.textContent = `${input.value}%`; onChange(Number(input.value)); };
  return el("div", { className: "inline" }, input, label);
}
function levelSlider(value: number, onChange: (v: number) => void, disabled = false): HTMLElement {
  const input = el("input", { type: "range", min: "1", max: "10", step: "1", value: String(value), disabled });
  const label = el("span", { className: "mono", textContent: String(value) });
  input.oninput = () => { label.textContent = input.value; onChange(Number(input.value)); };
  return el("div", { className: "inline" }, input, label);
}

function renderLighting() {
  const c = draft!;
  const s = c.strip;
  const info = STRIP_MODES.find((m) => m.mode === s.mode);

  const mode = el("select");
  STRIP_MODES.forEach((m) => mode.append(el("option", { value: String(m.mode), textContent: m.label, selected: m.mode === s.mode })));
  if (!info) mode.append(el("option", { value: String(s.mode), textContent: `Modo ${s.mode} (desconhecido)`, selected: true }));
  mode.onchange = () => update((d) => {
    const pct = speedToPct(d.strip.speed, d.strip.mode);
    d.strip.mode = Number(mode.value);
    d.strip.speed = pctToSpeed(pct, d.strip.mode);
  }, true);

  const synced = settings.sync_strip_to_dpi;
  const color = el("input", { type: "color", value: toHex(s.color), disabled: !info?.color || synced });
  color.oninput = () => update((d) => (d.strip.color = fromHex(color.value)));

  const sync = el("input", { type: "checkbox", checked: synced });
  sync.onchange = async () => {
    settings = { ...settings, sync_strip_to_dpi: sync.checked };
    try {
      await invoke("set_settings", { settings });
    } catch (e) {
      toast(`Configuração: ${e}`, "err");
    }
    update(() => {}, true);
  };

  $("#led-strip").replaceChildren(
    el("h3", { textContent: "Faixa RGB lateral" }),
    el("label", { className: "field" }, "Efeito", mode),
    el("label", { className: "field" }, "Cor", color),
    el("label", { className: "field check" }, sync, el("span", {}, "Acompanhar a cor do nível de DPI",
      el("small", { textContent: "Ao trocar o DPI, a faixa assume a cor do nível. Precisa do app aberto ou na bandeja." }))),
    el("label", { className: "field" }, "Velocidade",
      pctSlider(speedToPct(s.speed, s.mode), (v) => update((d) => (d.strip.speed = pctToSpeed(v, d.strip.mode))), !info?.speed)),
    el("label", { className: "field" }, "Brilho",
      pctSlider(brightToPct(s.brightness), (v) => update((d) => (d.strip.brightness = pctToBright(v))), s.mode === 4)),
    el("p", { className: "hint", textContent: "Velocidade e brilho vão além dos limites do app oficial." }),
  );

  const w = c.dpi_led;
  const wmode = el("select");
  WHEEL_MODES.forEach((m) => wmode.append(el("option", { value: String(m.mode), textContent: m.label, selected: m.mode === w.mode })));
  if (!WHEEL_MODES.some((m) => m.mode === w.mode)) wmode.append(el("option", { value: String(w.mode), textContent: `Modo ${w.mode}`, selected: true }));
  wmode.onchange = () => update((d) => (d.dpi_led.mode = Number(wmode.value)), true);

  $("#led-wheel").replaceChildren(
    el("h3", { textContent: "Luz da roda (indicador de DPI)" }),
    el("p", { className: "hint", textContent: "Mostra a cor do nível de DPI ativo — as cores ficam na aba DPI." }),
    el("label", { className: "field" }, "Efeito", wmode),
    el("label", { className: "field" }, "Velocidade", levelSlider(w.speed, (v) => update((d) => (d.dpi_led.speed = v)), w.mode !== 2)),
  );
}

function render() {
  if (!draft) return;
  renderDpi(draft);
  renderButtons(draft);
  renderLighting();
  $<HTMLSelectElement>("#polling").value = String(draft.polling_hz);
  markDirty();
}

// ---- Perfis ----
const isDirty = () => !!saved && !!draft && JSON.stringify(saved) !== JSON.stringify(draft);

async function saveProfiles() {
  try {
    await invoke("save_profiles", { profiles });
  } catch (e) {
    toast(`Perfis: ${e}`, "err");
  }
}

async function saveSettings() {
  try {
    await invoke("set_settings", { settings });
  } catch (e) {
    toast(`Configuração: ${e}`, "err");
  }
}

function profileSummary(c: Config): string {
  const dpis = c.levels.slice(0, c.level_count).map((l) => l.dpi).join(" / ");
  return `${dpis} DPI · ${c.polling_hz} Hz`;
}

async function pickApp(p: Profile) {
  const path = await open({ multiple: false, filters: [{ name: "Programa", extensions: ["exe"] }] });
  if (!path || Array.isArray(path)) return;
  addApp(p, path.split(/[\\/]/).pop()!);
}

function addApp(p: Profile, name: string) {
  name = name.trim().toLowerCase();
  if (!name) return;
  if (!name.endsWith(".exe")) name += ".exe";
  if (!p.apps.includes(name)) p.apps.push(name);
  saveProfiles();
  renderProfiles();
}

function renderProfiles() {
  $<HTMLInputElement>("#auto-switch").checked = settings.auto_switch;

  const def = $<HTMLSelectElement>("#default-profile");
  def.replaceChildren(el("option", { value: "", textContent: "Nenhum (mantém o atual)" }),
    ...profiles.map((p) => el("option", { value: p.id, textContent: p.name })));
  def.value = settings.default_profile ?? "";

  const list = $("#profile-list");
  if (!profiles.length) {
    list.replaceChildren(el("p", { className: "hint", textContent: "Nenhum perfil ainda. Ajuste o mouse e clique em \"Novo perfil\"." }));
    return;
  }
  list.replaceChildren(...profiles.map((p) => {
    const name = el("input", { type: "text", value: p.name, className: "profile-name", maxLength: 40 });
    name.onchange = () => { p.name = name.value.trim() || "Sem nome"; saveProfiles(); renderProfiles(); };

    const chips = el("div", { className: "chips" }, ...p.apps.map((a) => {
      const x = el("button", { className: "chip-x", title: `Remover ${a}`, textContent: "×" });
      x.onclick = () => { p.apps = p.apps.filter((b) => b !== a); saveProfiles(); renderProfiles(); };
      return el("span", { className: "chip" }, a, x);
    }));
    const typed = el("input", { type: "text", placeholder: "programa.exe", className: "app-input" });
    typed.onkeydown = (e) => { if (e.key === "Enter") addApp(p, typed.value); };
    const browse = el("button", { className: "ghost small", textContent: "Procurar…" });
    browse.onclick = () => pickApp(p);

    const applyBtn = el("button", { className: "primary small", textContent: "Aplicar no mouse" });
    applyBtn.onclick = () => guarded("Perfil", async () => {
      const cfg = await invoke<Config>("apply_profile", { id: p.id });
      activeProfile = p.id;
      saved = cfg;
      draft = clone(cfg);
      render();
      renderProfiles();
      toast(`Perfil "${p.name}" aplicado`);
    });
    const updateBtn = el("button", { className: "ghost small", textContent: "Usar configuração atual",
      title: "Substitui o conteúdo do perfil pela configuração que está no mouse agora" });
    updateBtn.onclick = () => {
      if (!saved) return;
      if (isDirty()) { toast("Aplique ou descarte as alterações antes.", "err"); return; }
      p.config = clone(saved);
      saveProfiles();
      renderProfiles();
      toast(`Perfil "${p.name}" atualizado`);
    };
    const del = el("button", { className: "ghost small danger", textContent: "Excluir" });
    del.onclick = async () => {
      if (!(await ask(`Excluir o perfil "${p.name}"?`, { title: "Perfis", kind: "warning" }))) return;
      profiles = profiles.filter((q) => q.id !== p.id);
      if (settings.default_profile === p.id) settings.default_profile = null;
      if (activeProfile === p.id) activeProfile = null;
      await saveProfiles();
      renderProfiles();
    };

    return el("div", { className: `card profile ${p.id === activeProfile ? "active" : ""}` },
      el("div", { className: "profile-head" }, name,
        p.id === activeProfile ? el("span", { className: "tag", textContent: "Ativo" }) : "",
        el("span", { className: "mono", textContent: profileSummary(p.config) })),
      el("div", { className: "field" }, "Programas", el("div", { className: "inline" }, typed, browse)),
      chips,
      el("div", { className: "actions" }, applyBtn, updateBtn, del),
    );
  }));
}

async function newProfile() {
  if (!saved) { toast("Conecte o mouse primeiro.", "err"); return; }
  if (isDirty()) { toast("Aplique ou descarte as alterações antes.", "err"); return; }
  profiles.push({ id: crypto.randomUUID(), name: `Perfil ${profiles.length + 1}`, config: clone(saved), apps: [] });
  await saveProfiles();
  renderProfiles();
  document.querySelector<HTMLInputElement>("#profile-list .card:last-child .profile-name")?.select();
}

// ---- Bateria ----
function renderBattery(level: number | null) {
  const b = $("#battery");
  b.hidden = level == null;
  if (level == null) return;
  b.querySelector("b")!.textContent = `${level}%`;
  b.querySelector<HTMLElement>("i")!.style.setProperty("--level", `${level}%`);
  b.classList.toggle("low", level <= 20);
  b.title = `Bateria do mouse: ${level}%`;
}

// ---- Ações ----
async function guarded<T>(label: string, fn: () => Promise<T>): Promise<T | undefined> {
  if (busy) return;
  busy = true;
  markDirty();
  document.body.classList.add("busy");
  try {
    return await fn();
  } catch (e) {
    toast(`${label}: ${e}`, "err");
    setStatus("err", String(e));
  } finally {
    busy = false;
    document.body.classList.remove("busy");
    markDirty();
  }
}

async function connect() {
  setStatus("wait", "Procurando mouse…");
  await guarded("Conexão", async () => {
    const info = await invoke<DeviceInfo>("connect");
    $("#device-info").textContent = `VID 25A7 · PID ${info.pid.toString(16).toUpperCase()} · ${info.wireless ? "receptor 2.4 GHz" : "cabo USB"}`;
    setStatus("wait", "Lendo configuração…");
    saved = await invoke<Config>("read_config");
    draft = clone(saved);
    normalize(draft);
    render();
    setStatus("ok", `Conectado${info.wireless ? " (sem fio)" : ""}`);
    renderBattery(await invoke<number | null>("get_battery"));
  });
}

async function apply() {
  if (!draft) return;
  if (!draft.buttons.some((b) => b.kind === 1 && b.code === 0x01)) {
    toast("Pelo menos um botão precisa ser clique esquerdo.", "err");
    return;
  }
  await guarded("Aplicar", async () => {
    const n = await invoke<number>("apply_config", { config: draft });
    saved = clone(draft!);
    toast(n ? `Aplicado (${n} bytes gravados)` : "Nada para gravar");
  });
}

async function backup() {
  const path = await save({ defaultPath: "m808-backup.m808bak", filters: [{ name: "Backup M808", extensions: ["m808bak"] }] });
  if (!path) return;
  await guarded("Backup", async () => {
    await invoke("save_backup", { path });
    toast("Backup salvo");
  });
}

async function restore() {
  const path = await open({ multiple: false, filters: [{ name: "Backup M808", extensions: ["m808bak"] }] });
  if (!path || Array.isArray(path)) return;
  await guarded("Restaurar", async () => {
    saved = await invoke<Config>("restore_backup", { path });
    draft = clone(saved);
    render();
    toast("Backup restaurado no mouse");
  });
}

// ---- Inicialização ----
window.addEventListener("DOMContentLoaded", async () => {
  document.querySelectorAll<HTMLButtonElement>(".tabs button").forEach((b) => {
    b.onclick = () => {
      document.querySelectorAll(".tabs button").forEach((x) => x.classList.toggle("active", x === b));
      document.querySelectorAll(".panel").forEach((p) => p.classList.toggle("active", p.id === `tab-${b.dataset.tab}`));
      if (b.dataset.tab === "buttons" && draft) renderButtons(draft); // reflete as luzes atuais
    };
  });

  const autostart = $<HTMLInputElement>("#autostart");
  invoke<boolean>("get_autostart").then((v) => (autostart.checked = v)).catch(() => {});
  autostart.onchange = async () => {
    try {
      await invoke("set_autostart", { enabled: autostart.checked });
    } catch (e) {
      autostart.checked = !autostart.checked;
      toast(`Inicialização automática: ${e}`, "err");
    }
  };
  $<HTMLSelectElement>("#polling").onchange = (e) => update((d) => (d.polling_hz = Number((e.target as HTMLSelectElement).value)));
  $("#btn-reconnect").onclick = connect;
  $("#btn-apply").onclick = apply;
  $("#btn-discard").onclick = () => { draft = clone(saved); normalize(draft); render(); };
  $("#btn-backup").onclick = backup;
  $("#btn-restore").onclick = restore;

  // Mudanças feitas pelos botões do próprio mouse (ou pela bandeja): refletem sem virar "alteração".
  listen<DeviceEvent>("device-event", ({ payload }) => {
    if (!saved || !draft) return;
    for (const c of [saved, draft]) {
      if (payload.dpi_level != null) c.active_level = payload.dpi_level;
      if (payload.polling_hz != null) c.polling_hz = payload.polling_hz;
      normalize(c);
    }
    render();
  });
  listen("device-lost", () => setStatus("err", "Mouse desconectado — aguardando…"));
  // O processo de fundo reconecta sozinho quando o mouse volta.
  listen("device-connected", () => { if (!busy) connect(); });

  const showOsd = $<HTMLInputElement>("#show-osd");
  showOsd.onchange = () => { settings.show_osd = showOsd.checked; saveSettings(); };
  $<HTMLInputElement>("#auto-switch").onchange = (e) => {
    settings.auto_switch = (e.target as HTMLInputElement).checked;
    saveSettings();
  };
  $<HTMLSelectElement>("#default-profile").onchange = (e) => {
    settings.default_profile = (e.target as HTMLSelectElement).value || null;
    saveSettings();
  };
  $("#btn-new-profile").onclick = newProfile;

  // Perfil aplicado pela troca automática ou pela bandeja.
  listen<Config>("config-changed", async ({ payload }) => {
    const hadEdits = isDirty();
    saved = payload;
    if (!hadEdits) draft = clone(payload);
    normalize(draft);
    render();
    const view = await invoke<ProfilesView>("get_profiles");
    activeProfile = view.active;
    renderProfiles();
    if (hadEdits) toast("Um perfil foi aplicado; suas alterações não salvas foram mantidas na tela.");
  });
  listen<Settings>("settings-changed", ({ payload }) => { settings = payload; renderProfiles(); });
  listen<number | null>("battery", ({ payload }) => renderBattery(payload));

  settings = await invoke<Settings>("get_settings").catch(() => settings);
  showOsd.checked = settings.show_osd;
  const trayIcon = $<HTMLSelectElement>("#tray-icon");
  trayIcon.value = settings.tray_icon;
  trayIcon.onchange = () => { settings.tray_icon = trayIcon.value as Settings["tray_icon"]; saveSettings(); };
  const view = await invoke<ProfilesView>("get_profiles").catch(() => ({ profiles: [], active: null }));
  profiles = view.profiles;
  activeProfile = view.active;
  renderProfiles();
  connect();
});
