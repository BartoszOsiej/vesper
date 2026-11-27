//! Wierzchołki i wczytywanie siatek OBJ (ten sam format, który produkuje MTS-Ships).

use glam::Vec3;
use std::collections::HashMap;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[derive(Default)]
pub struct MeshData {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Wczytuje OBJ. Normalne liczymy sami z geometrii (płasko), bo generator ich nie zapisuje.
    pub fn from_obj(text: &str) -> Result<Self, String> {
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut faces: Vec<Vec<u32>> = Vec::new();
        for (line_no, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if let Some(rest) = line.strip_prefix("v ") {
                let mut it = rest.split_whitespace();
                let x = it.next().ok_or_else(|| format!("linia {}: brak x", line_no + 1))?;
                let y = it.next().ok_or_else(|| format!("linia {}: brak y", line_no + 1))?;
                let z = it.next().ok_or_else(|| format!("linia {}: brak z", line_no + 1))?;
                positions.push([
                    x.parse::<f32>().map_err(|e| format!("linia {}: {e}", line_no + 1))?,
                    y.parse::<f32>().map_err(|e| format!("linia {}: {e}", line_no + 1))?,
                    z.parse::<f32>().map_err(|e| format!("linia {}: {e}", line_no + 1))?,
                ]);
            } else if let Some(rest) = line.strip_prefix("f ") {
                let mut idx = Vec::new();
                for tok in rest.split_whitespace() {
                    let first = tok.split('/').next().unwrap_or("");
                    let i: i64 = first
                        .parse()
                        .map_err(|e| format!("linia {}: {e}", line_no + 1))?;
                    let i = if i < 0 { positions.len() as i64 + i } else { i - 1 };
                    idx.push(i as u32);
                }
                if idx.len() >= 3 {
                    faces.push(idx);
                }
            }
        }
        if positions.is_empty() || faces.is_empty() {
            return Err("OBJ has no vertices or faces".into());
        }

        let mut out = MeshData::default();
        // ściany -> trójkąty, każdy z własną płaską normalną (hard-surface)
        let mut cache: HashMap<([i32; 12], [i32; 12]), u32> = HashMap::new();
        for face in &faces {
            let p0 = positions[face[0] as usize];
            for i in 1..face.len() - 1 {
                let p1 = positions[face[i] as usize];
                let p2 = positions[face[i + 1] as usize];
                let n = normal(p0, p1, p2);
                if len3(n) < 1e-9 {
                    continue;
                }
                let n = norm3(n);
                for p in [p0, p1, p2] {
                    let key = (quant(p), quant(n));
                    let idx = *cache.entry(key).or_insert_with(|| {
                        let i = out.verts.len() as u32;
                        out.verts.push(Vertex { pos: p, nrm: n });
                        i
                    });
                    out.indices.push(idx);
                }
            }
        }
        Ok(out)
    }

    /// Stacja: dwa pierścienie, szprychy, rdzeń, ramiona dokowania i anteny.
    /// Wszystko z klocków, ale z drobnym podziałem, żeby sylwetka nie była sześcianem.
    pub fn station(radius: f32) -> Self {
        let mut out = MeshData::default();
        let tau = std::f32::consts::TAU;

        // główny pierścień: gładki torus zamiast łańcucha klocków
        push_torus(&mut out, 0.0, radius, 8.0, 96, 10);
        // drugi, habitatowy pierścień nad głównym
        let r2 = radius * 0.76;
        push_torus(&mut out, 26.0, r2, 4.5, 72, 8);

        // drobnica na pierścieniu: kapsuły, chłodnice, zbiorniki
        for i in 0..24 {
            let a = i as f32 / 24.0 * tau + 0.05;
            let rr = radius + 2.0;
            let y = if i % 3 == 0 { 8.5 } else if i % 3 == 1 { -7.0 } else { 0.0 };
            let s = if i % 3 == 0 { 6.0 } else { 4.0 };
            push_box(&mut out, [a.cos() * rr, y, a.sin() * rr], [s, s, s], a);
        }

        // szprychy łączące rdzeń z pierścieniem
        for k in 0..8 {
            let a = k as f32 * tau / 8.0 + 0.08;
            let mid = radius * 0.58;
            push_box(
                &mut out,
                [a.cos() * mid, 3.0, a.sin() * mid],
                [radius * 0.95, 3.0, 5.0],
                a,
            );
        }

        // cztery ramiona dokowania zakończone klamrami
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4;
            push_box(
                &mut out,
                [a.cos() * radius * 0.88, 0.0, a.sin() * radius * 0.88],
                [radius * 0.55, 9.0, 18.0],
                a,
            );
            push_box(
                &mut out,
                [a.cos() * radius * 1.14, 0.0, a.sin() * radius * 1.14],
                [10.0, 24.0, 32.0],
                a,
            );
        }

