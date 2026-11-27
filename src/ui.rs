//! UI 2D: prostokąty + mikro-font 3x5 rysowany jako piksele. Bez tekstur i atlasów.

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiRect {
    /// x, y, szerokość, wysokość w pikselach (0,0 = lewy górny róg)
    pub rect: [f32; 4],
    pub color: [f32; 4],
}

impl UiRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) -> Self {
        Self { rect: [x, y, w, h], color: c }
    }
}

/// 3x5 pikseli na znak. Każdy wiersz to 3 bity (od lewej).
const FONT: &[(char, [u8; 5])] = &[
    (' ', [0, 0, 0, 0, 0]),
    ('A', [0b010, 0b101, 0b111, 0b101, 0b101]),
    ('B', [0b110, 0b101, 0b110, 0b101, 0b110]),
    ('C', [0b011, 0b100, 0b100, 0b100, 0b011]),
    ('D', [0b110, 0b101, 0b101, 0b101, 0b110]),
    ('E', [0b111, 0b100, 0b110, 0b100, 0b111]),
    ('F', [0b111, 0b100, 0b110, 0b100, 0b100]),
    ('G', [0b011, 0b100, 0b101, 0b101, 0b011]),
    ('H', [0b101, 0b101, 0b111, 0b101, 0b101]),
    ('I', [0b111, 0b010, 0b010, 0b010, 0b111]),
    ('J', [0b001, 0b001, 0b001, 0b101, 0b010]),
    ('K', [0b101, 0b101, 0b110, 0b101, 0b101]),
    ('L', [0b100, 0b100, 0b100, 0b100, 0b111]),
    ('M', [0b101, 0b111, 0b111, 0b101, 0b101]),
    ('N', [0b101, 0b111, 0b111, 0b111, 0b101]),
    ('O', [0b010, 0b101, 0b101, 0b101, 0b010]),
    ('P', [0b110, 0b101, 0b110, 0b100, 0b100]),
    ('Q', [0b010, 0b101, 0b101, 0b111, 0b011]),
    ('R', [0b110, 0b101, 0b110, 0b101, 0b101]),
    ('S', [0b011, 0b100, 0b010, 0b001, 0b110]),
    ('T', [0b111, 0b010, 0b010, 0b010, 0b010]),
    ('U', [0b101, 0b101, 0b101, 0b101, 0b011]),
    ('V', [0b101, 0b101, 0b101, 0b101, 0b010]),
    ('W', [0b101, 0b101, 0b111, 0b111, 0b101]),
    ('X', [0b101, 0b101, 0b010, 0b101, 0b101]),
    ('Y', [0b101, 0b101, 0b010, 0b010, 0b010]),
    ('Z', [0b111, 0b001, 0b010, 0b100, 0b111]),
    ('0', [0b111, 0b101, 0b101, 0b101, 0b111]),
    ('1', [0b010, 0b110, 0b010, 0b010, 0b111]),
    ('2', [0b111, 0b001, 0b111, 0b100, 0b111]),
    ('3', [0b111, 0b001, 0b011, 0b001, 0b111]),
    ('4', [0b101, 0b101, 0b111, 0b001, 0b001]),
    ('5', [0b111, 0b100, 0b111, 0b001, 0b111]),
    ('6', [0b111, 0b100, 0b111, 0b101, 0b111]),
    ('7', [0b111, 0b001, 0b010, 0b010, 0b010]),
    ('8', [0b111, 0b101, 0b111, 0b101, 0b111]),
    ('9', [0b111, 0b101, 0b111, 0b001, 0b111]),
    ('.', [0b000, 0b000, 0b000, 0b000, 0b010]),
    (':', [0b000, 0b010, 0b000, 0b010, 0b000]),
    ('-', [0b000, 0b000, 0b111, 0b000, 0b000]),
    ('/', [0b001, 0b001, 0b010, 0b100, 0b100]),
    ('%', [0b101, 0b001, 0b010, 0b100, 0b101]),
    ('+', [0b000, 0b010, 0b111, 0b010, 0b000]),
    ('>', [0b100, 0b010, 0b001, 0b010, 0b100]),
    ('<', [0b001, 0b010, 0b100, 0b010, 0b001]),
    ('!', [0b010, 0b010, 0b010, 0b000, 0b010]),
];

fn glyph(c: char) -> [u8; 5] {
    let up = c.to_ascii_uppercase();
    FONT.iter()
        .find(|(k, _)| *k == up)
        .map(|(_, v)| *v)
        .unwrap_or([0b111, 0b101, 0b101, 0b101, 0b111])
}

/// Tekst jako piksele. `px` = bok jednego piksela fontu.
pub fn text(out: &mut Vec<UiRect>, x: f32, y: f32, s: &str, px: f32, c: [f32; 4]) {
    let mut cx = x;
    for ch in s.chars() {
        if ch == '\n' {
            cx = x;
            continue;
        }
        let rows = glyph(ch);
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..3 {
                if bits & (0b100 >> rx) != 0 {
                    out.push(UiRect::new(cx + rx as f32 * px, y + ry as f32 * px, px, px, c));
                }
            }
        }
        cx += 4.0 * px;
    }
}

#[allow(dead_code)]
pub fn text_width(s: &str, px: f32) -> f32 {
    s.chars().count() as f32 * 4.0 * px
}
