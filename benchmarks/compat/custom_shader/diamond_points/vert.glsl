#version 330

uniform vec2 frame_shape;

in vec3 point;
in vec4 color;

out vec4 v_color;

void main() {
    gl_Position = vec4(
        point.x * 2.0 / frame_shape.x,
        point.y * 2.0 / frame_shape.y,
        -point.z * 0.01,
        1.0
    );
    v_color = color;
}