        // rdzeń: stos brył o różnych przekrojach
        for (y, sx, sy, sz) in [
            (-54.0, 24.0, 26.0, 24.0),
            (-22.0, 44.0, 38.0, 44.0),
            (22.0, 34.0, 48.0, 34.0),
            (54.0, 20.0, 18.0, 20.0),
        ] {
            push_box(&mut out, [0.0, y, 0.0], [sx, sy, sz], 0.0);
        }

        // drobnica wokół rdzenia
        for k in 0..12 {
            let a = k as f32 * tau / 12.0;
            let rr = 40.0;
            push_box(
                &mut out,
                [a.cos() * rr, -10.0 + (k % 3) as f32 * 9.0, a.sin() * rr],
                [9.0, 7.0, 6.0],
                a,
            );
        }

        // anteny i maszty
        push_box(&mut out, [12.0, 88.0, 0.0], [2.0, 34.0, 2.0], 0.0);
        push_box(&mut out, [-12.0, 82.0, 0.0], [2.0, 26.0, 2.0], 0.0);
        push_box(&mut out, [0.0, 106.0, 0.0], [28.0, 2.0, 2.0], 0.0);

        // panele słoneczne — szeroka, cienka sylwetka
        push_box(&mut out, [radius * 0.98, 44.0, 0.0], [radius * 0.62, 1.5, 40.0], 0.0);
        push_box(&mut out, [-radius * 0.98, 44.0, 0.0], [radius * 0.62, 1.5, 40.0], 0.0);
        out
    }

    /// Fregata gracza: kadłub z przekrojów, skrzydła delta, gondole silników
    /// i drobnica. Model jest procedurowany, więc sylwetka jest smukła
    /// niezależnie od assetu OBJ.
    pub fn frigate() -> Self {
        let mut out = MeshData::default();
        // (x, half width, half height, y center)
        let sections: [(f32, f32, f32, f32); 7] = [
            (23.0, 0.35, 0.30, 0.0),
            (18.0, 2.0, 0.9, 0.1),
            (12.0, 3.9, 1.7, 0.2),
            (3.0, 5.0, 2.2, 0.1),
            (-7.0, 4.6, 2.0, 0.0),
            (-16.0, 3.4, 1.5, -0.1),
            (-22.0, 2.2, 1.1, -0.2),
        ];
        let prof: [(f32, f32); 8] = [
            (0.0, 1.0),
            (0.85, 0.6),
            (1.0, 0.0),
            (0.85, -0.6),
            (0.0, -1.0),
            (-0.85, -0.6),
            (-1.0, 0.0),
            (-0.85, 0.6),
        ];
        for pair in sections.windows(2) {
            let (xa, wa, ha, ya) = pair[0];
            let (xb, wb, hb, yb) = pair[1];
            for k in 0..prof.len() {
                let (pa, pb) = (prof[k], prof[(k + 1) % prof.len()]);
                let p0 = [xa, ya + ha * pa.0, wa * pa.1];
                let p1 = [xa, ya + ha * pb.0, wa * pb.1];
                let p2 = [xb, yb + hb * pb.0, wb * pb.1];
                let p3 = [xb, yb + hb * pa.0, wb * pa.1];
                push_flat_quad(&mut out, p0, p1, p2, p3);
            }
        }
        // dziób: stożek z ostatniego przekroju
        let (bx, bw, bh, by) = sections[0];
        for k in 0..prof.len() {
            let (pa, pb) = (prof[k], prof[(k + 1) % prof.len()]);
            let p0 = [bx, by + bh * pa.0, bw * pa.1];
            let p1 = [bx, by + bh * pb.0, bw * pb.1];
            let tip = [bx + 4.5, by, 0.0];
            push_flat_tri(&mut out, p0, p1, tip);
        }
        // skrzydła delta
        push_box(&mut out, [-1.0, -0.15, 7.2], [15.0, 0.5, 6.5], 0.40);
        push_box(&mut out, [-1.0, -0.15, -7.2], [15.0, 0.5, 6.5], -0.40);
        push_box(&mut out, [-8.0, -0.05, 13.0], [7.0, 0.35, 3.0], 0.62);
        push_box(&mut out, [-8.0, -0.05, -13.0], [7.0, 0.35, 3.0], -0.62);
        // statecznik pionowy
        push_box(&mut out, [-16.0, 3.0, 0.0], [7.0, 4.2, 0.45], 0.0);
        push_box(&mut out, [-21.0, 1.0, 0.0], [3.0, 2.2, 0.4], 0.0);
        // gondole silników
        for side in [-1.0f32, 1.0] {
            push_cyl_x(&mut out, -20.5, 0.0, side * 4.4, 1.35, 6.4, 14);
            push_box(&mut out, [-21.5, 0.0, side * 4.4], [1.2, 2.4, 1.2], 0.0);
            push_box(&mut out, [-19.0, 0.9, side * 4.4], [4.0, 1.6, 0.5], 0.0);
        }
        // kokpit i grzbiet
        push_box(&mut out, [13.5, 1.6, 0.0], [5.5, 1.5, 3.4], 0.0);
        push_box(&mut out, [4.0, 2.2, 0.0], [8.0, 0.8, 2.6], 0.0);
        push_box(&mut out, [-6.0, 2.1, 0.0], [5.0, 0.7, 1.8], 0.0);
        // drobnica: zbiorniki i czujniki
        for k in 0..10 {
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let x = 8.0 - k as f32 * 2.6;
            push_box(
                &mut out,
                [x, 2.25, side * (2.6 + (k % 3) as f32 * 0.5)],
                [0.9, 0.5, 0.9],
                0.0,
            );
        }
        out
    }

    /// Kostka jednostkowa — cząstki, smugowce, znaczniki.
    pub fn cube(size: f32) -> Self {
        let mut out = MeshData::default();
        push_box(&mut out, [0.0, 0.0, 0.0], [size, size, size], 0.0);
        out
    }

    /// Kamienista asteroida: dwie rundy podziału dwudziestościanu, wypukłości
    /// wieloskalowe i lekko wydłużona bryła (deterministycznie z `seed`).
    pub fn asteroid(seed: u32, radius: f32, noise: f32) -> Self {
        let (tris, base_v) = icosahedron();
        let mut faces: Vec<[Vec3; 3]> = tris
            .iter()
            .map(|t| {
                [
                    Vec3::from(base_v[t[0]]).normalize(),
                    Vec3::from(base_v[t[1]]).normalize(),
                    Vec3::from(base_v[t[2]]).normalize(),
                ]
            })
            .collect();
        for _ in 0..2 {
            faces = faces.iter().flat_map(|f| subdivide(f)).collect();
        }

        let sx = 1.0 + (hash_u32(seed) - 0.5) * 0.34;
        let sy = 0.88 + (hash_u32(seed ^ 0x51ED) - 0.5) * 0.3;
        let sz = 1.0 + (hash_u32(seed ^ 0x9E37) - 0.5) * 0.3;
        let off = Vec3::new(
            hash_u32(seed ^ 0xA1) * 60.0,
            hash_u32(seed ^ 0xB2) * 60.0,
            hash_u32(seed ^ 0xC3) * 60.0,
        );
        let mut out = MeshData::default();
        for f in faces {
            let p: Vec<[f32; 3]> = f
                .iter()
                .map(|d| {
                    let n1 = fbm3(*d * 1.35 + off) - 0.5;
                    let n2 = fbm3(*d * 4.2 + off * 1.7) - 0.5;
                    let n3 = vnoise(*d * 11.0 + off * 2.3) - 0.5;
                    let k = 1.0 + noise * (n1 * 1.15 + n2 * 0.38 + n3 * 0.10);
                    let v = *d * radius * k * Vec3::new(sx, sy, sz);
                    [v.x, v.y, v.z]
                })
                .collect();
            let nrm = norm3(normal(p[0], p[1], p[2]));
            let base = out.verts.len() as u32;
            for q in &p {
                out.verts.push(Vertex { pos: *q, nrm });
            }
            out.indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
        out
    }

    /// Kwadrat jednostkowy w XY — sprite'y gwiazd, silników i świateł.
    pub fn quad() -> Self {
        let mut out = MeshData::default();
        for (x, y) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            out.verts.push(Vertex { pos: [x, y, 0.0], nrm: [0.0, 0.0, 1.0] });
        }
        out.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        out
    }
}

