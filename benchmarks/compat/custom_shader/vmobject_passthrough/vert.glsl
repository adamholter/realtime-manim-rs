#version 330

in vec3 point;
in vec3 unit_normal;
in vec4 color;
in float vert_index;

out vec4 v_color;

void main() {
    gl_Position = vec4(point.xy / 4.0, 0.0, 1.0);
    v_color = vec4(0.15 + 0.85 * color.r, 0.2, 1.0 - 0.7 * color.r, color.a);
}
