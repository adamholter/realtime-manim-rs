#version 330

layout(points) in;
layout(triangle_strip, max_vertices = 4) out;

in vec4 v_color[];
flat out vec4 g_color;
flat out int g_kind;

void main() {
    vec4 center = gl_in[0].gl_Position;
    vec2 offsets[4] = vec2[4](
        vec2(0.0, 0.15),
        vec2(-0.11, 0.0),
        vec2(0.11, 0.0),
        vec2(0.0, -0.15)
    );
    for (int index = 0; index < 4; index++) {
        gl_Position = center + vec4(offsets[index], 0.0, 0.0);
        g_color = v_color[0];
        g_kind = gl_PrimitiveIDIn % 2;
        EmitVertex();
    }
    EndPrimitive();
}
