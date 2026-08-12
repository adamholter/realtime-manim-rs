# Animation stress compatibility receipt

Updated: 2026-07-30

Four additional adversarial regular-Manim scenes compile to retained Rust tracks
without compatibility diagnostics:

- `TransformMatchingTex`, `ChangeDecimalToValue`, and `Circumscribe`
- `MoveAlongPath` and arbitrary `Homotopy`
- `LaggedStart`, `AnimationGroup`, `Wiggle`, `Indicate`, `Rotate`, and `Flash`
- `ApplyMatrix` and arbitrary `ApplyPointwiseFunction`

Each scene ran at its midpoint and endpoint in isolated headless Chrome/WebGPU with
no page or console errors. Final-frame differentials at 854×480 against real Manim
Community 0.20.1:

| Scene | Rust nodes | Rust tracks | RMSE |
|---|---:|---:|---:|
| `MatchingAndNumbersCompatibility` | 142 | 13 | 0.006090 |
| `DeformationAndMotionCompatibility` | 3 | 2 | 0.006641 |
| `EffectsAndCompositionCompatibility` | 15 | 20 | 0.006796 |
| `MatrixAndFunctionsCompatibility` | 12 | 12 | 0.009443 |

Sources, receipts, references, and Rust captures live under
`benchmarks/compat/`.
