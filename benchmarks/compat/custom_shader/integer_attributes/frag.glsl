#version 330

flat in int v_band;
out vec4 frag_color;

void main() {
    if (v_band == 1) {
        frag_color = vec4(0.1, 0.8, 1.0, 1.0);
    } else {
        frag_color = vec4(1.0, 0.12, 0.6, 1.0);
    }
}
