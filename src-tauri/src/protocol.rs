//! Protocolo HID do Redragon M808-KS (engenharia reversa — ver docs/PROTOCOL.md).
//!
//! Este módulo é puro (sem I/O): monta pacotes, calcula checksums e converte entre a
//! imagem da EEPROM do mouse e a estrutura `Config` usada pela interface.

use serde::{Deserialize, Serialize};

pub const VID: u16 = 0x25A7;
/// 0xFA08 = mouse no cabo (provável), 0xFA07 = dongle 2.4G (confirmado).
pub const PIDS: [u16; 2] = [0xFA08, 0xFA07];

/// Coleção vendor que recebe comandos (feature report 0x08).
pub const CMD_USAGE_PAGE: u16 = 0xFF02;
pub const CMD_USAGE: u16 = 0x02;
/// Coleção vendor que devolve respostas/eventos (input report 0x09).
pub const NOTIFY_USAGE_PAGE: u16 = 0xFF01;
pub const NOTIFY_USAGE: u16 = 0x00;

pub const REPORT_CMD: u8 = 0x08;
pub const REPORT_NOTIFY: u8 = 0x09;
pub const PACKET_LEN: usize = 17;

pub const CMD_HANDSHAKE: u8 = 0x01;
pub const CMD_WRITE_EEPROM: u8 = 0x07;
pub const CMD_READ_EEPROM: u8 = 0x08;
pub const EVT_DEVICE: u8 = 0x0A;

/// Chave do handshake (`PSD` no Cfg.ini do software oficial).
pub const PSD: [u8; 4] = [5, 0, 0, 0];

/// Máximo de bytes de dados por pacote de leitura/escrita da EEPROM.
pub const EEPROM_CHUNK: usize = 10;

/// Região da EEPROM que a interface lê e escreve (configuração, sem macros).
pub const CONFIG_LEN: usize = 0xB0;

pub mod addr {
    pub const POLLING: usize = 0x00;
    pub const LEVEL_COUNT: usize = 0x02;
    pub const ACTIVE_LEVEL: usize = 0x04;
    pub const DPI_TABLE: usize = 0x0C;
    pub const DPI_COLORS: usize = 0x2C;
    pub const DPI_LED: usize = 0x4C;
    pub const LOGO_LED: usize = 0x54;
    pub const BUTTONS: usize = 0x60;
    pub const STRIP: usize = 0xA0;
}

pub const MAX_LEVELS: usize = 8;
pub const BUTTON_SLOTS: usize = 16;

/// Velocidade dos efeitos da luz da roda e do logo, níveis 1..=10 → byte gravado.
pub const LED_SPEED: [u8; 10] = [0x01, 0x1E, 0x3C, 0x5A, 0x80, 0x96, 0xB4, 0xD2, 0xE6, 0xFF];
/// Faixa lateral, ondas (modos 0 e 3): nível 1 (lento) .. 10 (rápido). Menor = mais rápido.
pub const STRIP_SPEED_WAVE: [u8; 10] = [0x2D, 0x28, 0x23, 0x1E, 0x19, 0x13, 0x0F, 0x0A, 0x05, 0x03];
/// Faixa lateral, respiração (modo 1): nível 1 (lento) .. 10 (rápido).
pub const STRIP_SPEED_BREATH: [u8; 10] = [0xFF, 0xE6, 0xD2, 0xBE, 0xAA, 0x96, 0x82, 0x6E, 0x46, 0x28];
/// Faixa lateral, brilho: nível 1..10.
pub const STRIP_BRIGHTNESS: [u8; 10] = [0x19, 0x32, 0x4B, 0x64, 0x7D, 0x96, 0xAF, 0xC8, 0xE1, 0xFF];

/// Modos da faixa RGB lateral (byte 0 de `0xA0`).
pub mod strip_mode {
    pub const RAINBOW_WAVE: u8 = 0;
    pub const BREATH: u8 = 1;
    pub const STEADY: u8 = 2;
    /// Onda numa cor só — o firmware suporta, o app oficial não oferece.
    pub const COLOR_WAVE: u8 = 3;
    pub const OFF: u8 = 4;
}

