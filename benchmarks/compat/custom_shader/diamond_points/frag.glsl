#version 330

flat in vec4 g_color;
flat in int g_kind;
out vec4 frag_color;

void main() {
    vec3 tint = g_kind == 0
        ? vec3(0.1, 0.82, 1.0)
        : vec3(1.0, 0.12, 0.62);
    frag_color = vec4(tint, g_color.a);
}
