//! Wierzchołki i wczytywanie siatek OBJ (ten sam format, który produkuje MTS-Ships).

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
            return Err("OBJ bez wierzchołków lub ścian".into());
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

    /// Stacja: pierścień + kolumna + cztery ramiona dokowania, z jednego klocka-bryły.
    pub fn station(radius: f32) -> Self {
        let mut out = MeshData::default();
        let seg = 16;
        for i in 0..seg {
            let a = i as f32 / seg as f32 * std::f32::consts::TAU;
            let cx = a.cos() * radius;
            let cz = a.sin() * radius;
            // segmenty muszą na siebie zachodzić, inaczej pierścień wygląda jak odłamki
            let step = std::f32::consts::TAU * radius / seg as f32;
            let s = step * 1.35;
            push_box(&mut out, [cx, 0.0, cz], [s, s * 0.55, s], a);
        }
        // kolumna i piętra
        push_box(&mut out, [0.0, 0.0, 0.0], [radius * 0.30, radius * 0.55, radius * 0.30], 0.0);
        push_box(&mut out, [0.0, radius * 0.36, 0.0], [radius * 0.62, radius * 0.10, radius * 0.62], 0.0);
        push_box(&mut out, [0.0, -radius * 0.30, 0.0], [radius * 0.46, radius * 0.12, radius * 0.46], 0.0);
        // ramiona dokowania w czterech kierunkach
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2;
            let d = radius * 0.55;
            push_box(
                &mut out,
                [a.cos() * d, 0.0, a.sin() * d],
                [radius * 0.42, 6.0, 9.0],
                a,
            );
        }
        out
    }

    /// Asteroida: dwudziestościan rozbity na sferę z deterministycznymi wypukłościami.
    pub fn asteroid(seed: u32, radius: f32, noise: f32) -> Self {
        let (tris, base_v) = icosahedron();
        let mut out = MeshData::default();
        for tri in tris {
            let p: Vec<[f32; 3]> = tri
                .iter()
                .map(|&i| {
                    let n = norm3(base_v[i]);
                    let k = 1.0 + noise * (hash01(seed, n) - 0.5);
                    [n[0] * radius * k, n[1] * radius * k, n[2] * radius * k]
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
}

/// Dokłada do siatki prostopadłościan obrócony o `yaw` wokół Y.
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

/// Tani hash kierunku - deterministyczne wypukłości.
fn hash01(seed: u32, n: [f32; 3]) -> f32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9);
    for c in n {
        h ^= (c * 10_000.0) as i32 as u32;
        h = h.wrapping_mul(0x85EB_CA6B);
        h ^= h >> 13;
    }
    (h % 10_000) as f32 / 10_000.0
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
