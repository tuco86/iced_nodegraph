//! Pin label constants and marker types for type-safe connections.
//!
//! Each pin in a node has a unique label (String) that identifies it within that node.
//! These constants ensure consistent naming across the codebase.
//!
//! Marker types are used with `pin!(...).data_type::<MarkerType>()` to enable
//! automatic type-based connection validation.
//!
//! The vocabulary is complete for the demo graph. Part of it is read only by
//! the native entry point and the persistence codec, so a wasm build - where
//! the gallery boots the app directly and neither exists - has no reader for
//! those labels.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

// =============================================================================
// Marker Types for Connection Matching
// =============================================================================
// These are zero-sized types used with TypeId for automatic connection matching.
// Only pins with the same marker type can connect to each other.

/// Marker type for floating point number pins
pub struct Float;

/// Marker type for integer number pins
pub struct Int;

/// Marker type for boolean pins
pub struct Bool;

/// Marker type for color pins
pub struct ColorData;

/// Marker type for string pins
pub struct StringData;

/// Marker type for datetime pins
pub struct DateTime;

/// Marker type for email data pins
pub struct Email;

/// Marker type for edge curve selector pins
pub struct EdgeCurveData;

/// Marker type for pin shape selector pins
pub struct PinShapeData;

/// Marker type for pattern type selector pins
pub struct PatternTypeData;

/// Marker type for 2D vector pins (e.g. shadow offset)
pub struct Vec2Data;

/// Marker type for node configuration bundle pins
pub struct NodeConfigData;

/// Marker type for edge configuration bundle pins
pub struct EdgeConfigData;

/// Marker type for pin configuration bundle pins
pub struct PinConfigData;

/// Marker type for graph (canvas) configuration bundle pins
pub struct GraphConfigData;

/// Marker type for anchor configuration bundle pins
pub struct AnchorConfigData;

/// Marker type for selection-box configuration bundle pins
pub struct SelectionBoxConfigData;

/// Marker type for cutting-tool configuration bundle pins
pub struct CuttingToolConfigData;

/// Marker type for minimap configuration bundle pins
pub struct MinimapConfigData;

/// Marker type for tiling-kind selector pins (grid/dots/triangles/hex)
pub struct TilingKindData;

// =============================================================================
// Pin Label Constants
// =============================================================================

/// Pin labels for workflow nodes (email processing pipeline).
pub mod workflow {
    /// Email data output pin (email_trigger node)
    pub const ON_EMAIL: &str = "on email";

    /// Email data input pin (email_parser node)
    pub const EMAIL: &str = "email";

    /// Subject output pin (email_parser node)
    pub const SUBJECT: &str = "subject";

    /// DateTime output pin (email_parser node)
    pub const DATETIME: &str = "datetime";

    /// Body text output pin (email_parser node)
    pub const BODY: &str = "body";

    /// Generic input pin (filter node)
    pub const INPUT: &str = "input";

    /// Matches output pin (filter node)
    pub const MATCHES: &str = "matches";

    /// Title input pin (calendar node)
    pub const TITLE: &str = "title";

    /// Description input pin (calendar node)
    pub const DESCRIPTION: &str = "description";
}

/// Pin labels for input/value nodes.
pub mod input {
    /// Generic value output pin (sliders, toggles)
    pub const VALUE: &str = "value";

    /// Color output pin (color picker/preset)
    pub const COLOR: &str = "color";
}

/// Shared plumbing pins for configuration node chains: the inheritance
/// passthrough, the typed config outputs, and the sink inputs. Per-field pins
/// live in the per-class submodules ([`node`], [`pin`], [`edge`], [`graph`],
/// [`anchor`], [`selection_box`], [`cutting_tool`], [`minimap`]).
pub mod cfg {
    /// Config passthrough input pin (inherit from a parent config node)
    pub const CONFIG: &str = "config";

    // === Typed Config Output Pins ===

    /// NodeConfig output pin
    pub const NODE_OUT: &str = "node_config";

    /// EdgeConfig output pin
    pub const EDGE_OUT: &str = "edge_config";

    /// PinConfig output pin
    pub const PIN_OUT: &str = "pin_config";

    /// GraphConfig output pin
    pub const GRAPH_OUT: &str = "graph_config";

    /// AnchorConfig output pin
    pub const ANCHOR_OUT: &str = "anchor_config";

    /// SelectionBoxConfig output pin
    pub const SELECTION_BOX_OUT: &str = "selection_box_config";

    /// CuttingToolConfig output pin
    pub const CUTTING_TOOL_OUT: &str = "cutting_tool_config";

    /// MinimapConfig output pin
    pub const MINIMAP_OUT: &str = "minimap_config";

    // === Catalog Inputs: one per class and status ===

