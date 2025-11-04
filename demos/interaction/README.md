# Demo: interaction

## Input/Output Pin Rules and Connection Validation

This demo demonstrates the pin connection rules and type validation system in `iced_nodegraph`. It shows how to implement directional data flow and enforce connection constraints.

## Features Demonstrated

- **Input-only pins** (left side) - can only receive edges
- **Output-only pins** (right side) - can only send edges
- **Bidirectional pins** (top/bottom) - can send or receive
- Pin type validation (prevent incompatible connections)
- Multiple edges per pin vs. single connection enforcement
- Visual feedback for valid/invalid connection attempts
- Connection attempt rejection with user feedback

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
   - Connection validation rules
   - Type registry for pins
   - Connection attempt feedback messages

2. **Message Types**

   ```rust
   #[derive(Debug, Clone)]
   enum Message {
       NodeGraphMessage(iced_nodegraph::Message),
       ConnectionAttempt { from: PinId, to: PinId, valid: bool },
       ShowFeedback(String),
   }
   ```

3. **Pin Types**

   Define several data types to demonstrate type validation:

   ```rust
   #[derive(Debug, Clone, Copy, PartialEq)]
   enum PinType {
       Integer,
       Float,
       String,
       Boolean,
       Vector3,
       Color,
       Any,  // Compatible with all types
   }
   ```

4. **Pin Direction Rules**

   ```rust
   #[derive(Debug, Clone, Copy, PartialEq)]
   enum PinDirection {
       Input,   // Left side - can only receive
       Output,  // Right side - can only send
       Bidirectional, // Top/Bottom - can send or receive
   }
   ```

5. **Connection Validation**

   Implement validation logic:
   - Source must be Output or Bidirectional
   - Target must be Input or Bidirectional
   - Pin types must be compatible
   - Respect single-connection constraints where applicable

### Node Configuration

Create a demonstration graph showing all rule types:

**Node 0 - Number Generator** `(150.0, 200.0)`

- Output pins (right):
  - Pin 0: `Integer` output
  - Pin 1: `Float` output

**Node 1 - Math Operations** `(400.0, 150.0)`

- Input pins (left):
  - Pin 0: `Float` input (single connection only)
  - Pin 1: `Float` input (single connection only)
- Output pins (right):
  - Pin 0: `Float` output (result)

**Node 2 - Type Converter** `(400.0, 300.0)`

- Input pins (left):
  - Pin 0: `Any` input (accepts any type)
- Output pins (right):
  - Pin 0: `Integer` output
  - Pin 1: `Float` output
  - Pin 2: `String` output

**Node 3 - Display** `(650.0, 225.0)`

- Input pins (left):
  - Pin 0: `Any` input (multiple connections allowed)
  - Pin 1: `String` input

**Node 4 - Bidirectional Hub** `(400.0, 450.0)`

- Top pin: `Float` bidirectional
- Right pin: `Integer` bidirectional
- Bottom pin: `Any` bidirectional
- Left pin: `String` bidirectional

### Visual Feedback System

Implement real-time feedback during connection attempts:

**Valid Connection Indicators:**

- Target pin glows green when hovering with compatible connection
- Dashed line shows preview of connection path
- Cursor changes to connection cursor

**Invalid Connection Indicators:**

- Target pin shows red outline when hovering with incompatible type
- Red "X" icon appears near cursor
- Tooltip shows reason: "Incompatible types: Integer → Float"
- Connection line shows red dashed pattern

**Feedback Messages:**

Display temporary messages in UI for:

- Successful connections: "Connected: Node 0 (Integer) → Node 1 (Float)"
- Rejected connections: "Cannot connect: Type mismatch"
- Direction violations: "Cannot connect: Output → Output not allowed"
- Single-connection violations: "Pin already connected (single connection only)"

### UI Layout

```
+------------------------------------------+
|  [Clear All] [Reset Demo] [Show Rules]  |
+------------------------------------------+
|                                          |
|          NodeGraph Canvas                |
|                                          |
+------------------------------------------+
|  Feedback: [Last operation message]     |
+------------------------------------------+
```

### Connection Rules Table

Display rules when "Show Rules" is clicked:

| Rule | Description | Example |
|------|-------------|---------|
| Direction | Output/Bidirectional → Input/Bidirectional | Valid: Node 0 Output → Node 1 Input |
| Type Match | Compatible types only | Valid: Float → Float, Integer → Any |
| Single Connection | Some pins allow only one edge | Math input pins |
| Multiple Connections | Some pins allow many edges | Display "Any" input |

## Validation Logic

### Type Compatibility Matrix

```rust
fn are_types_compatible(source: PinType, target: PinType) -> bool {
    match (source, target) {
        (_, PinType::Any) => true,           // Any accepts all
        (PinType::Any, _) => true,           // Any can connect to all
        (a, b) if a == b => true,            // Same types compatible
        (PinType::Integer, PinType::Float) => true, // Implicit conversion
        _ => false,
    }
}
```

### Direction Validation

```rust
fn are_directions_compatible(source: PinDirection, target: PinDirection) -> bool {
    matches!(
        (source, target),
        (PinDirection::Output, PinDirection::Input) |
        (PinDirection::Output, PinDirection::Bidirectional) |
        (PinDirection::Bidirectional, PinDirection::Input) |
        (PinDirection::Bidirectional, PinDirection::Bidirectional)
    )
}
```

## User Interactions

All standard interactions from hello_world demo, plus:

- **Connection Attempt**: Drag from any pin to see validation feedback
- **Valid Connection**: Green indicators, smooth connection on release
- **Invalid Connection**: Red indicators, connection rejected with message
- **Rules Panel**: Toggle to see connection rules reference
- **Clear All**: Button to remove all connections for clean slate
- **Reset Demo**: Button to restore initial demo state

## Expected Output

When run with `cargo run -p iced_nodegraph_demo_interaction`, the user should see:

1. Multiple nodes with labeled pins showing types
2. Real-time visual feedback during connection attempts
3. Clear distinction between valid and invalid connections
4. Feedback messages explaining why connections succeed or fail
5. Demonstration of all pin rules in action

## Code Structure

```
demos/interaction/
├── Cargo.toml
├── README.md (this file)
└── src/
    ├── main.rs              # Application entry point
    ├── pin_types.rs         # Pin type definitions and compatibility
    ├── validation.rs        # Connection validation logic
    └── feedback.rs          # User feedback system
```

## Educational Objectives

This demo teaches users:

1. How to implement typed pin systems
2. How to enforce directional data flow
3. How to validate connections before accepting them
4. How to provide clear user feedback for invalid actions
5. How to design intuitive connection rules

## Copilot Initialization Instructions

To initialize this demo project:

1. Create `demos/interaction/Cargo.toml` with workspace dependency
2. Create `demos/interaction/src/main.rs` with validation UI
3. Implement `pin_types.rs` with PinType enum and compatibility logic
4. Implement `validation.rs` with direction and connection rules
5. Implement `feedback.rs` with visual and message feedback system
6. Create comprehensive set of test nodes demonstrating all rules
7. Ensure visual feedback is clear and immediate
8. Test all validation paths (valid, type mismatch, direction error, etc.)
9. Add rules reference panel for user education

## Implementation Notes

- Pin validation should happen in real-time during drag operations
- Visual feedback must be clear and unambiguous
- Error messages should be specific and educational
- The demo should serve as a reference implementation for node graph editors
- All validation logic should be reusable in real applications
