// Общие данные кадра.
struct Camera {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
    fog: vec4<f32>, // rgb — цвет неба/тумана, a — дальность тумана
};
@group(0) @binding(0) var<uniform> cam: Camera;

// ---------- Блоки ----------
@group(1) @binding(0) var<uniform> model: mat4x4<f32>; // поза сетки (мир или корабль)
@group(2) @binding(0) var tex: texture_2d_array<f32>;
@group(2) @binding(1) var samp: sampler;

struct BlockIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: u32,
    @location(3) light: f32,
};
struct BlockOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) light: f32,
    @location(3) world: vec3<f32>,
};

@vertex
fn vs_block(v: BlockIn) -> BlockOut {
    let world = model * vec4<f32>(v.pos, 1.0);
    var o: BlockOut;
    o.clip = cam.view_proj * world;
    o.uv = v.uv;
    o.layer = v.layer;
    o.light = v.light;
    o.world = world.xyz;
    return o;
}

@fragment
fn fs_block(i: BlockOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    if c.a < 0.5 { discard; } // стекло, листва — альфа-тест
    let d = distance(i.world, cam.position.xyz);
    let fog = smoothstep(cam.fog.a * 0.6, cam.fog.a, d);
    return vec4<f32>(mix(c.rgb * i.light, cam.fog.rgb, fog), 1.0);
}

// ---------- Линии (рамка выделения, прицел) ----------
struct LineIn {
    @location(0) pos: vec4<f32>, // w = 1: точка мира, w = 0: экранные координаты (-1..1)
    @location(1) color: vec4<f32>,
};
struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(v: LineIn) -> LineOut {
    var o: LineOut;
    if v.pos.w > 0.5 {
        o.clip = cam.view_proj * vec4<f32>(v.pos.xyz, 1.0);
    } else {
        o.clip = vec4<f32>(v.pos.xy, 0.0, 1.0);
    }
    o.color = v.color;
    return o;
}

@fragment
fn fs_line(i: LineOut) -> @location(0) vec4<f32> {
    return i.color;
}
