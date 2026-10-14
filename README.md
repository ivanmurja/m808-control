# M808 Control

**Open source configurator for the Redragon Storm Pro (M808-KS) mouse** — a lightweight replacement
for the official Windows-only software, with features it doesn't have.
Built with [Tauri](https://tauri.app) (Rust + web).

🇧🇷 [Leia em português](README.pt-BR.md)

![DPI tab](docs/screenshots/dpi.jpg)

> Independent project, not affiliated with Redragon. The HID protocol was reverse engineered for
> interoperability — see [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Install (Windows)

Download the `*-setup.exe` from the [latest release](../../releases/latest) and run it.

The installer is not code-signed yet, so SmartScreen may warn you: click **More info → Run anyway**.
Close the official Redragon software first — both fight over the same connection to the mouse.

From version 1.1 on, the app updates itself.

## Features

| | |
|---|---|
| ![Buttons](docs/screenshots/botoes.jpg) | ![Profiles](docs/screenshots/perfis.jpg) |

- **DPI**: up to 8 levels (100–16000, step 100), how many are in the cycle, active level and color of each
- **Buttons**: top view of the mouse with its 8 buttons — clicks, back/forward, DPI +/−/cycle,
  any **key or key combination** (Ctrl+Shift+Esc…), **media keys** (play/pause, volume…),
  polling switch, lights on/off, factory reset or disabled
- **Side RGB strip**: rainbow wave, **color wave**, breathing, steady or off, with speed and
  brightness from 1 to 100% — **beyond the official app limits**
- **Strip follows the DPI color**: change the level on the mouse and the strip takes its color
- **Wheel light** (DPI indicator): steady, breathing or off
- **Profiles** with automatic switching by **running program**, **time of day** or **low battery**;
  export and import profiles to share them
- **Battery level** in the window and in the tray, with a low-battery warning
- **Tray icon** showing the current DPI (in the level color) or the battery
- **On-screen notice** when the DPI, polling rate or profile changes
- **Polling rate** 125 / 250 / 500 / 1000 Hz
- **Backup and restore** of the mouse memory (automatic backup before writing the factory-reset button)
- **Starts with Windows**, minimized to the tray; lightweight — closing the window destroys the WebView
- **Portuguese and English**, responsive layout, auto-update

> Settings are stored **on the mouse itself** — it works the same with the app closed.
> The app only needs to run for automatic profiles, strip/DPI sync and the tray icon.

![Tray icons](docs/screenshots/icones-bandeja.png)

### What it does that the official app doesn't

| | Official | M808 Control |
|---|:---:|:---:|
| Single-color wave on the side strip | — | ✅ |
| Speed/brightness beyond the 10 official steps | — | ✅ |
| Strip following the DPI level color | — | ✅ |
| Profiles by program, schedule or battery | — | ✅ |
| Battery / DPI in the tray icon | — | ✅ |
| Mouse memory backup/restore | — | ✅ |
| Open source, English UI, Linux | — | ✅ |
| Keys, key combinations and media keys on buttons | ✅ | ✅ |
| Macros | ✅ | 🚧 |

## Platforms

| OS | Status |
|----|--------|
| Windows 10/11 | ✅ tested (2.4 GHz receiver) |
| Linux | 🧪 experimental — `.deb`, `.rpm` and `.AppImage` in the releases; see [docs/LINUX.md](docs/LINUX.md) |

## Building

Requirements: [Rust](https://rustup.rs), Node.js 20+ and, on Windows, the Microsoft C++ Build Tools.

```bash
npm install
npm run tauri dev      # development
cd src-tauri && cargo test --lib   # protocol, profiles and tray icon tests
```

`npm run tauri build` also produces the auto-update files, which must be signed: set
`TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) to your own key
(`npx tauri signer generate`), or set `"createUpdaterArtifacts": false` in `src-tauri/tauri.conf.json`
for a local build.

Releases: push a `vX.Y.Z` tag and GitHub Actions builds, signs and drafts the release.

## Project layout

```
src/                 UI (TypeScript + Vite, no framework)
  i18n.ts            translations (pt, en)
  mouse.ts           Storm Pro illustration with clickable buttons
src-tauri/src/
  protocol.rs        pure protocol: packets, checksums, EEPROM map (with tests)
  device.rs          HID transport (hidapi)
  profiles.rs        profiles and automatic switching rules
  trayicon.rs        tray icon drawn at runtime (DPI / battery)
  lib.rs             commands, tray, autostart, background workers
tools/m808.py        Python read/write script used during reverse engineering
docs/PROTOCOL.md     protocol documentation
```

## Contributing

Issues and pull requests are welcome — especially captures of other Redragon models built on the same
protocol. New UI languages: copy the `en` block in `src/i18n.ts` and the strings in `src-tauri/src/i18n.rs`.

## License

[MIT](LICENSE)
