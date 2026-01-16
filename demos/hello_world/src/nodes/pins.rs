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

/// Marker type for node configuration bundle pins
pub struct NodeConfigData;

/// Marker type for edge configuration bundle pins
pub struct EdgeConfigData;

/// Marker type for pin configuration bundle pins
pub struct PinConfigData;

/// Marker type for shadow configuration bundle pins
pub struct ShadowConfigData;

/// Marker type for background configuration bundle pins
pub struct BackgroundConfigData;

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

/// Pin labels for configuration nodes.
pub mod config {
    /// Config passthrough pin (input and output)
    pub const CONFIG: &str = "config";

    // === Config Output Pins (typed) ===

    /// NodeConfig output pin
    pub const NODE_OUT: &str = "node_cfg";

    /// EdgeConfig output pin
    pub const EDGE_OUT: &str = "edge_cfg";

    /// PinConfig output pin
    pub const PIN_OUT: &str = "pin_cfg";

    /// ShadowConfig output pin
    pub const SHADOW_OUT: &str = "shadow_cfg";

    // === Edge Config Pins ===

    /// Start color input pin
    pub const START: &str = "start";

    /// End color input pin
    pub const END: &str = "end";

    /// Thickness input pin
    pub const THICK: &str = "thick";

    /// Curve type input pin
    pub const CURVE: &str = "curve";

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

    // === Border Config Pins ===

    /// Border toggle input pin
    pub const BORDER: &str = "border";

    /// Border width input pin
    pub const BORDER_WIDTH: &str = "b.width";

    /// Border gap input pin
    pub const BORDER_GAP: &str = "b.gap";

    /// Border start color input pin
    pub const BORDER_START_COLOR: &str = "b.start";

    /// Border end color input pin
    pub const BORDER_END_COLOR: &str = "b.end";

    // === Outline Config Pins ===

    /// Inner outline toggle
    pub const INNER_OUTLINE: &str = "in.ol";

    /// Inner outline width
    pub const INNER_OUTLINE_WIDTH: &str = "in.w";

    /// Inner outline color
    pub const INNER_OUTLINE_COLOR: &str = "in.c";

    /// Outer outline toggle
    pub const OUTER_OUTLINE: &str = "out.ol";

    /// Outer outline width
    pub const OUTER_OUTLINE_WIDTH: &str = "out.w";

    /// Outer outline color
    pub const OUTER_OUTLINE_COLOR: &str = "out.c";

    // === Shadow Config Pins ===

    /// Shadow toggle input pin
    pub const SHADOW: &str = "shadow";

    /// Shadow blur input pin
    pub const SHADOW_BLUR: &str = "s.blur";

    /// Shadow offset input pin (combined, sets both x and y)
    pub const SHADOW_OFFSET: &str = "s.offs";

    /// Shadow offset X input pin (for ShadowConfig node)
    pub const SHADOW_OFFSET_X: &str = "off_x";

    /// Shadow offset Y input pin (for ShadowConfig node)
    pub const SHADOW_OFFSET_Y: &str = "off_y";

    /// Shadow color input pin
    pub const SHADOW_COLOR: &str = "s.color";

    // === Node Config Pins ===

    /// Background color input pin
    pub const BG_COLOR: &str = "bg";

    /// Border radius input pin
    pub const RADIUS: &str = "radius";

    /// Border width input pin (node)
    pub const WIDTH: &str = "width";

    /// Border color input pin (node)
    pub const COLOR: &str = "color";

    /// Opacity input pin
    pub const OPACITY: &str = "opacity";

    // === Pin Config Pins ===

    /// Pin size input pin
    pub const SIZE: &str = "size";

    /// Pin shape input pin
    pub const SHAPE: &str = "shape";

    /// Pin glow input pin
    pub const GLOW: &str = "glow";

    /// Pin pulse input pin
    pub const PULSE: &str = "pulse";

    // === Apply Nodes ===

    /// Node config input pin (apply nodes)
    pub const NODE_CONFIG: &str = "node";

    /// Edge config input pin (apply nodes)
    pub const EDGE_CONFIG: &str = "edge";

    /// Pin config input pin (apply nodes)
    pub const PIN_CONFIG: &str = "pin";

    /// Background config input pin (apply nodes)
    pub const BACKGROUND_CONFIG: &str = "background";

    /// Background config output pin
    pub const BACKGROUND_OUT: &str = "bg_out";

    /// Background color input pin
    pub const BACKGROUND_COLOR: &str = "bg_color";

    /// Primary pattern color input pin
    pub const PRIMARY_COLOR: &str = "primary";

    /// Minor spacing input pin
    pub const MINOR_SPACING: &str = "minor_spacing";

    /// Adaptive zoom toggle input pin
    pub const ADAPTIVE_ZOOM: &str = "adaptive";

    /// Toggle on/off input pin
    pub const ON: &str = "on";

    /// Target ID input pin (apply to node)
    pub const TARGET: &str = "target";
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
