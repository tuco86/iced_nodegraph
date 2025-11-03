# NodeGraph Rendering Architecture

## Overview

The `iced_nodegraph` project supports **two rendering modes** depending on the target platform:

### 🎮 Native WGPU Rendering (Recommended)
**Platform:** Desktop (macOS, Windows, Linux)  
**Command:** `cargo run --example hello_world`

**Features:**
- ✅ Full **WGPU** graphics pipeline with custom shaders
- ✅ GPU-accelerated rendering via `iced_wgpu`
- ✅ Custom vertex/fragment shaders in `shader.wgsl`
- ✅ Hardware-accelerated node graph visualization
- ✅ Advanced visual effects (anti-aliasing, gradients, shadows)
- ✅ Optimal performance for complex graphs

**Technical Stack:**
```rust
iced_wgpu::primitive::Renderer
↓
Custom Pipeline (src/node_grapgh/effects/pipeline/)
↓
WGSL Shaders (shader.wgsl)
↓
GPU Rendering
```

### 🌐 WASM Canvas2D Rendering
**Platform:** Web browsers (GitHub Pages)  
**URL:** https://tuco86.github.io/iced_nodegraph/hello-world.html

**Features:**
- ✅ Broad browser compatibility
- ✅ No WebGPU requirement
- ✅ Interactive drag-and-drop
- ✅ Dynamic edge creation
- ⚠️ Simplified rendering (Canvas 2D API)
- ⚠️ No custom shader effects

**Technical Stack:**
```javascript
WASM Module (wasm-bindgen)
↓
JavaScript Canvas 2D Context
↓
Browser Rendering Engine
```

## Why Two Modes?

### Native WGPU
The core `iced_nodegraph` widget is built on Iced's advanced WGPU rendering pipeline. This provides:
- Maximum performance
- Custom visual effects via shaders
- Hardware acceleration
- Complete control over rendering

### WASM Canvas2D
WebGPU browser support is still limited (as of 2024). The WASM demo uses Canvas2D for:
- Universal browser compatibility
- Demonstration of core functionality
- Cross-platform validation
- Quick prototyping

## Performance Comparison

| Feature | Native WGPU | WASM Canvas2D |
|---------|-------------|---------------|
| Rendering | GPU Shaders | CPU/Browser |
| FPS (1000 nodes) | 60+ | 30-60 |
| Visual Effects | ✅ Full | ⚠️ Limited |
| Startup Time | Fast | Moderate |
| Browser Support | N/A | ✅ Universal |

## Migration Path

As WebGPU adoption increases, the WASM version can be upgraded to use `iced_wgpu`'s WebGPU backend:

```rust
// Future: Enable WebGPU in WASM
[features]
wasm = [
    "iced_wgpu/webgpu",  // When browser support is ready
    "wasm-bindgen",
    // ...
]
```

This will bring full WGPU rendering to the browser once WebGPU is widely supported.

## Running Examples

### Native (Full WGPU)
```bash
cargo run --example hello_world
```

### WASM (Canvas2D)
```bash
# Build
wasm-pack build --target web --release -- --features wasm

# Copy to docs
cp -r pkg docs/

# Serve locally
cd docs && python3 -m http.server 8080
```

## Conclusion

The dual rendering approach ensures:
- **Best experience** on native platforms with full WGPU
- **Maximum compatibility** on web with Canvas2D fallback
- **Future-ready** architecture for WebGPU adoption

For production node graph editors, **native builds with WGPU are recommended** for optimal performance and visual quality.
