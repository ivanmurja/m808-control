//! Ícone da bandeja desenhado na hora: valor de DPI na cor do nível, ou a bateria.
//!
//! Tudo em RGBA 32×32 com uma fonte de dígitos 5×7 embutida — sem dependências de fonte.

use serde::{Deserialize, Serialize};

pub const SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrayIconMode {
    /// Ícone fixo do app.
    #[default]
    Logo,
    /// Valor do DPI ativo sobre a cor do nível.
    Dpi,
    /// Pilha com o nível da bateria.
    Battery,
}

/// Glifos 5×7: cada linha é um byte com os 5 bits menos significativos (bit 4 = coluna 0).
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        _ => return None,
    })
}

/// Largura em colunas de um glifo (o ponto é estreito).
fn glyph_width(c: char) -> u32 {
    if c == '.' { 1 } else { 5 }
}

struct Canvas {
    px: Vec<u8>,
}

type Rgba = [u8; 4];

impl Canvas {
    fn new() -> Canvas {
        Canvas { px: vec![0; (SIZE * SIZE * 4) as usize] }
    }

    fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
            let i = ((y as u32 * SIZE + x as u32) * 4) as usize;
            self.px[i..i + 4].copy_from_slice(&c);
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: Rgba) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.set(x, y, c);
            }
        }
    }

    /// Retângulo com cantos arredondados (raio pequeno, recortando os cantos).
    fn rounded(&mut self, x0: i32, y0: i32, w: i32, h: i32, r: i32, c: Rgba) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let dx = (x0 + r - x).max(x - (x0 + w - 1 - r)).max(0);
                let dy = (y0 + r - y).max(y - (y0 + h - 1 - r)).max(0);
                if dx * dx + dy * dy <= r * r {
                    self.set(x, y, c);
                }
            }
        }
    }

    /// Escreve `text` centralizado horizontalmente na linha `y`, com pixels de `scale`.
    fn text(&mut self, text: &str, y: i32, scale: i32, c: Rgba) {
        let chars: Vec<char> = text.chars().filter(|c| glyph(*c).is_some()).collect();
        let glyphs: i32 = chars.iter().map(|&ch| glyph_width(ch) as i32 * scale).sum();
        let gaps = (chars.len() as i32 - 1).max(0);
        // espaço entre letras de 1 "pixel" ampliado; se não couber, 1 pixel real
        let gap = if glyphs + gaps * scale <= SIZE as i32 { scale } else { 1 };
        let mut x = (SIZE as i32 - (glyphs + gaps * gap)) / 2;
        for ch in chars {
            let g = glyph(ch).unwrap();
            let gw = glyph_width(ch) as i32;
            for (row, bits) in g.iter().enumerate() {
                for col in 0..gw {
                    // o ponto usa a coluna central do glifo
                    let bit = if ch == '.' { 2 } else { 4 - col };
                    if bits >> bit & 1 == 1 {
                        self.rect(x + col * scale, y + row as i32 * scale, scale, scale, c);
                    }
                }
            }
            x += gw * scale + gap;
        }
    }
}

/// Texto curto para o DPI: milhares redondos com K ("2K", "16K"), senão uma casa ("1.6").
pub fn dpi_label(dpi: u32) -> String {
    let tenths = (dpi + 50) / 100;
    if tenths.is_multiple_of(10) || dpi >= 10_000 {
        format!("{}K", (dpi + 500) / 1000)
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    }
}

/// Branco ou quase-preto, o que tiver mais contraste com o fundo.
fn contrast_on(bg: [u8; 3]) -> Rgba {
    let luma = 0.299 * bg[0] as f32 + 0.587 * bg[1] as f32 + 0.114 * bg[2] as f32;
    if luma > 150.0 { [16, 17, 19, 255] } else { [255, 255, 255, 255] }
}

