//! Pin label constants and marker types for type-safe connections.
//!
//! Each pin in a node has a unique label (String) that identifies it within that node.
//! These constants ensure consistent naming across the codebase.
//!
//! Marker types are used with `pin!(...).data_type::<MarkerType>()` to enable
//! automatic type-based connection validation.

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
/// passthrough, the typed config outputs, and the apply-node inputs. Per-field
/// pins live in the per-target [`node`], [`pin`], and [`edge`] submodules.
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

    // === Apply Node Inputs ===

    /// Node config input pin (apply nodes)
    pub const NODE_CONFIG: &str = "node";

    /// Edge config input pin (apply nodes)
    pub const EDGE_CONFIG: &str = "edge";

    /// Pin config input pin (apply nodes)
    pub const PIN_CONFIG: &str = "pin";

    /// Toggle on/off input pin
    pub const ON: &str = "on";

    /// Target ID input pin (apply to node)
    pub const TARGET: &str = "target";
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

/// PinConfig field pins, mirroring [`iced_nodegraph::PinStyle`].
pub mod pin {
    /// Indicator color input pin
    pub const COLOR: &str = "color";

    /// Indicator radius input pin
    pub const RADIUS: &str = "radius";

    /// Indicator shape input pin
    pub const SHAPE: &str = "shape";

    /// Border color input pin
    pub const BORDER_COLOR: &str = "border_color";

    /// Border width input pin
    pub const BORDER_WIDTH: &str = "border_width";
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
