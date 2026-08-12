#version 330

in vec2 point;
in mat2 transform;
out vec3 tint;

void main() {
    vec2 transformed = transform * point;
    tint = vec3(0.12 + 0.2 * transformed.x, 0.72, 0.95);
    gl_Position = vec4(transformed, 0.0, 1.0);
}
