struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) gradient_position: vec2<f32>,
    @location(3) gradient_meta: vec4<f32>,
    @location(4) mask_meta: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) gradient_position: vec2<f32>,
    @location(2) @interpolate(flat) gradient_meta: vec4<f32>,
    @location(3) @interpolate(flat) mask_meta: vec2<u32>,
}

struct GradientStop {
    color: vec4<f32>,
    data: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> gradient_stops: array<GradientStop>;
@group(1) @binding(0) var svg_masks: texture_2d_array<f32>;
@group(1) @binding(1) var<storage, read> svg_mask_indices: array<u32>;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 1.0);
    output.color = input.color;
    output.gradient_position = input.gradient_position;
    output.gradient_meta = input.gradient_meta;
    output.mask_meta = vec2<u32>(input.mask_meta);
    return output;
}

fn shade(input: VertexOutput) -> vec4<f32> {
    let count = u32(input.gradient_meta.y);
    if count == 0u {
        return input.color;
    }
    let start = u32(input.gradient_meta.x);
    var amount = input.gradient_position.x;
    let kind = u32(input.gradient_meta.w);
    if kind == 1u {
        let geometry = gradient_stops[start - 1u];
        let focal = geometry.color.xy;
        let center = geometry.color.zw;
        let focal_radius = geometry.data.x;
        let radius = geometry.data.y;
        let center_delta = center - focal;
        let radius_delta = radius - focal_radius;
        let point_delta = input.gradient_position - focal;
        let a = dot(center_delta, center_delta) - radius_delta * radius_delta;
        let b = -2.0 * (dot(point_delta, center_delta) + focal_radius * radius_delta);
        let c = dot(point_delta, point_delta) - focal_radius * focal_radius;
        if abs(a) <= 0.000001 {
            amount = select(0.0, -c / b, abs(b) > 0.000001);
        } else {
            let discriminant = max(b * b - 4.0 * a * c, 0.0);
            amount = (-b - sqrt(discriminant)) / (2.0 * a);
        }
    }
    let spread = u32(input.gradient_meta.z);
    if spread == 0u {
        amount = clamp(amount, 0.0, 1.0);
    } else if spread == 1u {
        amount = amount - floor(amount);
    } else {
        let reflected = amount - floor(amount / 2.0) * 2.0;
        amount = select(2.0 - reflected, reflected, reflected <= 1.0);
    }
    let first = gradient_stops[start];
    if count == 1u || amount <= first.data.x {
        return first.color;
    }
    for (var index: u32 = 0u; index + 1u < count; index = index + 1u) {
        let left = gradient_stops[start + index];
        let right = gradient_stops[start + index + 1u];
        if amount <= right.data.x {
            let span = max(right.data.x - left.data.x, 0.000001);
            return mix(left.color, right.color, clamp((amount - left.data.x) / span, 0.0, 1.0));
        }
    }
    return gradient_stops[start + count - 1u].color;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return shade(input);
}

@fragment
fn fs_masked(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = shade(input);
    var coverage = 1.0;
    let pixel = vec2<i32>(floor(input.position.xy));
    for (var index = 0u; index < input.mask_meta.y; index = index + 1u) {
        let descriptor = svg_mask_indices[input.mask_meta.x + index];
        let layer = i32(descriptor & 0x7fffffffu);
        let sample = textureLoad(svg_masks, pixel, layer, 0);
        if (descriptor & 0x80000000u) == 0u {
            coverage *= sample.a;
        } else {
            coverage *= dot(sample.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        }
    }
    color.a *= coverage;
    return color;
}

@fragment
fn fs_clip() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

struct ImageVertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) opacity: f32,
    @location(3) mask_meta: vec2<f32>,
}

struct ImageVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
    @location(2) @interpolate(flat) mask_meta: vec2<u32>,
}

@group(0) @binding(0) var image_texture: texture_2d<f32>;
@group(0) @binding(1) var dark_image_texture: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;

@vertex
fn vs_image(input: ImageVertexInput) -> ImageVertexOutput {
    var output: ImageVertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.opacity = input.opacity;
    output.mask_meta = vec2<u32>(input.mask_meta);
    return output;
}

@fragment
fn fs_image_sample(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image_texture, image_sampler, input.uv);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

fn apply_image_mask(input: ImageVertexOutput, source: vec4<f32>) -> vec4<f32> {
    var color = source;
    var coverage = 1.0;
    let pixel = vec2<i32>(floor(input.position.xy));
    for (var index = 0u; index < input.mask_meta.y; index = index + 1u) {
        let descriptor = svg_mask_indices[input.mask_meta.x + index];
        let layer = i32(descriptor & 0x7fffffffu);
        let sample = textureLoad(svg_masks, pixel, layer, 0);
        if (descriptor & 0x80000000u) == 0u {
            coverage *= sample.a;
        } else {
            coverage *= dot(sample.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        }
    }
    color.a *= coverage;
    return color;
}

@fragment
fn fs_image_sample_masked(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image_texture, image_sampler, input.uv);
    return apply_image_mask(input, vec4<f32>(color.rgb, color.a * input.opacity));
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

fn filtered_light(uv: vec2<f32>, kind: u32) -> vec4<f32> {
    let dimensions_u = textureDimensions(image_texture);
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
            color = color + textureLoad(image_texture, sample_position, 0) * weight;
            total_weight = total_weight + weight;
        }
    }
    return clamp(
        color / max(abs(total_weight), 0.00001),
        vec4<f32>(0.0),
        vec4<f32>(1.0),
    );
}

fn filtered_dark(uv: vec2<f32>, kind: u32) -> vec4<f32> {
    let dimensions_u = textureDimensions(dark_image_texture);
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
            color = color + textureLoad(dark_image_texture, sample_position, 0) * weight;
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
    let color = filtered_light(input.uv, 0u);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

@fragment
fn fs_image_hamming(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = filtered_light(input.uv, 1u);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

@fragment
fn fs_image_bicubic(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = filtered_light(input.uv, 2u);
    return vec4<f32>(color.rgb, color.a * input.opacity);
}

@fragment
fn fs_image_lanczos(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    let color = filtered_light(input.uv, 3u);
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
    return vec4<f32>(lit.rgb, input.opacity);
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
    return shade_mesh_texture(input, filtered_light(input.uv, 0u), filtered_dark(input.uv, 0u));
}

@fragment
fn fs_mesh_texture_hamming(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(input, filtered_light(input.uv, 1u), filtered_dark(input.uv, 1u));
}

@fragment
fn fs_mesh_texture_bicubic(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(input, filtered_light(input.uv, 2u), filtered_dark(input.uv, 2u));
}

@fragment
fn fs_mesh_texture_lanczos(input: MeshTextureVertexOutput) -> @location(0) vec4<f32> {
    return shade_mesh_texture(input, filtered_light(input.uv, 3u), filtered_dark(input.uv, 3u));
}