/// Tipos de função de botão (byte 0 de cada entrada em `0x60`).
pub mod button_kind {
    pub const DISABLED: u8 = 0;
    pub const MOUSE: u8 = 1;
    pub const DPI: u8 = 2;
    pub const POLLING_SWITCH: u8 = 7;
    pub const LIGHTS_TOGGLE: u8 = 8;
    /// Restaura a configuração de fábrica do mouse ao ser pressionado.
    pub const FACTORY_RESET: u8 = 9;
}

/// Sensor 0x3335: código gravado para cada DPI de 100 a 16000 (passo 100).
/// Extraído do OemDrv.exe (.data 0x5cd670 / 0x5cd8f0).
const DPI_CODES: [u8; 160] = [
    0x00, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x10, 0x11, 0x12,
    0x13, 0x14, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x23, 0x25, 0x26,
    0x27, 0x28, 0x29, 0x2A, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x32, 0x33, 0x34, 0x35, 0x36, 0x38, 0x39,
    0x3A, 0x3B, 0x3C, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x44, 0x45, 0x46, 0x47, 0x48, 0x4A, 0x4B, 0x4C,
    0x4D, 0x4E, 0x50, 0x51, 0x52, 0x53, 0x54, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5C, 0x5D, 0x5E, 0x5F,
    0x60, 0x62, 0x63, 0x64, 0x65, 0x66, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6E, 0x6F, 0x70, 0x71, 0x72,
    0x74, 0x75, 0x76, 0x77, 0x78, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x80, 0x81, 0x82, 0x83, 0x84, 0x86,
    0x87, 0x88, 0x89, 0x8A, 0x8C, 0x8D, 0x8E, 0x8F, 0x90, 0x92, 0x93, 0x94, 0x95, 0x96, 0x98, 0x99,
    0x9A, 0x9B, 0x9C, 0x9E, 0x9F, 0xA0, 0xA1, 0xA2, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xAA, 0xAB, 0xAC,
    0xAD, 0xAE, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD,
];

pub const DPI_MIN: u32 = 100;
pub const DPI_MAX: u32 = 16000;
pub const DPI_STEP: u32 = 100;

pub fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b))
}

/// Checksum usado em todo o protocolo: o total (dados + checksum) soma 0x55.
pub fn checksum(bytes: &[u8]) -> u8 {
    0x55u8.wrapping_sub(sum(bytes))
}

/// `[v..., 0x55 - soma(v)]`
pub fn entry<const N: usize>(vals: [u8; N]) -> Vec<u8> {
    let mut v = vals.to_vec();
    v.push(checksum(&vals));
    v
}

/// `[v, 0x55 - v]`
pub fn pair(v: u8) -> [u8; 2] {
    [v, 0x55u8.wrapping_sub(v)]
}

/// Pacote de comando: `[0x08, cmd, payload..., chk]`, 17 bytes.
pub fn packet(cmd: u8, payload: &[u8]) -> [u8; PACKET_LEN] {
    assert!(payload.len() <= PACKET_LEN - 3, "payload grande demais");
    let mut p = [0u8; PACKET_LEN];
    p[0] = REPORT_CMD;
    p[1] = cmd;
    p[2..2 + payload.len()].copy_from_slice(payload);
    p[PACKET_LEN - 1] = checksum(&p[..PACKET_LEN - 1]);
    p
}

pub fn read_eeprom_payload(addr: usize, len: usize) -> [u8; 4] {
    [0, (addr >> 8) as u8, addr as u8, len as u8]
}

pub fn write_eeprom_payload(addr: usize, data: &[u8]) -> Vec<u8> {
    let mut p = vec![0, (addr >> 8) as u8, addr as u8, data.len() as u8];
    p.extend_from_slice(data);
    p
}

pub fn handshake_payload(rnd: [u8; 4]) -> [u8; 8] {
    [0, 0, 0, 0x04, rnd[0], rnd[1], rnd[2], rnd[3]]
}