    /// Idle node class (Catalog); also the Node Class node's single input
    pub const NODE_CONFIG: &str = "node";
    /// Selected node class
    pub const NODE_SELECTED: &str = "node:selected";
    /// Idle pin class
    pub const PIN_CONFIG: &str = "pin";
    /// Pin class while a drag could connect to it
    pub const PIN_VALID_TARGET: &str = "pin:valid_target";
    /// Idle edge class
    pub const EDGE_CONFIG: &str = "edge";
    /// Edge class while the cutting tool crosses it
    pub const EDGE_PENDING_CUT: &str = "edge:pending_cut";
    /// The edge being dragged out of a pin (an `EdgeStyle`)
    pub const DRAG_EDGE: &str = "drag_edge";
    /// Idle anchor class
    pub const ANCHOR: &str = "anchor";
    /// Hovered anchor class
    pub const ANCHOR_HOVERED: &str = "anchor:hovered";
    /// Anchor class while a route drag could attach to it
    pub const ANCHOR_VALID_TARGET: &str = "anchor:valid_target";
    /// Canvas class
    pub const GRAPH_CONFIG: &str = "graph";
    /// Selection box class
    pub const SELECTION_BOX: &str = "selection_box";
    /// Cutting tool class
    pub const CUTTING_TOOL: &str = "cutting_tool";
    /// Minimap class
    pub const MINIMAP: &str = "minimap";

    /// Every Catalog input, in row order.
    pub const CATALOG_INPUTS: [&str; 14] = [
        NODE_CONFIG,
        NODE_SELECTED,
        PIN_CONFIG,
        PIN_VALID_TARGET,
        EDGE_CONFIG,
        EDGE_PENDING_CUT,
        DRAG_EDGE,
        ANCHOR,
        ANCHOR_HOVERED,
        ANCHOR_VALID_TARGET,
        GRAPH_CONFIG,
        SELECTION_BOX,
        CUTTING_TOOL,
        MINIMAP,
    ];
}

/// NodeConfig field pins, mirroring [`iced_nodegraph::NodeStyle`]. The `border`
/// width feeds the border `Pattern` thickness; the `pattern` group shapes the
/// same border stroke (dash/gap/angle/flow). All color pins carry a `ColorQuad`
/// (a solid `Color` coerces in).
pub mod node {
    // === Fill ===

    /// Fill color input pin
    pub const FILL_COLOR: &str = "fill_color";

    /// Corner radius input pin
    pub const CORNER_RADIUS: &str = "corner_radius";

    /// Opacity input pin
    pub const OPACITY: &str = "opacity";

    // === Border ===

    /// Border color input pin
    pub const BORDER_COLOR: &str = "border_color";

    /// Border width input pin (border pattern thickness)
    pub const BORDER_WIDTH: &str = "border_width";

    /// Border outline width input pin
    pub const BORDER_OUTLINE_WIDTH: &str = "border_outline_width";

    /// Border outline color input pin
    pub const BORDER_OUTLINE_COLOR: &str = "border_outline_color";

    // === Border Pattern ===

    /// Pattern type input pin
    pub const PATTERN: &str = "pattern";

    /// Dash length input pin
    pub const DASH: &str = "dash";

    /// Gap length input pin
    pub const GAP: &str = "gap";

    /// Angle input pin
    pub const ANGLE: &str = "angle";

    /// Animation speed input pin (0 = off, > 0 = animated)
    pub const SPEED: &str = "speed";

    // === Shadow ===

    /// Shadow color input pin
    pub const SHADOW_COLOR: &str = "shadow_color";

    /// Shadow distance (blur half-width) input pin
    pub const SHADOW_DISTANCE: &str = "shadow_distance";

    /// Shadow offset input pin (2D vector)
    pub const SHADOW_OFFSET: &str = "shadow_offset";
}

/// GraphConfig field pins, mirroring [`iced_nodegraph::GraphStyle`] and its
/// optional [`iced_nodegraph::TilingBackground`]. The `tiling_kind` pin selects
/// the repeating pattern; `spacing`/`thickness`/`line_color` shape it. Both color
/// pins carry a `ColorQuad` (its near corner is taken, since the canvas fields are
/// plain `Color`).
pub mod graph {
    /// Canvas background color input pin
    pub const BACKGROUND: &str = "background";

    /// Tiling pattern kind input pin (grid/dots/triangles/hex)
    pub const TILING_KIND: &str = "tiling_kind";

    /// Tiling cell spacing input pin (world units)
    pub const SPACING: &str = "spacing";

    /// Tiling line thickness / dot radius input pin (world units)
    pub const THICKNESS: &str = "thickness";

    /// Tiling pattern color input pin
    pub const LINE_COLOR: &str = "line_color";
}

