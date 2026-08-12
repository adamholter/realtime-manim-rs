#version 330

in vec2 point;
in float size;
in vec3 tint;

out float v_size;
out vec3 v_tint;

void main() {
    gl_Position = vec4(point, 0.0, 1.0);
    v_size = size;
    v_tint = tint;
}