/// Resposta esperada do mouse ao handshake (bytes 6..10 da resposta).
pub fn handshake_expected(r: [u8; 4]) -> [u8; 4] {
    let k = PSD;
    [
        k[0].wrapping_add(r[0]).wrapping_add(r[1]),
        k[1].wrapping_add(r[1].wrapping_mul(2)).wrapping_add(r[2]),
        k[2].wrapping_add(r[2].wrapping_mul(3)).wrapping_add(r[3]),
        k[3].wrapping_add(r[3].wrapping_mul(4)).wrapping_add(r[0]),
    ]
}

pub fn dpi_to_code(dpi: u32) -> u8 {
    let dpi = dpi.clamp(DPI_MIN, DPI_MAX);
    let idx = ((dpi + DPI_STEP / 2) / DPI_STEP - 1) as usize;
    DPI_CODES[idx.min(DPI_CODES.len() - 1)]
}

/// Converte código → DPI. Códigos fora da tabela usam o vizinho mais próximo.
pub fn code_to_dpi(code: u8) -> u32 {
    let (idx, _) = DPI_CODES
        .iter()
        .enumerate()
        .min_by_key(|(_, &c)| (c as i32 - code as i32).abs())
        .unwrap();
    (idx as u32 + 1) * DPI_STEP
}

pub fn polling_code(hz: u16) -> u8 {
    match hz {
        1000 => 1,
        500 => 2,
        250 => 4,
        _ => 8,
    }
}

pub fn polling_hz(code: u8) -> u16 {
    match code {
        1 => 1000,
        2 => 500,
        4 => 250,
        _ => 125,
    }
}

/// Nível 1..=10 cujo valor na tabela está mais próximo de `raw`.
fn level_of(raw: u8, table: &[u8; 10]) -> u8 {
    table
        .iter()
        .enumerate()
        .min_by_key(|(_, &s)| (s as i32 - raw as i32).abs())
        .map(|(i, _)| i as u8 + 1)
        .unwrap()
}

/// Byte a gravar para `level`; se o byte atual já corresponde a esse nível ele é mantido
/// (preserva valores fora da tabela e garante releitura idêntica).
fn raw_for(level: u8, current: u8, table: &[u8; 10]) -> u8 {
    let level = level.clamp(1, 10);
    if level_of(current, table) == level {
        current
    } else {
        table[(level - 1) as usize]
    }
}

fn strip_speed_table(mode: u8) -> &'static [u8; 10] {
    if mode == strip_mode::BREATH {
        &STRIP_SPEED_BREATH
    } else {
        &STRIP_SPEED_WAVE
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DpiLevel {
    pub dpi: u32,
    pub color: [u8; 3],
}

/// Efeito de LED com velocidade em níveis 1..=10 e brilho cru.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedEffect {
    /// Luz da roda: 0 desligada, 1 fixa, 2 respiração (sempre na cor do nível de DPI).
    pub mode: u8,
    /// 1..=10
    pub speed: u8,
    pub brightness: u8,
}

/// Bloco `0x54`. No Storm Pro não há LED visível ligado a ele; é só preservado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogoLed {
    pub color: [u8; 3],
    #[serde(flatten)]
    pub effect: LedEffect,
}

/// Faixa RGB lateral (`0xA0`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripLed {
    /// Ver [`strip_mode`].
    pub mode: u8,
    pub color: [u8; 3],
    /// 1 (lento) ..= 10 (rápido)
    pub speed: u8,
    /// 1..=10
    pub brightness: u8,
}

/// Função de um botão como gravada no mouse: `[kind, code, param]`. Ver [`button_kind`].
/// kind 1 = botão de mouse (code = bitmask: 1 esq, 2 dir, 4 meio, 8 voltar, 0x10 avançar);
/// kind 2 = DPI (code 1 ciclo, 2 mais, 3 menos).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub kind: u8,
    pub code: u8,
    pub param: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub polling_hz: u16,
    /// Quantos níveis de DPI estão ativos no ciclo (1..=8).
    pub level_count: u8,
    /// Nível ativo, 0-based.
    pub active_level: u8,
    pub levels: Vec<DpiLevel>,
    /// Luz da roda (indicador de DPI).
    pub dpi_led: LedEffect,
    pub logo_led: LogoLed,
    pub strip: StripLed,
    pub buttons: Vec<Button>,
}

