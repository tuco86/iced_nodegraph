# Demo: styling

## Visual Customization and Theming

This demo showcases the theming and visual customization capabilities of `iced_nodegraph`. It demonstrates how to create visually distinct nodes, customize pin appearances, and implement theme switching.

## Features Demonstrated

- Custom node styles (colors, borders, shadows)
- Pin appearance customization per type
- Theme switching (light/dark modes)
- Edge styling variations
- Visual feedback states (hover, selected, dragging)
- Custom background patterns

## Implementation Requirements

### Dependencies

```toml
[dependencies]
iced_nodegraph = { path = "../../iced_nodegraph" }
iced = { git = "https://github.com/iced-rs/iced", features = ["advanced", "wgpu"] }
```

### Application Structure

1. **Main Application State**

   - `NodeGraph` instance
   - `Theme` enum (Light, Dark, Custom variants)
   - Style configuration per node type

2. **Message Types**

   ```rust
   #[derive(Debug, Clone)]
   enum Message {
       NodeGraphMessage(iced_nodegraph::Message),
       ToggleTheme,
       ChangeNodeStyle(usize, NodeStyle),
   }
   ```

3. **Node Types with Different Styles**

   - **Input Node**: Blue color scheme, rounded corners
   - **Process Node**: Green color scheme, sharp corners
   - **Output Node**: Orange color scheme, elevated shadow
   - **Comment Node**: Gray color scheme, dashed border

4. **Pin Styling by Type**

   - Data pins: Circular, colored by type
   - Control flow pins: Diamond shape
   - Event pins: Triangle shape

5. **UI Layout**

   ```
   [Theme Toggle Button] [Node Style Controls]
   +------------------------------------------+
   |                                          |
   |          NodeGraph Canvas                |
   |                                          |
   +------------------------------------------+
   ```

### Node Configuration

Position 4-5 nodes demonstrating different styles:

- Node 0 (Input): Position `(150.0, 200.0)` - Blue theme
- Node 1 (Process): Position `(400.0, 150.0)` - Green theme
- Node 2 (Process): Position `(400.0, 300.0)` - Green theme
- Node 3 (Output): Position `(650.0, 225.0)` - Orange theme
- Node 4 (Comment): Position `(300.0, 450.0)` - Gray theme

### Visual States

Implement visual feedback for:

- **Idle**: Default appearance
- **Hover**: Slightly brighter border
- **Selected**: Bold border, accent color
- **Dragging**: Semi-transparent, elevated shadow
- **Pin Hover**: Glow effect
- **Pin Connected**: Solid fill

### Theme Switching

Implement light and dark mode variants:

**Light Theme:**

- Background: `#F5F5F5`
- Grid lines: `#E0E0E0`
- Node shadows: Soft, subtle
- Text: Dark gray `#333333`

**Dark Theme:**

- Background: `#1E1E1E`
- Grid lines: `#2D2D2D`
- Node shadows: Deeper, more pronounced
- Text: Light gray `#CCCCCC`

### Edge Styling

Demonstrate different edge styles:

- Solid lines for data connections
- Dashed lines for control flow
- Animated gradient for active data flow (optional)
- Bezier curves vs straight lines toggle

## User Interactions

All standard interactions from hello_world demo, plus:

- **Theme Toggle**: Button in UI to switch light/dark mode
- **Style Selection**: UI controls to change node styles dynamically
- **Visual Feedback**: Hover and selection states clearly visible

## Expected Output

When run with `cargo run -p iced_nodegraph_demo_styling`, the user should see:

1. Multiple nodes with visually distinct styles
2. A theme toggle button that switches between light/dark modes
3. Smooth visual transitions when interacting with nodes
4. Different pin shapes indicating different data types
5. Clear visual feedback for all interaction states

## Code Structure

```
demos/styling/
├── Cargo.toml
├── README.md (this file)
└── src/
    ├── main.rs              # Application entry point
    ├── theme.rs             # Theme definitions and switching logic
    ├── node_styles.rs       # Custom node style implementations
    └── pin_styles.rs        # Pin appearance customization
```

## Styling API Reference

This demo should demonstrate the styling hooks provided by `iced_nodegraph`:

- Node appearance customization
- Pin visual customization
- Edge rendering styles
- Theme integration with Iced's theme system

## Copilot Initialization Instructions

To initialize this demo project:

1. Create `demos/styling/Cargo.toml` with workspace dependency
2. Create `demos/styling/src/main.rs` with theme switching UI
3. Implement `theme.rs` with Light/Dark theme definitions
4. Implement `node_styles.rs` with custom node appearances
5. Implement `pin_styles.rs` with type-specific pin rendering
6. Ensure visual consistency with Iced 0.14 theme system
7. Test theme switching and style variations
8. Verify visual feedback for all interaction states

## Design Principles

- Visual hierarchy: Important nodes should stand out
- Color accessibility: Ensure sufficient contrast
- Consistent spacing: Maintain visual rhythm
- Smooth transitions: Avoid jarring visual changes
- Performance: Keep styling operations efficient
