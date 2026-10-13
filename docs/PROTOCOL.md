# Protocolo HID — Redragon M808-KS

Obtido por engenharia reversa do software oficial (`OemDrv.exe` 3.1 / `Lowerdev.dll`) e validado
no hardware (receptor 2.4 GHz). ✔ = confirmado no mouse; demais itens vêm da análise estática.

## Dispositivo

| | |
|---|---|
| VID | `0x25A7` |
| PID | `0xFA07` receptor 2.4 GHz ✔ · `0xFA08` cabo (provável) |
| Interface | `MI_01`, coleções vendor-defined |

Duas coleções HID são usadas:

| Função | Usage Page | Usage | Report |
|--------|-----------|-------|--------|
| Comandos | `0xFF02` | `0x02` | Feature report ID `0x08`, 17 bytes ✔ |
| Respostas / eventos | `0xFF01` | `0x00` | Input report ID `0x09`, 17 bytes ✔ |

## Pacote

```
byte  0     1     2 .. 15     16
      0x08  cmd   payload     chk
chk = (0x55 - soma(bytes 0..15)) & 0xFF     →  soma de todos os 17 bytes ≡ 0x55
```

Resposta (input report `0x09`): `byte1` = eco do comando, `byte2` = status (`00` OK, `01` erro de
parâmetro ✔), dados no mesmo layout do pedido.

Se `byte1 == 0x0A` é um **evento espontâneo** do mouse: `byte6` = flags; com bit 0 ligado,
`byte7` = novo nível de DPI (0-based).

O receptor responde sozinho aos comandos `0x01`–`0x04`. Leitura/escrita da EEPROM vai até o mouse:
se ele estiver em repouso o primeiro pacote se perde — reenviar ✔.

## Comandos

| cmd | Função | Payload (bytes 2..) |
|-----|--------|---------------------|
| `0x01` | Handshake ✔ | `00 00 00 04 r0 r1 r2 r3` (r = aleatórios 1..100) |
| `0x02` | "TellDrvStatus" | `00 00 00 01 02` |
| `0x03` | ? | — |
| `0x04` | ? (verificação; responde `[5]=02 [6]=09`) | — |
| `0x07` | Escrever EEPROM ✔ | `00 addrH addrL len dados[≤10]` |
| `0x08` | Ler EEPROM ✔ | `00 addrH addrL len(≤10)` → dados em `resp[6..]` |
| `0x09` | Enviado pelo app oficial após certas gravações | — |

Handshake: com a chave `k = 05 00 00 00`, o mouse responde em `resp[6..9]`:
`k0+r0+r1, k1+2·r1+r2, k2+3·r2+r3, k3+4·r3+r0` (mod 256) ✔.

Gravações têm efeito imediato, sem comando extra ✔.

## Mapa da EEPROM

Quase todo campo tem checksum: entradas `[a, b, c, 0x55-a-b-c]` ou pares `[v, 0x55-v]`.

| Endereço | Formato | Conteúdo |
|----------|---------|----------|
| `0x00` | par | Polling: `1`=1000 Hz, `2`=500, `4`=250, `8`=125 ✔ |
| `0x02` | par | Nº de níveis de DPI no ciclo (1..8) ✔ |
| `0x04` | par | Nível de DPI ativo, 0-based ✔ |
| `0x06`–`0x0B` | pares | ? (`00 55`) |
| `0x0C` | 8 × entrada | DPI: `[códX, códY, 0, chk]` ✔ |
| `0x2C` | 8 × entrada | Cor do nível: `[R, G, B, chk]` ✔ |
| `0x4C` | 3 pares | LED da roda (indicador de DPI): efeito, velocidade, brilho ✔ |
| `0x54` | entrada + 3 pares | "Logo": `[R, G, B, chk]`, efeito, velocidade, brilho — sem LED visível no Storm Pro (`ShowLogoEffect=0`) |
| `0x60` | 16 × entrada | Botões: `[tipo, código, param, chk]` ✔ |
| `0xA0` | entrada de 7 | **Faixa RGB lateral**: `[modo, R, G, B, vel, brilho, chk]` ✔ |
| `0xA7`–`0xAA` | pares | ? (`00 55`, `01 54`) |

### Faixa RGB lateral (`0xA0`) ✔

| modo | efeito | observação |
|------|--------|------------|
| `00` | Onda arco-íris | o app oficial chama de "Neon"; velocidade: tabela `2D 28 23 1E 19 13 0F 0A 05 03` (menor = mais rápido) |
| `01` | Respiração na cor | velocidade: tabela `FF E6 D2 BE AA 96 82 6E 46 28` |
| `02` | Fixo na cor | |
| `03` | **Onda na cor escolhida** | não exposto pelo app oficial |
| `04` | Desligado | cor fica guardada |
| `05`+ | respiração rápida | inválido — fallback do firmware |

Brilho: tabela `19 32 4B 64 7D 96 AF C8 E1 FF` (níveis 1..10).

### LED da roda (`0x4C`) ✔

Mostra sempre a cor do nível de DPI ativo (`0x2C`). Efeito: `00` desligado, `01` fixo,
`02` respiração; `03`/`04` apagam (inválidos).

### Observação sobre gravação

Se o mouse estiver acordando, a escrita pode ser aplicada **sem** a resposta chegar ✔ — confirmar
relendo a EEPROM em vez de confiar só no ACK.
| `0x100 + i·0x20` | | Combinação de teclas do botão i |
| `0x300 + i·0x180` | | Macro i |

### DPI (sensor `0x3335`)

Código ≈ DPI / 84, tabela exata de 100 a 16000 em passos de 100 em
[`dpi_3335.json`](dpi_3335.json) e em `src-tauri/src/protocol.rs`.

### Velocidade dos efeitos (LED de DPI e logo)

Níveis 1..10 → `01 1E 3C 5A 80 96 B4 D2 E6 FF`.

Efeitos no software oficial: Steady, Respiration, Neon, Streaming, OFF — a correspondência com os
valores gravados ainda não foi confirmada (`0` = desligado).

### Botões

Slot na matriz de 16 posições por botão físico (do `Cfg.ini`): 1→0, 2→1, 3→2, 4→4, 5→3, 6→10,
7→11, 8→9, 9→5, 10→6.

| Tipo | Código | Função |
|------|--------|--------|
| `0` | `0` | Desativado |
| `1` | bitmask | Botão de mouse: `01` esq, `02` dir, `04` meio, `08` voltar, `10` avançar ✔ |
| `2` | `01`/`02`/`03` | DPI ciclo / DPI+ / DPI− |
| `4` | `hi`, `lo` | Tecla única (código no `param`) |
| `5` | | Combinação de teclas (dados em `0x100+`) |
| `6` | | Macro (`param` = índice / `FF`/`FE` = modos de repetição) |
| `7` | `0` | **Troca de polling rate** ✔ — 1000 → 125 → 250 → 500 Hz…; a roda pisca e o mouse grava `0x00` |
| `8` | `0` | **Liga/desliga todas as luzes** ✔ — ao apagar grava faixa `04` e roda `00` na EEPROM |
| `9` | `0` | ⚠️ **Reset de fábrica** ✔ — restaura DPIs 1000/2000/4000/8000/16000, cores, botões e polling 500 Hz |

Padrões de fábrica observados após o reset: slot 7 = `02 01` (DPI ciclo), slot 8 = `04 28 03`.

Evento `0x0A` com flag bit 1: `byte8 & 0x0F` = novo código de polling ✔.
