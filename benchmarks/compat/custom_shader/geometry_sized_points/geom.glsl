#version 330

layout(points) in;
layout(points, max_vertices = 1) out;

uniform float pulse;

in float v_size[];
in vec3 v_tint[];
flat out vec3 g_tint;

void main() {
    gl_Position = gl_in[0].gl_Position;
    gl_PointSize = max(v_size[0] * pulse, 1.0);
    g_tint = v_tint[0];
    EmitVertex();
    EndPrimitive();
}