/// Dokłada do siatki prostopadłościan obrócony o `yaw` wokół Y.
/// Wycinek torusa z gładkimi normalnymi — pierścień stacji i habitat.
fn push_torus(out: &mut MeshData, y: f32, major: f32, minor: f32, major_seg: usize, minor_seg: usize) {
    let tau = std::f32::consts::TAU;
    let base = out.verts.len() as u32;
    for i in 0..major_seg {
        let u = i as f32 / major_seg as f32 * tau;
        let (su, cu) = u.sin_cos();
        for j in 0..minor_seg {
            let v = j as f32 / minor_seg as f32 * tau;
            let (sv, cv) = v.sin_cos();
            let r = major + minor * cv;
            out.verts.push(Vertex {
                pos: [r * cu, y + minor * sv, r * su],
                nrm: [cv * cu, sv, cv * su],
            });
        }
    }
    for i in 0..major_seg {
        let i2 = (i + 1) % major_seg;
        for j in 0..minor_seg {
            let j2 = (j + 1) % minor_seg;
            let a = base + (i * minor_seg + j) as u32;
            let b = base + (i2 * minor_seg + j) as u32;
            let c = base + (i * minor_seg + j2) as u32;
            let d = base + (i2 * minor_seg + j2) as u32;
            out.indices.extend_from_slice(&[a, c, d, a, d, b]);
        }
    }
}

