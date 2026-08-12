#version 330

in vec3 tint;
out vec4 frag_color;

void main() {
    frag_color = vec4(tint, 1.0);
}
