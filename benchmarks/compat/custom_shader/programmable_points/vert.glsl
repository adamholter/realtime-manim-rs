#version 330

in vec2 point;
uniform float point_size;
out vec3 tint;

void main() {
    tint = vec3(1.0, 0.3 + 0.15 * point.x, 0.12);
    gl_Position = vec4(point, 0.0, 1.0);
    gl_PointSize = point_size;
}