fn push_box(out: &mut MeshData, c: [f32; 3], size: [f32; 3], yaw: f32) {
    let (s, co) = yaw.sin_cos();
    let (hx, hy, hz) = (size[0] * 0.5, size[1] * 0.5, size[2] * 0.5);
    let mut p = [[0.0f32; 3]; 8];
    let mut i = 0;
    for sy in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            for sx in [-1.0f32, 1.0] {
                let x = sx * hx;
                let z = sz * hz;
                p[i] = [c[0] + x * co - z * s, c[1] + sy * hy, c[2] + x * s + z * co];
                i += 1;
            }
        }
    }
    // (0)=-x-y-z (1)=+x-y-z (2)=-x-y+z (3)=+x-y+z
    // (4)=-x+y-z (5)=+x+y-z (6)=-x+y+z (7)=+x+y+z
    let quads: [[usize; 4]; 6] = [
        [0, 2, 3, 1], // dol
        [4, 5, 7, 6], // gora
        [0, 1, 5, 4], // -z
        [2, 6, 7, 3], // +z
        [0, 4, 6, 2], // -x
        [1, 3, 7, 5], // +x
    ];
    for q in quads {
        let base = out.verts.len() as u32;
        let n = norm3(normal(p[q[0]], p[q[1]], p[q[2]]));
        for k in q {
            out.verts.push(Vertex { pos: p[k], nrm: n });
        }
        out.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn push_flat_tri(out: &mut MeshData, a: [f32; 3], b: [f32; 3], c: [f32; 3]) {
    let n = norm3(normal(a, b, c));
    let base = out.verts.len() as u32;
    for p in [a, b, c] {
        out.verts.push(Vertex { pos: p, nrm: n });
    }
    out.indices.extend_from_slice(&[base, base + 1, base + 2]);
}

fn push_flat_quad(out: &mut MeshData, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
    let n = norm3(normal(a, b, c));
    let base = out.verts.len() as u32;
    for p in [a, b, c, d] {
        out.verts.push(Vertex { pos: p, nrm: n });
    }
    out.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_cyl_x(out: &mut MeshData, x: f32, y: f32, z: f32, radius: f32, len: f32, seg: usize) {
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
        let p0 = [x - len * 0.5, y + a0.cos() * radius, z + a0.sin() * radius];
        let p1 = [x - len * 0.5, y + a1.cos() * radius, z + a1.sin() * radius];
        let p2 = [x + len * 0.5, y + a1.cos() * radius, z + a1.sin() * radius];
        let p3 = [x + len * 0.5, y + a0.cos() * radius, z + a0.sin() * radius];
        push_flat_quad(out, p0, p1, p2, p3);
        push_flat_tri(out, [x - len * 0.5, y, z], p1, p0);
        push_flat_tri(out, [x + len * 0.5, y, z], p3, p2);
    }
}

fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

fn len3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn norm3(v: [f32; 3]) -> [f32; 3] {
    let l = len3(v).max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn quant(v: [f32; 3]) -> [i32; 12] {
    [
        (v[0] * 8192.0) as i32,
        (v[1] * 8192.0) as i32,
        (v[2] * 8192.0) as i32,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ]
}

/// Dzieli trójkąt na cztery; wierzchołki normalizujemy, więc sąsiednie
/// trójkąty dzielą dokładnie te same pozycje (brak pęknięć).
fn subdivide(f: &[Vec3; 3]) -> Vec<[Vec3; 3]> {
    let (a, b, c) = (f[0], f[1], f[2]);
    let ab = (a + b).normalize();
    let bc = (b + c).normalize();
    let ca = (c + a).normalize();
    vec![
        [a, ab, ca],
        [ab, b, bc],
        [ca, bc, c],
        [ab, bc, ca],
    ]
}

fn hash_u32(mut h: u32) -> f32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Hash punktu 3D bez alokacji — podstawa szumu proceduralnego.
fn hash3(p: Vec3) -> f32 {
    let mut h = 0x9E37_79B9u32;
    for c in [p.x, p.y, p.z] {
        h ^= (c * 8192.0) as i32 as u32;
        h = h.wrapping_mul(0x85EB_CA6B);
        h ^= h >> 13;
    }
    (h & 0x00FF_FFFF) as f32 / 16_777_216.0
}

fn vnoise(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let mut acc = 0.0;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let w = Vec3::new(dx as f32, dy as f32, dz as f32);
                let h = hash3(i + w);
                let wx = if dx == 0 { 1.0 - u.x } else { u.x };
                let wy = if dy == 0 { 1.0 - u.y } else { u.y };
                let wz = if dz == 0 { 1.0 - u.z } else { u.z };
                acc += h * wx * wy * wz;
            }
        }
    }
    acc
}

fn fbm3(p: Vec3) -> f32 {
    let mut q = p;
    let mut amp = 0.5;
    let mut sum = 0.0;
    for _ in 0..3 {
        sum += amp * vnoise(q);
        q = q * 2.03 + Vec3::new(17.0, 31.0, 7.0);
        amp *= 0.5;
    }
    sum / 0.875
}

fn icosahedron() -> (Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let v = vec![
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    let f = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    (f, v)
}
