struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // xyz = kierunek światła kluczowego, w = rezerwa
    light: vec4<f32>,
    // xyz = prawo kamery, w = tan(fov/2)
    right: vec4<f32>,
    // xyz = góra kamery, w = aspect
    up: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var detail_tex: texture_2d_array<f32>;
@group(0) @binding(2) var detail_samp: sampler;

/// Triplanarne próbkowanie detalu — siatki nie mają UV, a szwy nie mogą
/// przecinać poszycia. `scale` to odwrotność rozmiaru kafla w metrach.
fn detail(p: vec3<f32>, n: vec3<f32>, scale: f32, layer: i32) -> vec4<f32> {
    let w = abs(n);
    let ww = w / max(w.x + w.y + w.z, 0.0001);
    let a = textureSampleLevel(detail_tex, detail_samp, p.zy * scale, layer, 0.0);
    let b = textureSampleLevel(detail_tex, detail_samp, p.xz * scale, layer, 0.0);
    let c = textureSampleLevel(detail_tex, detail_samp, p.xy * scale, layer, 0.0);
    return a * ww.x + b * ww.y + c * ww.z;
}

/// Poszycie: jedna dominująca oś zamiast mieszania trzech — płyty zostają
/// ostre, bez kamuflażu z nałożonych rzutów.
fn detail_axis(p: vec3<f32>, n: vec3<f32>, scale: f32, layer: i32) -> vec4<f32> {
    let a = abs(n);
    if (a.y >= a.x && a.y >= a.z) {
        return textureSampleLevel(detail_tex, detail_samp, p.xz * scale, layer, 0.0);
    }
    if (a.x >= a.z) {
        return textureSampleLevel(detail_tex, detail_samp, p.zy * scale, layer, 0.0);
    }
    return textureSampleLevel(detail_tex, detail_samp, p.xy * scale, layer, 0.0);
}

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    // rgb = kolor, a = materiał (1 = poszycie, 0.6 = stacja, 0 = skała)
    @location(6) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) local: vec3<f32>,
};

@vertex
fn vs(in: VsIn) -> VsOut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    let world = model * vec4<f32>(in.pos, 1.0);
    var out: VsOut;
    out.clip = cam.view_proj * world;
    out.world = world.xyz;
    out.nrm = normalize((model * vec4<f32>(in.nrm, 0.0)).xyz);
    out.color = in.color;
    out.local = in.pos;
    return out;
}

// ---------------------------------------------------------------- tło

struct BgOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_bg(@builtin(vertex_index) i: u32) -> BgOut {
    var p = vec2<f32>(-1.0, -1.0);
    if (i == 1u) { p = vec2<f32>(3.0, -1.0); }
    if (i == 2u) { p = vec2<f32>(-1.0, 3.0); }
    var out: BgOut;
    out.clip = vec4<f32>(p, 1.0, 1.0);
    out.ndc = p;
    return out;
}

