#version 330

in vec3 xyz_coords;
in vec3 v_normal;
in vec4 v_color;

uniform int bands;
uniform bool invert;

out vec4 frag_color;

void main() {
    float wave = 0.5 + 0.5 * sin(float(bands) * xyz_coords.x + 4.0 * xyz_coords.y);
    vec3 cyan = vec3(0.05, 0.82, 1.0);
    vec3 magenta = vec3(1.0, 0.08, 0.62);
    vec3 tint = mix(cyan, magenta, smoothstep(0.2, 0.8, wave));
    if (invert) {
        tint = vec3(1.0) - tint;
    }
    frag_color = vec4(tint, v_color.a);
}
