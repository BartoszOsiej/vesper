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

// Mapa cieni: macierz światła i bufor głębi z passu cieni.
@group(1) @binding(0) var<uniform> light_vp: mat4x4<f32>;
@group(1) @binding(1) var shadow_map: texture_depth_2d;
@group(1) @binding(2) var shadow_samp: sampler_comparison;

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

/// Bump z wysokości tekstury: gradient wysokości w przestrzeni ekranu
/// obraca normalną, więc spoiny płyt i kratery mają realny relief.
fn perturb_normal(n: vec3<f32>, p: vec3<f32>, h: f32, strength: f32) -> vec3<f32> {
    let dx = dpdx(p);
    let dy = dpdy(p);
    let dhx = dpdx(h);
    let dhy = dpdy(h);
    let r1 = cross(dy, n);
    let r2 = cross(n, dx);
    let det = dot(dx, r1);
    let grad = sign(det) * (dhx * r1 + dhy * r2);
    return normalize(abs(det) * n - strength * grad);
}

/// Tanie środowisko: pas Drogi Mlecznej odbity w metalu.
fn env_light(refl: vec3<f32>) -> vec3<f32> {
    let axis = normalize(vec3<f32>(0.32, 0.86, -0.39));
    let bd = dot(refl, axis) * 2.4;
    let band = exp(-bd * bd);
    return vec3<f32>(0.012, 0.018, 0.032) + vec3<f32>(0.10, 0.15, 0.26) * band;
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
fn vs_shadow(in: VsIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    return light_vp * model * vec4<f32>(in.pos, 1.0);
}

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
    let n1 = fbm(dir * 3.4 + vec3<f32>(1.7, 0.0, 3.1));
    let n2 = fbm(dir * 8.8 - vec3<f32>(2.0, 1.0, 0.5));
    let fine = fbm(dir * 26.0 + vec3<f32>(9.0, 4.0, 1.0));
    let dust = fbm(dir * 15.0 + vec3<f32>(4.0, 2.0, 6.0));

    var col = vec3<f32>(0.002, 0.003, 0.008);
    let glow = smoothstep(0.46, 0.80, n1) * (0.20 + 0.80 * band);
    col += vec3<f32>(0.045, 0.075, 0.16) * glow * (0.65 + 0.70 * fine);
    col += vec3<f32>(0.16, 0.05, 0.23) * smoothstep(0.52, 0.86, n2) * band * 0.85;
    col += vec3<f32>(0.05, 0.09, 0.14) * band * 0.22;

    // jasny rdzeń Drogi Mlecznej + ciemne pasma pyłu
    col += vec3<f32>(0.11, 0.13, 0.19) * pow(band, 3.0) * smoothstep(0.38, 0.80, n1) * 0.8;
    let lane = smoothstep(0.44, 0.74, dust) * band;
    col *= 1.0 - 0.62 * lane;

    // Planeta: ray-sphere w tle, ląd + chmury + atmosfera, ostra linia terminatora.
    let pc = normalize(vec3<f32>(-0.50, 0.28, -0.82)) * 60000.0;
    let prad = 13000.0;
    let oc = -pc;
    let bq = dot(oc, dir);
    let cq = dot(oc, oc) - prad * prad;
    let hq = bq * bq - cq;
    if (hq > 0.0) {
        let t = -bq - sqrt(hq);
        let pn = normalize(dir * t - pc);
        let sun = normalize(vec3<f32>(0.35, 0.42, 0.55));
        let lam = max(dot(pn, sun), 0.0);
        let cn = fbm(pn * 2.1 + vec3<f32>(3.0, 1.0, 2.0));
        let cl = fbm(pn * 6.5 - vec3<f32>(1.0, 2.0, 2.0));
        let ice = smoothstep(0.62, 0.88, abs(pn.y));
        var land = mix(vec3<f32>(0.015, 0.045, 0.09), vec3<f32>(0.11, 0.13, 0.10), smoothstep(0.47, 0.60, cn));
        land = mix(land, vec3<f32>(0.55, 0.60, 0.66), ice * 0.85);
        land = mix(land, vec3<f32>(0.50, 0.54, 0.60), smoothstep(0.50, 0.70, cl) * 0.45);
        var pcol = land * lam * 1.25;
        let prim = pow(1.0 - max(dot(pn, -dir), 0.0), 3.0);
        pcol += vec3<f32>(0.10, 0.26, 0.55) * prim * (0.20 + 0.80 * lam);
        col = mix(col, pcol, 0.99);
    }

    // delikatna winieta i dithering przeciw pasmom gradientu
    let v = length(in.ndc) * 0.5;
    col *= 1.0 - 0.20 * v * v;
    col += (hash11(dot(in.clip.xy, vec2<f32>(12.9898, 78.233))) - 0.5) * 0.003;

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

/// PCF 3x3 po mapie cieni; poza mapą i na krawędziach brak cienia.
fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let lp = light_vp * vec4<f32>(world + n * 1.2, 1.0);
    let ndc = lp.xyz / lp.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (uv.x < 0.001 || uv.x > 0.999 || uv.y < 0.001 || uv.y > 0.999) {
        return 1.0;
    }
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_map));
    var sum = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let o = vec2<f32>(f32(x), f32(y)) * texel * 1.1;
            sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + o, ndc.z - 0.0005);
        }
    }
    return sum / 9.0;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.nrm);
    let key = normalize(cam.light.xyz);
    let fill_dir = normalize(vec3<f32>(-0.62, -0.18, -0.55));
    let v = normalize(cam.eye.xyz - in.world);
    let amb = mix(vec3<f32>(0.012, 0.016, 0.026), vec3<f32>(0.050, 0.064, 0.094), n.y * 0.5 + 0.5);

    let mat = in.color.a;
    let base = in.color.rgb;
    var col = vec3<f32>(0.0);

    if (mat < 0.25) {
        // --- skała: relief z bump mapy, ostre szczeliny, śladowe odbicia
        let p = in.local;
        let det = detail(p, n, 0.10, 1);
        let n1 = fbm(p * 0.22);
        let n2 = fbm(p * 1.1 + vec3<f32>(7.0, 3.0, 5.0));
        let grain = vnoise3(p * 5.0);
        let sn = perturb_normal(n, in.world, det.g * 0.9 + n2 * 0.25, 0.45);

        var rock = mix(vec3<f32>(0.045, 0.042, 0.038), vec3<f32>(0.28, 0.25, 0.21), n1);
        rock = mix(rock, vec3<f32>(0.18, 0.19, 0.21), n2 * 0.4);
        rock *= 0.66 + 0.50 * grain;
        rock *= 0.70 + 0.42 * smoothstep(0.30, 0.72, n2);
        rock *= 0.60 + 0.55 * det.r;
        rock *= 0.60 + 0.55 * det.g;

        let ndl = max(dot(sn, key), 0.0);
        let fill = max(dot(sn, fill_dir), 0.0);
        col = rock * (amb * (0.5 + 0.6 * det.g)
            + vec3<f32>(1.0, 0.95, 0.88) * ndl * 1.05
            + vec3<f32>(0.35, 0.45, 0.60) * fill * 0.22);

        let refl = reflect(-v, sn);
        let fr = 0.02 + 0.6 * pow(1.0 - max(dot(sn, v), 0.0), 5.0);
        col += env_light(refl) * fr * (0.35 + 0.4 * (1.0 - det.b));
        let spec = pow(max(dot(sn, normalize(key + v)), 0.0), 18.0);
        col += vec3<f32>(1.0, 0.94, 0.85) * spec * 0.10 * (1.0 - det.b);
        let rim = pow(1.0 - max(dot(sn, v), 0.0), 3.0);
        col += vec3<f32>(0.22, 0.30, 0.42) * rim * 0.12;
    } else {
        // --- poszycie: płyty, spoiny, relief, lakier i odbicie mgławicy
        let det = detail_axis(in.local, n, 0.05, 0);
        let cell = floor(in.local * 0.16);
        let panel = hash31(cell);
        let g = abs(fract(in.local * 0.16) - vec3<f32>(0.5));
        let seam = smoothstep(0.455, 0.5, max(g.x, max(g.y, g.z)));
        let fine = hash31(floor(in.local * 1.3));
        let grime = fbm(in.local * 0.55);

        let sn = perturb_normal(n, in.world, det.g * 1.8, 0.55);
        let ndl = max(dot(sn, key), 0.0);
        let fill = max(dot(sn, fill_dir), 0.0);
        let rough = clamp(det.b * 1.15, 0.06, 1.0);

        var metal = base * (0.62 + 0.38 * panel);
        metal *= 0.70 + 0.40 * det.r;
        metal *= 0.86 + 0.20 * grime;
        metal *= 0.95 + 0.10 * fine;
        metal *= 1.0 - 0.22 * seam;

        col = metal * (amb * (0.45 + 0.8 * det.g) + vec3<f32>(1.0, 0.97, 0.92) * ndl * 1.0);
        col += metal * vec3<f32>(0.20, 0.30, 0.50) * fill * 0.30;

        let refl = reflect(-v, sn);
        let fr = 0.04 + 0.96 * pow(1.0 - max(dot(sn, v), 0.0), 5.0);
        col += env_light(refl) * fr * (1.7 - 1.1 * rough);

        let spec_pow = mix(220.0, 12.0, rough);
        let spec = pow(max(dot(sn, normalize(key + v)), 0.0), spec_pow);
        let spec2 = pow(max(dot(sn, normalize(fill_dir + v)), 0.0), spec_pow * 0.6);
        col += vec3<f32>(1.0, 0.97, 0.90) * spec * (1.5 * (1.0 - rough) + 0.18) * ndl;
        col += vec3<f32>(0.40, 0.58, 0.95) * spec2 * 0.55 * (1.0 - rough);

        let rim = pow(1.0 - max(dot(sn, v), 0.0), 2.2);
        col += vec3<f32>(0.30, 0.44, 0.72) * rim * 0.22;

        // --- okna stacji: pasy na pierścieniach, część zgaszona
        if (mat > 0.4 && mat < 0.8) {
            let radial = length(in.local.xz);
            let ang = atan2(in.local.z, in.local.x) / 6.28318 + 0.5;
            let strip = floor(ang * 96.0);
            let lit = step(0.72, hash11(strip + 7.0));
            if (abs(in.local.y) < 5.5 && radial > 105.0 && radial < 136.0) {
                col += vec3<f32>(0.30, 0.66, 1.0) * lit * 0.55;
            }
            if (in.local.y > 22.0 && in.local.y < 30.0 && radial > 78.0 && radial < 101.0) {
                col += vec3<f32>(1.0, 0.68, 0.36) * lit * 0.35;
            }
        }

        // kokpit / czujniki z przodu poszycia
        if (mat > 0.9 && in.local.x > 9.0 && abs(in.local.y - 1.2) < 1.6 && abs(in.local.z) < 3.2) {
            col += vec3<f32>(0.22, 0.62, 0.95) * 0.85;
        }
    }

    // cień od słońca — tylko na bezpośrednim świetle, ambient zostaje
    let sh = 1.0;
    col *= mix(0.24, 1.0, sh);

    // mgła: dalsze obiekty bledną w kierunku tła
    let dist = length(cam.eye.xyz - in.world);
    let fog = 1.0 - exp(-dist / 7000.0);
    col = mix(col, vec3<f32>(0.004, 0.006, 0.014), fog * 0.9);

    return vec4<f32>(col, 1.0);
}
