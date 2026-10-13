mod device;
pub mod protocol;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use device::{Device, DeviceError};
use protocol::{button_kind, Config, CONFIG_LEN};

/// Região salva em backups: configuração + combos (macros ficam de fora por enquanto).
const BACKUP_LEN: usize = 0x300;
const BACKUP_MAGIC: &[u8; 8] = b"M808BAK1";

/// Argumento usado pela inicialização automática: sobe só na bandeja, sem janela.
const ARG_MINIMIZED: &str = "--minimized";
const MAIN_WINDOW: &str = "main";
const TRAY_ID: &str = "main";

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);

/// Preferências do app (não ficam no mouse), salvas em `settings.json`.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    /// A faixa lateral assume a cor do nível de DPI ativo sempre que ele muda.
    sync_strip_to_dpi: bool,
}

struct Inner {
    api: HidApi,
    dev: Option<Device>,
    /// Última imagem lida/gravada de 0..CONFIG_LEN, base para gravar só o que mudou.
    image: Option<Vec<u8>>,
    settings: Settings,
}

struct AppState(Mutex<Inner>);

#[derive(Serialize)]
struct DeviceInfo {
    pid: u16,
    wireless: bool,
}

#[derive(Clone, Serialize)]
struct LevelChanged {
    dpi_level: u8,
}

type CmdResult<T> = Result<T, String>;

impl Inner {
    fn device(&mut self) -> Result<&mut Device, DeviceError> {
        if self.dev.is_none() {
            let mut d = Device::open(&mut self.api)?;
            d.handshake()?;
            self.dev = Some(d);
            self.image = None;
        }
        Ok(self.dev.as_mut().unwrap())
    }

    /// Executa `f` com o dispositivo; em erro de E/S descarta a conexão para reconectar depois.
    fn with_device<T>(
        &mut self,
        f: impl FnOnce(&mut Device) -> Result<T, DeviceError>,
    ) -> CmdResult<T> {
        let r = self.device().and_then(f);
        if let Err(e) = &r {
            if !matches!(e, DeviceError::Rejected { .. }) {
                self.dev = None;
            }
        }
        r.map_err(|e| e.to_string())
    }

    fn load_config(&mut self) -> CmdResult<Config> {
        let img = self.with_device(|d| d.read_eeprom(0, CONFIG_LEN))?;
        let cfg = Config::from_eeprom(&img);
        self.image = Some(img);
        Ok(cfg)
    }

    fn config(&self) -> Option<Config> {
        self.image.as_deref().map(Config::from_eeprom)
    }

    /// Com a sincronização ligada, a faixa recebe a cor do nível de DPI ativo.
    fn synced(&self, mut config: Config) -> Config {
        if self.settings.sync_strip_to_dpi {
            config.strip.color = config.active_color();
        }
        config
    }

    /// Grava `config` sobre a imagem atual; só os bytes alterados vão para o mouse.
    fn write_config(&mut self, config: &Config) -> CmdResult<usize> {
        let old = match self.image.clone() {
            Some(i) => i,
            None => self.with_device(|d| d.read_eeprom(0, CONFIG_LEN))?,
        };
        let mut new = old.clone();
        config.apply_to(&mut new);
        let runs = protocol::diff_runs(&old, &new);
        let written = runs.iter().map(|(_, b)| b.len()).sum();
        self.with_device(|d| {
            for (addr, bytes) in &runs {
                d.write_eeprom(*addr, bytes)?;
            }
            Ok(())
        })?;
        self.image = Some(new);
        Ok(written)
    }
}

// ---------------------------------------------------------------- comandos da interface

#[tauri::command]
fn connect(app: AppHandle, state: State<AppState>) -> CmdResult<DeviceInfo> {
    let info = {
        let mut s = state.0.lock().unwrap();
        s.dev = None;
        s.with_device(|d| Ok(DeviceInfo { pid: d.pid, wireless: d.pid == 0xFA07 }))?
    };
    Ok(info).inspect(|_| refresh_tray(&app))
}

#[tauri::command]
fn read_config(app: AppHandle, state: State<AppState>) -> CmdResult<Config> {
    let cfg = state.0.lock().unwrap().load_config()?;
    refresh_tray(&app);
    Ok(cfg)
}

