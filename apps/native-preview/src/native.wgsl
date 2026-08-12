struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}

struct ImageVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) opacity: f32,
}

struct ImageVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
}

@group(0) @binding(0) var image_texture: texture_2d<f32>;
@group(0) @binding(1) var dark_image_texture: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;

@vertex
fn vs_image(input: ImageVertexInput) -> ImageVertexOutput {
    var output: ImageVertexOutput;
    output.position = vec4<f32>(input.position, 1.0);
    output.uv = input.uv;
    output.opacity = input.opacity;
    return output;
}

@fragment
fn fs_image_sample(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image_texture, image_sampler, input.uv);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

fn cubic_weight(value: f32) -> f32 {
    let distance = abs(value);
    if distance <= 1.0 {
        return 1.5 * distance * distance * distance
            - 2.5 * distance * distance
            + 1.0;
    }
    if distance < 2.0 {
        return -0.5 * distance * distance * distance
            + 2.5 * distance * distance
            - 4.0 * distance
            + 2.0;
    }
    return 0.0;
}

fn sinc(value: f32) -> f32 {
    let scaled = 3.141592653589793 * value;
    if abs(scaled) < 0.00001 {
        return 1.0;
    }
    return sin(scaled) / scaled;
}

fn reconstruction_weight(kind: u32, value: f32) -> f32 {
    let distance = abs(value);
    if kind == 0u {
        return select(0.0, 1.0, distance <= 0.5);
    }
    if kind == 1u {
        if distance >= 1.0 {
            return 0.0;
        }
        return sinc(value) * (0.54 + 0.46 * cos(3.141592653589793 * value));
    }
    if kind == 2u {
        return cubic_weight(value);
    }
    if distance >= 3.0 {
        return 0.0;
    }
    return sinc(value) * sinc(value / 3.0);
}

fn filtered_texture(source: texture_2d<f32>, uv: vec2<f32>, kind: u32) -> vec4<f32> {
    let dimensions_u = textureDimensions(source);
    let dimensions = vec2<i32>(dimensions_u);
    let coordinate = uv * vec2<f32>(dimensions_u) - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coordinate));
    let fraction = fract(coordinate);
    var color = vec4<f32>(0.0);
    var total_weight = 0.0;
    for (var y: i32 = -3; y <= 3; y = y + 1) {
        for (var x: i32 = -3; x <= 3; x = x + 1) {
            let weight = reconstruction_weight(kind, f32(x) - fraction.x)
                * reconstruction_weight(kind, f32(y) - fraction.y);
            let sample_position = clamp(
                base + vec2<i32>(x, y),
                vec2<i32>(0),
                dimensions - vec2<i32>(1),
            );
            color = color + textureLoad(source, sample_position, 0) * weight;
            total_weight = total_weight + weight;
        }
    }
    return clamp(
        color / max(abs(total_weight), 0.00001),
        vec4<f32>(0.0),
        vec4<f32>(1.0),
    );
}

@fragment
fn fs_image_box(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = filtered_texture(image_texture, input.uv, 0u);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

@fragment
fn fs_image_hamming(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = filtered_texture(image_texture, input.uv, 1u);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

@fragment
fn fs_image_bicubic(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = filtered_texture(image_texture, input.uv, 2u);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

@fragment
fn fs_image_lanczos(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = filtered_texture(image_texture, input.uv, 3u);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

struct MeshTextureVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) opacity: f32,
    @location(3) point: vec3<f32>,
    @location(4) normal: vec3<f32>,
    @location(5) light_position: vec3<f32>,
    @location(6) gloss: f32,
    @location(7) shadow: f32,
    @location(8) has_dark_texture: f32,
    @location(9) unlit: f32,
}

struct MeshTextureVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
    @location(2) point: vec3<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) light_position: vec3<f32>,
    @location(5) gloss: f32,
    @location(6) shadow: f32,
    @location(7) has_dark_texture: f32,
    @location(8) unlit: f32,
}

@vertex
fn vs_mesh_texture(input: MeshTextureVertexInput) -> MeshTextureVertexOutput {
    var output: MeshTextureVertexOutput;
    output.position = vec4<f32>(input.position, 1.0);
    output.uv = input.uv;
    output.opacity = input.opacity;
    output.point = input.point;
    output.normal = input.normal;
    output.light_position = input.light_position;
    output.gloss = input.gloss;
    output.shadow = input.shadow;
    output.has_dark_texture = input.has_dark_texture;
    output.unlit = input.unlit;
    return output;
}

fn add_manim_light(
    source_color: vec4<f32>,
    point: vec3<f32>,
    source_normal: vec3<f32>,
    light_position: vec3<f32>,
    gloss: f32,
    shadow: f32,
) -> vec4<f32> {
    if gloss == 0.0 && shadow == 0.0 {
        return source_color;
    }
    var normal = normalize(source_normal);
    if normal.z < 0.0 {
        normal = -normal;
    }
    let to_camera = vec3<f32>(0.0, 0.0, 6.0) - point;
    let to_light = light_position - point;
    let reflection = -to_light + 2.0 * normal * dot(to_light, normal);
    let reflection_dot = dot(normalize(reflection), normalize(to_camera));
    let shine = gloss * exp(-3.0 * pow(1.0 - reflection_dot, 2.0));
    let light_dot = dot(normalize(to_light), normal);
    let darkening = mix(1.0, max(light_dot, 0.0), shadow);
    return vec4<f32>(
        darkening * mix(source_color.rgb, vec3<f32>(1.0), shine),
        source_color.a,
    );
}

fn shade_mesh_texture(
    input: MeshTextureVertexOutput,
    light_color: vec4<f32>,
    dark_color: vec4<f32>,
) -> vec4<f32> {
    if input.unlit >= 0.5 {
        return vec4<f32>(light_color.rgb, light_color.a * input.opacity);
    }
    let light_dot = dot(
        normalize(input.light_position - input.point),
        normalize(input.normal),
    );
    let dual_texture_color = mix(
        dark_color,
        light_color,
        smoothstep(-0.2, 0.2, light_dot),
    );
    let color = mix(light_color, dual_texture_color, step(0.5, input.has_dark_texture));
    let lit = add_manim_light(
        color,
        input.point,
        input.normal,
        input.light_position,
        input.gloss,
        input.shadow,
    );
    return vec4<f32>(lit.rgb, lit.a * input.opacity);
}

@fragment
fn fs_mesh_texture_sample(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(
        input,
        textureSample(image_texture, image_sampler, input.uv),
        textureSample(dark_image_texture, image_sampler, input.uv),
    );
}

@fragment
fn fs_mesh_texture_box(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(
        input,
        filtered_texture(image_texture, input.uv, 0u),
        filtered_texture(dark_image_texture, input.uv, 0u),
    );
}

@fragment
fn fs_mesh_texture_hamming(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(
        input,
        filtered_texture(image_texture, input.uv, 1u),
        filtered_texture(dark_image_texture, input.uv, 1u),
    );
}

@fragment
fn fs_mesh_texture_bicubic(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(
        input,
        filtered_texture(image_texture, input.uv, 2u),
        filtered_texture(dark_image_texture, input.uv, 2u),
    );
}

@fragment
fn fs_mesh_texture_lanczos(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(
        input,
        filtered_texture(image_texture, input.uv, 3u),
        filtered_texture(dark_image_texture, input.uv, 3u),
    );
}
