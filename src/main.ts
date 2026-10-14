import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { applyStatic, setLang, t, type I18nKey, type LangSetting } from "./i18n";
import { HID_BY_CODE, MEDIA_KEYS, MODIFIERS, MODIFIER_CODES, keyName } from "./keys";
import { PHYSICAL_BUTTONS, mouseSvg, type StripLook } from "./mouse";

// ---- Tipos espelhando src-tauri/src (protocol.rs, profiles.rs, lib.rs) ----
type RGB = [number, number, number];
interface DpiLevel { dpi: number; color: RGB }
interface LedEffect { mode: number; speed: number; brightness: number }
interface LogoLed extends LedEffect { color: RGB }
interface StripLed { mode: number; color: RGB; speed: number; brightness: number }
interface Button { kind: number; code: number; param: number }
type KeyAction = { type: "keys"; modifiers: number; keys: number[] } | { type: "media"; usage: number };
interface Config {
  polling_hz: number;
  level_count: number;
  active_level: number;
  levels: DpiLevel[];
  dpi_led: LedEffect;
  logo_led: LogoLed;
  strip: StripLed;
  buttons: Button[];
  key_actions: (KeyAction | null)[];
}
interface Settings {
  sync_strip_to_dpi: boolean;
  show_osd: boolean;
  auto_switch: boolean;
  default_profile: string | null;
  tray_icon: "logo" | "dpi" | "battery";
  language: LangSetting;
  low_battery_profile: string | null;
  low_battery_threshold: number;
}
/** Dias: 0 = domingo … 6 = sábado. Horários "HH:MM"; se fim < início, cruza a meia-noite. */
interface Schedule { start: string; end: string; days: number[] }
interface Profile { id: string; name: string; config: Config; apps: string[]; schedule: Schedule | null }
interface ProfilesView { profiles: Profile[]; active: string | null }
interface UpdateInfo { version: string; current: string; notes: string | null }
interface DeviceInfo { pid: number; wireless: boolean }
interface DeviceEvent { dpi_level: number | null; polling_hz: number | null }

// ---- Tabelas de domínio ----
const DPI_MIN = 100, DPI_MAX = 16000, DPI_STEP = 100;

const KIND_KEYS = 5;
const KIND_FACTORY_RESET = 9;

/** `action` marca as entradas do tipo tecla (o mesmo `kind` 5, distinguidas pela ação). */
const FUNCTIONS: { label: I18nKey; b: Button; action?: KeyAction["type"] }[] = [
  { label: "fn.leftClick", b: { kind: 1, code: 0x01, param: 0 } },
  { label: "fn.rightClick", b: { kind: 1, code: 0x02, param: 0 } },
  { label: "fn.middleClick", b: { kind: 1, code: 0x04, param: 0 } },
  { label: "fn.back", b: { kind: 1, code: 0x08, param: 0 } },
  { label: "fn.forward", b: { kind: 1, code: 0x10, param: 0 } },
  { label: "fn.dpiUp", b: { kind: 2, code: 0x02, param: 0 } },
  { label: "fn.dpiDown", b: { kind: 2, code: 0x03, param: 0 } },
  { label: "fn.dpiCycle", b: { kind: 2, code: 0x01, param: 0 } },
  { label: "fn.polling", b: { kind: 7, code: 0, param: 0 } },
  { label: "fn.lights", b: { kind: 8, code: 0, param: 0 } },
  { label: "fn.keys", b: { kind: KIND_KEYS, code: 0, param: 0 }, action: "keys" },
  { label: "fn.media", b: { kind: KIND_KEYS, code: 0, param: 0 }, action: "media" },
  { label: "fn.reset", b: { kind: KIND_FACTORY_RESET, code: 0, param: 0 } },
  { label: "fn.disabled", b: { kind: 0, code: 0, param: 0 } },
];