impl Config {
    /// Lê a configuração de uma imagem da EEPROM (pelo menos `CONFIG_LEN` bytes).
    pub fn from_eeprom(img: &[u8]) -> Config {
        assert!(img.len() >= CONFIG_LEN);
        let levels = (0..MAX_LEVELS)
            .map(|i| {
                let d = addr::DPI_TABLE + i * 4;
                let c = addr::DPI_COLORS + i * 4;
                DpiLevel {
                    dpi: code_to_dpi(img[d]),
                    color: [img[c], img[c + 1], img[c + 2]],
                }
            })
            .collect();
        let dl = addr::DPI_LED;
        let ll = addr::LOGO_LED;
        let s = addr::STRIP;
        let buttons = (0..BUTTON_SLOTS)
            .map(|i| {
                let b = addr::BUTTONS + i * 4;
                Button { kind: img[b], code: img[b + 1], param: img[b + 2] }
            })
            .collect();
        Config {
            polling_hz: polling_hz(img[addr::POLLING]),
            level_count: img[addr::LEVEL_COUNT].clamp(1, MAX_LEVELS as u8),
            active_level: img[addr::ACTIVE_LEVEL].min(MAX_LEVELS as u8 - 1),
            levels,
            dpi_led: LedEffect {
                mode: img[dl],
                speed: level_of(img[dl + 2], &LED_SPEED),
                brightness: img[dl + 4],
            },
            logo_led: LogoLed {
                color: [img[ll], img[ll + 1], img[ll + 2]],
                effect: LedEffect {
                    mode: img[ll + 4],
                    speed: level_of(img[ll + 6], &LED_SPEED),
                    brightness: img[ll + 8],
                },
            },
            strip: StripLed {
                mode: img[s],
                color: [img[s + 1], img[s + 2], img[s + 3]],
                speed: level_of(img[s + 4], strip_speed_table(img[s])),
                brightness: level_of(img[s + 5], &STRIP_BRIGHTNESS),
            },
            buttons,
        }
    }

    /// Aplica a configuração sobre uma imagem existente, preservando bytes desconhecidos.
    pub fn apply_to(&self, img: &mut [u8]) {
        assert!(img.len() >= CONFIG_LEN);
        let old = img.to_vec();
        let mut put = |at: usize, bytes: &[u8]| img[at..at + bytes.len()].copy_from_slice(bytes);

        put(addr::POLLING, &pair(polling_code(self.polling_hz)));
        put(addr::LEVEL_COUNT, &pair(self.level_count.clamp(1, MAX_LEVELS as u8)));
        put(addr::ACTIVE_LEVEL, &pair(self.active_level.min(MAX_LEVELS as u8 - 1)));

        for (i, l) in self.levels.iter().take(MAX_LEVELS).enumerate() {
            let c = dpi_to_code(l.dpi);
            put(addr::DPI_TABLE + i * 4, &entry([c, c, 0]));
            put(addr::DPI_COLORS + i * 4, &entry(l.color));
        }

        let dl = addr::DPI_LED;
        let e = &self.dpi_led;
        let mut v = Vec::new();
        v.extend(pair(e.mode));
        v.extend(pair(raw_for(e.speed, old[dl + 2], &LED_SPEED)));
        v.extend(pair(e.brightness));
        put(dl, &v);

        let ll = addr::LOGO_LED;
        let l = &self.logo_led;
        let mut v = entry(l.color);
        v.extend(pair(l.effect.mode));
        v.extend(pair(raw_for(l.effect.speed, old[ll + 6], &LED_SPEED)));
        v.extend(pair(l.effect.brightness));
        put(ll, &v);

        let s = &self.strip;
        let speed = raw_for(s.speed, old[addr::STRIP + 4], strip_speed_table(s.mode));
        let bri = raw_for(s.brightness, old[addr::STRIP + 5], &STRIP_BRIGHTNESS);
        put(addr::STRIP, &entry([s.mode, s.color[0], s.color[1], s.color[2], speed, bri]));

        for (i, b) in self.buttons.iter().take(BUTTON_SLOTS).enumerate() {
            put(addr::BUTTONS + i * 4, &entry([b.kind, b.code, b.param]));
        }
    }