fn hash11(n: f32) -> f32 {
    return fract(sin(n * 127.1 + 311.7) * 43758.5453);
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn vnoise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash31(i);
    let c100 = hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let c010 = hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let c110 = hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let c001 = hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let c101 = hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let c011 = hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let c111 = hash31(i + vec3<f32>(1.0, 1.0, 1.0));
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var q = p;
    var amp = 0.5;
    var sum = 0.0;
    for (var i = 0; i < 4; i = i + 1) {
        sum += amp * vnoise3(q);
        q = q * 2.02 + vec3<f32>(11.0, 7.0, 19.0);
        amp *= 0.5;
    }
    return sum / 0.9375;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_bg(in: BgOut) -> @location(0) vec4<f32> {
    let tan_half = cam.right.w;
    let aspect = cam.up.w;
    let fwd = normalize(cross(cam.up.xyz, cam.right.xyz));
    let dir = normalize(
        fwd
        + cam.right.xyz * in.ndc.x * tan_half * aspect
        + cam.up.xyz * in.ndc.y * tan_half
    );

    // pas Drogi Mlecznej i dwie warstwy mgławicy
    let band_axis = normalize(vec3<f32>(0.32, 0.86, -0.39));
    let bd = dot(dir, band_axis) * 2.4;
    let band = exp(-bd * bd);
    let n1 = fbm(dir * 2.3 + vec3<f32>(1.7, 0.0, 3.1));
    let n2 = fbm(dir * 5.6 - vec3<f32>(2.0, 1.0, 0.5));
    let dust = fbm(dir * 8.5 + vec3<f32>(4.0, 2.0, 6.0));

    var col = vec3<f32>(0.004, 0.006, 0.014);
    let glow = smoothstep(0.42, 0.82, n1) * (0.28 + 0.72 * band);
    col += vec3<f32>(0.035, 0.065, 0.15) * glow;
    col += vec3<f32>(0.14, 0.05, 0.17) * smoothstep(0.54, 0.92, n2) * band * 0.65;
    col += vec3<f32>(0.07, 0.11, 0.16) * band * 0.25;

    // ciemne pasma pyłu przecinające jasną część
    let lane = smoothstep(0.48, 0.78, dust) * band;
    col *= 1.0 - 0.45 * lane;

    // delikatna winieta i dithering przeciw pasmom gradientu
    let v = length(in.ndc) * 0.5;
    col *= 1.0 - 0.28 * v * v;
    col += (hash11(dot(in.clip.xy, vec2<f32>(12.9898, 78.233))) - 0.5) * 0.004;

    return vec4<f32>(col * 1.25, 1.0);
}

// ---------------------------------------------------------------- emisja

// Wiązki: sześcian rozciągnięty wzdłuż X, miękki przekrój Y/Z.
@fragment
fn fs_glow(in: VsOut) -> @location(0) vec4<f32> {
    let r = length(in.local.yz) * 2.0;
    let a = smoothstep(1.0, 0.05, r);
    return vec4<f32>(in.color.rgb, in.color.a * a);
}

// Sprite: kwadrat w XY, okrągły zanik — gwiazdy, silniki, światła.
@fragment
fn fs_sprite(in: VsOut) -> @location(0) vec4<f32> {
    let d = length(in.local.xy) * 2.0;
    let soft = pow(clamp(1.0 - d, 0.0, 1.0), 1.6);
    let core = pow(clamp(1.0 - d * 1.9, 0.0, 1.0), 3.0);
    return vec4<f32>(in.color.rgb * (0.55 + 0.9 * core), in.color.a * soft);
}

// ---------------------------------------------------------------- siatki

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.nrm);
    let key = normalize(cam.light.xyz);
    let fill_dir = normalize(vec3<f32>(-0.62, -0.18, -0.55));
    let v = normalize(cam.eye.xyz - in.world);

    let diff = max(dot(n, key), 0.0);
    let fill = max(dot(n, fill_dir), 0.0);
    let amb = mix(vec3<f32>(0.025, 0.03, 0.045), vec3<f32>(0.11, 0.13, 0.17), n.y * 0.5 + 0.5);

    let mat = in.color.a;
    let base = in.color.rgb;
    var col = vec3<f32>(0.0);

    if (mat < 0.25) {
        // --- skała: wieloskalowy szum, ciemne szczeliny, prawie bez odbicia
        let p = in.local;
        let det = detail(p, n, 0.085, 1);
        let n1 = fbm(p * 0.22);
        let n2 = fbm(p * 1.1 + vec3<f32>(7.0, 3.0, 5.0));
        let grain = vnoise3(p * 5.0);
        var rock = mix(vec3<f32>(0.11, 0.10, 0.09), vec3<f32>(0.34, 0.31, 0.27), n1);
        rock = mix(rock, vec3<f32>(0.24, 0.25, 0.27), n2 * 0.4);
        rock *= 0.72 + 0.42 * grain;
        rock *= 0.78 + 0.30 * smoothstep(0.30, 0.72, n2);
        // detal z tekstury: albedo, wysokość (AO) i szorstkość
        rock *= 0.68 + 0.50 * det.r;
        rock *= 0.70 + 0.40 * det.g;
        let cold = vec3<f32>(0.52, 0.58, 0.66);
        col = rock * (amb + vec3<f32>(1.0, 0.92, 0.80) * diff * 0.95 + cold * fill * 0.18);
        let spec = pow(max(dot(n, normalize(key + v)), 0.0), 10.0);
        col += vec3<f32>(0.5, 0.55, 0.6) * spec * 0.045 * (1.0 - det.b * 0.6);
        let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
        col += vec3<f32>(0.22, 0.30, 0.42) * rim * 0.18;
    } else {
        // --- poszycie: płyty, spoiny, zabrudzenia, mocne odbicia
        let det = detail_axis(in.local, n, 0.05, 0);
        let cell = floor(in.local * 0.16);
        let panel = hash31(cell);
        let g = abs(fract(in.local * 0.16) - vec3<f32>(0.5));
        let seam = smoothstep(0.465, 0.5, max(g.x, max(g.y, g.z)));
        let fine = hash31(floor(in.local * 1.3));
        let grime = fbm(in.local * 0.55);

        var metal = base * (0.96 + 0.08 * panel);
        metal *= 0.84 + 0.18 * grime;
        metal *= 0.95 + 0.08 * fine;
        metal *= 0.78 + 0.34 * det.r;
        metal *= 0.84 + 0.24 * det.g;
        col = metal * (amb + vec3<f32>(1.0, 0.94, 0.84) * diff * 0.92);
        col += vec3<f32>(0.22, 0.34, 0.58) * fill * 0.22;
        col *= 1.0 - 0.18 * seam;

        let spec = pow(max(dot(n, normalize(key + v)), 0.0), 72.0);
        let spec2 = pow(max(dot(n, normalize(fill_dir + v)), 0.0), 40.0);
        let sheen = 1.15 - det.b * 0.65;
        col += vec3<f32>(1.0, 0.96, 0.88) * spec * 0.55 * sheen;
        col += vec3<f32>(0.35, 0.5, 0.8) * spec2 * 0.18 * sheen;

        let rim = pow(1.0 - max(dot(n, v), 0.0), 2.6);
        col += vec3<f32>(0.35, 0.46, 0.72) * rim * 0.30;

        // --- okna stacji: pasy na pierścieniach, część zgaszona
        if (mat > 0.4 && mat < 0.8) {
            let radial = length(in.local.xz);
            let ang = atan2(in.local.z, in.local.x) / 6.28318 + 0.5;
            let strip = floor(ang * 96.0);
            let lit = step(0.58, hash11(strip + 7.0));
            if (abs(in.local.y) < 5.5 && radial > 105.0 && radial < 136.0) {
                col += vec3<f32>(0.32, 0.70, 1.0) * lit * 0.55;
            }
            if (in.local.y > 22.0 && in.local.y < 30.0 && radial > 78.0 && radial < 101.0) {
                col += vec3<f32>(1.0, 0.68, 0.36) * lit * 0.30;
            }
        }

        // kokpit / czujniki z przodu poszycia
        if (mat > 0.9 && in.local.x > 9.0 && abs(in.local.y - 1.2) < 1.6 && abs(in.local.z) < 3.2) {
            col += vec3<f32>(0.22, 0.62, 0.95) * 0.35;
        }
    }

    // mgła: dalsze obiekty bledną w kierunku tła
    let dist = length(cam.eye.xyz - in.world);
    let fog = 1.0 - exp(-dist / 5200.0);
    col = mix(col, vec3<f32>(0.006, 0.009, 0.020), fog * 0.88);

    return vec4<f32>(col, 1.0);
}