/// Ícone com o valor de DPI sobre a cor do nível ativo.
pub fn dpi_icon(dpi: u32, color: [u8; 3]) -> Vec<u8> {
    let mut c = Canvas::new();
    let bg = [color[0], color[1], color[2], 255];
    c.rounded(0, 0, SIZE as i32, SIZE as i32, 6, bg);
    let label = dpi_label(dpi);
    c.text(&label, 9, 2, contrast_on(color));
    c.px
}

/// Ícone de pilha preenchida conforme `percent`, com o número embaixo.
pub fn battery_icon(percent: u8) -> Vec<u8> {
    let mut c = Canvas::new();
    let p = percent.min(100) as i32;
    let fill: Rgba = match p {
        0..=20 => [235, 64, 52, 255],
        21..=40 => [242, 177, 52, 255],
        _ => [62, 207, 142, 255],
    };
    let white = [235, 236, 238, 255];
    // contorno da pilha (28×14) e o polo
    c.rounded(0, 1, 28, 14, 3, white);
    c.rounded(2, 3, 24, 10, 1, [0, 0, 0, 0]);
    c.rect(28, 5, 3, 6, white);
    let w = (24 * p + 99) / 100;
    c.rect(2, 3, w, 10, fill);
    let digits = if p >= 100 { "100".to_string() } else { p.to_string() };
    c.text(&digits, 18, 2, white);
    c.px
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(px: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn labels() {
        assert_eq!(dpi_label(1600), "1.6");
        assert_eq!(dpi_label(800), "0.8");
        assert_eq!(dpi_label(2450), "2.5");
        assert_eq!(dpi_label(2000), "2K");
        assert_eq!(dpi_label(12400), "12K");
        assert_eq!(dpi_label(16000), "16K");
    }

    #[test]
    fn dpi_icon_uses_level_color_and_contrasting_text() {
        let px = dpi_icon(2000, [255, 255, 0]);
        assert_eq!(px.len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(pixel(&px, 16, 2), [255, 255, 0, 255], "fundo na cor do nível");
        assert_eq!(pixel(&px, 0, 0)[3], 0, "canto arredondado transparente");
        // fundo claro → texto escuro em algum lugar da faixa do texto
        let dark = (9..23).any(|y| (0..SIZE).any(|x| pixel(&px, x, y) == [16, 17, 19, 255]));
        assert!(dark);
    }

    #[test]
    fn battery_fill_grows_with_level() {
        let count = |px: &[u8], color: [u8; 4]| (0..SIZE).filter(|&x| pixel(px, x, 8) == color).count();
        let green = [62, 207, 142, 255];
        assert!(count(&battery_icon(90), green) > count(&battery_icon(50), green));
        assert_eq!(count(&battery_icon(10), green), 0);
        assert!(count(&battery_icon(10), [235, 64, 52, 255]) > 0, "bateria fraca em vermelho");
    }
}

#[cfg(test)]
mod dump {
    /// Gera amostras em RGBA cru para inspeção visual: `cargo test --lib dump_samples -- --ignored`.
    #[test]
    #[ignore]
    fn dump_samples() {
        let dir = std::env::var("ICON_DUMP_DIR").expect("ICON_DUMP_DIR");
        let samples: Vec<(&str, Vec<u8>)> = vec![
            ("dpi_1600_white", super::dpi_icon(1600, [255, 255, 255])),
            ("dpi_2000_yellow", super::dpi_icon(2000, [255, 255, 0])),
            ("dpi_12400_green", super::dpi_icon(12400, [0, 200, 80])),
            ("dpi_800_blue", super::dpi_icon(800, [0, 0, 255])),
            ("dpi_16000_red", super::dpi_icon(16000, [229, 56, 59])),
            ("bat_90", super::battery_icon(90)),
            ("bat_35", super::battery_icon(35)),
            ("bat_10", super::battery_icon(10)),
            ("bat_100", super::battery_icon(100)),
        ];
        for (name, px) in samples {
            std::fs::write(format!("{dir}/{name}.rgba"), px).unwrap();
        }
    }
}