/// PinConfig field pins, mirroring [`iced_nodegraph::PinStyle`].
pub mod pin {
    /// Indicator color input pin
    pub const COLOR: &str = "color";

    /// Indicator radius input pin
    pub const RADIUS: &str = "radius";

    /// Body-cutout radius input pin
    pub const CUTOUT_RADIUS: &str = "cutout_radius";

    /// Indicator shape input pin
    pub const SHAPE: &str = "shape";

    /// Border color input pin
    pub const BORDER_COLOR: &str = "border_color";

    /// Border width input pin
    pub const BORDER_WIDTH: &str = "border_width";
}

/// AnchorConfig field pins, mirroring [`iced_nodegraph::AnchorStyle`].
pub mod anchor {
    /// Core side length input pin
    pub const CORE_SIZE: &str = "core_size";
    /// Core corner radius input pin
    pub const CORE_RADIUS: &str = "core_radius";
    /// Core fill color input pin
    pub const CORE_COLOR: &str = "core_color";
    /// Core border color input pin
    pub const CORE_BORDER_COLOR: &str = "core_border_color";
    /// Core border width input pin
    pub const CORE_BORDER_WIDTH: &str = "core_border_width";
    /// Radius of orbit 0 input pin
    pub const ORBIT_OFFSET: &str = "orbit_offset";
    /// Radius step per orbit input pin
    pub const ORBIT_SPACING: &str = "orbit_spacing";
    /// Orbit ring color input pin
    pub const RING_COLOR: &str = "ring_color";
    /// Orbit ring width input pin
    pub const RING_WIDTH: &str = "ring_width";
}

/// SelectionBoxConfig field pins, mirroring [`iced_nodegraph::SelectionBoxStyle`].
pub mod selection_box {
    /// Fill color input pin
    pub const FILL: &str = "fill";
    /// Border color input pin
    pub const BORDER_COLOR: &str = "border_color";
    /// Border width input pin
    pub const BORDER_WIDTH: &str = "border_width";
}

/// CuttingToolConfig field pins, mirroring [`iced_nodegraph::CuttingToolStyle`].
pub mod cutting_tool {
    /// Trail color input pin
    pub const COLOR: &str = "color";
    /// Trail width input pin
    pub const WIDTH: &str = "width";
}

/// MinimapConfig field pins, mirroring [`iced_nodegraph::MinimapStyle`].
pub mod minimap {
    /// Map background input pin
    pub const BACKGROUND: &str = "background";
    /// Map border color input pin
    pub const BORDER_COLOR: &str = "border_color";
    /// Map border width input pin
    pub const BORDER_WIDTH: &str = "border_width";
    /// Node mark color input pin
    pub const NODE_COLOR: &str = "node_color";
    /// Selected node mark color input pin
    pub const SELECTED_NODE_COLOR: &str = "selected_node_color";
    /// Viewport rectangle fill input pin
    pub const VIEWPORT_FILL: &str = "viewport_fill";
    /// Viewport rectangle border color input pin
    pub const VIEWPORT_BORDER_COLOR: &str = "viewport_border_color";
    /// Viewport rectangle border width input pin
    pub const VIEWPORT_BORDER_WIDTH: &str = "viewport_border_width";
}

/// EdgeConfig field pins, mirroring [`iced_nodegraph::EdgeStyle`]. Each color is
/// a single `ColorQuad` pin (the start/end gradient is encoded in the quad, so
/// there is no separate "end" pin).
pub mod edge {
    // === Stroke ===

    /// Stroke color input pin (arc gradient start -> end encoded in the quad)
    pub const STROKE_COLOR: &str = "stroke_color";

    /// Thickness input pin
    pub const THICKNESS: &str = "thickness";

    /// Curve type input pin
    pub const CURVE: &str = "curve";

    /// Stroke outline width input pin
    pub const STROKE_OUTLINE_WIDTH: &str = "stroke_outline_width";

    /// Stroke outline color input pin
    pub const STROKE_OUTLINE_COLOR: &str = "stroke_outline_color";

    // === Pattern ===

    /// Pattern type input pin
    pub const PATTERN: &str = "pattern";

    /// Dash length input pin
    pub const DASH: &str = "dash";

    /// Gap length input pin
    pub const GAP: &str = "gap";

    /// Dot radius input pin (dotted pattern)
    pub const DOT_RADIUS: &str = "dot_radius";

    /// Angle input pin
    pub const ANGLE: &str = "angle";

    /// Animation speed input pin (0 = off, > 0 = animated)
    pub const SPEED: &str = "speed";

    // === Border ===

    /// Border width input pin
    pub const BORDER_WIDTH: &str = "border_width";

    /// Border gap input pin
    pub const BORDER_GAP: &str = "border_gap";

    /// Border color input pin
    pub const BORDER_COLOR: &str = "border_color";

