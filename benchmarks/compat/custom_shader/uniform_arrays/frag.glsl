#version 330

in float shade;
out vec4 frag_color;

void main() {
    frag_color = vec4(0.08 + shade * 0.2, 0.32, 1.0 - shade * 0.3, 1.0);
}