/// Grava a configuração; só os bytes que mudaram são enviados. Retorna quantos bytes.
#[tauri::command]
fn apply_config(app: AppHandle, state: State<AppState>, config: Config) -> CmdResult<usize> {
    let mut s = state.0.lock().unwrap();
    let adds_reset = config.buttons.iter().any(|b| b.kind == button_kind::FACTORY_RESET)
        && !s.config().is_some_and(|c| c.buttons.iter().any(|b| b.kind == button_kind::FACTORY_RESET));
    if adds_reset {
        // Um toque acidental apaga tudo: guarda uma cópia antes de gravar essa função.
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("backups");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        write_backup(&mut s, &dir.join(format!("antes-do-reset-{stamp}.m808bak")))?;
    }
    let config = s.synced(config);
    let n = s.write_config(&config)?;
    drop(s);
    refresh_tray(&app);
    Ok(n)
}

fn write_backup(s: &mut Inner, path: &Path) -> CmdResult<()> {
    let img = s.with_device(|d| d.read_eeprom(0, BACKUP_LEN))?;
    let mut file = BACKUP_MAGIC.to_vec();
    file.extend_from_slice(&img);
    std::fs::write(path, file).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_backup(state: State<AppState>, path: String) -> CmdResult<()> {
    write_backup(&mut state.0.lock().unwrap(), Path::new(&path))
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.0.lock().unwrap().settings.clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> CmdResult<()> {
    if let Some(p) = settings_path(&app) {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        std::fs::write(p, serde_json::to_vec_pretty(&settings).unwrap()).map_err(|e| e.to_string())?;
    }
    state.0.lock().unwrap().settings = settings;
    Ok(())
}

#[tauri::command]
fn restore_backup(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<Config> {
    let file = std::fs::read(&path).map_err(|e| e.to_string())?;
    if file.len() != BACKUP_MAGIC.len() + BACKUP_LEN || !file.starts_with(BACKUP_MAGIC) {
        return Err("arquivo de backup inválido".into());
    }
    let img = &file[BACKUP_MAGIC.len()..];
    {
        let mut s = state.0.lock().unwrap();
        let current = s.with_device(|d| d.read_eeprom(0, BACKUP_LEN))?;
        s.with_device(|d| {
            for (addr, bytes) in protocol::diff_runs(&current, img) {
                d.write_eeprom(addr, &bytes)?;
            }
            Ok(())
        })?;
        s.image = Some(img[..CONFIG_LEN].to_vec());
    }
    refresh_tray(&app);
    Ok(Config::from_eeprom(img))
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> CmdResult<bool> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let al = app.autolaunch();
    if enabled { al.enable() } else { al.disable() }.map_err(|e| e.to_string())?;
    refresh_tray(&app);
    Ok(())
}

// ---------------------------------------------------------------- janela e bandeja

/// Abre (ou recria) a janela principal. Ao fechar, ela é destruída para liberar o WebView.
fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::App("index.html".into()))
        .title("M808 Control")
        .inner_size(980.0, 700.0)
        .min_inner_size(760.0, 560.0)
        .build();
}

fn set_active_level(app: &AppHandle, level: u8) {
    let state = app.state::<AppState>();
    let result = {
        let mut s = state.0.lock().unwrap();
        match s.config().map(Ok).unwrap_or_else(|| s.load_config()) {
            Ok(mut cfg) if level < cfg.level_count => {
                cfg.active_level = level;
                let cfg = s.synced(cfg);
                s.write_config(&cfg).map(|_| ())
            }
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    };
    if result.is_ok() {
        let _ = app.emit("device-event", LevelChanged { dpi_level: level });
    }
    refresh_tray(app);
}

fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.state::<AppState>();
    let cfg = state.0.lock().unwrap().config();
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);

    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = vec![
        Box::new(MenuItem::with_id(app, "open", "Abrir M808 Control", true, None::<&str>)?),
        Box::new(PredefinedMenuItem::separator(app)?),
    ];
    match &cfg {
        Some(c) => {
            for (i, l) in c.levels.iter().take(c.level_count as usize).enumerate() {
                let label = format!("Nível {} · {} DPI", i + 1, l.dpi);
                let checked = i == c.active_level as usize;
                items.push(Box::new(CheckMenuItem::with_id(app, format!("level:{i}"), label, true, checked, None::<&str>)?));
            }
        }
        None => items.push(Box::new(MenuItem::with_id(app, "noop", "Mouse não conectado", false, None::<&str>)?)),
    }
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(CheckMenuItem::with_id(app, "autostart", "Iniciar com o sistema", true, autostart, None::<&str>)?));
    items.push(Box::new(MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

fn tray_tooltip(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let s = state.0.lock().unwrap();
    match s.config() {
        Some(c) if s.dev.is_some() => {
            let dpi = c.levels.get(c.active_level as usize).map_or(0, |l| l.dpi);
            format!("M808 Control — {dpi} DPI")
        }
        _ => "M808 Control — mouse desconectado".into(),
    }
}

fn refresh_tray(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    if let Ok(menu) = build_tray_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
    let _ = tray.set_tooltip(Some(tray_tooltip(app)));
}

fn on_tray_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main_window(app),
        "quit" => app.exit(0),
        "autostart" => {
            let al = app.autolaunch();
            let _ = if al.is_enabled().unwrap_or(false) { al.disable() } else { al.enable() };
            refresh_tray(app);
        }
        _ => {
            if let Some(level) = id.strip_prefix("level:").and_then(|n| n.parse().ok()) {
                set_active_level(app, level);
            }
        }
    }
}

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_tray_menu(app)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("ícone do app"))
        .tooltip("M808 Control")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| on_tray_menu(app, e.id().as_ref()))
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Na primeira execução liga a inicialização automática (o usuário pode desligar depois).
fn enable_autostart_on_first_run(app: &AppHandle) {
    // Em `tauri dev` isso registraria o exe de debug no boot do Windows.
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(dir) = app.path().app_config_dir() else { return };
    let marker = dir.join("first-run-done");
    if marker.exists() {
        return;
    }
    let _ = app.autolaunch().enable();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(marker, b"");
}

