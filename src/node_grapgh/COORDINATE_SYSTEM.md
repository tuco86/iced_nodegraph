# Coordinate System Documentation

## Overview

The node graph uses **two coordinate spaces** with explicit type safety via the `euclid` crate:

- **Screen Space** - The raw pixel coordinates from user input (mouse, viewport)
- **World Space** - The virtual canvas where nodes exist, affected by camera zoom and pan

## Type System

```rust
// Phantom types for compile-time space checking
enum Screen {}  // Marker for screen coordinates
enum World {}   // Marker for world coordinates

// Typed coordinates
type ScreenPoint = Point2D<f32, Screen>;
type WorldPoint = Point2D<f32, World>;

// Transformation matrices
type ScreenToWorld = Transform2D<f32, Screen, World>;
type WorldToScreen = Transform2D<f32, World, Screen>;
```

**Key Insight**: The compiler prevents mixing coordinate spaces accidentally.

## Camera2D Structure

```rust
pub struct Camera2D {
    zoom: Scale<f32, Screen, World>,  // How many screen pixels = 1 world unit
    position: WorldPoint,              // Where camera is looking in world space
}
```

### Transform Formula

**Screen → World:**
```
world_point = (screen_point * zoom) - camera_position
```

Implementation:
```rust
pub fn screen_to_world(&self) -> ScreenToWorld {
    Transform2D::translation(-self.position.x, -self.position.y)
        .pre_scale(zoom, zoom)
}
```

**Order of operations:**
1. Scale screen coordinates by zoom (1.0 = 1:1, 2.0 = zoomed in 2x)
2. Translate by negative camera position

**World → Screen:** (inverse of above)
```
screen_point = (world_point + camera_position) / zoom
```

## Usage Patterns

### 1. Mouse Input (Screen → World)

```rust
// Mouse events arrive in screen space
Event::Mouse(mouse::Event::ButtonPressed(_)) => {
    if let Some(cursor_position) = screen_cursor.position() {
        // Step 1: Convert Iced Point to ScreenPoint
        let cursor_position: ScreenPoint = cursor_position.into_euclid();
        
        // Step 2: Transform to world space
        let world_cursor: WorldPoint = 
            state.camera.screen_to_world().transform_point(cursor_position);
        
        // Now we can compare with node positions in world space
        for (node_index, node_layout) in layout.children().enumerate() {
            if world_cursor.is_over(node_layout.bounds()) {
                // Hit test in world space
            }
        }
    }
}
```

### 2. Rendering (World → Screen via Transformation Stack)

```rust
// Rendering happens in world space, GPU does the transformation
state.camera.draw_with(renderer, viewport, screen_cursor, |renderer, world_viewport, world_cursor| {
    // Inside this closure:
    // - renderer is pre-configured with zoom scale + translation
    // - world_viewport is the visible rectangle in world coordinates
    // - world_cursor is the mouse position in world coordinates
    
    // Just draw at world positions
    for (position, element) in &self.nodes {
        element.draw(tree, renderer, theme, style, layout, world_cursor, world_viewport);
    }
});
```

### 3. Dragging (World Space Offset)

```rust
// When dragging starts, store world position
Dragging::Node(node_index, origin: WorldPoint)

// During drag, calculate offset in world space
let cursor_position: WorldPoint = /* transformed from screen */;
let offset: WorldVector = cursor_position - origin;

// Apply offset to node's world position
let new_position = self.nodes[node_index].0 + offset.into_iced();
```

### 4. Zoom (Keep Cursor Position Stable)

```rust
pub fn zoom_at(&self, cursor: WorldPoint, offset: f32) -> Self {
    let old_zoom = self.zoom;
    let new_zoom = Scale::new(self.zoom.get() + offset);
    
    // Calculate how much the cursor would move in world space
    // due to zoom change, then compensate by moving camera
    let cursor_screen = old_zoom.inverse().transform_point(cursor);
    let cursor_world_after = new_zoom.transform_point(cursor_screen);
    let offset = cursor_world_after - cursor;
    
    Self {
        zoom: new_zoom,
        position: self.position + offset,
    }
}
```

**Mental Model**: Imagine the cursor is a nail pinning a map to a table. When you zoom, the point under the nail stays fixed.

## Common Pitfalls

### ❌ Wrong: Mixing spaces

```rust
// WRONG: Comparing screen position with world layout
let cursor: Point = screen_cursor.position().unwrap();
if cursor.x > node_layout.bounds().x { /* BAD */ }
```

### ✅ Right: Convert first

```rust
// RIGHT: Transform cursor to world space first
let cursor: WorldPoint = state.camera
    .screen_to_world()
    .transform_point(screen_cursor.position().unwrap().into_euclid());
if cursor.x > node_layout.bounds().x { /* GOOD */ }
```

### ❌ Wrong: Double transformation

```rust
// WRONG: Transforming twice
let world_pos = camera.screen_to_world().transform_point(screen_pos);
let offset = camera.screen_to_world().transform_vector(screen_offset); // BAD!
```

### ✅ Right: Transform once

```rust
// RIGHT: Work in one space consistently
let world_pos = camera.screen_to_world().transform_point(screen_pos);
let world_offset = world_pos - origin; // Both in world space
```

## Debugging Checklist

When coordinates seem wrong:

1. **Print coordinate spaces**:
   ```rust
   println!("screen: {:?}", screen_pos);
   println!("world: {:?}", camera.screen_to_world().transform_point(screen_pos));
   println!("camera: zoom={}, pos={:?}", camera.zoom(), camera.position());
   ```

2. **Check transformation order**:
   - Zoom BEFORE translation? ✅ `pre_scale(zoom, zoom)`
   - Zoom AFTER translation? ❌ Wrong order

3. **Verify cursor is transformed**:
   - Using raw `screen_cursor` inside `draw_with`? ❌ Use `world_cursor`
   - Using `world_cursor` outside `draw_with`? ❌ Transform manually first

4. **Test at zoom = 1.0, position = (0, 0)**:
   - At identity transform, screen space = world space
   - Good for isolating zoom vs position bugs

## Test Scenarios

See `camera.rs` module tests for verification:

- `test_identity_transform` - No zoom/pan = 1:1 mapping
- `test_zoom_transform` - Screen coordinates scale by zoom
- `test_pan_transform` - Camera position offsets correctly
- `test_zoom_at_cursor` - Cursor stays fixed during zoom
- `test_round_trip` - Screen → World → Screen = original

## Visual Reference

```
Screen Space (pixels)          World Space (virtual)
┌─────────────────┐           ┌─────────────────────────┐
│ (0,0)           │           │                         │
│   ╭─────╮       │           │  ╭──────────╮           │
│   │ 👆  │       │  ──────►  │  │  Node    │           │
│   ╰─────╯       │  zoom +   │  ╰──────────╯           │
│                 │  pan      │                         │
│        (800,600)│           │              (1000,1000)│
└─────────────────┘           └─────────────────────────┘
   Fixed viewport                 Infinite canvas
```

- **Left**: What the user sees (screen/viewport)
- **Right**: Where nodes actually live (world space)
- **Camera**: Controls the mapping between the two

## Implementation Files

- `euclid.rs` - Type definitions and conversion traits
- `camera.rs` - Camera2D with transformation logic
- `widget.rs` - Usage of coordinate transformations
- `effects/pipeline/mod.rs` - Shader uniforms (already in world space)
