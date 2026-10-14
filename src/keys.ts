// Teclas para os botões do tipo "tecla": KeyboardEvent.code → código HID (página Keyboard/Keypad)
// e a lista de teclas de mídia (página Consumer).

import type { I18nKey } from "./i18n";

/** Bits de modificador do HID (iguais aos gravados no mouse). */
export const MODIFIERS: { bit: number; label: string }[] = [
  { bit: 0x01, label: "Ctrl" },
  { bit: 0x02, label: "Shift" },
  { bit: 0x04, label: "Alt" },
  { bit: 0x08, label: "Win" },
];

const letters = Object.fromEntries(
  Array.from({ length: 26 }, (_, i) => [`Key${String.fromCharCode(65 + i)}`, 0x04 + i]),
);
const digits = Object.fromEntries([
  ...Array.from({ length: 9 }, (_, i) => [`Digit${i + 1}`, 0x1e + i]),
  ["Digit0", 0x27],
]);
const fkeys = Object.fromEntries([
  ...Array.from({ length: 12 }, (_, i) => [`F${i + 1}`, 0x3a + i]),
  ...Array.from({ length: 12 }, (_, i) => [`F${i + 13}`, 0x68 + i]),
]);
const numpad = Object.fromEntries([
  ...Array.from({ length: 9 }, (_, i) => [`Numpad${i + 1}`, 0x59 + i]),
  ["Numpad0", 0x62],
]);

/** KeyboardEvent.code → código HID. */
export const HID_BY_CODE: Record<string, number> = {
  ...letters, ...digits, ...fkeys, ...numpad,
  Enter: 0x28, Escape: 0x29, Backspace: 0x2a, Tab: 0x2b, Space: 0x2c,
  Minus: 0x2d, Equal: 0x2e, BracketLeft: 0x2f, BracketRight: 0x30, Backslash: 0x31,
  Semicolon: 0x33, Quote: 0x34, Backquote: 0x35, Comma: 0x36, Period: 0x37, Slash: 0x38,
  CapsLock: 0x39, PrintScreen: 0x46, ScrollLock: 0x47, Pause: 0x48,
  Insert: 0x49, Home: 0x4a, PageUp: 0x4b, Delete: 0x4c, End: 0x4d, PageDown: 0x4e,
  ArrowRight: 0x4f, ArrowLeft: 0x50, ArrowDown: 0x51, ArrowUp: 0x52,
  NumLock: 0x53, NumpadDivide: 0x54, NumpadMultiply: 0x55, NumpadSubtract: 0x56,
  NumpadAdd: 0x57, NumpadEnter: 0x58, NumpadDecimal: 0x63,
  IntlBackslash: 0x64, ContextMenu: 0x65, NumpadComma: 0x85, IntlRo: 0x87,
};

/** Códigos de KeyboardEvent dos modificadores (viram bits, não teclas). */
export const MODIFIER_CODES: Record<string, number> = {
  ControlLeft: 0x01, ControlRight: 0x01, ShiftLeft: 0x02, ShiftRight: 0x02,
  AltLeft: 0x04, AltRight: 0x04, MetaLeft: 0x08, MetaRight: 0x08,
};

const NAMES: Record<number, string> = {
  0x28: "Enter", 0x29: "Esc", 0x2a: "Backspace", 0x2b: "Tab", 0x2c: "Space",
  0x2d: "-", 0x2e: "=", 0x2f: "[", 0x30: "]", 0x31: "\\", 0x33: ";", 0x34: "'", 0x35: "`",
  0x36: ",", 0x37: ".", 0x38: "/", 0x39: "Caps Lock", 0x46: "Print Screen", 0x47: "Scroll Lock",
  0x48: "Pause", 0x49: "Insert", 0x4a: "Home", 0x4b: "Page Up", 0x4c: "Delete", 0x4d: "End",
  0x4e: "Page Down", 0x4f: "→", 0x50: "←", 0x51: "↓", 0x52: "↑", 0x53: "Num Lock",
  0x54: "Num /", 0x55: "Num *", 0x56: "Num -", 0x57: "Num +", 0x58: "Num Enter", 0x63: "Num .",
  0x64: "\\ (ISO)", 0x65: "Menu", 0x85: "Num ,", 0x87: "/ (ABNT2)",
};

/** Nome legível de um código HID de tecla. */
export function keyName(hid: number): string {
  if (hid >= 0x04 && hid <= 0x1d) return String.fromCharCode(65 + hid - 0x04);
  if (hid >= 0x1e && hid <= 0x26) return String(hid - 0x1d);
  if (hid === 0x27) return "0";
  if (hid >= 0x3a && hid <= 0x45) return `F${hid - 0x39}`;
  if (hid >= 0x68 && hid <= 0x73) return `F${hid - 0x68 + 13}`;
  if (hid >= 0x59 && hid <= 0x61) return `Num ${hid - 0x58}`;
  if (hid === 0x62) return "Num 0";
  return NAMES[hid] ?? `0x${hid.toString(16)}`;
}

/** Teclas de mídia (HID Consumer) — mesmas que o software oficial oferece. */
export const MEDIA_KEYS: { usage: number; label: I18nKey }[] = [
  { usage: 0x00cd, label: "media.playPause" },
  { usage: 0x00b7, label: "media.stop" },
  { usage: 0x00b6, label: "media.previous" },
  { usage: 0x00b5, label: "media.next" },
  { usage: 0x00e9, label: "media.volumeUp" },
  { usage: 0x00ea, label: "media.volumeDown" },
  { usage: 0x00e2, label: "media.mute" },
  { usage: 0x0183, label: "media.player" },
  { usage: 0x018a, label: "media.mail" },
  { usage: 0x0192, label: "media.calculator" },
  { usage: 0x0194, label: "media.computer" },
  { usage: 0x0221, label: "media.search" },
  { usage: 0x0223, label: "media.home" },
  { usage: 0x0224, label: "media.back" },
  { usage: 0x0225, label: "media.forward" },
  { usage: 0x0226, label: "media.browserStop" },
  { usage: 0x0227, label: "media.refresh" },
  { usage: 0x022a, label: "media.favorites" },
];
