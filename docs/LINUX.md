# M808 Control no Linux (experimental)

O app é o mesmo do Windows: o código usa `hidapi` (backend `hidraw`), que funciona igual no Linux.
O suporte ainda é **experimental** — compila e é empacotado pelo CI, mas precisa de testes com o mouse
de verdade. Relatos (funcionou / não funcionou, distro, receptor ou cabo) são muito bem-vindos nas
[issues](https://github.com/ivanmurja/m808-control/issues).

## Instalação

Baixe da [última release](https://github.com/ivanmurja/m808-control/releases/latest):

| Distro | Arquivo | Regra udev |
|--------|---------|------------|
| Debian, Ubuntu, Mint, Pop!_OS | `.deb` | instalada automaticamente |
| Fedora, openSUSE | `.rpm` | instalada automaticamente |
| Qualquer outra | `.AppImage` | instalar à mão (abaixo) |

Depois de instalar o `.deb`/`.rpm`, **desconecte e reconecte o receptor** (ou reinicie) para a regra valer.

## Permissão de acesso ao mouse (udev)

Por padrão só o root acessa dispositivos HID. A regra
[`packaging/linux/70-m808-control.rules`](../packaging/linux/70-m808-control.rules) libera o acesso para
o usuário logado. Para o AppImage:

```bash
sudo curl -L -o /etc/udev/rules.d/70-m808-control.rules \
  https://raw.githubusercontent.com/ivanmurja/m808-control/main/packaging/linux/70-m808-control.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Sem a regra o app mostra "mouse M808 não encontrado".

## Diferenças em relação ao Windows

- **Troca de perfil pelo programa em uso** ainda não funciona no Linux (detectar a janela em primeiro
  plano depende do ambiente: X11, cada compositor Wayland…). Horário e bateria fraca funcionam.
- O **ícone da bandeja** precisa de suporte a AppIndicator (GNOME: extensão "AppIndicator Support").
- **Iniciar com o sistema** cria um atalho em `~/.config/autostart`.

## Compilar

```bash
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libudev-dev
npm install
npm run tauri dev
```
