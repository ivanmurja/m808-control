mod device;
mod foreground;
mod i18n;
mod osd;
mod profiles;
mod trayicon;
mod updates;
pub mod protocol;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::image::Image;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use device::{Device, DeviceError};
use profiles::Profile;
use protocol::{button_kind, Config, DeviceEvent, CONFIG_LEN};
use i18n::{tr, Lang, LangSetting, Text};
use trayicon::TrayIconMode;

/// Região salva em backups: configuração + combos (macros ficam de fora por enquanto).
const BACKUP_LEN: usize = 0x300;
const BACKUP_MAGIC: &[u8; 8] = b"M808BAK1";

/// Argumento usado pela inicialização automática: sobe só na bandeja, sem janela.
const ARG_MINIMIZED: &str = "--minimized";
const MAIN_WINDOW: &str = "main";
const TRAY_ID: &str = "main";

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
const BATTERY_INTERVAL: Duration = Duration::from_secs(60);
const FOREGROUND_INTERVAL: Duration = Duration::from_secs(1);

/// Preferências do app (não ficam no mouse), salvas em `settings.json`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    /// A faixa lateral assume a cor do nível de DPI ativo sempre que ele muda.
    sync_strip_to_dpi: bool,
    /// Mostra um aviso rápido na tela ao trocar DPI, polling ou perfil.
    show_osd: bool,
    /// Troca de perfil conforme o programa em primeiro plano.
    auto_switch: bool,
    /// Perfil usado quando nenhum programa da lista está em primeiro plano.
    default_profile: Option<String>,
    /// O que o ícone da bandeja mostra.
    tray_icon: TrayIconMode,
    language: LangSetting,
    /// Perfil aplicado quando a bateria chega em `low_battery_threshold` %.
    low_battery_profile: Option<String>,
    low_battery_threshold: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sync_strip_to_dpi: false,
            show_osd: true,
            auto_switch: false,
            default_profile: None,
            tray_icon: TrayIconMode::Dpi,
            language: LangSetting::Auto,
            low_battery_profile: None,
            low_battery_threshold: 20,
        }
    }
}

struct Inner {
    api: HidApi,
    dev: Option<Device>,
    /// Última imagem lida/gravada de 0..CONFIG_LEN, base para gravar só o que mudou.
    image: Option<Vec<u8>>,
    settings: Settings,
    profiles: Vec<Profile>,
    /// Perfil aplicado por último (pelo usuário ou pela troca automática).
    active_profile: Option<String>,
    battery: Option<u8>,
    low_battery_warned: bool,
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

#[derive(Serialize)]
struct ProfilesView {
    profiles: Vec<Profile>,
    active: Option<String>,
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
        r.map_err(|e| i18n::device_error(self.lang(), &e))
    }

    fn lang(&self) -> Lang {
        self.settings.language.resolve()
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

    fn profile_name(&self, id: &str) -> Option<String> {
        self.profiles.iter().find(|p| p.id == id).map(|p| p.name.clone())
    }
}

fn config_path(app: &AppHandle, file: &str) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(file))
}

fn lang_of(app: &AppHandle) -> Lang {
    app.state::<AppState>().0.lock().unwrap().lang()
}

