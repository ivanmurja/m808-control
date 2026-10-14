//! Textos mostrados pelo lado Rust (bandeja, avisos na tela, erros) em português ou inglês.
//! A interface web tem suas próprias traduções em `src/i18n.ts`.

use serde::{Deserialize, Serialize};

use crate::device::DeviceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LangSetting {
    #[default]
    Auto,
    Pt,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Pt,
    En,
}

impl LangSetting {
    pub fn resolve(self) -> Lang {
        match self {
            LangSetting::Pt => Lang::Pt,
            LangSetting::En => Lang::En,
            LangSetting::Auto => from_locale(sys_locale::get_locale().as_deref()),
        }
    }
}

fn from_locale(locale: Option<&str>) -> Lang {
    match locale {
        Some(l) if l.to_lowercase().starts_with("pt") => Lang::Pt,
        _ => Lang::En,
    }
}

#[derive(Clone, Copy)]
pub enum Text {
    Open,
    Quit,
    Autostart,
    Profiles,
    AutoSwitch,
    NotConnected,
    Battery,
    InstallUpdate,
    UpdateAvailable,
    UpdateFailed,
    LowBattery,
    Profile,
    Level,
    TooltipDisconnected,
}

pub fn tr(lang: Lang, text: Text) -> &'static str {
    use Text::*;
    match (lang, text) {
        (Lang::Pt, Open) => "Abrir M808 Control",
        (Lang::En, Open) => "Open M808 Control",
        (Lang::Pt, Quit) => "Sair",
        (Lang::En, Quit) => "Quit",
        (Lang::Pt, Autostart) => "Iniciar com o sistema",
        (Lang::En, Autostart) => "Start with the system",
        (Lang::Pt, Profiles) => "Perfis",
        (Lang::En, Profiles) => "Profiles",
        (Lang::Pt, AutoSwitch) => "Trocar automaticamente",
        (Lang::En, AutoSwitch) => "Switch automatically",
        (Lang::Pt, NotConnected) => "Mouse não conectado",
        (Lang::En, NotConnected) => "Mouse not connected",
        (Lang::Pt, Battery) => "Bateria",
        (Lang::En, Battery) => "Battery",
        (Lang::Pt, InstallUpdate) => "Instalar atualização",
        (Lang::En, InstallUpdate) => "Install update",
        (Lang::Pt, UpdateAvailable) => "Atualização disponível",
        (Lang::En, UpdateAvailable) => "Update available",
        (Lang::Pt, UpdateFailed) => "Falha ao atualizar",
        (Lang::En, UpdateFailed) => "Update failed",
        (Lang::Pt, LowBattery) => "Bateria fraca",
        (Lang::En, LowBattery) => "Low battery",
        (Lang::Pt, Profile) => "Perfil",
        (Lang::En, Profile) => "Profile",
        (Lang::Pt, Level) => "Nível",
        (Lang::En, Level) => "Level",
        (Lang::Pt, TooltipDisconnected) => "mouse desconectado",
        (Lang::En, TooltipDisconnected) => "mouse disconnected",
    }
}

pub fn device_error(lang: Lang, e: &DeviceError) -> String {
    match (lang, e) {
        (Lang::Pt, DeviceError::NotFound) => "mouse M808 não encontrado".into(),
        (Lang::En, DeviceError::NotFound) => "M808 mouse not found".into(),
        (Lang::Pt, DeviceError::Timeout) => {
            "o mouse não respondeu (está dormindo ou desligado? mexa o mouse e tente de novo)".into()
        }
        (Lang::En, DeviceError::Timeout) => {
            "the mouse did not respond (asleep or off? move the mouse and try again)".into()
        }
        (Lang::Pt, DeviceError::Rejected { cmd, status }) => {
            format!("o mouse recusou o comando 0x{cmd:02x} (status 0x{status:02x})")
        }
        (Lang::En, DeviceError::Rejected { cmd, status }) => {
            format!("the mouse rejected command 0x{cmd:02x} (status 0x{status:02x})")
        }
        (Lang::Pt, DeviceError::Handshake) => "falha na autenticação do handshake".into(),
        (Lang::En, DeviceError::Handshake) => "handshake authentication failed".into(),
        (Lang::Pt, DeviceError::Hid(e)) => format!("erro HID: {e}"),
        (Lang::En, DeviceError::Hid(e)) => format!("HID error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection() {
        assert_eq!(from_locale(Some("pt-BR")), Lang::Pt);
        assert_eq!(from_locale(Some("PT_pt")), Lang::Pt);
        assert_eq!(from_locale(Some("en-US")), Lang::En);
        assert_eq!(from_locale(Some("es-ES")), Lang::En);
        assert_eq!(from_locale(None), Lang::En);
    }

    #[test]
    fn settings_values() {
        let s: LangSetting = serde_json::from_str("\"pt\"").unwrap();
        assert_eq!(s, LangSetting::Pt);
        assert_eq!(serde_json::to_string(&LangSetting::Auto).unwrap(), "\"auto\"");
    }
}
