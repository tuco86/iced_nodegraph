# Node Style Guide

Design guidelines for creating consistent, professional nodes in iced_nodegraph.

## 1. Color System

### Semantic Pin Colors

Pin colors communicate data types at a glance. Use consistent colors across all nodes.

```rust
use iced::Color;

// Data type colors
pub const PIN_NUMBER:   Color = Color::from_rgb(0.20, 0.83, 0.60);  // Emerald #34D399
pub const PIN_STRING:   Color = Color::from_rgb(0.98, 0.75, 0.14);  // Amber   #FBBF24
pub const PIN_BOOL:     Color = Color::from_rgb(0.98, 0.57, 0.24);  // Orange  #FB923C
pub const PIN_COLOR:    Color = Color::from_rgb(0.96, 0.45, 0.71);  // Pink    #F472B6
pub const PIN_DATETIME: Color = Color::from_rgb(0.65, 0.55, 0.98);  // Violet  #A78BFA
pub const PIN_EMAIL:    Color = Color::from_rgb(0.22, 0.74, 0.97);  // Sky     #38BDF8
pub const PIN_ANY:      Color = Color::from_rgb(0.58, 0.64, 0.72);  // Slate   #94A3B8
pub const PIN_CONFIG:   Color = Color::from_rgb(0.13, 0.83, 0.93);  // Cyan    #22D3EE
```

### Surface Colors

```rust
pub const SURFACE_ELEVATED: Color = Color::from_rgb(0.165, 0.165, 0.235);  // #2A2A3C
pub const BORDER_SUBTLE:    Color = Color::from_rgb(0.227, 0.227, 0.298);  // #3A3A4C
pub const TEXT_PRIMARY:     Color = Color::from_rgb(0.894, 0.894, 0.906);  // #E4E4E7
pub const TEXT_MUTED:       Color = Color::from_rgb(0.631, 0.631, 0.667);  // #A1A1AA
```

### Theme Integration

Use `NodeContentStyle` from the theme for dynamic colors:

```rust
let style = NodeContentStyle::from_theme(theme);
// style.title_background, style.body_background, style.title_text, etc.
```

---

## 2. Typography

### Font Size Hierarchy

| Element       | Size  | Usage                          |
|---------------|-------|--------------------------------|
| Title         | 13px  | Node header text               |
| Label         | 11px  | Pin labels, field names        |
| Value         | 10px  | Slider values, small text      |
| Meta          | 9px   | Tooltips, secondary info       |

### Text Colors

- **Primary**: Main content, important labels
- **Muted**: Secondary labels, hints, less important info
- **Accent**: Highlights, active states (use pin colors)

---

## 3. Spacing & Padding

### Constants

```rust
pub const SPACING_PIN: f32 = 6.0;        // Between pin rows
pub const SPACING_ROW: f32 = 8.0;        // Between content rows
pub const PADDING_CONTENT: [u16; 2] = [10, 12];  // [vertical, horizontal]
pub const PADDING_TITLE: [u16; 2] = [4, 8];      // Compact title padding
```

### Guidelines

- Minimum spacing between interactive elements: 6px
- Generous horizontal padding (12px) for edge clearance
- Consistent vertical rhythm with 6-8px spacing

---

## 4. Node Dimensions

### Width Guidelines

| Node Type       | Width    | Notes                         |
|-----------------|----------|-------------------------------|
| Simple (1 pin)  | 120-150  | Minimal nodes                 |
| Standard        | 160-200  | Most nodes                    |
| Complex         | 200-250  | Multiple controls             |
| Wide            | 250-320  | Multi-column layouts          |

### Height

Always auto (content-driven). Never set fixed heights.

---

## 5. Border & Corner Radius

```rust
pub const CORNER_RADIUS_NODE: f32 = 6.0;     // Node container
pub const CORNER_RADIUS_CONTROL: f32 = 4.0;  // Buttons, inputs
pub const CORNER_RADIUS_PILL: f32 = 10.0;    // Pill buttons
pub const CORNER_RADIUS_SWATCH: f32 = 2.0;   // Color swatches
```

---

## 6. Shadows

Use shadows sparingly. Available via `ShadowConfig`:

```rust
// Subtle - for depth
ShadowConfig::subtle()  // offset: (2, 2), blur: 4, color: 0.3 opacity

// Glow - for selection/hover
ShadowConfig::glow(color)  // offset: (0, 0), blur: 8, spread: 2
```

---

## 7. Pin Layout & Design

### Pin Mechanism

The `pin!` macro creates an invisible container that marks an anchor point:
- `Left` pins anchor to the left edge of the node
- `Right` pins anchor to the right edge of the node
- Position is calculated from the NodePin widget bounds

### Pattern 1: Horizontal Pairing

Place input and output on the same row to save vertical space:

```
┌─────────────────────────┐
│  Add                    │
├─────────────────────────┤
│ ○ a              sum ○  │  ← Input + Output on same row
│ ○ b                     │
└─────────────────────────┘
```