/** Modos da faixa lateral (byte 0 de 0xA0), na ordem do menu. */
const STRIP_MODES: { mode: number; label: I18nKey; color: boolean; speed: boolean }[] = [
  { mode: 0, label: "strip.rainbow", color: false, speed: true },
  { mode: 3, label: "strip.colorWave", color: true, speed: true },
  { mode: 1, label: "strip.breath", color: true, speed: true },
  { mode: 2, label: "strip.steady", color: true, speed: false },
  { mode: 4, label: "strip.off", color: false, speed: false },
];

/** Luz da roda (0x4C): sempre na cor do nível de DPI ativo. */
const WHEEL_MODES: { mode: number; label: I18nKey }[] = [
  { mode: 1, label: "wheel.steady" },
  { mode: 2, label: "wheel.breath" },
  { mode: 0, label: "wheel.off" },
];

// ---- Estado ----
let saved: Config | null = null; // o que está no mouse
let draft: Config | null = null; // o que está na tela
let settings: Settings = {
  sync_strip_to_dpi: false, show_osd: true, auto_switch: false, default_profile: null, tray_icon: "dpi",
  language: "auto", low_battery_profile: null, low_battery_threshold: 20,
};
let profiles: Profile[] = [];
let activeProfile: string | null = null;
let deviceInfo: DeviceInfo | null = null;
let batteryLevel: number | null = null;
let pendingUpdate: UpdateInfo | null = null;
let busy = false;

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const sameButton = (a: Button, b: Button) => a.kind === b.kind && a.code === b.code && a.param === b.param;
const hex2 = (n: number) => `0x${n.toString(16)}`;

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
  const box = $("#toast");
  box.textContent = msg;
  box.className = `toast show ${kind}`;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (box.className = "toast"), 3500);
}

type StatusState = "ok" | "wait" | "err";
let status: { state: StatusState; key: I18nKey; extra?: string } = { state: "wait", key: "status.searching" };
function setStatus(state: StatusState, key: I18nKey, extra?: string) {
  status = { state, key, extra };
  renderStatus();
}
function renderStatus() {
  $("#status-dot").className = `dot ${status.state}`;
  $("#status-text").textContent = status.extra ?? t(status.key);
}

/** Aplica regras derivadas (ex.: faixa sincronizada com a cor do DPI). */
function normalize(c: Config | null) {
  if (c && settings.sync_strip_to_dpi) c.strip.color = activeColor(c);
}

const isDirty = () => !!saved && !!draft && JSON.stringify(saved) !== JSON.stringify(draft);