// ---------------------------------------------------------------- segundo plano

/// Lê eventos do mouse (ex.: troca de DPI pelo botão) e reconecta quando ele aparece.
fn spawn_background(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_attempt: Option<Instant> = None;
        loop {
            std::thread::sleep(POLL_INTERVAL);
            let state = app.state::<AppState>();
            let mut s = state.0.lock().unwrap();

            if s.dev.is_none() {
                if last_attempt.is_some_and(|t| t.elapsed() < RECONNECT_INTERVAL) {
                    continue;
                }
                last_attempt = Some(Instant::now());
                let ok = s.load_config().is_ok();
                drop(s);
                if ok {
                    let _ = app.emit("device-connected", ());
                }
                refresh_tray(&app);
                continue;
            }

            let events = match s.dev.as_mut().unwrap().poll_events() {
                Ok(ev) => ev,
                Err(_) => {
                    s.dev = None;
                    drop(s);
                    let _ = app.emit("device-lost", ());
                    refresh_tray(&app);
                    continue;
                }
            };
            // O próprio mouse já gravou a mudança: só atualiza a cópia local.
            let mut changed = false;
            let mut level_changed = false;
            for e in &events {
                let Some(img) = s.image.as_mut() else { break };
                if let Some(level) = e.dpi_level {
                    img[protocol::addr::ACTIVE_LEVEL..][..2].copy_from_slice(&protocol::pair(level));
                    changed = true;
                    level_changed = true;
                }
                if let Some(hz) = e.polling_hz {
                    img[protocol::addr::POLLING..][..2].copy_from_slice(&protocol::pair(protocol::polling_code(hz)));
                    changed = true;
                }
            }
            if level_changed && s.settings.sync_strip_to_dpi {
                if let Some(cfg) = s.config() {
                    let cfg = s.synced(cfg);
                    let _ = s.write_config(&cfg);
                }
            }
            drop(s);
            for e in events {
                let _ = app.emit("device-event", e);
            }
            if changed {
                refresh_tray(&app);
            }
        }
    });
}

// ---------------------------------------------------------------- entrada

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let api = HidApi::new().expect("não foi possível iniciar o hidapi");
    let start_minimized = std::env::args().any(|a| a == ARG_MINIMIZED);

    let app = tauri::Builder::default()
        // precisa ser o primeiro plugin: a segunda instância só traz a janela para frente
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![ARG_MINIMIZED])))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState(Mutex::new(Inner { api, dev: None, image: None, settings: Settings::default() })))
        .setup(move |app| {
            let handle = app.handle().clone();
            app.state::<AppState>().0.lock().unwrap().settings = load_settings(&handle);
            enable_autostart_on_first_run(&handle);
            create_tray(&handle)?;
            spawn_background(handle.clone());
            if !start_minimized {
                show_main_window(&handle);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            read_config,
            apply_config,
            save_backup,
            restore_backup,
            get_autostart,
            set_autostart,
            get_settings,
            set_settings
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o app");

    app.run(|_, event| {
        // Fechar a janela não encerra o app: ele continua na bandeja. "Sair" usa exit(0).
        if let RunEvent::ExitRequested { api, code: None, .. } = event {
            api.prevent_exit();
        }
    });
}