```rust
column![
    row![
        pin!(Left, text("a"), Input, "float", PIN_NUMBER),
        container(()).width(Fill),  // spacer
        pin!(Right, text("sum"), Output, "float", PIN_NUMBER),
    ],
    pin!(Left, text("b"), Input, "float", PIN_NUMBER),
]
```

### Pattern 2: Control-Wrapping Pins

Wrap controls with pins instead of separate rows:

```
┌─────────────────────────┐
│  Float Slider           │
├─────────────────────────┤
│ [━━━━━●━━━] 5.2 ○       │  ← Pin wraps slider row
└─────────────────────────┘
```

```rust
row![
    slider_widget,
    pin!(Right, value_display, Output, "float", PIN_NUMBER)
]
```

AVOID separate pin rows:
```
│   [━━━━━●━━━━━] 5.2     │  ← Slider
│              value ○    │  ← BAD: separate row with generic label
```

### Pattern 3: Label Guidelines

**AVOID generic labels:**
- `value`, `output`, `result`, `input`, `data`

**PREFER meaningful labels:**
- `sum`, `product`, `color`, `texture`, `a`, `b`, `min`, `max`

**OMIT when obvious:**
- When a control makes the purpose clear (slider = number output)
- When there's only one pin of that type

### Pattern 4: Pass-through Nodes

Symmetric input/output on first row:

```
┌─────────────────────────┐
│  Clamp                  │
├─────────────────────────┤
│ ○ in             out ○  │  ← Symmetric I/O
│   min [━━●━━] 0.0       │
│   max [━━━━●] 1.0       │
└─────────────────────────┘
```

### Pin Shapes

| Shape    | Usage              | When to use                    |
|----------|--------------------|--------------------------------|
| Circle   | Data flow (default)| Numbers, strings, colors, etc. |
| Diamond  | Control flow       | Triggers, events               |
| Square   | Arrays/Collections | Lists, buffers                 |
| Triangle | References         | Pointers, handles              |

---

## 8. Animation & Edge Styling

### Edge Types

| Type    | Usage                           |
|---------|---------------------------------|
| Bezier  | Default - smooth data flow      |
| Step    | Control flow, state machines    |
| Line    | Simple connections              |

### Flow Animation

Use sparingly for active connections. Default speed: 1.0

---

## 9. Node Type Categories

Visual hierarchy through consistent title bar colors:

| Category | Accent Color | Example Nodes             |
|----------|--------------|---------------------------|
| Input    | Blue tint    | Sliders, pickers, triggers|
| Process  | Green tint   | Math, filters, transforms |
| Output   | Orange tint  | Display, export, actions  |
| Config   | Cyan tint    | Settings, parameters      |
| Comment  | Gray         | Notes, documentation      |

Use `NodeContentStyle::input(theme)`, `.process(theme)`, etc.

---

## 10. Code Patterns

### Node Builder Function

Standard signature:

```rust
pub fn my_node<'a, Message>(
    theme: &'a iced::Theme,
    // node-specific params...
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::from_theme(theme);
    // or: NodeContentStyle::input(theme)

    column![
        node_header(title, style),
        container(content).padding(PADDING_CONTENT),
        // optional: node_footer(...)
    ]
    .width(180.0)
    .into()
}
```

### Colors Module

Create a `colors` module in your demo:

```rust
pub mod colors {
    use iced::Color;

    // Pin colors
    pub const PIN_NUMBER: Color = Color::from_rgb(0.20, 0.83, 0.60);
    pub const PIN_STRING: Color = Color::from_rgb(0.98, 0.75, 0.14);
    // ... etc

    // Surface colors
    pub const SURFACE_ELEVATED: Color = Color::from_rgb(0.165, 0.165, 0.235);
    pub const TEXT_MUTED: Color = Color::from_rgb(0.631, 0.631, 0.667);
}
```

### Naming Conventions

- `PIN_*` for pin type colors
- `SURFACE_*` for backgrounds
- `TEXT_*` for text colors
- `BORDER_*` for borders
- `SPACING_*` for spacing values
- `PADDING_*` for padding arrays

---

## Quick Reference

```
Node Structure:
┌─────────────────────────┐
│  Title Bar (13px)       │ ← node_header()
├─────────────────────────┤
│  ○ pin   [control] ○    │ ← padding: [10, 12]
│  ○ pin                  │ ← spacing: 6px
│     value: 0.5          │
├─────────────────────────┤
│  [ Footer ]             │ ← node_footer() (optional)
└─────────────────────────┘
     │                │
     └── 160-200px ───┘
```

Pin Colors Quick Chart:
```
Number   ████ Emerald
String   ████ Amber
Bool     ████ Orange
Color    ████ Pink
DateTime ████ Violet
Email    ████ Sky
Any      ████ Slate
Config   ████ Cyan
```
