# Demo: hello_world

**Basic Node Graph Usage**

This demo demonstrates the fundamental usage of `iced_nodegraph` with minimal code. It shows how to create a simple node graph application with basic interactions.

## Features Demonstrated

- Creating a `NodeGraph` widget
- Adding nodes to the canvas
- Basic camera controls (pan, zoom)
- Connecting pins between nodes
- Handling connection events

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
   - Optional: Track connections for debugging

2. **Message Types**
   ```rust
   #[derive(Debug, Clone)]
   enum Message {
       NodeGraphMessage(iced_nodegraph::Message),
   }
   ```

3. **Node Content**
   - Simple text widgets showing node IDs
   - 2-3 pre-positioned nodes:
     - Node 0: Position `(200.0, 150.0)` with 1-2 output pins (right side)
     - Node 1: Position `(525.0, 175.0)` with 1-2 input pins (left side)
     - Optional Node 2: Position `(400.0, 350.0)` with mixed pins

4. **View Function**
   ```rust
   fn view(&self) -> Element<Message> {
       self.node_graph
           .clone()
           .map(Message::NodeGraphMessage)
           .into()
   }
   ```

5. **Update Function**
   - Forward `NodeGraphMessage` to the node graph
   - Optional: Log connection events to console

### Window Configuration

- Title: "iced_nodegraph - Hello World Demo"
- Size: 1280x720 pixels
- Background: Theme default

## User Interactions

- **Pan**: Middle mouse button drag
- **Zoom**: Mouse wheel (maintains cursor position)
- **Connect Pins**: Left-click on output pin, drag to input pin, release
- **Move Nodes**: Left-click and drag node header

## Migration Notes

This demo is migrated from `examples/hello_world.rs`. The core functionality remains the same, but it's now part of a structured demo workspace.

## Expected Output

When run with `cargo run -p iced_nodegraph_demo_hello_world`, the user should see:
1. A canvas with 2-3 nodes positioned as specified
2. Ability to pan and zoom the canvas
3. Ability to connect pins by dragging
4. Console output when connections are made (optional)

## Code Reference

See the original implementation in the workspace root:
- `examples/hello_world.rs` - Original example code to be migrated
- `src/hello_world_demo.rs` - Demo-specific widget builders

## Copilot Initialization Instructions

To initialize this demo project:

1. Create `demos/hello_world/Cargo.toml` with workspace dependency on `iced_nodegraph`
2. Create `demos/hello_world/src/main.rs` implementing the structure above
3. Migrate relevant code from `../../examples/hello_world.rs`
4. Ensure all paths reference `../../iced_nodegraph` for the library
5. Test with `cargo run -p hello_world_demo`
