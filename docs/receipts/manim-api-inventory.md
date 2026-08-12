# Manim public API inventory

Updated: 2026-07-30

`scripts/audit-manim-api.py` inventories the classes exported by Manim Community
0.20.1 plus the explicitly importable OpenGL modules. The generated inventory is
`benchmarks/corpus/manim-api-inventory.json`.

Current result:

- 296 public/exported classes inspected
- 81 Scene/Animation classes execute through Manim's own semantics
- 162 Cairo/OpenGL vector classes lower through the retained cubic path route
- 11 Cairo/OpenGL point-cloud classes lower through retained point marks
- 4 OpenGL surface classes lower through retained smooth/textured meshes
- 2 raster image classes lower through retained RGBA images
- 4 composite/nonvisual Mobject classes use family traversal or tracker suppression
- 3 generic OpenGL base/group/point classes use bounded shader-data inspection
- 29 non-scene support classes need no visual lowering or remain manual review

The 29 manual-review entries are renderers, cameras, color/value types, TeX
configuration, sections, and file-writer infrastructure—not additional Mobject or
Animation families. This inventory establishes routing coverage, not constructor-
by-constructor visual proof; the expanded differential corpus remains the release
gate for behavior.
