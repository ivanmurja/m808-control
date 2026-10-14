//! Perfis: configurações completas do mouse salvas no PC, com regras de troca automática
//! (programa em primeiro plano, bateria fraca, horário) e importação/exportação.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::protocol::Config;

/// Intervalo de horário em que um perfil vale. Dias: 0 = domingo … 6 = sábado.
/// Se `end` < `start`, o intervalo cruza a meia-noite (ex.: 22:00–02:00).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub start: String,
    pub end: String,
    pub days: Vec<u8>,
}

impl Schedule {
    fn minutes(hhmm: &str) -> Option<u32> {
        let (h, m) = hhmm.split_once(':')?;
        let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
        (h < 24 && m < 60).then_some(h * 60 + m)
    }

    /// `weekday` 0..=6 (domingo = 0), `minute` = minutos desde a meia-noite.
    pub fn contains(&self, weekday: u8, minute: u32) -> bool {
        let (Some(start), Some(end)) = (Self::minutes(&self.start), Self::minutes(&self.end)) else {
            return false;
        };
        let on = |d: u8| self.days.contains(&d);
        if start <= end {
            on(weekday) && (start..end).contains(&minute)
        } else {
            // a parte depois da meia-noite pertence ao dia em que o intervalo começou
            (on(weekday) && minute >= start) || (on((weekday + 6) % 7) && minute < end)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub config: Config,
    /// Executáveis (ex.: `cs2.exe`) que ativam este perfil quando estão em primeiro plano.
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub schedule: Option<Schedule>,
}

impl Profile {
    pub fn matches_app(&self, exe: &str) -> bool {
        self.apps.iter().any(|a| a.trim().eq_ignore_ascii_case(exe))
    }
}

/// Situação atual usada pelas regras.
pub struct Context<'a> {
    pub exe: Option<&'a str>,
    pub battery: Option<u8>,
    pub weekday: u8,
    pub minute: u32,
}

/// Preferências globais das regras.
pub struct Rules<'a> {
    pub default_profile: Option<&'a str>,
    pub low_battery_profile: Option<&'a str>,
    pub low_battery_threshold: u8,
}

/// Perfil que deve valer agora, por prioridade: programa em uso > bateria fraca > horário > padrão.
pub fn pick<'a>(profiles: &'a [Profile], ctx: &Context, rules: &Rules) -> Option<&'a Profile> {
    let by_id = |id: Option<&str>| id.and_then(|id| profiles.iter().find(|p| p.id == id));

    ctx.exe
        .and_then(|exe| profiles.iter().find(|p| p.matches_app(exe)))
        .or_else(|| {
            ctx.battery
                .filter(|b| *b <= rules.low_battery_threshold)
                .and_then(|_| by_id(rules.low_battery_profile))
        })
        .or_else(|| {
            profiles
                .iter()
                .find(|p| p.schedule.as_ref().is_some_and(|s| s.contains(ctx.weekday, ctx.minute)))
        })
        .or_else(|| by_id(rules.default_profile))
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

const EXPORT_FORMAT: &str = "m808-profiles";

#[derive(Serialize, Deserialize)]
struct ExportFile {
    format: String,
    version: u32,
    profiles: Vec<Profile>,
}

pub fn export(path: &Path, profiles: &[Profile]) -> std::io::Result<()> {
    let file = ExportFile { format: EXPORT_FORMAT.into(), version: 1, profiles: profiles.to_vec() };
    std::fs::write(path, serde_json::to_vec_pretty(&file).expect("perfis serializáveis"))
}

/// Lê um arquivo exportado; os perfis ganham ids novos para não colidir com os existentes.
pub fn import(path: &Path) -> Result<Vec<Profile>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let file: ExportFile = serde_json::from_slice(&bytes).map_err(|_| "arquivo de perfis inválido".to_string())?;
    if file.format != EXPORT_FORMAT {
        return Err("arquivo de perfis inválido".into());
    }
    Ok(file.profiles.into_iter().map(|p| Profile { id: new_id(), ..p }).collect())
}