    /// Cor do nível de DPI ativo.
    pub fn active_color(&self) -> [u8; 3] {
        self.levels.get(self.active_level as usize).map_or([0; 3], |l| l.color)
    }
}

/// Trechos (≤ `EEPROM_CHUNK` bytes) que cobrem tudo onde `new` difere de `old`:
/// `(endereço, bytes)`. Diferenças próximas vão no mesmo pacote, levando junto os
/// bytes iguais entre elas.
pub fn diff_runs(old: &[u8], new: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let differs = |i: usize| old[i] != new[i];
    let mut runs = Vec::new();
    let mut i = 0;
    while i < new.len() {
        if !differs(i) {
            i += 1;
            continue;
        }
        let start = i;
        let limit = (start + EEPROM_CHUNK).min(new.len());
        let end = (start..limit).rev().find(|&j| differs(j)).unwrap() + 1;
        runs.push((start, new[start..end].to_vec()));
        i = end;
    }
    runs
}

/// Evento espontâneo do mouse (input report com byte1 = 0x0A).
#[derive(Debug, Clone, Serialize)]
pub struct DeviceEvent {
    /// Novo nível de DPI ativo (0-based), quando o usuário troca pelo botão.
    pub dpi_level: Option<u8>,
    /// Novo polling rate, quando trocado pelo botão de polling.
    pub polling_hz: Option<u16>,
    pub raw: Vec<u8>,
}

