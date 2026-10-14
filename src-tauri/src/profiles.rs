//! Perfis: configurações completas do mouse salvas no PC, opcionalmente ligadas a programas.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::protocol::Config;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub config: Config,
    /// Executáveis (ex.: `cs2.exe`) que ativam este perfil quando estão em primeiro plano.
    #[serde(default)]
    pub apps: Vec<String>,
}

impl Profile {
    pub fn matches(&self, exe: &str) -> bool {
        self.apps.iter().any(|a| a.trim().eq_ignore_ascii_case(exe))
    }
}

/// Perfil que deve valer com `exe` em primeiro plano: o primeiro que lista o programa,
/// senão o perfil padrão (se houver).
pub fn pick<'a>(profiles: &'a [Profile], exe: &str, default: Option<&str>) -> Option<&'a Profile> {
    profiles
        .iter()
        .find(|p| p.matches(exe))
        .or_else(|| default.and_then(|id| profiles.iter().find(|p| p.id == id)))
}

pub fn load(path: &Path) -> Vec<Profile> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, profiles: &[Profile]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(profiles).expect("perfis serializáveis"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CONFIG_LEN;

    fn profile(id: &str, apps: &[&str]) -> Profile {
        Profile {
            id: id.into(),
            name: id.into(),
            config: Config::from_eeprom(&[0x55; CONFIG_LEN]),
            apps: apps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn pick_by_app_then_default() {
        let ps = [profile("jogo", &["CS2.exe", "valorant.exe"]), profile("trabalho", &[])];
        assert_eq!(pick(&ps, "cs2.exe", Some("trabalho")).unwrap().id, "jogo");
        assert_eq!(pick(&ps, "chrome.exe", Some("trabalho")).unwrap().id, "trabalho");
        assert!(pick(&ps, "chrome.exe", None).is_none());
        assert!(pick(&ps, "chrome.exe", Some("apagado")).is_none());
    }

    #[test]
    fn roundtrip_file() {
        let dir = std::env::temp_dir().join(format!("m808-test-{}", std::process::id()));
        let path = dir.join("profiles.json");
        let ps = vec![profile("a", &["x.exe"])];
        save(&path, &ps).unwrap();
        assert_eq!(load(&path), ps);
        let _ = std::fs::remove_dir_all(dir);
    }
}
