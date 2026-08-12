#version 330

flat in vec3 g_tint;
out vec4 frag_color;

void main() {
    vec2 centered = gl_PointCoord * 2.0 - 1.0;
    float radius = length(centered);
    if (radius > 1.0) {
        discard;
    }
    float alpha = 1.0 - smoothstep(0.76, 1.0, radius);
    frag_color = vec4(g_tint, alpha);
}