pub fn parse_event(r: &[u8]) -> Option<DeviceEvent> {
    if r.len() < 9 || r[0] != REPORT_NOTIFY || r[1] != EVT_DEVICE {
        return None;
    }
    let flags = r[6];
    Some(DeviceEvent {
        dpi_level: (flags & 1 != 0).then(|| r[7]),
        polling_hz: (flags & 2 != 0).then(|| polling_hz(r[8] & 0x0F)),
        raw: r.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Primeiros 0xB0 bytes lidos de um M808-KS real (eeprom_dump.txt).
    const DUMP: &str = "01 54 05 50 02 53 00 55 00 55 00 55 12 12 00 31 \
        14 14 00 2d 17 17 00 27 19 19 00 23 1b 1b 00 1f \
        bd bd 00 db bd bd 00 db bd bd 00 db ff ff ff 58 \
        ff 80 00 d6 ff ff 00 57 00 ff 00 56 00 00 ff 56 \
        ff 00 ff 57 ff 7d 7d 5c 7d 7d ff 5c 01 54 3c 19 \
        03 52 00 55 ff 00 ff 57 03 52 80 d5 03 52 00 55 \
        01 01 00 53 01 02 00 52 01 04 00 50 01 08 00 4c \
        01 10 00 44 00 00 00 55 09 00 00 4c 00 00 00 55 \
        00 00 00 55 07 00 00 4e 02 02 00 51 02 03 00 50 \
        00 00 00 55 00 00 00 55 00 00 00 55 00 00 00 55 \
        04 ff ff 00 13 ff 41 00 55 01 54 ff ff ff ff ff";

    fn dump() -> Vec<u8> {
        DUMP.split_whitespace().map(|h| u8::from_str_radix(h, 16).unwrap()).collect()
    }

    #[test]
    fn packet_checksum_sums_to_0x55() {
        let p = packet(CMD_READ_EEPROM, &read_eeprom_payload(0, 10));
        assert_eq!(p[..6], [0x08, 0x08, 0, 0, 0, 0x0A]);
        assert_eq!(sum(&p), 0x55);
    }

    #[test]
    fn handshake_matches_real_device() {
        // Observado no hardware: r = 10,20,30,40 → 23 46 82 aa
        assert_eq!(handshake_expected([10, 20, 30, 40]), [0x23, 0x46, 0x82, 0xAA]);
    }

    #[test]
    fn dpi_table() {
        assert_eq!(dpi_to_code(1600), 0x12);
        assert_eq!(dpi_to_code(16000), 0xBD);
        assert_eq!(code_to_dpi(0x17), 2000);
        for dpi in (DPI_MIN..=DPI_MAX).step_by(DPI_STEP as usize) {
            assert_eq!(code_to_dpi(dpi_to_code(dpi)), dpi);
        }
    }

    #[test]
    fn parse_real_dump() {
        let c = Config::from_eeprom(&dump());
        assert_eq!(c.polling_hz, 1000);
        assert_eq!(c.level_count, 5);
        assert_eq!(c.active_level, 2);
        let dpis: Vec<u32> = c.levels.iter().take(5).map(|l| l.dpi).collect();
        assert_eq!(dpis, [1600, 1800, 2000, 2200, 2400]);
        assert_eq!(c.levels[2].color, [0xFF, 0xFF, 0x00]);
        assert_eq!(c.dpi_led, LedEffect { mode: 1, speed: 3, brightness: 3 });
        assert_eq!(c.logo_led.color, [0xFF, 0x00, 0xFF]);
        assert_eq!(c.strip.mode, strip_mode::OFF);
        assert_eq!(c.strip.color, [0xFF, 0xFF, 0x00]);
        assert_eq!(c.strip.brightness, 10);
        assert_eq!(c.buttons[0], Button { kind: 1, code: 1, param: 0 });
        assert_eq!(c.buttons[9].kind, button_kind::POLLING_SWITCH);
    }

    #[test]
    fn roundtrip_is_byte_identical() {
        let img = dump();
        let mut out = img.clone();
        Config::from_eeprom(&img).apply_to(&mut out);
        assert_eq!(out, img);
        assert!(diff_runs(&img, &out).is_empty());
    }

    #[test]
    fn diff_only_touches_changed_bytes() {
        let img = dump();
        let mut c = Config::from_eeprom(&img);
        c.levels[2].color = [0x00, 0xFF, 0xFF];
        let mut out = img.clone();
        c.apply_to(&mut out);
        // Mesmo teste feito no hardware: 0x34 = 00 ff ff 57
        assert_eq!(diff_runs(&img, &out), vec![(0x34, vec![0x00, 0xFF, 0xFF])]);
        assert_eq!(&out[0x34..0x38], &[0x00, 0xFF, 0xFF, 0x57]);
    }

    #[test]
    fn strip_rainbow_wave_matches_hardware_test() {
        // Gravado no hardware e confirmado visualmente: 00 00 40 ff 0f ff 08 = onda arco-íris
        let mut img = dump();
        let mut c = Config::from_eeprom(&img);
        c.strip = StripLed { mode: strip_mode::RAINBOW_WAVE, color: [0, 0x40, 0xFF], speed: 7, brightness: 10 };
        c.apply_to(&mut img);
        assert_eq!(&img[0xA0..0xA7], &[0x00, 0x00, 0x40, 0xFF, 0x0F, 0xFF, 0x08]);
        assert_eq!(&img[0xA7..0xAB], &[0x00, 0x55, 0x01, 0x54], "bytes desconhecidos preservados");
    }

    #[test]
    fn polling_event() {
        // Observado ao apertar o botão de polling: 1000 → 125 Hz
        let e = parse_event(&[0x09, 0x0A, 0, 0, 0, 0x0A, 0x02, 0x02, 0xE8, 0, 0, 0, 0, 0, 0, 0, 0x4C]).unwrap();
        assert_eq!(e.polling_hz, Some(125));
        assert_eq!(e.dpi_level, None);
    }
}
