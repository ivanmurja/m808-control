// Janela do aviso rápido: o texto inicial vem na URL; avisos seguidos chegam por evento.
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const el = document.getElementById("text")!;

function show(text: string) {
  el.textContent = text;
  // reinicia a animação de entrada
  el.style.animation = "none";
  void el.offsetWidth;
  el.style.animation = "";
}

show(decodeURIComponent(location.hash.slice(1)));
getCurrentWebviewWindow().listen<string>("osd-text", ({ payload }) => show(payload));
