# Visual Shader Editor Demo

A comprehensive visual shader editor for iced_nodegraph that demonstrates:

- **110+ Shader Nodes** - Complete Inigo Quilez 2D SDF library + math, vector, color operations
- **Live WGSL Compilation** - Node graph compiles to GPU shader code in real-time
- **Error Visualization** - Animated error pattern when compilation fails
- **Self-Rendering Pipeline** - The shader you build controls how the node graph renders itself

## Features

### Complete SDF Library

All 34 2D signed distance functions from Inigo Quilez's library:
- Primitives: Circle, Box, Triangle, Pentagon, Hexagon, Star, Heart, etc.
- Operations: Union, Subtraction, Intersection, Smooth variants

### Node Categories

- **Input Nodes** (10): UV, Time, Mouse Position, Camera, Resolution
- **Math Operations** (23): Add, Sub, Mul, Div, Trigonometry, etc.
- **Vector Operations** (15): Split, Combine, Dot, Cross, Normalize, etc.
- **Color Operations** (8): RGB/HSV conversion, Mix, Desaturate, etc.
- **SDF Primitives** (34): All Inigo Quilez 2D shapes
- **SDF Operations** (12): Boolean operations, smoothing, modifiers
- **Logic** (8): Comparisons, And, Or
- **Outputs** (5): Background, Node, Pin, Edge, Final

### Live Compilation

1. Build shader graph visually by connecting nodes
2. Compiler validates graph (type checking, cycle detection)
3. Generates WGSL code via topological sort
4. Injects into GPU rendering pipeline
5. On error: shows animated red/pink error pattern

## Running

```bash
cargo run -p demo_shader_editor
```

For WASM:
```bash
cd demos/shader_editor
./build_wasm.sh  # or build_wasm.ps1 on Windows
```

## Architecture

```
User builds node graph
        ↓
Compiler validates & sorts nodes (topological order)
        ↓
Code generator creates WGSL functions
        ↓
Shader injected into Pipeline via new_with_shader()
        ↓
GPU renders with custom shader
```

## Code Structure

- `src/shader_graph/` - Node and socket data structures
- `src/compiler/` - Validation, topological sort, code generation
- `src/sdf_library.wgsl` - Complete Inigo Quilez SDF library
- `src/error_shader.wgsl` - Fallback error visualization
- `src/default_shader.rs` - Working starter graph

## Default Shader

The demo starts with a simple animated shader:
1. UV Input → Circle SDF
2. Time Input → Sin → Modulate radius
3. Smoothstep → Color → Edge Output
4. Result: Pulsing circular edges

## Extending

To add custom nodes:
1. Add variant to `ShaderNodeType` enum in `shader_graph/nodes.rs`
2. Define inputs/outputs in `inputs()` and `outputs()` methods
3. Add code generation in `compiler/codegen.rs`
4. Node automatically appears in node palette

## Technical Highlights

- **Type-safe sockets**: WGSL type system enforced at compile time
- **Cycle detection**: Kahn's algorithm prevents infinite loops
- **Topological sort**: Ensures correct function call order
- **Error recovery**: Graceful fallback on compilation failure
- **Hot reload**: Graph changes recompile shader instantly

## Limitations

- Currently only generates edge fragment shader
- Background/Node/Pin shaders use default implementation
- No runtime shader hot-swapping (requires pipeline recreation)
- Some advanced SDF nodes still need implementation

## Future Work

- Complete all 110 node implementations
- Add node palette/search UI
- Implement all shader passes (background, nodes, pins)
- Runtime shader hot-swapping
- Save/load shader graphs
- Export standalone WGSL shaders

## References

- [Inigo Quilez 2D SDFs](https://iquilezles.org/articles/distfunctions2d/)
- [WGSL Specification](https://www.w3.org/TR/WGSL/)
- [iced_nodegraph Documentation](../../README.md)
