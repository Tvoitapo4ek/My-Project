// Интерфейс: цветные текстурированные треугольники в пикселях экрана.
struct Screen {
    size: vec2<f32>,
    srgb_target: f32, // 1 — цель sRGB, цвета вершин надо перевести в линейные
    _pad: f32,
};
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(0) var blocks: texture_2d_array<f32>;
@group(2) @binding(1) var block_sampler: sampler;

struct VIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) layer: i32,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) layer: i32,
};

@vertex
fn vs_ui(v: VIn) -> VOut {
    var o: VOut;
    o.clip = vec4<f32>(v.pos.x / screen.size.x * 2.0 - 1.0, 1.0 - v.pos.y / screen.size.y * 2.0, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.layer = v.layer;
    return o;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_ui(i: VOut) -> @location(0) vec4<f32> {
    var t: vec4<f32>;
    if i.layer < 0 {
        t = textureSampleLevel(atlas, atlas_sampler, i.uv, 0.0);
    } else {
        t = textureSampleLevel(blocks, block_sampler, i.uv, i.layer, 0.0);
    }
    var rgb = i.color.rgb;
    if screen.srgb_target > 0.5 {
        rgb = to_linear(rgb);
    }
    return vec4<f32>(rgb * t.rgb, i.color.a * t.a);
}
