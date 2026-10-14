//! Aviso rápido na tela ("2000 DPI", "Perfil: Jogo").
//!
//! A janela só existe enquanto o aviso aparece: é criada sob demanda e destruída em seguida,
//! para não manter um WebView ocupando memória.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const OSD_WINDOW: &str = "osd";
const VISIBLE_FOR: Duration = Duration::from_millis(1600);
const WIDTH: f64 = 260.0;
const HEIGHT: f64 = 64.0;
/// Distância da borda inferior da tela (fica acima da barra de tarefas).
const BOTTOM_MARGIN: f64 = 120.0;

/// Cada aviso novo invalida o temporizador de fechamento do anterior.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn show(app: &AppHandle, text: &str) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    if app.get_webview_window(OSD_WINDOW).is_some() {
        let _ = app.emit_to(OSD_WINDOW, "osd-text", text);
    } else {
        let url = format!("osd.html#{}", percent_encode(text));
        let mut builder = WebviewWindowBuilder::new(app, OSD_WINDOW, WebviewUrl::App(url.into()))
            .title("M808 Control")
            .inner_size(WIDTH, HEIGHT)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .visible(true);
        if let Ok(Some(m)) = app.primary_monitor() {
            let scale = m.scale_factor();
            let pos = m.position().to_logical::<f64>(scale);
            let size = m.size().to_logical::<f64>(scale);
            builder = builder.position(pos.x + (size.width - WIDTH) / 2.0, pos.y + size.height - HEIGHT - BOTTOM_MARGIN);
        }
        if let Ok(w) = builder.build() {
            // Não pode roubar cliques nem foco (ex.: no meio de um jogo).
            let _ = w.set_ignore_cursor_events(true);
        }
    }

    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(VISIBLE_FOR);
        if GENERATION.load(Ordering::SeqCst) == generation {
            if let Some(w) = app.get_webview_window(OSD_WINDOW) {
                let _ = w.destroy();
            }
        }
    });
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_utf8() {
        assert_eq!(super::percent_encode("Nível 2 · 800"), "N%C3%ADvel%202%20%C2%B7%20800");
    }
}
