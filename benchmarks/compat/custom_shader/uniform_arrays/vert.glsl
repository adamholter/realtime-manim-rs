#version 330

in vec2 point;
uniform float weights[2];
out float shade;

void main() {
    shade = weights[0] * 0.35 + weights[1] * 0.65;
    gl_Position = vec4(point, 0.0, 1.0);
}
