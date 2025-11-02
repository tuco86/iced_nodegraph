# Coordinate Spaces Analysis - Where Things Go Wrong

## The Problem

When zooming, pin detection stops working. Root cause: **Mixing coordinate spaces**.

## Data Flow Analysis

### 1. Node Positions (self.nodes)
```rust
// In NodeGraph struct
nodes: Vec<(Point, Element)>  // What space are these Points in?
```

**Answer**: These are in **WORLD SPACE** - they represent where nodes exist on the infinite canvas.

### 2. Layout System
```rust
fn layout(&self, ...) -> layout::Node {
    let nodes = self.elements_iter()
        .map(|((position, element), node_tree)| {
            element.as_widget()
                .layout(node_tree, renderer, &limits)
                .move_to(position)  // <-- PROBLEM HERE
        })
}
```

**Issue**: `layout` operates in **SCREEN SPACE** (pixels on viewport), but we're passing **WORLD SPACE** positions!

### 3. Draw Function
```rust
fn draw(&self, tree: &Tree, renderer: &mut Renderer, ...) {
    camera.draw_with(renderer, viewport, cursor, |renderer, world_viewport, world_cursor| {
        // Inside here, renderer is transformed to world space
        for (node_index, ((position, element), tree), layout)) in ... {
            element.draw(tree, renderer, theme, style, layout, world_cursor, world_viewport);
        }
    });
}
```

**Current Behavior**: 
- `camera.draw_with()` applies transformation to renderer
- `layout` (from layout phase) is in SCREEN space
- Renderer is in WORLD space
- **Mismatch!**

### 4. Update Function (Mouse Input)
```rust
fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout, screen_cursor: ...) {
    camera.update_with(viewport, screen_cursor, |viewport, world_cursor| {
        // world_cursor is in WORLD SPACE ✅
        
        for (node_index, (node_layout, node_tree)) in layout.children().zip(...) {
            // node_layout is from layout phase = SCREEN SPACE ❌
            for (pin_index, _, (a, b)) in find_pins(node_tree, node_layout) {
                // a, b are computed from node_layout.bounds() = SCREEN SPACE ❌
                let distance = a.distance(world_cursor.position()); // MISMATCH!
            }
        }
    });
}
```

**Current Behavior**: Comparing SCREEN SPACE pins with WORLD SPACE cursor!

## The Core Issue

The Iced layout system operates in **screen space** (viewport coordinates), but we want nodes to live in **world space** (infinite canvas with camera).

### Two Possible Solutions

#### Option A: Keep Layout in Screen Space, Transform Pins
```rust
// In update()
let pin_screen: Point = /* from layout */;
let pin_world: WorldPoint = camera.screen_to_world().transform_point(pin_screen.into_euclid());
// Compare pin_world with world_cursor ✅
```

**Problem**: Layout positions are already wrong because we passed world positions to `.move_to()`!

#### Option B: Make Layout Work in World Space
```rust
// In layout()
let nodes = self.elements_iter()
    .map(|((world_position, element), node_tree)| {
        let node_layout = element.as_widget().layout(...);
        // Don't use move_to with world positions!
        // Layout should be at (0,0), positioning happens in draw()
        node_layout
    })
```

Then in `draw()`, manually offset each node by its world position when rendering.

## Current State

We're in a hybrid broken state:
- Layout phase: Receives world positions, creates screen space layouts ❌
- Draw phase: Applies camera transform, but layouts are already positioned wrong ❌  
- Update phase: Layouts in screen space, cursor in world space, positions don't match ❌

## Solution Strategy

**Option B is correct**: Layout should be at (0,0), camera transformation happens during rendering.

### Required Changes

1. **layout()**: Don't use `.move_to(position)` - keep all layouts at origin
2. **draw()**: Manually translate renderer by world position before drawing each node
3. **update()**: Node layouts are at (0,0), add world position offset when checking bounds
4. **find_pins()**: Calculate pin positions relative to world-positioned nodes

This way:
- Layout system stays in its natural screen space
- World space transformation happens explicitly in draw/update
- No coordinate space confusion