function markDirty() {
  const dirty = isDirty();
  $("#dirty-text").textContent = t(dirty ? "apply.dirty" : "apply.clean");
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
    const radio = el("input", { type: "radio", name: "active", checked: i === c.active_level, disabled: !enabled, title: t("dpi.active") });
    radio.onchange = () => update((d) => (d.active_level = i), true);

    const color = el("input", { type: "color", value: toHex(lvl.color), title: t("dpi.color") });
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

function actionLabel(a: KeyAction | null | undefined): string {
  if (!a) return t("keys.none");
  if (a.type === "media") {
    const m = MEDIA_KEYS.find((k) => k.usage === a.usage);
    return m ? t(m.label) : `0x${a.usage.toString(16)}`;
  }
  const parts = MODIFIERS.filter((m) => a.modifiers & m.bit).map((m) => m.label);
  parts.push(...a.keys.map(keyName));
  return parts.length ? parts.join(" + ") : t("keys.none");
}

/** Índice em FUNCTIONS que corresponde ao que está gravado no slot. */
function functionIndex(c: Config, slot: number): number {
  const b = c.buttons[slot];
  if (b.kind === KIND_KEYS) return FUNCTIONS.findIndex((f) => f.action === (c.key_actions[slot]?.type ?? "keys"));
  return FUNCTIONS.findIndex((f) => !f.action && sameButton(f.b, b));
}

function functionLabel(c: Config, slot: number): string {
  const b = c.buttons[slot];
  if (b.kind === KIND_KEYS) return actionLabel(c.key_actions[slot]);
  const i = functionIndex(c, slot);
  return i >= 0 ? t(FUNCTIONS[i].label) : t("buttons.unknown", { kind: b.kind, code: hex2(b.code) });
}

/** Ação de tecla incompleta (nada para enviar). */
const emptyAction = (a: KeyAction | null | undefined) => !a || (a.type === "keys" && !a.keys.length && !a.modifiers);

function keyEditor(slot: number): HTMLElement | null {
  const c = draft!;
  if (c.buttons[slot].kind !== KIND_KEYS) return null;
  const action = c.key_actions[slot];

  if (action?.type === "media") {
    const sel = el("select");
    MEDIA_KEYS.forEach((m) => sel.append(el("option", { value: String(m.usage), textContent: t(m.label), selected: m.usage === action.usage })));
    sel.onchange = () => update((d) => (d.key_actions[slot] = { type: "media", usage: Number(sel.value) }), true);
    return el("div", { className: "key-editor" }, sel);
  }

  const cur = action?.type === "keys" ? action : { type: "keys" as const, modifiers: 0, keys: [] };
  const setKeys = (modifiers: number, keys: number[]) =>
    update((d) => (d.key_actions[slot] = { type: "keys", modifiers, keys }), true);

  const mods = MODIFIERS.map((m) => {
    const b = el("button", { className: `mod ${cur.modifiers & m.bit ? "on" : ""}`, textContent: m.label });
    b.onclick = (e) => { e.stopPropagation(); setKeys(cur.modifiers ^ m.bit, cur.keys); };
    return b;
  });

  // Captura: aperta a combinação no teclado. Só modificadores (ex.: Shift sozinho) também vale.
  const capture = el("input", { type: "text", readOnly: true, className: "key-capture",
    value: emptyAction(cur) ? "" : actionLabel(cur), placeholder: t("keys.capture") });
  let held = 0;
  let gotKey = false;
  capture.onfocus = () => { held = 0; gotKey = false; capture.placeholder = t("keys.press"); };
  capture.onblur = () => { capture.placeholder = t("keys.capture"); };
  capture.onkeydown = (e) => {
    e.preventDefault();
    e.stopPropagation();
    const mod = MODIFIER_CODES[e.code];
    if (mod) { held |= mod; return; }
    const hid = HID_BY_CODE[e.code];
    if (hid == null) return;
    gotKey = true;
    const modifiers = (e.ctrlKey ? 1 : 0) | (e.shiftKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
    setKeys(modifiers, [hid]);
  };
  capture.onkeyup = (e) => {
    if (MODIFIER_CODES[e.code] && !gotKey && held) { setKeys(held, []); held = 0; }
  };
  const clear = el("button", { className: "ghost small", textContent: t("keys.clear") });
  clear.onclick = (e) => { e.stopPropagation(); setKeys(0, []); };

  return el("div", { className: "key-editor" }, ...mods, capture, clear);
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
    title.textContent = `${t(b.name)}: ${functionLabel(c, b.slot)}`;
    g.prepend(title);
    g.setAttribute("aria-label", `${n} ${t(b.name)}`);
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
    const selected = functionIndex(c, slot);
    FUNCTIONS.forEach((f, i) => sel.append(el("option", { value: String(i), textContent: t(f.label), selected: i === selected })));
    if (selected < 0) {
      sel.prepend(el("option", { value: "keep", selected: true, textContent: t("buttons.current", { kind: cur.kind, code: hex2(cur.code) }) }));
    }
    sel.onchange = async () => {
      if (sel.value === "keep") return;
      const f = FUNCTIONS[Number(sel.value)];
      if (f.b.kind === KIND_FACTORY_RESET) {
        const ok = await ask(t("reset.body"), { title: t("reset.title"), kind: "warning", okLabel: t("reset.ok"), cancelLabel: t("common.cancel") });
        if (!ok) { renderButtons(draft!); return; }
      }
      update((d) => {
        d.buttons[slot] = { ...f.b };
        if (f.action === "media") d.key_actions[slot] = { type: "media", usage: MEDIA_KEYS[0].usage };
        else if (f.action === "keys" && d.key_actions[slot]?.type !== "keys") d.key_actions[slot] = { type: "keys", modifiers: 0, keys: [] };
      }, true);
      selectButton(n);
      if (f.action === "keys") document.querySelector<HTMLInputElement>(`#button-list .row[data-n="${n}"] .key-capture`)?.focus();
    };
    sel.onfocus = () => selectButton(n);
    const editor = keyEditor(slot);
    const row = el("div", { className: `row ${editor ? "has-editor" : ""}` },
      el("span", { className: "label" }, el("span", { className: "num", textContent: String(n) }), t(name)), sel,
      ...(editor ? [editor] : []));
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

function rangeSlider(max: number, value: number, suffix: string, onChange: (v: number) => void, disabled = false): HTMLElement {
  const input = el("input", { type: "range", min: "1", max: String(max), step: "1", value: String(value), disabled });
  const label = el("span", { className: "mono pct", textContent: `${value}${suffix}` });
  input.oninput = () => { label.textContent = `${input.value}${suffix}`; onChange(Number(input.value)); };
  return el("div", { className: "inline" }, input, label);
}

function renderLighting() {
  const c = draft!;
  const s = c.strip;
  const info = STRIP_MODES.find((m) => m.mode === s.mode);

  const mode = el("select");
  STRIP_MODES.forEach((m) => mode.append(el("option", { value: String(m.mode), textContent: t(m.label), selected: m.mode === s.mode })));
  if (!info) mode.append(el("option", { value: String(s.mode), textContent: t("light.unknownMode", { mode: s.mode }), selected: true }));
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
    await saveSettings();
    update(() => {}, true);
  };

  $("#led-strip").replaceChildren(
    el("h3", { textContent: t("light.strip") }),
    el("label", { className: "field" }, t("light.effect"), mode),
    el("label", { className: "field" }, t("light.color"), color),
    el("label", { className: "field check" }, sync, el("span", {}, t("light.sync"), el("small", { textContent: t("light.syncHint") }))),
    el("label", { className: "field" }, t("light.speed"),
      rangeSlider(100, speedToPct(s.speed, s.mode), "%", (v) => update((d) => (d.strip.speed = pctToSpeed(v, d.strip.mode))), !info?.speed)),
    el("label", { className: "field" }, t("light.brightness"),
      rangeSlider(100, brightToPct(s.brightness), "%", (v) => update((d) => (d.strip.brightness = pctToBright(v))), s.mode === 4)),
    el("p", { className: "hint", textContent: t("light.beyond") }),
  );

  const w = c.dpi_led;
  const wmode = el("select");
  WHEEL_MODES.forEach((m) => wmode.append(el("option", { value: String(m.mode), textContent: t(m.label), selected: m.mode === w.mode })));
  if (!WHEEL_MODES.some((m) => m.mode === w.mode)) wmode.append(el("option", { value: String(w.mode), textContent: t("light.unknownMode", { mode: w.mode }), selected: true }));
  wmode.onchange = () => update((d) => (d.dpi_led.mode = Number(wmode.value)), true);

  $("#led-wheel").replaceChildren(
    el("h3", { textContent: t("light.wheel") }),
    el("p", { className: "hint", textContent: t("light.wheelHint") }),
    el("label", { className: "field" }, t("light.effect"), wmode),
    el("label", { className: "field" }, t("light.speed"), rangeSlider(10, w.speed, "", (v) => update((d) => (d.dpi_led.speed = v)), w.mode !== 2)),
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
async function saveProfiles() {
  try {
    await invoke("save_profiles", { profiles });
  } catch (e) {
    toast(`${t("err.profiles")}: ${e}`, "err");
  }
}

async function saveSettings() {
  try {
    await invoke("set_settings", { settings });
  } catch (e) {
    toast(`${t("err.settings")}: ${e}`, "err");
  }
}

function profileSummary(c: Config): string {
  const dpis = c.levels.slice(0, c.level_count).map((l) => l.dpi).join(" / ");
  return `${dpis} DPI · ${c.polling_hz} Hz`;
}

async function pickApp(p: Profile) {
  const path = await open({ multiple: false, filters: [{ name: t("profiles.programFilter"), extensions: ["exe"] }] });
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

function profileOptions(none: I18nKey, value: string | null): HTMLOptionElement[] {
  return [el("option", { value: "", textContent: t(none) }),
    ...profiles.map((p) => el("option", { value: p.id, textContent: p.name, selected: p.id === value }))];
}

function scheduleEditor(p: Profile): HTMLElement {
  const enabled = el("input", { type: "checkbox", checked: !!p.schedule });
  const sched = p.schedule ?? { start: "09:00", end: "18:00", days: [1, 2, 3, 4, 5] };
  const start = el("input", { type: "time", value: sched.start, disabled: !p.schedule, className: "time" });
  const end = el("input", { type: "time", value: sched.end, disabled: !p.schedule, className: "time" });
  const commit = () => {
    p.schedule = enabled.checked ? { start: start.value || "00:00", end: end.value || "00:00", days: sched.days } : null;
    saveProfiles();
  };
  enabled.onchange = () => { commit(); renderProfiles(); };
  start.onchange = commit;
  end.onchange = commit;

  const names = t("days.short").split(",");
  const days = el("div", { className: "days" }, ...names.map((label, d) => {
    const on = sched.days.includes(d);
    const b = el("button", { className: `day ${on ? "on" : ""}`, textContent: label, disabled: !p.schedule });
    b.onclick = () => {
      sched.days = on ? sched.days.filter((x) => x !== d) : [...sched.days, d].sort();
      commit();
      renderProfiles();
    };
    return b;
  }));

  return el("div", { className: "schedule" },
    el("label", { className: "field check", title: t("profiles.scheduleHint") }, enabled, el("span", {}, t("profiles.schedule"))),
    el("div", { className: "inline" }, start, "–", end),
    days);
}

function renderProfiles() {
  $<HTMLInputElement>("#auto-switch").checked = settings.auto_switch;
  $<HTMLSelectElement>("#default-profile").replaceChildren(...profileOptions("profiles.none", settings.default_profile));
  $<HTMLSelectElement>("#low-battery-profile").replaceChildren(...profileOptions("profiles.none", settings.low_battery_profile));
  $<HTMLSelectElement>("#low-battery-threshold").replaceChildren(...[10, 20, 30, 40].map((v) =>
    el("option", { value: String(v), textContent: `≤ ${v}%`, selected: v === settings.low_battery_threshold })));

  const list = $("#profile-list");
  if (!profiles.length) {
    list.replaceChildren(el("p", { className: "hint", textContent: t("profiles.empty") }));
    return;
  }
  list.replaceChildren(...profiles.map((p) => {
    const name = el("input", { type: "text", value: p.name, className: "profile-name", maxLength: 40 });
    name.onchange = () => { p.name = name.value.trim() || t("profiles.noName"); saveProfiles(); renderProfiles(); };

    const chips = el("div", { className: "chips" }, ...p.apps.map((a) => {
      const x = el("button", { className: "chip-x", title: t("profiles.remove", { name: a }), textContent: "×" });
      x.onclick = () => { p.apps = p.apps.filter((b) => b !== a); saveProfiles(); renderProfiles(); };
      return el("span", { className: "chip" }, a, x);
    }));
    const typed = el("input", { type: "text", placeholder: "app.exe", className: "app-input" });
    typed.onkeydown = (e) => { if (e.key === "Enter") addApp(p, typed.value); };
    const browse = el("button", { className: "ghost small", textContent: t("profiles.browse") });
    browse.onclick = () => pickApp(p);

    const applyBtn = el("button", { className: "primary small", textContent: t("profiles.apply") });
    applyBtn.onclick = () => guarded("err.profile", async () => {
      const cfg = await invoke<Config>("apply_profile", { id: p.id });
      activeProfile = p.id;
      saved = cfg;
      draft = clone(cfg);
      render();
      renderProfiles();
      toast(t("profiles.applied", { name: p.name }));
    });
    const updateBtn = el("button", { className: "ghost small", textContent: t("profiles.useCurrent"), title: t("profiles.useCurrentHint") });
    updateBtn.onclick = () => {
      if (!saved) return;
      if (isDirty()) { toast(t("apply.applyFirst"), "err"); return; }
      p.config = clone(saved);
      saveProfiles();
      renderProfiles();
      toast(t("profiles.updated", { name: p.name }));
    };
    const del = el("button", { className: "ghost small danger", textContent: t("profiles.delete") });
    del.onclick = async () => {
      if (!(await ask(t("profiles.deleteAsk", { name: p.name }), { title: t("tab.profiles"), kind: "warning" }))) return;
      profiles = profiles.filter((q) => q.id !== p.id);
      if (settings.default_profile === p.id) settings.default_profile = null;
      if (settings.low_battery_profile === p.id) settings.low_battery_profile = null;
      if (activeProfile === p.id) activeProfile = null;
      await saveProfiles();
      renderProfiles();
    };

    return el("div", { className: `card profile ${p.id === activeProfile ? "active" : ""}` },
      el("div", { className: "profile-head" }, name,
        p.id === activeProfile ? el("span", { className: "tag", textContent: t("profiles.active") }) : "",
        el("span", { className: "mono", textContent: profileSummary(p.config) })),
      el("div", { className: "field" }, t("profiles.programs"), el("div", { className: "inline" }, typed, browse)),
      chips,
      scheduleEditor(p),
      el("div", { className: "actions" }, applyBtn, updateBtn, del),
    );
  }));
}

async function newProfile() {
  if (!saved) { toast(t("apply.connectFirst"), "err"); return; }
  if (isDirty()) { toast(t("apply.applyFirst"), "err"); return; }
  profiles.push({ id: crypto.randomUUID(), name: t("profiles.defaultName", { n: profiles.length + 1 }), config: clone(saved), apps: [], schedule: null });
  await saveProfiles();
  renderProfiles();
  document.querySelector<HTMLInputElement>("#profile-list .card:last-child .profile-name")?.select();
}

const PROFILE_FILTER = () => [{ name: t("profiles.fileFilter"), extensions: ["m808profiles", "json"] }];

async function exportProfiles() {
  if (!profiles.length) return;
  const path = await save({ defaultPath: "m808-perfis.m808profiles", filters: PROFILE_FILTER() });
  if (!path) return;
  try {
    const n = await invoke<number>("export_profiles", { path });
    toast(t("profiles.exported", { n }));
  } catch (e) {
    toast(`${t("err.profiles")}: ${e}`, "err");
  }
}

async function importProfiles() {
  const path = await open({ multiple: false, filters: PROFILE_FILTER() });
  if (!path || Array.isArray(path)) return;
  try {
    const before = profiles.length;
    profiles = await invoke<Profile[]>("import_profiles", { path });
    renderProfiles();
    toast(t("profiles.imported", { n: profiles.length - before }));
  } catch (e) {
    toast(`${t("err.profiles")}: ${e}`, "err");
  }
}

// ---- Atualizações ----
function showUpdate(info: UpdateInfo | null) {
  pendingUpdate = info;
  const install = $<HTMLButtonElement>("#btn-install-update");
  install.hidden = !info;
  if (info) {
    $("#update-status").textContent = t("update.available", { version: info.version });
    install.textContent = t("update.install", { version: info.version });
  }
}

async function checkUpdate() {
  const btn = $<HTMLButtonElement>("#btn-check-update");
  btn.disabled = true;
  $("#update-status").textContent = t("update.checking");
  try {
    const info = await invoke<UpdateInfo | null>("check_update");
    if (info) showUpdate(info);
    else $("#update-status").textContent = t("update.latest");
  } catch (e) {
    $("#update-status").textContent = t("update.failed", { error: String(e) });
  } finally {
    btn.disabled = false;
  }
}

async function installUpdate() {
  const btn = $<HTMLButtonElement>("#btn-install-update");
  btn.disabled = true;
  btn.textContent = t("update.downloading");
  try {
    await invoke("install_update"); // o app fecha e reabre já atualizado
  } catch (e) {
    toast(`${t("err.update")}: ${e}`, "err");
    btn.disabled = false;
    btn.textContent = t("update.retry");
  }
}

// ---- Bateria ----
function renderBattery(level: number | null) {
  batteryLevel = level;
  const b = $("#battery");
  b.hidden = level == null;
  if (level == null) return;
  b.querySelector("b")!.textContent = `${level}%`;
  b.querySelector<HTMLElement>("i")!.style.setProperty("--level", `${level}%`);
  b.classList.toggle("low", level <= settings.low_battery_threshold);
  b.title = t("battery.title", { level });
}

// ---- Idioma ----
function renderDeviceInfo() {
  if (!deviceInfo) return;
  const link = deviceInfo.wireless ? t("system.receiver") : t("system.cable");
  $("#device-info").textContent = `VID 25A7 · PID ${deviceInfo.pid.toString(16).toUpperCase()} · ${link}`;
}

/** Re-renderiza tudo que é gerado por código depois de trocar o idioma. */
function relocalize() {
  setLang(settings.language);
  applyStatic();
  renderStatus();
  renderDeviceInfo();
  renderBattery(batteryLevel);
  showUpdate(pendingUpdate);
  render();
  renderProfiles();
  markDirty();
}

// ---- Ações ----
async function guarded<T>(label: I18nKey, fn: () => Promise<T>): Promise<T | undefined> {
  if (busy) return;
  busy = true;
  markDirty();
  document.body.classList.add("busy");
  try {
    return await fn();
  } catch (e) {
    toast(`${t(label)}: ${e}`, "err");
    setStatus("err", "status.lost", String(e));
  } finally {
    busy = false;
    document.body.classList.remove("busy");
    markDirty();
  }
}

async function connect() {
  setStatus("wait", "status.searching");
  await guarded("err.connect", async () => {
    deviceInfo = await invoke<DeviceInfo>("connect");
    renderDeviceInfo();
    setStatus("wait", "status.reading");
    saved = await invoke<Config>("read_config");
    draft = clone(saved);
    normalize(draft);
    render();
    setStatus("ok", deviceInfo.wireless ? "status.connectedWireless" : "status.connected");
    renderBattery(await invoke<number | null>("get_battery"));
  });
}

async function apply() {
  if (!draft) return;
  if (!draft.buttons.some((b) => b.kind === 1 && b.code === 0x01)) {
    toast(t("apply.needLeft"), "err");
    return;
  }
  const missing = PHYSICAL_BUTTONS.find(({ slot }) => draft!.buttons[slot].kind === KIND_KEYS && emptyAction(draft!.key_actions[slot]));
  if (missing) {
    toast(t("keys.missing", { button: t(missing.name) }), "err");
    return;
  }
  await guarded("err.apply", async () => {
    const n = await invoke<number>("apply_config", { config: draft });
    saved = clone(draft!);
    toast(n ? t("apply.done", { n }) : t("apply.nothing"));
  });
}

const BACKUP_FILTER = () => [{ name: t("system.backupFilter"), extensions: ["m808bak"] }];

async function backup() {
  const path = await save({ defaultPath: "m808-backup.m808bak", filters: BACKUP_FILTER() });
  if (!path) return;
  await guarded("err.backup", async () => {
    await invoke("save_backup", { path });
    toast(t("system.backupSaved"));
  });
}

async function restore() {
  const path = await open({ multiple: false, filters: BACKUP_FILTER() });
  if (!path || Array.isArray(path)) return;
  await guarded("err.restore", async () => {
    saved = await invoke<Config>("restore_backup", { path });
    draft = clone(saved);
    render();
    toast(t("system.backupRestored"));
  });
}

// ---- Inicialização ----
window.addEventListener("DOMContentLoaded", async () => {
  settings = await invoke<Settings>("get_settings").catch(() => settings);
  setLang(settings.language);

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
      toast(`${t("err.autostart")}: ${e}`, "err");
    }
  };
  $<HTMLSelectElement>("#polling").onchange = (e) => update((d) => (d.polling_hz = Number((e.target as HTMLSelectElement).value)));
  $("#btn-reconnect").onclick = connect;
  $("#btn-apply").onclick = apply;
  $("#btn-discard").onclick = () => { draft = clone(saved); normalize(draft); render(); };
  $("#btn-backup").onclick = backup;
  $("#btn-restore").onclick = restore;

  const language = $<HTMLSelectElement>("#language");
  language.value = settings.language;
  language.onchange = () => { settings.language = language.value as LangSetting; saveSettings(); relocalize(); };

  const showOsd = $<HTMLInputElement>("#show-osd");
  showOsd.checked = settings.show_osd;
  showOsd.onchange = () => { settings.show_osd = showOsd.checked; saveSettings(); };

  const trayIcon = $<HTMLSelectElement>("#tray-icon");
  trayIcon.value = settings.tray_icon;
  trayIcon.onchange = () => { settings.tray_icon = trayIcon.value as Settings["tray_icon"]; saveSettings(); };

  $<HTMLInputElement>("#auto-switch").onchange = (e) => { settings.auto_switch = (e.target as HTMLInputElement).checked; saveSettings(); };
  $<HTMLSelectElement>("#default-profile").onchange = (e) => { settings.default_profile = (e.target as HTMLSelectElement).value || null; saveSettings(); };
  $<HTMLSelectElement>("#low-battery-profile").onchange = (e) => { settings.low_battery_profile = (e.target as HTMLSelectElement).value || null; saveSettings(); };
  $<HTMLSelectElement>("#low-battery-threshold").onchange = (e) => {
    settings.low_battery_threshold = Number((e.target as HTMLSelectElement).value);
    saveSettings();
    renderBattery(batteryLevel);
  };
  $("#btn-new-profile").onclick = newProfile;
  $("#btn-export-profiles").onclick = exportProfiles;
  $("#btn-import-profiles").onclick = importProfiles;
  $("#btn-check-update").onclick = checkUpdate;
  $("#btn-install-update").onclick = installUpdate;
  getVersion().then((v) => ($("#app-version").textContent = `v${v}`));
  invoke<UpdateInfo | null>("pending_update").then(showUpdate);

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
  listen("device-lost", () => setStatus("err", "status.lost"));
  // O processo de fundo reconecta sozinho quando o mouse volta.
  listen("device-connected", () => { if (!busy) connect(); });
  listen<UpdateInfo>("update-available", ({ payload }) => showUpdate(payload));
  listen<number | null>("battery", ({ payload }) => renderBattery(payload));
  listen<Settings>("settings-changed", ({ payload }) => {
    const langChanged = payload.language !== settings.language;
    settings = payload;
    if (langChanged) relocalize();
    else renderProfiles();
  });
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
    if (hadEdits) toast(t("profiles.appliedKeepEdits"));
  });

  const view = await invoke<ProfilesView>("get_profiles").catch(() => ({ profiles: [], active: null }));
  profiles = view.profiles;
  activeProfile = view.active;
  renderProfiles();
  connect();
});
