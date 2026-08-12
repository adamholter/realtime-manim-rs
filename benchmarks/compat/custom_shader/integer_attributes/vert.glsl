#version 330

in vec2 point;
in int band;

flat out int v_band;

void main() {
    gl_Position = vec4(point, 0.0, 1.0);
    v_band = band;
}
