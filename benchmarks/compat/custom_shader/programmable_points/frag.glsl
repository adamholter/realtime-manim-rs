#version 330

in vec3 tint;
out vec4 frag_color;

void main() {
    vec2 centered = gl_PointCoord * 2.0 - 1.0;
    float distance_from_center = length(centered);
    if (distance_from_center > 1.0) {
        discard;
    }
    frag_color = vec4(tint, 1.0 - smoothstep(0.82, 1.0, distance_from_center));
}
