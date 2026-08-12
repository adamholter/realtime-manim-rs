#version 330

layout(triangles) in;
layout(triangle_strip, max_vertices = 3) out;

in vec4 v_color[];
out vec4 g_color;

void main() {
    for (int index = 0; index < 3; index++) {
        gl_Position = gl_in[index].gl_Position;
        g_color = v_color[index];
        EmitVertex();
    }
    EndPrimitive();
}
