# M808 Control

Configurador **open source** para o mouse **Redragon Storm Pro (M808-KS)** — uma alternativa leve
ao software oficial, com recursos que ele não oferece. Feito com [Tauri](https://tauri.app) (Rust + web).

> Projeto independente, sem relação com a Redragon. O protocolo foi obtido por engenharia reversa
> para fins de interoperabilidade — veja [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Instalação (Windows)

Baixe o instalador `.exe` (ou `.msi`) na página de [Releases](../../releases) e execute.

O instalador não é assinado digitalmente, então o SmartScreen pode mostrar um aviso:
clique em **Mais informações → Executar assim mesmo**.

Feche o software oficial da Redragon antes de usar — os dois disputam a mesma conexão com o mouse.

## Recursos

- **DPI**: até 8 níveis (100–16000, passo 100), quantos entram no ciclo, nível ativo e cor de cada um
- **Botões**: vista de cima do mouse com os 8 botões clicáveis — cliques, voltar/avançar, DPI +/−/ciclo,
  troca de polling rate, liga/desliga luzes, reset de fábrica ou desativado
- **Faixa RGB lateral**: onda arco-íris, **onda colorida**, respiração, fixo ou desligado;
  velocidade e brilho em 10 níveis
- **Faixa sincronizada com o DPI**: ao trocar o nível no mouse, a faixa assume a cor dele
- **Luz da roda** (indicador de DPI): fixa, respiração ou desligada
- **Polling rate**: 125 / 250 / 500 / 1000 Hz
- **Backup e restauração** da memória do mouse (backup automático antes de gravar a função de reset)
- **Bandeja do sistema**: troca rápida de DPI; inicia com o Windows já minimizado (opcional)
- Leve: fechar a janela destrói o WebView; na bandeja fica só o processo Rust
- Grava **só os bytes que mudaram** e confere relendo a memória

> As configurações ficam gravadas **no próprio mouse** — ele funciona igual com o app fechado.
> O app na bandeja só é necessário para a sincronização da faixa com o DPI e para a troca rápida.

### O que este app faz e o oficial não

| | Oficial | M808 Control |
|---|:---:|:---:|
| Onda numa cor só na faixa lateral | — | ✅ |
| Faixa acompanhando a cor do nível de DPI | — | ✅ |
| Backup/restauração da memória do mouse | — | ✅ |
| Troca de DPI pela bandeja | — | ✅ |
| Código aberto / versão Linux (planejada) | — | ✅ |
| Macros e combinações de teclas | ✅ | 🚧 |

## Plataformas

| Sistema | Situação |
|---------|----------|
| Windows 10/11 | ✅ testado (receptor 2.4 GHz) |
| Linux | 🚧 planejado — o código já usa hidapi/hidraw; falta regra udev e testes |

## Compilar

Pré-requisitos: [Rust](https://rustup.rs), Node.js 20+ e, no Windows, Microsoft C++ Build Tools.

```bash
npm install
npm run tauri dev      # modo desenvolvimento
npm run tauri build    # instaladores em src-tauri/target/release/bundle
cd src-tauri && cargo test --lib   # testes do protocolo
```

Releases: crie uma tag `vX.Y.Z` e o GitHub Actions gera os instaladores num rascunho de release.

## Estrutura

```
src/                 interface (TypeScript + Vite, sem framework)
  mouse.ts           ilustração do Storm Pro com os botões clicáveis
src-tauri/src/
  protocol.rs        protocolo puro: pacotes, checksums, mapa da EEPROM (com testes)
  device.rs          transporte HID (hidapi)
  lib.rs             comandos, bandeja, inicialização automática e eventos do mouse
tools/m808.py        script Python de leitura/escrita usado na engenharia reversa
docs/PROTOCOL.md     documentação do protocolo
```

## Licença

[MIT](LICENSE)