pub fn new_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{:x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CONFIG_LEN;

    fn profile(id: &str, apps: &[&str], schedule: Option<(&str, &str, &[u8])>) -> Profile {
        Profile {
            id: id.into(),
            name: id.into(),
            config: Config::from_eeprom(&[0x55; CONFIG_LEN]),
            apps: apps.iter().map(|s| s.to_string()).collect(),
            schedule: schedule.map(|(s, e, d)| Schedule { start: s.into(), end: e.into(), days: d.to_vec() }),
        }
    }

    const RULES: Rules = Rules { default_profile: Some("padrao"), low_battery_profile: Some("economia"), low_battery_threshold: 20 };

    fn ctx(exe: Option<&str>, battery: Option<u8>, weekday: u8, hh: u32) -> Context<'_> {
        Context { exe, battery, weekday, minute: hh * 60 }
    }

    #[test]
    fn priority_app_battery_schedule_default() {
        let ps = [
            profile("jogo", &["CS2.exe"], None),
            profile("economia", &[], None),
            profile("trabalho", &[], Some(("09:00", "18:00", &[1, 2, 3, 4, 5]))),
            profile("padrao", &[], None),
        ];
        let id = |c: &Context| pick(&ps, c, &RULES).map(|p| p.id.clone());
        // programa ganha de tudo
        assert_eq!(id(&ctx(Some("cs2.exe"), Some(5), 1, 10)).as_deref(), Some("jogo"));
        // bateria fraca ganha do horário
        assert_eq!(id(&ctx(Some("chrome.exe"), Some(20), 1, 10)).as_deref(), Some("economia"));
        // horário de trabalho numa segunda às 10h
        assert_eq!(id(&ctx(Some("chrome.exe"), Some(80), 1, 10)).as_deref(), Some("trabalho"));
        // fora do horário (domingo) → padrão
        assert_eq!(id(&ctx(None, None, 0, 10)).as_deref(), Some("padrao"));
        // às 18h em ponto já terminou
        assert_eq!(id(&ctx(None, None, 1, 18)).as_deref(), Some("padrao"));
    }

    #[test]
    fn schedule_across_midnight() {
        let s = Schedule { start: "22:00".into(), end: "02:00".into(), days: vec![5] }; // sexta à noite
        assert!(s.contains(5, 23 * 60));
        assert!(s.contains(6, 60), "1h de sábado ainda é a noite de sexta");
        assert!(!s.contains(6, 23 * 60), "sábado à noite não está marcado");
        assert!(!s.contains(5, 60), "1h de sexta pertence à quinta");
    }

    #[test]
    fn invalid_schedule_never_matches() {
        let s = Schedule { start: "25:00".into(), end: "x".into(), days: vec![0, 1, 2, 3, 4, 5, 6] };
        assert!(!s.contains(1, 600));
    }

    #[test]
    fn export_import_gives_new_ids() {
        let dir = std::env::temp_dir().join(format!("m808-test-exp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.m808profiles");
        let ps = vec![profile("a", &["x.exe"], Some(("08:00", "12:00", &[1])))];
        export(&path, &ps).unwrap();
        let back = import(&path).unwrap();
        assert_eq!(back.len(), 1);
        assert_ne!(back[0].id, "a");
        assert_eq!(back[0].apps, ps[0].apps);
        assert_eq!(back[0].schedule, ps[0].schedule);
        std::fs::write(&path, b"{\"format\":\"outro\",\"version\":1,\"profiles\":[]}").unwrap();
        assert!(import(&path).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn old_profiles_without_schedule_still_load() {
        let mut v = serde_json::to_value(profile("a", &[], None)).unwrap();
        v.as_object_mut().unwrap().remove("schedule");
        let p: Profile = serde_json::from_value(v).unwrap();
        assert!(p.schedule.is_none());
    }
}
