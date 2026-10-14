//! Atualização automática pelas releases do GitHub (updater oficial do Tauri, com assinatura).

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Primeira verificação depois de abrir (não atrasa a inicialização) e intervalo entre elas.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
}

/// Atualização encontrada e ainda não instalada.
#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<Update>>);

impl PendingUpdate {
    pub fn info(&self) -> Option<UpdateInfo> {
        self.0.lock().unwrap().as_ref().map(info_of)
    }
}

fn info_of(u: &Update) -> UpdateInfo {
    UpdateInfo { version: u.version.clone(), current: u.current_version.clone(), notes: u.body.clone() }
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let info = update.as_ref().map(info_of);
    *app.state::<PendingUpdate>().0.lock().unwrap() = update;
    Ok(info)
}

/// Baixa, confere a assinatura e instala; no Windows o instalador fecha e reabre o app.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app.state::<PendingUpdate>().0.lock().unwrap().take();
    let update = match update {
        Some(u) => u,
        None => match app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())? {
            Some(u) => u,
            None => return Err("o app já está na versão mais recente".into()),
        },
    };
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    app.restart();
}

/// Verifica em segundo plano; `on_found` é chamado uma vez por versão nova.
pub fn spawn_periodic(app: AppHandle, on_found: fn(&AppHandle, &UpdateInfo)) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        let mut announced: Option<String> = None;
        loop {
            if let Ok(Some(info)) = tauri::async_runtime::block_on(check(&app)) {
                if announced.as_deref() != Some(info.version.as_str()) {
                    announced = Some(info.version.clone());
                    on_found(&app, &info);
                }
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}
