# Demo: interaction

Connection validation with typed, directional pins.

This demo shows how to enforce connection rules in `iced_nodegraph`: directional
data flow, type compatibility, single-connection constraints, duplicate and
self-loop rejection, and live snap feedback while dragging an edge.

The whole application lives in `src/lib.rs` (the native `main.rs` and the WASM
entry point both call into it).

## Features

- **Directional pins**: input-only (left), output-only (right), and
  bidirectional (top/bottom). Input-to-input and output-to-output are rejected.
- **Typed pins**: Integer, Float, String, Boolean, and Any. `Any` is compatible
  with everything; Integer and Float convert implicitly.
- **Single-connection constraints**: some pins accept only one edge.
- **Duplicate rejection**: the same pair cannot be connected twice.
- **Self-loop rejection**: a pin cannot be wired to itself.
- **Live snap feedback**: `can_connect` validates direction, type, and self-loops
  during the drag so the edge only snaps to compatible targets. Full validation
  (including single-connection and duplicate rules) runs on release and is
  reported in the feedback log.
- **Selection and group move**: nodes can be selected and dragged as a group.
- **Feedback log and rules panel**: every accepted or rejected attempt is logged;
  toggle the rules reference with "Show Rules".

## Demo Graph

| Node | Pins |
|------|------|
| 0 Number Generator | Integer out, Float out |
| 1 Math Operations | Float in A (single), Float in B (single), Float out |
| 2 Type Converter | Any in, Integer out, Float out, String out |
| 3 Display | Any in, String in |
| 4 Bidirectional Hub | Float, Integer, Any, String (all bidirectional) |

## Controls

- **Drag from a pin** to a compatible pin to create a connection.
- **Drag a node** (or a selection) to move it.
- **Scroll** to zoom, **middle-drag** to pan.
- **Clear All** removes every connection; **Reset** restores the initial graph;
  **Show Rules** toggles the rules reference.

## Connection Rules

1. Output and bidirectional pins may send; input and bidirectional pins may receive.
2. Types must be compatible (same type, or `Any`, or Integer to/from Float).
3. Single-connection pins reject a second edge.
4. Duplicate connections are rejected.
5. A pin cannot connect to itself.

## Running

```bash
cargo run -p demo_interaction
```

## Implementation Notes

- `validate_connection` returns `Result<String, String>` so accepted and
  rejected attempts produce a human-readable feedback message.
- `can_connect` is the lighter, state-independent predicate used for the live
  snap; it mirrors the direction, type, and self-loop checks.
- The validation logic is plain data over a `pin_registry` map and is easy to
  lift into a real application.