    /// Border background color input pin
    pub const BORDER_BACKGROUND: &str = "border_background";

    /// Border outline width input pin
    pub const BORDER_OUTLINE_WIDTH: &str = "border_outline_width";

    /// Border outline color input pin
    pub const BORDER_OUTLINE_COLOR: &str = "border_outline_color";

    // === Shadow ===

    /// Shadow blur input pin
    pub const SHADOW_BLUR: &str = "shadow_blur";

    /// Shadow expand input pin
    pub const SHADOW_EXPAND: &str = "shadow_expand";

    /// Shadow color input pin
    pub const SHADOW_COLOR: &str = "shadow_color";

    /// Shadow offset input pin (2D vector)
    pub const SHADOW_OFFSET: &str = "shadow_offset";
}

/// Builder node pins: combine primitive inputs into a `ColorQuad` or a 2D
/// vector that feeds the single-pin color/offset inputs above.
pub mod build {
    // === ColorQuad builder (4 corners -> 1 quad) ===

    /// Near-start corner color input
    pub const NEAR_START: &str = "near_start";

    /// Near-end corner color input
    pub const NEAR_END: &str = "near_end";

    /// Far-start corner color input
    pub const FAR_START: &str = "far_start";

    /// Far-end corner color input
    pub const FAR_END: &str = "far_end";

    /// ColorQuad output
    pub const QUAD_OUT: &str = "quad";

    // === Vec2 builder (x, y -> vec2) ===

    /// X component input
    pub const X: &str = "x";

    /// Y component input
    pub const Y: &str = "y";

    /// Vec2 output
    pub const VEC2_OUT: &str = "vec2";

    // === Alpha builder (color, alpha -> color) ===

    /// Color input whose alpha is replaced
    pub const ALPHA_COLOR: &str = "alpha_color";

    /// Alpha input (0..1)
    pub const ALPHA: &str = "alpha";

    /// Color output
    pub const ALPHA_OUT: &str = "alpha_out";
}

/// Output pins of the Theme node: the active theme's basic [`iced::theme::Palette`]
/// as colors (the six flat palette entries, no weak/strong steps). The graded
/// variants live on the Theme Extended node, see [`theme_ext`].
pub mod theme {
    /// Background color
    pub const BACKGROUND: &str = "background";

    /// Text color
    pub const TEXT: &str = "text";

    /// Primary color
    pub const PRIMARY: &str = "primary";

    /// Success color
    pub const SUCCESS: &str = "success";

    /// Warning color
    pub const WARNING: &str = "warning";

    /// Danger color
    pub const DANGER: &str = "danger";
}

/// Output pins of the Theme Extended node: the active theme's
/// [`extended_palette`](iced::Theme::extended_palette) as colors. Each accent
/// group exposes its `base`/`weak`/`strong` step. Labels are distinct from the
/// basic [`theme`] module so the two nodes never share a pin id.
pub mod theme_ext {
    /// Base background color
    pub const BACKGROUND_BASE: &str = "background_base";
    /// Weak background variant
    pub const BACKGROUND_WEAK: &str = "background_weak";
    /// Strong background variant
    pub const BACKGROUND_STRONG: &str = "background_strong";

    /// Base primary color
    pub const PRIMARY_BASE: &str = "primary_base";
    /// Weak primary variant
    pub const PRIMARY_WEAK: &str = "primary_weak";
    /// Strong primary variant
    pub const PRIMARY_STRONG: &str = "primary_strong";

    /// Base secondary color
    pub const SECONDARY_BASE: &str = "secondary_base";
    /// Weak secondary variant
    pub const SECONDARY_WEAK: &str = "secondary_weak";
    /// Strong secondary variant
    pub const SECONDARY_STRONG: &str = "secondary_strong";

    /// Base success color
    pub const SUCCESS_BASE: &str = "success_base";
    /// Weak success variant
    pub const SUCCESS_WEAK: &str = "success_weak";
    /// Strong success variant
    pub const SUCCESS_STRONG: &str = "success_strong";

    /// Base warning color
    pub const WARNING_BASE: &str = "warning_base";
    /// Weak warning variant
    pub const WARNING_WEAK: &str = "warning_weak";
    /// Strong warning variant
    pub const WARNING_STRONG: &str = "warning_strong";

    /// Base danger color
    pub const DANGER_BASE: &str = "danger_base";
    /// Weak danger variant
    pub const DANGER_WEAK: &str = "danger_weak";
    /// Strong danger variant
    pub const DANGER_STRONG: &str = "danger_strong";
}

/// Pin labels for math nodes.
pub mod math {
    /// First input operand
    pub const A: &str = "A";

    /// Second input operand
    pub const B: &str = "B";

    /// Result output (uses the operation symbol as display, but label is "result")
    pub const RESULT: &str = "result";
}