fn osd(app: &AppHandle, text: &str) {
    let enabled = app.state::<AppState>().0.lock().unwrap().settings.show_osd;
    if enabled {
        osd::show(app, text);
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

fn load_settings(app: &AppHandle) -> Settings {
    config_path(app, "settings.json")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn store_settings(app: &AppHandle, settings: &Settings) -> CmdResult<()> {
    if let Some(p) = config_path(app, "settings.json") {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        std::fs::write(p, serde_json::to_vec_pretty(settings).unwrap()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.0.lock().unwrap().settings.clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> CmdResult<()> {
    store_settings(&app, &settings)?;
    state.0.lock().unwrap().settings = settings;
    refresh_tray(&app);
    Ok(())
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

#[tauri::command]
async fn check_update(app: AppHandle) -> CmdResult<Option<updates::UpdateInfo>> {
    let info = updates::check(&app).await?;
    refresh_tray(&app);
    Ok(info)
}

#[tauri::command]
async fn install_update(app: AppHandle) -> CmdResult<()> {
    updates::install(&app).await
}

#[tauri::command]
fn pending_update(app: AppHandle) -> Option<updates::UpdateInfo> {
    app.state::<updates::PendingUpdate>().info()
}

fn on_update_found(app: &AppHandle, info: &updates::UpdateInfo) {
    let _ = app.emit("update-available", info);
    osd(app, &format!("{}: v{}", tr(lang_of(app), Text::UpdateAvailable), info.version));
    refresh_tray(app);
}

#[tauri::command]
fn get_battery(state: State<AppState>) -> Option<u8> {
    state.0.lock().unwrap().battery
}

#[tauri::command]
fn get_profiles(state: State<AppState>) -> ProfilesView {
    let s = state.0.lock().unwrap();
    ProfilesView { profiles: s.profiles.clone(), active: s.active_profile.clone() }
}

/// Substitui a lista de perfis (a interface edita a lista inteira).
#[tauri::command]
fn save_profiles(app: AppHandle, state: State<AppState>, profiles: Vec<Profile>) -> CmdResult<()> {
    if let Some(p) = config_path(&app, "profiles.json") {
        profiles::save(&p, &profiles).map_err(|e| e.to_string())?;
    }
    {
        let mut s = state.0.lock().unwrap();
        s.profiles = profiles;
        // Referências a perfis apagados deixam de valer.
        let exists = |id: &Option<String>, ps: &[Profile]| id.as_ref().is_some_and(|i| ps.iter().any(|p| &p.id == i));
        if !exists(&s.active_profile, &s.profiles) {
            s.active_profile = None;
        }
        let mut changed = false;
        if s.settings.default_profile.is_some() && !exists(&s.settings.default_profile, &s.profiles) {
            s.settings.default_profile = None;
            changed = true;
        }
        if s.settings.low_battery_profile.is_some() && !exists(&s.settings.low_battery_profile, &s.profiles) {
            s.settings.low_battery_profile = None;
            changed = true;
        }
        if changed {
            let settings = s.settings.clone();
            drop(s);
            store_settings(&app, &settings)?;
        }
    }
    refresh_tray(&app);
    Ok(())
}

#[tauri::command]
fn export_profiles(state: State<AppState>, path: String) -> CmdResult<usize> {
    let s = state.0.lock().unwrap();
    profiles::export(Path::new(&path), &s.profiles).map_err(|e| e.to_string())?;
    Ok(s.profiles.len())
}

/// Acrescenta os perfis do arquivo aos existentes e devolve a lista completa.
#[tauri::command]
fn import_profiles(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<Vec<Profile>> {
    let imported = profiles::import(Path::new(&path))?;
    let all = {
        let mut s = state.0.lock().unwrap();
        s.profiles.extend(imported);
        s.profiles.clone()
    };
    if let Some(p) = config_path(&app, "profiles.json") {
        profiles::save(&p, &all).map_err(|e| e.to_string())?;
    }
    refresh_tray(&app);
    Ok(all)
}

#[tauri::command]
fn apply_profile(app: AppHandle, id: String) -> CmdResult<Config> {
    activate_profile(&app, &id)
}

/// Grava o perfil no mouse e avisa a interface, a bandeja e a tela.
fn activate_profile(app: &AppHandle, id: &str) -> CmdResult<Config> {
    let state = app.state::<AppState>();
    let (cfg, name) = {
        let mut s = state.0.lock().unwrap();
        let p = s.profiles.iter().find(|p| p.id == id).cloned().ok_or("perfil não encontrado")?;
        let cfg = s.synced(p.config);
        s.write_config(&cfg)?;
        s.active_profile = Some(p.id);
        (cfg, p.name)
    };
    let _ = app.emit("config-changed", &cfg);
    osd(app, &format!("{}: {name}", tr(lang_of(app), Text::Profile)));
    refresh_tray(app);
    Ok(cfg)
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
        .inner_size(1000.0, 720.0)
        .min_inner_size(560.0, 480.0)
        .build();
}

fn level_text(lang: Lang, cfg: &Config, level: u8) -> String {
    let dpi = cfg.levels.get(level as usize).map_or(0, |l| l.dpi);
    format!("{} {} · {dpi} DPI", tr(lang, Text::Level), level + 1)
}

fn set_active_level(app: &AppHandle, level: u8) {
    let state = app.state::<AppState>();
    let result = {
        let mut s = state.0.lock().unwrap();
        match s.config().map(Ok).unwrap_or_else(|| s.load_config()) {
            Ok(mut cfg) if level < cfg.level_count => {
                cfg.active_level = level;
                let cfg = s.synced(cfg);
                s.write_config(&cfg).map(|_| Some(cfg))
            }
            Ok(_) => Ok(None),
            Err(e) => Err(e),
        }
    };
    if let Ok(Some(cfg)) = result {
        let _ = app.emit("device-event", LevelChanged { dpi_level: level });
        osd(app, &level_text(lang_of(app), &cfg, level));
    }
    refresh_tray(app);
}

fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.state::<AppState>();
    let (cfg, battery, profiles, active, auto_switch, lang) = {
        let s = state.0.lock().unwrap();
        let connected = s.dev.is_some();
        (
            s.config().filter(|_| connected),
            s.battery.filter(|_| connected),
            s.profiles.clone(),
            s.active_profile.clone(),
            s.settings.auto_switch,
            s.lang(),
        )
    };
    let l = |t: Text| tr(lang, t);
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);

    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = vec![
        Box::new(MenuItem::with_id(app, "open", l(Text::Open), true, None::<&str>)?),
        Box::new(PredefinedMenuItem::separator(app)?),
    ];
    match &cfg {
        Some(c) => {
            if let Some(b) = battery {
                items.push(Box::new(MenuItem::with_id(app, "noop", format!("{}: {b}%", l(Text::Battery)), false, None::<&str>)?));
            }
            for (i, lv) in c.levels.iter().take(c.level_count as usize).enumerate() {
                let label = format!("{} {} · {} DPI", l(Text::Level), i + 1, lv.dpi);
                let checked = i == c.active_level as usize;
                items.push(Box::new(CheckMenuItem::with_id(app, format!("level:{i}"), label, true, checked, None::<&str>)?));
            }
        }
        None => items.push(Box::new(MenuItem::with_id(app, "noop", l(Text::NotConnected), false, None::<&str>)?)),
    }

    if !profiles.is_empty() {
        let sub = Submenu::with_id(app, "profiles", l(Text::Profiles), true)?;
        for p in &profiles {
            let checked = active.as_deref() == Some(p.id.as_str());
            sub.append(&CheckMenuItem::with_id(app, format!("profile:{}", p.id), &p.name, cfg.is_some(), checked, None::<&str>)?)?;
        }
        sub.append(&PredefinedMenuItem::separator(app)?)?;
        sub.append(&CheckMenuItem::with_id(app, "auto_switch", l(Text::AutoSwitch), true, auto_switch, None::<&str>)?)?;
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
        items.push(Box::new(sub));
    }

    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    if let Some(u) = app.state::<updates::PendingUpdate>().info() {
        let label = format!("{} v{}", l(Text::InstallUpdate), u.version);
        items.push(Box::new(MenuItem::with_id(app, "install_update", label, true, None::<&str>)?));
    }
    items.push(Box::new(CheckMenuItem::with_id(app, "autostart", l(Text::Autostart), true, autostart, None::<&str>)?));
    items.push(Box::new(MenuItem::with_id(app, "quit", l(Text::Quit), true, None::<&str>)?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

fn tray_tooltip(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let s = state.0.lock().unwrap();
    let lang = s.lang();
    match s.config() {
        Some(c) if s.dev.is_some() => {
            let dpi = c.levels.get(c.active_level as usize).map_or(0, |l| l.dpi);
            let mut t = format!("M808 Control — {dpi} DPI");
            if let Some(b) = s.battery {
                t += &format!(" · {} {b}%", tr(lang, Text::Battery).to_lowercase());
            }
            if let Some(name) = s.active_profile.as_deref().and_then(|id| s.profile_name(id)) {
                t += &format!("\n{}: {name}", tr(lang, Text::Profile));
            }
            t
        }
        _ => format!("M808 Control — {}", tr(lang, Text::TooltipDisconnected)),
    }
}

/// Ícone dinâmico: (chave para evitar redesenho, pixels RGBA 32×32). `None` = logo do app.
fn tray_icon_pixels(app: &AppHandle) -> Option<(String, Vec<u8>)> {
    let state = app.state::<AppState>();
    let s = state.0.lock().unwrap();
    s.dev.as_ref()?; // desconectado: logo
    match s.settings.tray_icon {
        TrayIconMode::Logo => None,
        TrayIconMode::Dpi => {
            let c = s.config()?;
            let level = c.levels.get(c.active_level as usize)?;
            let key = format!("dpi:{}:{:?}", level.dpi, level.color);
            Some((key, trayicon::dpi_icon(level.dpi, level.color)))
        }
        TrayIconMode::Battery => {
            let b = s.battery?;
            Some((format!("bat:{b}"), trayicon::battery_icon(b)))
        }
    }
}

fn refresh_tray(app: &AppHandle) {
    static LAST_ICON: Mutex<Option<String>> = Mutex::new(None);
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    if let Ok(menu) = build_tray_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
    let _ = tray.set_tooltip(Some(tray_tooltip(app)));

    let icon = tray_icon_pixels(app);
    let key = icon.as_ref().map_or_else(|| "logo".to_string(), |(k, _)| k.clone());
    let mut last = LAST_ICON.lock().unwrap();
    if last.as_deref() != Some(key.as_str()) {
        let image = match icon {
            Some((_, px)) => Some(Image::new_owned(px, trayicon::SIZE, trayicon::SIZE)),
            None => app.default_window_icon().cloned(),
        };
        let _ = tray.set_icon(image);
        *last = Some(key);
    }
}

fn on_tray_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main_window(app),
        "install_update" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = updates::install(&app).await {
                    osd(&app, &format!("{}: {e}", tr(lang_of(&app), Text::UpdateFailed)));
                }
            });
        }
        "quit" => app.exit(0),
        "autostart" => {
            let al = app.autolaunch();
            let _ = if al.is_enabled().unwrap_or(false) { al.disable() } else { al.enable() };
            refresh_tray(app);
        }
        "auto_switch" => {
            let settings = {
                let state = app.state::<AppState>();
                let mut s = state.0.lock().unwrap();
                s.settings.auto_switch = !s.settings.auto_switch;
                s.settings.clone()
            };
            let _ = store_settings(app, &settings);
            let _ = app.emit("settings-changed", &settings);
            refresh_tray(app);
        }
        _ => {
            if let Some(level) = id.strip_prefix("level:").and_then(|n| n.parse().ok()) {
                set_active_level(app, level);
            } else if let Some(pid) = id.strip_prefix("profile:") {
                let _ = activate_profile(app, pid);
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

/// Lê eventos do mouse (ex.: troca de DPI pelo botão), a bateria, e reconecta quando ele aparece.
fn spawn_device_worker(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_attempt: Option<Instant> = None;
        let mut last_battery: Option<Instant> = None;
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
                    last_battery = None;
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

            let mut battery_changed = false;
            let mut low_battery = None;
            if last_battery.is_none_or(|t| t.elapsed() >= BATTERY_INTERVAL) {
                last_battery = Some(Instant::now());
                if let Ok(level) = s.dev.as_mut().unwrap().battery() {
                    battery_changed = level != s.battery;
                    s.battery = level;
                    let limit = s.settings.low_battery_threshold;
                    match level {
                        Some(b) if b <= limit && !s.low_battery_warned => {
                            s.low_battery_warned = true;
                            low_battery = Some(b);
                        }
                        Some(b) if b > limit + 10 => s.low_battery_warned = false,
                        _ => {}
                    }
                }
            }

            let (changed, messages) = absorb_events(&mut s, &events);
            let battery = s.battery;
            drop(s);

            for e in events {
                let _ = app.emit("device-event", e);
            }
            for m in messages {
                osd(&app, &m);
            }
            if battery_changed {
                let _ = app.emit("battery", battery);
            }
            if let Some(b) = low_battery {
                osd(&app, &format!("{}: {b}%", tr(lang_of(&app), Text::LowBattery)));
            }
            if changed || battery_changed {
                refresh_tray(&app);
            }
        }
    });
}

/// Atualiza a cópia local com o que o próprio mouse já gravou e devolve os avisos a mostrar.
fn absorb_events(s: &mut Inner, events: &[DeviceEvent]) -> (bool, Vec<String>) {
    let mut changed = false;
    let mut level_changed = false;
    let mut messages = Vec::new();
    for e in events {
        let Some(img) = s.image.as_mut() else { break };
        if let Some(level) = e.dpi_level {
            img[protocol::addr::ACTIVE_LEVEL..][..2].copy_from_slice(&protocol::pair(level));
            changed = true;
            level_changed = true;
        }
        if let Some(hz) = e.polling_hz {
            img[protocol::addr::POLLING..][..2].copy_from_slice(&protocol::pair(protocol::polling_code(hz)));
            changed = true;
            messages.push(format!("Polling {hz} Hz"));
        }
    }
    if level_changed {
        if let Some(cfg) = s.config() {
            messages.insert(0, level_text(s.lang(), &cfg, cfg.active_level));
            if s.settings.sync_strip_to_dpi {
                let cfg = s.synced(cfg);
                let _ = s.write_config(&cfg);
            }
        }
    }
    (changed, messages)
}

/// Troca de perfil automaticamente: programa em primeiro plano, bateria fraca, horário, padrão.
///
/// Só age quando o perfil *calculado* muda — se o usuário aplicar outro perfil na mão, ele
/// continua valendo até a situação mudar (outro programa, outro horário…).
fn spawn_rules_engine(app: AppHandle) {
    let own = std::env::current_exe().ok().and_then(|p| foreground::exe_name(&p.to_string_lossy()));
    std::thread::spawn(move || {
        let mut exe: Option<String> = None;
        let mut last_target: Option<String> = None;
        loop {
            std::thread::sleep(FOREGROUND_INTERVAL);
            // Mexer no próprio app (ex.: editando perfis) não conta como troca de programa.
            if let Some(now) = foreground::foreground_exe().filter(|e| Some(e) != own.as_ref()) {
                exe = Some(now);
            }
            let target = {
                let state = app.state::<AppState>();
                let s = state.0.lock().unwrap();
                if !s.settings.auto_switch || s.dev.is_none() {
                    last_target = None;
                    continue;
                }
                let now = chrono::Local::now();
                use chrono::{Datelike, Timelike};
                let ctx = profiles::Context {
                    exe: exe.as_deref(),
                    battery: s.battery,
                    weekday: now.weekday().num_days_from_sunday() as u8,
                    minute: now.hour() * 60 + now.minute(),
                };
                let rules = profiles::Rules {
                    default_profile: s.settings.default_profile.as_deref(),
                    low_battery_profile: s.settings.low_battery_profile.as_deref(),
                    low_battery_threshold: s.settings.low_battery_threshold,
                };
                profiles::pick(&s.profiles, &ctx, &rules).map(|p| p.id.clone())
            };
            if target != last_target {
                last_target = target.clone();
                let already_active = {
                    let state = app.state::<AppState>();
                    let s = state.0.lock().unwrap();
                    s.active_profile == target
                };
                if let Some(id) = target.filter(|_| !already_active) {
                    let _ = activate_profile(&app, &id);
                }
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::PendingUpdate::default())
        .manage(AppState(Mutex::new(Inner {
            api,
            dev: None,
            image: None,
            settings: Settings::default(),
            profiles: Vec::new(),
            active_profile: None,
            battery: None,
            low_battery_warned: false,
        })))
        .setup(move |app| {
            let handle = app.handle().clone();
            {
                let state = app.state::<AppState>();
                let mut s = state.0.lock().unwrap();
                s.settings = load_settings(&handle);
                s.profiles = config_path(&handle, "profiles.json").map(|p| profiles::load(&p)).unwrap_or_default();
            }
            enable_autostart_on_first_run(&handle);
            create_tray(&handle)?;
            spawn_device_worker(handle.clone());
            spawn_rules_engine(handle.clone());
            updates::spawn_periodic(handle.clone(), on_update_found);
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
            set_settings,
            get_battery,
            get_profiles,
            save_profiles,
            apply_profile,
            export_profiles,
            import_profiles,
            check_update,
            install_update,
            pending_update
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
