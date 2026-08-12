#!/usr/bin/env python3
"""Translate a Manim OpenGL vertex/fragment program into WebGPU WGSL.

Manim shaders are written for desktop GLSL 3.30 and leave locations/bindings
implicit. WebGPU requires all of them to be explicit, so this module first
normalizes the declarations to Vulkan GLSL 4.50 and then invokes the small
Naga-based translator built from tools/glsl-to-wgsl.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
TRANSLATOR = ROOT / "target" / "release" / "realtime-manim-glsl-to-wgsl"

DECLARATION = re.compile(
    r"^(?P<indent>\s*)"
    r"(?:(?P<layout>layout\s*\([^)]*\))\s+)?"
    r"(?P<modifiers>(?:(?:flat|smooth|noperspective|centroid|sample|"
    r"invariant)\s+)*)"
    r"(?P<qualifier>uniform|in|out)\s+"
    r"(?P<type>[A-Za-z_]\w*)\s+"
    r"(?P<name>[A-Za-z_]\w*)"
    r"(?P<array>\s*\[[^\]]+\])?\s*;\s*(?://.*)?$"
)
COMMA_DECLARATION = re.compile(
    r"^(?P<indent>\s*)"
    r"(?:(?P<layout>layout\s*\([^)]*\))\s+)?"
    r"(?P<modifiers>(?:(?:flat|smooth|noperspective|centroid|sample|"
    r"invariant)\s+)*)"
    r"(?P<qualifier>uniform|in|out)\s+"
    r"(?P<type>[A-Za-z_]\w*)\s+"
    r"(?P<declarators>[A-Za-z_]\w*(?:\s*\[[^\]]+\])?"
    r"(?:\s*,\s*[A-Za-z_]\w*(?:\s*\[[^\]]+\])?)+)"
    r"\s*;\s*(?P<comment>//.*)?$"
)


def _split_global_statements(source: str) -> str:
    """Put top-level declarations on lines without touching function bodies."""

    output: list[str] = []
    brace_depth = 0
    index = 0
    line_comment = False
    block_comment = False
    while index < len(source):
        character = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if line_comment:
            output.append(character)
            if character == "\n":
                line_comment = False
            index += 1
            continue
        if block_comment:
            output.append(character)
            if character == "*" and following == "/":
                output.append(following)
                block_comment = False
                index += 2
            else:
                index += 1
            continue
        if character == "/" and following == "/":
            output.extend((character, following))
            line_comment = True
            index += 2
            continue
        if character == "/" and following == "*":
            output.extend((character, following))
            block_comment = True
            index += 2
            continue
        if character == "{":
            brace_depth += 1
        elif character == "}":
            brace_depth = max(0, brace_depth - 1)
        output.append(character)
        if (
            (character == ";" and brace_depth == 0)
            or (character == "}" and brace_depth == 0)
        ):
            output.append("\n")
        index += 1
    return "".join(output)


def _expand_comma_declarations(source: str) -> str:
    output: list[str] = []
    for line in source.splitlines():
        match = COMMA_DECLARATION.match(line)
        if match is None:
            output.append(line)
            continue
        prefix = (
            match.group("indent")
            + (
                f"{match.group('layout')} "
                if match.group("layout")
                else ""
            )
            + match.group("modifiers")
            + match.group("qualifier")
            + " "
            + match.group("type")
            + " "
        )
        declarators = [
            declarator.strip()
            for declarator in match.group("declarators").split(",")
        ]
        for index, declarator in enumerate(declarators):
            comment = (
                f" {match.group('comment')}"
                if index == len(declarators) - 1
                and match.group("comment")
                else ""
            )
            output.append(f"{prefix}{declarator};{comment}")
    return "\n".join(output)


def _array_length(match: re.Match[str]) -> int:
    suffix = match.group("array")
    if suffix is None:
        return 1
    length_match = re.fullmatch(r"\s*\[\s*(\d+)\s*\]", suffix)
    if length_match is None:
        raise ValueError(
            f"Shader array {match.group('name')!r} needs a fixed integer length."
        )
    length = int(length_match.group(1))
    if length < 1 or length > 256:
        raise ValueError(
            f"Shader array {match.group('name')!r} length must be within 1–256."
        )
    return length


def _collect(source: str, qualifier: str) -> list[tuple[str, str, int]]:
    declarations: list[tuple[str, str, int]] = []
    for line in source.splitlines():
        match = DECLARATION.match(line)
        if match and match.group("qualifier") == qualifier:
            declarations.append(
                (
                    match.group("name"),
                    match.group("type"),
                    _array_length(match),
                )
            )
    return declarations


def _annotate(
    source: str,
    stage: str,
    attribute_locations: dict[str, int],
    varying_locations: dict[str, int],
    uniform_bindings: dict[str, int],
    sampler_bindings: dict[str, int],
) -> str:
    if stage == "vertex":
        source, replacements = re.subn(
            r"\bvoid\s+main\s*\(\s*\)",
            "void realtime_manim_gl_main()",
            source,
            count=1,
        )
        if replacements != 1:
            raise ValueError("Vertex shader must declare exactly one void main().")
    output: list[str] = []
    saw_version = False
    fragment_output_location = 0
    boolean_uniforms: dict[str, tuple[str, str]] = {}
    boolean_uniform_arrays: dict[str, tuple[str, str, int]] = {}
    matrix_attribute_assignments: list[str] = []
    for line in source.splitlines():
        if line.lstrip().startswith("#version"):
            output.append("#version 450")
            saw_version = True
            continue
        match = DECLARATION.match(line)
        if not match:
            output.append(line)
            continue
        qualifier = match.group("qualifier")
        name = match.group("name")
        if qualifier == "uniform":
            array_length = _array_length(match)
            if array_length > 1:
                if match.group("type") == "sampler2D":
                    raise ValueError(
                        "sampler2D arrays are not yet a WebGPU-compatible "
                        "Manim shader contract."
                    )
                if match.group("type") in {
                    "bool",
                    "bvec2",
                    "bvec3",
                    "bvec4",
                }:
                    bool_type = match.group("type")
                    gpu_type = {
                        "bool": "uint",
                        "bvec2": "uvec2",
                        "bvec3": "uvec3",
                        "bvec4": "uvec4",
                    }[bool_type]
                    gpu_name = f"{name}_realtime_manim_boolean"
                    boolean_uniform_arrays[name] = (
                        gpu_name,
                        gpu_type,
                        1 if bool_type == "bool" else int(bool_type[-1]),
                    )
                    block_name = f"RealtimeManim_{name}_Block"
                    output.append(
                        f"{match.group('indent')}layout(std140, set = 0, "
                        f"binding = {uniform_bindings[name]}) uniform "
                        f"{block_name} {{ {gpu_type} {gpu_name}"
                        f"{match.group('array')}; }};"
                    )
                    continue
                block_name = f"RealtimeManim_{name}_Block"
                output.append(
                    f"{match.group('indent')}layout(std140, set = 0, "
                    f"binding = {uniform_bindings[name]}) uniform "
                    f"{block_name} {{ {match.group('type')} {name}"
                    f"{match.group('array')}; }};"
                )
                continue
            if match.group("type") == "sampler2D":
                output.append(
                    f"{match.group('indent')}layout(set = 0, binding = "
                    f"{uniform_bindings[name]}) uniform texture2D {name}_texture;"
                )
                output.append(
                    f"{match.group('indent')}layout(set = 0, binding = "
                    f"{sampler_bindings[name]}) uniform sampler {name}_sampler;"
                )
                continue
            if match.group("type") in {"bool", "bvec2", "bvec3", "bvec4"}:
                bool_type = match.group("type")
                gpu_type = {
                    "bool": "uint",
                    "bvec2": "uvec2",
                    "bvec3": "uvec3",
                    "bvec4": "uvec4",
                }[bool_type]
                gpu_name = f"{name}_realtime_manim_boolean"
                expression = (
                    f"({gpu_name} != 0u)"
                    if bool_type == "bool"
                    else (
                        f"notEqual({gpu_name}, "
                        f"{gpu_type}({', '.join('0u' for _ in range(int(bool_type[-1]))) }))"
                    )
                )
                boolean_uniforms[name] = (gpu_name, expression)
                output.append(
                    f"{match.group('indent')}layout(set = 0, binding = "
                    f"{uniform_bindings[name]}) uniform {gpu_type} {gpu_name};"
                )
                continue
            layout = f"layout(set = 0, binding = {uniform_bindings[name]}) "
        elif stage == "vertex" and qualifier == "in":
            matrix_match = re.fullmatch(r"mat([234])", match.group("type"))
            if matrix_match is not None:
                if _array_length(match) != 1:
                    raise ValueError(
                        "Matrix vertex-attribute arrays are unsupported."
                    )
                dimension = int(matrix_match.group(1))
                column_names = [
                    f"{name}_realtime_manim_column_{column}"
                    for column in range(dimension)
                ]
                for column, column_name in enumerate(column_names):
                    output.append(
                        f"{match.group('indent')}layout(location = "
                        f"{attribute_locations[name] + column}) in "
                        f"vec{dimension} {column_name};"
                    )
                output.append(
                    f"{match.group('indent')}{match.group('type')} {name};"
                )
                matrix_attribute_assignments.append(
                    f"    {name} = {match.group('type')}"
                    f"({', '.join(column_names)});"
                )
                continue
            layout = f"layout(location = {attribute_locations[name]}) "
        elif stage == "vertex" and qualifier == "out":
            layout = f"layout(location = {varying_locations[name]}) "
        elif stage == "fragment" and qualifier == "in":
            layout = f"layout(location = {varying_locations[name]}) "
        else:
            if fragment_output_location > 0:
                # Manim's OpenGL framebuffer exposes one color attachment.
                # Preserve writes to extra user outputs as private values
                # without asking WebGPU for nonexistent render targets.
                output.append(
                    f"{match.group('indent')}{match.group('type')} "
                    f"{name}{match.group('array') or ''};"
                )
                fragment_output_location += 1
                continue
            layout = f"layout(location = {fragment_output_location}) "
            fragment_output_location += 1
        output.append(
            f"{match.group('indent')}{layout}{match.group('modifiers')}"
            f"{qualifier} "
            f"{match.group('type')} {name}{match.group('array') or ''};"
        )
    if not saw_version:
        output.insert(0, "#version 450")
    if stage == "vertex":
        output.extend(
            [
                "",
                "void main() {",
                *matrix_attribute_assignments,
                "    realtime_manim_gl_main();",
                "    // OpenGL clip depth is -w..w; WebGPU clip depth is 0..w.",
                "    gl_Position.z = 0.5 * (gl_Position.z + gl_Position.w);",
                "}",
            ]
        )
    annotated = "\n".join(output) + "\n"
    for name, (_gpu_name, expression) in boolean_uniforms.items():
        annotated = re.sub(rf"\b{re.escape(name)}\b", expression, annotated)
    for name, (gpu_name, gpu_type, components) in (
        boolean_uniform_arrays.items()
    ):
        def replace_boolean_array(
            array_match: re.Match[str],
            *,
            gpu_name: str = gpu_name,
            gpu_type: str = gpu_type,
            components: int = components,
        ) -> str:
            index = array_match.group(1)
            value = f"{gpu_name}[{index}]"
            if components == 1:
                return f"({value} != 0u)"
            zeros = ", ".join("0u" for _ in range(components))
            return f"notEqual({value}, {gpu_type}({zeros}))"

        annotated = re.sub(
            rf"\b{re.escape(name)}\s*\[([^\[\]]+)\]",
            replace_boolean_array,
            annotated,
        )
    for name in sampler_bindings:
        combined = f"sampler2D({name}_texture, {name}_sampler)"
        for function in (
            "texture",
            "texture2D",
            "textureLod",
            "textureGrad",
            "textureProj",
            "textureOffset",
            "textureGather",
            "textureSize",
            "texelFetch",
        ):
            replacement = "texture" if function == "texture2D" else function
            annotated = re.sub(
                rf"\b{function}\s*\(\s*{re.escape(name)}\s*,",
                f"{replacement}({combined},",
                annotated,
            )
    return annotated


def _translate(stage: str, source: str) -> str:
    if not TRANSLATOR.exists():
        subprocess.run(
            ["cargo", "build", "--release", "-p", "realtime-manim-glsl-to-wgsl"],
            cwd=ROOT,
            check=True,
        )
    result = subprocess.run(
        [str(TRANSLATOR), stage],
        input=source,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise ValueError(
            f"{stage} GLSL could not be translated to WebGPU WGSL:\n"
            f"{result.stderr.strip()}"
        )
    return result.stdout


def translate_program(
    vertex_source: str,
    fragment_source: str,
    attribute_order: list[str] | None = None,
) -> dict[str, Any]:
    vertex_source = _expand_comma_declarations(
        _split_global_statements(vertex_source)
    )
    fragment_source = _expand_comma_declarations(
        _split_global_statements(fragment_source)
    )
    attributes = _collect(vertex_source, "in")
    attribute_names = [name for name, _field_type, _length in attributes]
    if attribute_order:
        ordered = [name for name in attribute_order if name in attribute_names]
        ordered.extend(name for name in attribute_names if name not in ordered)
        attribute_names = ordered
    attribute_types = {
        name: (field_type, length)
        for name, field_type, length in attributes
    }
    attribute_locations: dict[str, int] = {}
    next_attribute_location = 0
    for name in attribute_names:
        field_type, length = attribute_types[name]
        if length != 1:
            raise ValueError("Vertex-attribute arrays are unsupported.")
        attribute_locations[name] = next_attribute_location
        matrix_match = re.fullmatch(r"mat([234])", field_type)
        next_attribute_location += (
            int(matrix_match.group(1)) if matrix_match is not None else 1
        )
    if next_attribute_location > 16:
        raise ValueError("Shader vertex attributes consume more than 16 locations.")

    vertex_varyings = _collect(vertex_source, "out")
    fragment_varyings = _collect(fragment_source, "in")
    varying_types = {
        name: (field_type, length)
        for name, field_type, length in vertex_varyings
    }
    for name, field_type, length in fragment_varyings:
        if name in varying_types and varying_types[name] != (
            field_type,
            length,
        ):
            raise ValueError(
                f"Shader varying {name!r} has mismatched types "
                f"{varying_types[name]!r} and {(field_type, length)!r}."
            )
    varying_names = [
        name for name, _field_type, _length in vertex_varyings
    ]
    varying_names.extend(
        name
        for name, _field_type, _length in fragment_varyings
        if name not in varying_names
    )
    varying_locations = {
        name: index for index, name in enumerate(varying_names)
    }

    vertex_uniforms = _collect(vertex_source, "uniform")
    fragment_uniforms = _collect(fragment_source, "uniform")
    uniform_types = {
        name: (field_type, length)
        for name, field_type, length in vertex_uniforms
    }
    for name, field_type, length in fragment_uniforms:
        if name in uniform_types and uniform_types[name] != (
            field_type,
            length,
        ):
            raise ValueError(
                f"Shader uniform {name!r} has mismatched types "
                f"{uniform_types[name]!r} and {(field_type, length)!r}."
            )
        uniform_types.setdefault(name, (field_type, length))
    uniform_names = [
        name for name, _field_type, _length in vertex_uniforms
    ]
    uniform_names.extend(
        name
        for name, _field_type, _length in fragment_uniforms
        if name not in uniform_names
    )
    uniform_bindings: dict[str, int] = {}
    sampler_bindings: dict[str, int] = {}
    next_binding = 0
    for name in uniform_names:
        uniform_bindings[name] = next_binding
        next_binding += 1
        if uniform_types[name][0] == "sampler2D":
            sampler_bindings[name] = next_binding
            next_binding += 1

    annotated_vertex = _annotate(
        vertex_source,
        "vertex",
        attribute_locations,
        varying_locations,
        uniform_bindings,
        sampler_bindings,
    )
    annotated_fragment = _annotate(
        fragment_source,
        "fragment",
        attribute_locations,
        varying_locations,
        uniform_bindings,
        sampler_bindings,
    )
    translated_attributes: list[dict[str, Any]] = []
    for name in attribute_names:
        field_type = attribute_types[name][0]
        matrix_match = re.fullmatch(r"mat([234])", field_type)
        if matrix_match is None:
            translated_attributes.append(
                {
                    "name": name,
                    "sourceName": name,
                    "type": field_type,
                    "location": attribute_locations[name],
                }
            )
            continue
        dimension = int(matrix_match.group(1))
        translated_attributes.extend(
            {
                "name": f"{name}_realtime_manim_column_{column}",
                "sourceName": name,
                "sourceColumn": column,
                "type": f"vec{dimension}",
                "location": attribute_locations[name] + column,
            }
            for column in range(dimension)
        )
    return {
        "vertexWgsl": _translate("vertex", annotated_vertex),
        "fragmentWgsl": _translate("fragment", annotated_fragment),
        "attributes": translated_attributes,
        "varyings": [
            {
                "name": name,
                "type": varying_types[name][0],
                "location": varying_locations[name],
            }
            for name in varying_names
        ],
        "uniformBindings": [
            {
                "name": name,
                "type": uniform_types[name][0],
                "binding": uniform_bindings[name],
                "arrayLength": uniform_types[name][1],
                **(
                    {"samplerBinding": sampler_bindings[name]}
                    if name in sampler_bindings
                    else {}
                ),
            }
            for name in uniform_names
        ],
        "annotatedVertex": annotated_vertex,
        "annotatedFragment": annotated_fragment,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("vertex", type=Path)
    parser.add_argument("fragment", type=Path)
    parser.add_argument("--attribute", action="append", default=[])
    args = parser.parse_args()
    result = translate_program(
        args.vertex.read_text(),
        args.fragment.read_text(),
        args.attribute or None,
    )
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
