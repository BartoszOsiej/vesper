struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cam: Camera;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) color: vec4<f32>,
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
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.nrm);
    let l = normalize(cam.light.xyz);
    let v = normalize(cam.eye.xyz - in.world);

    // twarde, "metalowe" światło: mocny kierunek + ciemne wypełnienie
    let diff = max(dot(n, l), 0.0);
    let rim = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    let spec = pow(max(dot(n, normalize(l + v)), 0.0), 24.0);

    var base = in.color.rgb;
    var col = base * (0.16 + 0.84 * diff);
    col += vec3<f32>(0.35, 0.45, 0.70) * rim * 0.35;
    col += vec3<f32>(0.85, 0.92, 1.0) * spec * 0.25;

    // lekka mgła głębi: dalsze obiekty bledną do tła
    let dist = length(cam.eye.xyz - in.world);
    let fog = clamp(dist / 9000.0, 0.0, 1.0);
    let bg = vec3<f32>(0.008, 0.011, 0.024);
    col = mix(col, bg, fog * 0.85);

    return vec4<f32>(col, 1.0);
}
