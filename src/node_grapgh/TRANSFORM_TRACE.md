# Transform Trace - Every Coordinate Space Conversion

## The Iced Widget Lifecycle

### 1. layout() - Layout Phase
```rust
fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node
```

**What Iced expects**: Return a layout tree with positions in **SCREEN SPACE** (pixels relative to parent)

**What we're doing**:
```rust
.map(|((position, element), node_tree)| {
    element.as_widget()
        .layout(node_tree, renderer, &limits)
        .move_to(position)  // position is WORLD SPACE Point { x: 200.0, y: 150.0 }
})
```

**Result**: Layout nodes positioned at WORLD coordinates (200, 150) not screen coordinates!

❌ **PROBLEM**: We're putting world positions into screen space layout!

---

### 2. draw() - Rendering Phase
```rust
fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, ...)
```

**What happens**:
```rust
camera.draw_with(renderer, viewport, cursor, |renderer, world_viewport, world_cursor| {
    // renderer is transformed by:
    // 1. renderer.with_transformation(scale(zoom))
    // 2. renderer.with_translation(camera_position)
    
    for (node_index, ((_position, element), tree), layout)) in ... {
        // layout.bounds() returns what we put in layout phase
        // We put world coordinates (200, 150) there
        // Renderer draws at those coordinates
        // BUT renderer is already transformed!
        
        element.draw(tree, renderer, theme, style, layout, world_cursor, world_viewport);
    }
});
```

**Coordinate Math**:
- Layout says: "draw at (200, 150)"  (world space from our layout phase)
- Renderer transform: `scale(2.0) * translate(0, 0) * point(200, 150)`
- Screen result: `(400, 300)` ❌ WRONG! Should be at (200, 150) screen pixels

---

### 3. update() - Event Handling Phase  
```rust
fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout, screen_cursor: mouse::Cursor, ...)
```

**What happens**:
```rust
camera.update_with(viewport, screen_cursor, |world_viewport, world_cursor| {
    // world_cursor is transformed: screen → world ✅
    // layout is NOT transformed (still from layout phase)
    
    for (node_index, (node_layout, node_tree)) in layout.children().zip(...) {
        // node_layout.bounds() = Rectangle { x: 200, y: 150, ... }
        // Is this screen or world space? 
        // It's whatever we put in layout phase = WORLD SPACE
        
        for (pin_index, _, (a, b)) in find_pins(node_tree, node_layout) {
            // a, b are calculated from node_layout.bounds()
            // So they're in WORLD SPACE (200 + offset)
            
            let distance = a.distance(world_cursor.position());
            // Comparing WORLD with WORLD ✅ This should work!
        }
    }
});
```

---

## The Real Problem

When we **zoom in (scale > 1.0)**:

### In draw():
```
Layout says position: (200, 150) [world]
Renderer transform: scale(2.0) * (200, 150) = (400, 300) [screen]
Node appears at screen position (400, 300) ✅ This is correct for zoomed view
```

### In update():
```
Mouse at screen: (400, 300)
Mouse in world: camera.screen_to_world(400, 300) = ???

Let's calculate:
screen_to_world formula: world = screen * zoom - camera_position
world = (400, 300) * 2.0 - (0, 0) = (800, 600) ❌ WRONG!

But node layout says: (200, 150) [world]
So we're looking for pin at ~(200, 150) [world]
But cursor is at (800, 600) [world]
Distance = 600+ pixels → No match!
```

---

## Root Cause

The `screen_to_world()` formula is **BACKWARDS**:

```rust
pub fn screen_to_world(&self) -> ScreenToWorld {
    Transform2D::translation(-self.position.x, -self.position.y)
        .pre_scale(zoom, zoom)
}
```

This creates: `scale(zoom) * translate(-position)`

Which means: `world = screen * zoom - position`

**But our draw does**: `renderer.scale(zoom) * renderer.translate(position) * world_point`

So draw does: `screen = world * zoom + position`
And screen_to_world does: `world = screen * zoom - position`

These are NOT inverses of each other!

---

## The Correct Formula

If draw does: `screen = world * zoom + position`
Then screen_to_world should do: `world = (screen - position) / zoom`

```rust
pub fn screen_to_world(&self) -> ScreenToWorld {
    Transform2D::scale(1.0 / self.zoom.get(), 1.0 / self.zoom.get())
        .then_translate(-self.position.to_vector())
}
```

Or algebraically:
```
screen = world * zoom + position
screen - position = world * zoom
world = (screen - position) / zoom
```

---

## Test Case

Node at world (200, 150), zoom=2.0, camera_position=(0, 0)

**Draw**: `screen = 200 * 2.0 + 0 = 400` ✅
**Current screen_to_world**: `world = 400 * 2.0 - 0 = 800` ❌
**Correct screen_to_world**: `world = (400 - 0) / 2.0 = 200` ✅
