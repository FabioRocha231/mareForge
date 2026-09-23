// Mar e terra do mareForge (MF-058). Um único quad cobre o mundo; a terra
// vem das mesmas `LandMass` que o servidor usa para colisão, então o que o
// jogador vê é exatamente onde o casco encalha. Pixelado em grade de 1 m
// para casar com os sprites pixel art.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

struct SeaParams {
    land: array<vec4<f32>, 48>,
    safe: array<vec4<f32>, 4>,
    // x: nº de discos de terra, y: nº de círculos protegidos, z: perigo 0..1
    info: vec4<f32>,
};

@group(2) @binding(0) var<uniform> sea: SeaParams;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 4; i++) {
        v += a * noise(q);
        q = q * 2.03 + vec2<f32>(17.0, 9.0);
        a *= 0.5;
    }
    return v;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

// x: distância com sinal até a terra (negativa dentro), y: 1 se rochedo.
fn land_field(p: vec2<f32>) -> vec2<f32> {
    var d = 1e5;
    var nearest = 1e5;
    var rocky = 0.0;
    let count = i32(sea.info.x);
    for (var i = 0; i < count; i++) {
        let disc = sea.land[i];
        let di = length(p - disc.xy) - disc.z;
        d = smin(d, di, 24.0);
        if (di < nearest) {
            nearest = di;
            rocky = select(0.0, 1.0, disc.z < 40.0);
        }
    }
    // Costa irregular: ruído grande na terra firme, fino nos rochedos.
    let warp = select(16.0, 5.0, rocky > 0.5);
    d += (fbm(p * 0.018) - 0.5) * warp * 2.0;
    return vec2<f32>(d, rocky);
}

fn quantize(v: f32, steps: f32) -> f32 {
    return floor(v * steps) / steps;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = floor(mesh.world_position.xy) + 0.5;
    let t = globals.time;
    let field = land_field(p);
    let d = field.x;
    let rocky = field.y;

    var col: vec3<f32>;
    if (d > 0.0) {
        // Água: raso turquesa perto da costa, azul profundo no alto-mar.
        let depth = clamp(d / 170.0, 0.0, 1.0);
        let swell = fbm(p * 0.012 + vec2<f32>(t * 0.02, t * 0.015));
        let level = quantize(clamp(depth + (swell - 0.5) * 0.25, 0.0, 1.0), 5.0);
        let shallow = vec3<f32>(0.36, 0.74, 0.86);
        let mid = vec3<f32>(0.16, 0.50, 0.76);
        let deep = vec3<f32>(0.08, 0.30, 0.56);
        col = mix(shallow, mid, smoothstep(0.0, 0.45, level));
        col = mix(col, deep, smoothstep(0.45, 1.0, level));

        // Ondinhas: traços claros curtos que derivam com o vento.
        let ripple = fbm(vec2<f32>(p.x * 0.045 + t * 0.35, p.y * 0.11 - t * 0.12));
        if (abs(ripple - 0.52) < 0.016) {
            col = mix(col, vec3<f32>(0.80, 0.93, 1.0), 0.55);
        }

        // Espuma que respira na linha d'água.
        let foam_w = 2.5 + 2.0 * sin(t * 1.3 + fbm(p * 0.05) * 9.0);
        if (d < foam_w) {
            col = vec3<f32>(0.93, 0.97, 1.0);
        } else if (d < foam_w + 5.0 && hash(floor(p * 0.5) + floor(t * 2.0)) > 0.8) {
            col = mix(col, vec3<f32>(0.93, 0.97, 1.0), 0.6);
        }

        // Alto-mar sem lei: água mais escura e fria.
        col = mix(col, col * vec3<f32>(0.78, 0.74, 0.86), sea.info.z * smoothstep(40.0, 160.0, d));

        // Águas protegidas: anel tracejado discreto marcando o limite.
        let safe_count = i32(sea.info.y);
        for (var i = 0; i < safe_count; i++) {
            let c = sea.safe[i];
            let r = length(p - c.xy);
            let a = atan2(p.y - c.y, p.x - c.x);
            let dash = fract(a * c.z / 18.0) < 0.5;
            if (abs(r - c.z) < 1.2 && dash) {
                col = mix(col, vec3<f32>(0.78, 1.0, 0.84), 0.45);
            }
        }
    } else if (rocky > 0.5) {
        // Rochedo: pedra com musgo.
        let n = fbm(p * 0.25);
        col = mix(vec3<f32>(0.40, 0.42, 0.46), vec3<f32>(0.60, 0.62, 0.64), quantize(n, 3.0));
        if (d < -4.0 && fbm(p * 0.12 + 5.0) > 0.6) {
            col = vec3<f32>(0.33, 0.50, 0.30);
        }
        if (d > -1.5) {
            col = vec3<f32>(0.30, 0.32, 0.36);
        }
    } else if (d > -10.0) {
        // Praia: areia molhada na linha d'água, seca atrás.
        let n = hash(floor(p * 0.5));
        col = select(vec3<f32>(0.93, 0.84, 0.64), vec3<f32>(0.86, 0.75, 0.55), n > 0.85);
        if (d > -2.5) {
            col = vec3<f32>(0.80, 0.70, 0.52);
        }
    } else {
        // Terra firme: grama com mata densa no interior.
        let n = fbm(p * 0.06);
        col = mix(vec3<f32>(0.27, 0.66, 0.26), vec3<f32>(0.34, 0.74, 0.30), quantize(n, 3.0));
        let forest = fbm(p * 0.035 + 11.0) + clamp(-d / 220.0, 0.0, 0.35);
        if (forest > 0.62) {
            col = vec3<f32>(0.16, 0.45, 0.19);
            if (hash(floor(p * 0.34)) > 0.9) {
                col = vec3<f32>(0.11, 0.33, 0.15);
            }
        }
        // Borda da grama sobre a areia.
        if (d > -12.0) {
            col = vec3<f32>(0.22, 0.56, 0.22);
        }
    }
    return vec4<f32>(col, 1.0);
}
