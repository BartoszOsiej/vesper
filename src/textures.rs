//! Proceduralne tekstury materiałów (2 warstwy, bez plików zewnętrznych).
//!
//! Kanały: R = albedo/detal, G = wysokość (AO i szczeliny), B = szorstkość,
//! A = rezerwa. Tekstury są bezszwowe, bo szum próbkuje okresowo.

pub const SIZE: usize = 512;
pub const LAYERS: usize = 2;

/// Warstwa 0: poszycie okrętu/stacji. Warstwa 1: skała.
pub fn build() -> Vec<u8> {
    let mut out = vec![0u8; SIZE * SIZE * LAYERS * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let u = x as f32 / SIZE as f32;
            let v = y as f32 / SIZE as f32;
            for layer in 0..LAYERS {
                let px = if layer == 0 { metal(u, v) } else { rock(u, v) };
                let i = ((layer * SIZE * SIZE) + y * SIZE + x) * 4;
                out[i] = to_u8(px[0]);
                out[i + 1] = to_u8(px[1]);
                out[i + 2] = to_u8(px[2]);
                out[i + 3] = 255;
            }
        }
    }
    out
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Płyty poszycia: spoiny, nity, smugi i zabrudzenia.
fn metal(u: f32, v: f32) -> [f32; 3] {
    let cells = 6.0;
    let gx = u * cells;
    let gy = v * cells;
    let cx = gx.floor();
    let cy = gy.floor();
    let fx = gx - cx;
    let fy = gy - cy;

    // odległość od krawędzi płyty (0 = spoina, 0.5 = środek)
    let ex = fx.min(1.0 - fx);
    let ey = fy.min(1.0 - fy);
    let edge = ex.min(ey);
    let seam = 1.0 - smoothstep(0.012, 0.045, edge);

    let panel = hash(cx as i32, cy as i32, 11);
    let grime = fbm(u, v, 4, 4, 3);
    let streak = fbm(u * 0.4, v * 4.0, 8, 2, 8);
    let speck = hash((gx * 24.0) as i32, (gy * 24.0) as i32, 21);

    // nity w narożnikach płyt
    let rv = 0.028;
    let corner = (ex * ex + ey * ey).sqrt().min(
        ((1.0 - ex) * (1.0 - ex) + ey * ey).sqrt(),
    );
    let rivet = 1.0 - smoothstep(rv * 0.45, rv, corner.min((ex * ex + (1.0 - ey) * (1.0 - ey)).sqrt()));

    let mut albedo = 0.66 + 0.24 * panel;
    albedo *= 0.82 + 0.26 * grime;
    albedo *= 1.0 - 0.20 * (streak - 0.5);
    albedo *= 1.0 - 0.55 * seam;
    albedo += rivet * 0.16;
    albedo += (speck - 0.5) * 0.05;

    let height = (1.0 - seam * 0.85) * (0.55 + 0.35 * grime) + rivet * 0.35;
    let rough = (0.38 + 0.34 * grime + seam * 0.18 - rivet * 0.12).clamp(0.0, 1.0);
    [albedo, height, rough]
}

/// Skała: wieloskalowy szum, szczeliny i kratery.
fn rock(u: f32, v: f32) -> [f32; 3] {
    let base = fbm(u, v, 3, 5, 31);
    let mid = fbm(u, v, 11, 3, 77);
    let fine = fbm(u, v, 41, 2, 13);

    // szczeliny: ostre doliny w szumie
    let ridge = 1.0 - (mid * 2.0 - 1.0).abs();
    let crack = smoothstep(0.82, 0.99, ridge);
    let crevice = smoothstep(0.72, 0.95, mid);

    let mut height = 0.35 + 0.45 * base + 0.18 * mid + 0.07 * fine;
    height -= crack * 0.35;
    height -= crevice * 0.08;

    // kratery (deterministyczne, okresowe)
    for k in 0..18u32 {
        let cx = hash(k as i32, 4, 5);
        let cy = hash(k as i32, 9, 6);
        let cr = 0.035 + hash(k as i32, 2, 7) * 0.09;
        let mut dx = (u - cx).abs();
        dx = dx.min(1.0 - dx);
        let mut dy = (v - cy).abs();
        dy = dy.min(1.0 - dy);
        let d = (dx * dx + dy * dy).sqrt();
        if d < cr {
            let t = d / cr;
            height -= 0.30 * (1.0 - t) * (1.0 - t);
            if t > 0.78 {
                height += 0.12 * (1.0 - (t - 0.78) / 0.22);
            }
        }
    }

    let mut albedo = 0.52 + 0.42 * base;
    albedo *= 0.90 + 0.18 * mid;
    albedo *= 1.0 - 0.30 * crack;
    albedo *= 0.92 + 0.14 * fine;
    albedo += (hash((u * 512.0) as i32, (v * 512.0) as i32, 3) - 0.5) * 0.06;

    let rough = 0.80 + 0.16 * fine - 0.10 * crack;
    [albedo, height.clamp(0.0, 1.0), rough.clamp(0.0, 1.0)]
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x27D4_EB2D)
        ^ (y as u32).wrapping_mul(0x1656_67B1)
        ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Bezszwowy szum wartości: siatka zawijana modulo `period`.
fn vnoise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let x = u * period as f32;
    let y = v * period as f32;
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);

    let x0 = xi.rem_euclid(period);
    let y0 = yi.rem_euclid(period);
    let x1 = (xi + 1).rem_euclid(period);
    let y1 = (yi + 1).rem_euclid(period);

    let a = hash(x0, y0, seed);
    let b = hash(x1, y0, seed);
    let c = hash(x0, y1, seed);
    let d = hash(x1, y1, seed);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

fn fbm(u: f32, v: f32, period: i32, octaves: u32, seed: u32) -> f32 {
    let mut amp = 0.5;
    let mut sum = 0.0;
    let mut p = period;
    for i in 0..octaves {
        sum += amp * vnoise(u, v, p, seed ^ (i.wrapping_mul(0x9E37)));
        p *= 2;
        amp *= 0.5;
    }
    sum / (1.0 - 0.5f32.powi(octaves as i32))
}
