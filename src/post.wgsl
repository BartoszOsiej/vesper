//! Kompozytor klatki: scena HDR -> bloom -> ACES -> grading -> ekran.

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var p = vec2<f32>(-1.0, -1.0);
    if (i == 1u) { p = vec2<f32>(3.0, -1.0); }
    if (i == 2u) { p = vec2<f32>(-1.0, 3.0); }
    var out: VsOut;
    out.clip = vec4<f32>(p, 1.0, 1.0);
    out.ndc = p;
    return out;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn hash12(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q += dot(q, q.yx + 33.33);
    return fract((q.x + q.y) * q.x);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(scene_tex));
    let uv = in.clip.xy / dims;
    let base = textureSample(scene_tex, scene_samp, uv).rgb;

    // --- bloom: dwa pierścienie próbek, tylko jasne piksele ---
    let px = 1.0 / dims;
    var bloom = vec3<f32>(0.0);
    for (var i = 0; i < 8; i = i + 1) {
        let a = f32(i) * 0.785398;
        let d = vec2<f32>(cos(a), sin(a));
        let near = textureSample(scene_tex, scene_samp, uv + d * 5.0 * px).rgb;
        let far = textureSample(scene_tex, scene_samp, uv + d * 18.0 * px).rgb;
        bloom += max(near - vec3<f32>(0.62), vec3<f32>(0.0));
        bloom += max(far - vec3<f32>(0.58), vec3<f32>(0.0)) * 0.45;
    }
    bloom /= 11.6;

    var col = base + bloom * 1.15;

    // ekspozycja i filmowa krzywa
    col *= 1.35;
    col = aces(col);

    // grading: zimne cienie, ciepłe światła, lekkie podbicie kontrastu
    let lum = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    col = mix(col, col * vec3<f32>(0.86, 0.95, 1.12), (1.0 - lum) * 0.42);
    col = mix(col, col * vec3<f32>(1.08, 1.02, 0.92), smoothstep(0.55, 1.0, lum) * 0.30);
    col = mix(vec3<f32>(lum), col, 1.06);

    // subtelny chroma przy krawędziach kadru
    let edge = clamp(length(in.ndc) * 0.55, 0.0, 0.35);
    let dir = normalize(in.ndc + vec2<f32>(0.0001));
    let shifted = textureSample(scene_tex, scene_samp, uv + dir * edge * 0.0022).rgb;
    col = mix(col, shifted * 0.55 + col * 0.45, edge);

    // winieta i ziarno
    let c = in.ndc;
    col *= 1.0 - dot(c, c) * 0.10;
    let grain = hash12(in.clip.xy) - 0.5;
    col += grain * 0.014;

    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
