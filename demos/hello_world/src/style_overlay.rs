//! Partial style overlays for the live config editor.
//!
//! `iced_nodegraph` styles are concrete structs: the renderer consumes a fully
//! populated `NodeStyle`/`EdgeStyle`/`PinStyle`. This demo, however, builds a
//! style up from individual pin inputs and layers config nodes over one another,
//! so it needs overrides as *composable data* (each field optional, `None` =
//! inherit) before a theme base exists to resolve against. These overlay structs
//! provide exactly that: builder setters, `merge` (self wins, fills the rest
//! from another overlay), and `resolve_over` (apply the set fields onto a
//! concrete base). They mirror the library style structs field-for-field where
//! the editor exposes them, and there is one overlay per `Catalog` class so the
//! config-node rig can drive every style the widget resolves.

use iced::Color;
use iced_nodegraph::{
    AnchorStyle, ColorQuad, CuttingToolStyle, EdgeCurve, EdgeStyle, GraphStyle, MinimapStyle,
    NodeStyle, Pattern, PinShape, PinStyle, SelectionBoxStyle, TilingBackground, TilingKind,
};

/// Overlay over [`NodeStyle`]: mirrors every field, since the node-config editor
/// exposes them all. Shadow color is a plain [`Color`] (not a [`ColorQuad`]),
/// matching `NodeStyle::shadow_color`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeOverlay {
    pub fill_color: Option<ColorQuad>,
    pub corner_radius: Option<f32>,
    pub opacity: Option<f32>,
    pub border_color: Option<ColorQuad>,
    pub border_pattern: Option<Pattern>,
    pub border_outline_width: Option<f32>,
    pub border_outline_color: Option<ColorQuad>,
    pub shadow_color: Option<Color>,
    pub shadow_distance: Option<f32>,
    pub shadow_offset: Option<(f32, f32)>,
}

impl NodeOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fill_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.fill_color = Some(v.into());
        self
    }
    pub fn corner_radius(mut self, v: f32) -> Self {
        self.corner_radius = Some(v);
        self
    }
    pub fn opacity(mut self, v: f32) -> Self {
        self.opacity = Some(v);
        self
    }
    pub fn border_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_color = Some(v.into());
        self
    }
    pub fn border_pattern(mut self, v: impl Into<Pattern>) -> Self {
        self.border_pattern = Some(v.into());
        self
    }
    pub fn border_outline_width(mut self, v: f32) -> Self {
        self.border_outline_width = Some(v);
        self
    }
    pub fn border_outline_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_outline_color = Some(v.into());
        self
    }
    pub fn shadow_color(mut self, v: impl Into<Color>) -> Self {
        self.shadow_color = Some(v.into());
        self
    }
    pub fn shadow_distance(mut self, v: f32) -> Self {
        self.shadow_distance = Some(v);
        self
    }
    pub fn shadow_offset(mut self, v: impl Into<(f32, f32)>) -> Self {
        self.shadow_offset = Some(v.into());
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            fill_color: self.fill_color.or(other.fill_color),
            corner_radius: self.corner_radius.or(other.corner_radius),
            opacity: self.opacity.or(other.opacity),
            border_color: self.border_color.or(other.border_color),
            border_pattern: self.border_pattern.or(other.border_pattern),
            border_outline_width: self.border_outline_width.or(other.border_outline_width),
            border_outline_color: self.border_outline_color.or(other.border_outline_color),
            shadow_color: self.shadow_color.or(other.shadow_color),
            shadow_distance: self.shadow_distance.or(other.shadow_distance),
            shadow_offset: self.shadow_offset.or(other.shadow_offset),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    pub fn resolve_over(&self, mut base: NodeStyle) -> NodeStyle {
        if let Some(v) = self.fill_color {
            base.fill_color = v;
        }
        if let Some(v) = self.corner_radius {
            base.corner_radius = v;
        }
        if let Some(v) = self.opacity {
            base.opacity = v;
        }
        if let Some(v) = self.border_color {
            base.border_color = v;
        }
        if let Some(v) = self.border_pattern {
            base.border_pattern = v;
        }
        if let Some(v) = self.border_outline_width {
            base.border_outline_width = v;
        }
        if let Some(v) = self.border_outline_color {
            base.border_outline_color = v;
        }
        if let Some(v) = self.shadow_color {
            base.shadow_color = v;
        }
        if let Some(v) = self.shadow_distance {
            base.shadow_distance = v;
        }
        if let Some(v) = self.shadow_offset {
            base.shadow_offset = v;
        }
        base
    }
}

/// Overlay over [`EdgeStyle`]: mirrors every field, since the edge-config editor
/// exposes them all.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeOverlay {
    pub stroke_color: Option<ColorQuad>,
    pub pattern: Option<Pattern>,
    pub stroke_outline_width: Option<f32>,
    pub stroke_outline_color: Option<ColorQuad>,
    pub border_color: Option<ColorQuad>,
    pub border_width: Option<f32>,
    pub border_gap: Option<f32>,
    pub border_outline_width: Option<f32>,
    pub border_outline_color: Option<ColorQuad>,
    pub border_background: Option<ColorQuad>,
    pub shadow_color: Option<ColorQuad>,
    pub shadow_expand: Option<f32>,
    pub shadow_blur: Option<f32>,
    pub shadow_offset: Option<(f32, f32)>,
    pub glow_color: Option<ColorQuad>,
    pub glow_width: Option<f32>,
    pub curve: Option<EdgeCurve>,
}

impl EdgeOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stroke_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.stroke_color = Some(v.into());
        self
    }
    pub fn pattern(mut self, v: impl Into<Pattern>) -> Self {
        self.pattern = Some(v.into());
        self
    }
    pub fn stroke_outline_width(mut self, v: f32) -> Self {
        self.stroke_outline_width = Some(v);
        self
    }
    pub fn stroke_outline_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.stroke_outline_color = Some(v.into());
        self
    }
    pub fn border_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_color = Some(v.into());
        self
    }
    pub fn border_width(mut self, v: f32) -> Self {
        self.border_width = Some(v);
        self
    }
    pub fn border_gap(mut self, v: f32) -> Self {
        self.border_gap = Some(v);
        self
    }
    pub fn border_outline_width(mut self, v: f32) -> Self {
        self.border_outline_width = Some(v);
        self
    }
    pub fn border_outline_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_outline_color = Some(v.into());
        self
    }
    pub fn border_background(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_background = Some(v.into());
        self
    }
    pub fn shadow_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.shadow_color = Some(v.into());
        self
    }
    pub fn shadow_expand(mut self, v: f32) -> Self {
        self.shadow_expand = Some(v);
        self
    }
    pub fn shadow_blur(mut self, v: f32) -> Self {
        self.shadow_blur = Some(v);
        self
    }
    pub fn shadow_offset(mut self, v: impl Into<(f32, f32)>) -> Self {
        self.shadow_offset = Some(v.into());
        self
    }
    pub fn glow_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.glow_color = Some(v.into());
        self
    }
    pub fn glow_width(mut self, v: f32) -> Self {
        self.glow_width = Some(v);
        self
    }
    pub fn curve(mut self, v: impl Into<EdgeCurve>) -> Self {
        self.curve = Some(v.into());
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            stroke_color: self.stroke_color.or(other.stroke_color),
            pattern: self.pattern.or(other.pattern),
            stroke_outline_width: self.stroke_outline_width.or(other.stroke_outline_width),
            stroke_outline_color: self.stroke_outline_color.or(other.stroke_outline_color),
            border_color: self.border_color.or(other.border_color),
            border_width: self.border_width.or(other.border_width),
            border_gap: self.border_gap.or(other.border_gap),
            border_outline_width: self.border_outline_width.or(other.border_outline_width),
            border_outline_color: self.border_outline_color.or(other.border_outline_color),
            border_background: self.border_background.or(other.border_background),
            shadow_color: self.shadow_color.or(other.shadow_color),
            shadow_expand: self.shadow_expand.or(other.shadow_expand),
            shadow_blur: self.shadow_blur.or(other.shadow_blur),
            shadow_offset: self.shadow_offset.or(other.shadow_offset),
            glow_color: self.glow_color.or(other.glow_color),
            glow_width: self.glow_width.or(other.glow_width),
            curve: self.curve.or(other.curve),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    pub fn resolve_over(&self, mut base: EdgeStyle) -> EdgeStyle {
        if let Some(v) = self.stroke_color {
            base.stroke_color = v;
        }
        if let Some(v) = self.pattern {
            base.pattern = v;
        }
        if let Some(v) = self.stroke_outline_width {
            base.stroke_outline_width = v;
        }
        if let Some(v) = self.stroke_outline_color {
            base.stroke_outline_color = v;
        }
        if let Some(v) = self.border_color {
            base.border_color = v;
        }
        if let Some(v) = self.border_width {
            base.border_width = v;
        }
        if let Some(v) = self.border_gap {
            base.border_gap = v;
        }
        if let Some(v) = self.border_outline_width {
            base.border_outline_width = v;
        }
        if let Some(v) = self.border_outline_color {
            base.border_outline_color = v;
        }
        if let Some(v) = self.border_background {
            base.border_background = v;
        }
        if let Some(v) = self.shadow_color {
            base.shadow_color = v;
        }
        if let Some(v) = self.shadow_expand {
            base.shadow_expand = v;
        }
        if let Some(v) = self.shadow_blur {
            base.shadow_blur = v;
        }
        if let Some(v) = self.shadow_offset {
            base.shadow_offset = v;
        }
        if let Some(v) = self.glow_color {
            base.glow_color = v;
        }
        if let Some(v) = self.glow_width {
            base.glow_width = v;
        }
        if let Some(v) = self.curve {
            base.curve = v;
        }
        base
    }
}

/// Overlay over [`GraphStyle`]: the canvas background plus the optional
/// [`TilingBackground`] fields the graph-config editor exposes. The tiling fields
/// override the base tiling in place (the theme base ships a subtle grid), so a
/// node that sets only `tiling_spacing` keeps the base kind/color and just
/// re-pitches it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphOverlay {
    pub background_color: Option<Color>,
    pub tiling_kind: Option<TilingKind>,
    pub tiling_spacing: Option<f32>,
    pub tiling_thickness: Option<f32>,
    pub tiling_color: Option<Color>,
}

impl GraphOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn background_color(mut self, v: impl Into<Color>) -> Self {
        self.background_color = Some(v.into());
        self
    }
    pub fn tiling_kind(mut self, v: impl Into<TilingKind>) -> Self {
        self.tiling_kind = Some(v.into());
        self
    }
    pub fn tiling_spacing(mut self, v: f32) -> Self {
        self.tiling_spacing = Some(v);
        self
    }
    pub fn tiling_thickness(mut self, v: f32) -> Self {
        self.tiling_thickness = Some(v);
        self
    }
    pub fn tiling_color(mut self, v: impl Into<Color>) -> Self {
        self.tiling_color = Some(v.into());
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            background_color: self.background_color.or(other.background_color),
            tiling_kind: self.tiling_kind.or(other.tiling_kind),
            tiling_spacing: self.tiling_spacing.or(other.tiling_spacing),
            tiling_thickness: self.tiling_thickness.or(other.tiling_thickness),
            tiling_color: self.tiling_color.or(other.tiling_color),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    /// If any tiling field is set, the base tiling is overridden in place. The
    /// base normally already carries a tiling (the theme default ships a grid),
    /// so the fallback only fires for a base built without one; it uses a neutral
    /// semi-transparent line so it stays visible on any background.
    pub fn resolve_over(&self, mut base: GraphStyle) -> GraphStyle {
        if let Some(v) = self.background_color {
            base.background_color = v;
        }
        let has_tiling = self.tiling_kind.is_some()
            || self.tiling_spacing.is_some()
            || self.tiling_thickness.is_some()
            || self.tiling_color.is_some();
        if has_tiling {
            let mut tiling = base.tiling.unwrap_or_else(|| {
                TilingBackground::grid(40.0, 1.0, Color::from_rgba(0.5, 0.5, 0.5, 0.35))
            });
            if let Some(v) = self.tiling_kind {
                tiling.kind = v;
            }
            if let Some(v) = self.tiling_spacing {
                tiling.spacing = v;
            }
            if let Some(v) = self.tiling_thickness {
                tiling.thickness = v;
            }
            if let Some(v) = self.tiling_color {
                tiling.color = v;
            }
            base.tiling = Some(tiling);
        }
        base
    }
}

/// Overlay over [`PinStyle`]: the fields the pin-config editor exposes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PinOverlay {
    pub color: Option<ColorQuad>,
    pub radius: Option<f32>,
    pub cutout_radius: Option<f32>,
    pub shape: Option<PinShape>,
    pub border_color: Option<ColorQuad>,
    pub border_width: Option<f32>,
}

impl PinOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.color = Some(v.into());
        self
    }
    pub fn radius(mut self, v: f32) -> Self {
        self.radius = Some(v);
        self
    }
    pub fn cutout_radius(mut self, v: f32) -> Self {
        self.cutout_radius = Some(v);
        self
    }
    pub fn shape(mut self, v: impl Into<PinShape>) -> Self {
        self.shape = Some(v.into());
        self
    }
    pub fn border_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_color = Some(v.into());
        self
    }
    pub fn border_width(mut self, v: f32) -> Self {
        self.border_width = Some(v);
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            color: self.color.or(other.color),
            radius: self.radius.or(other.radius),
            cutout_radius: self.cutout_radius.or(other.cutout_radius),
            shape: self.shape.or(other.shape),
            border_color: self.border_color.or(other.border_color),
            border_width: self.border_width.or(other.border_width),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    pub fn resolve_over(&self, mut base: PinStyle) -> PinStyle {
        if let Some(v) = self.color {
            base.color = v;
        }
        if let Some(v) = self.radius {
            base.radius = v;
        }
        if let Some(v) = self.cutout_radius {
            base.cutout_radius = v;
        }
        if let Some(v) = self.shape {
            base.shape = v;
        }
        if let Some(v) = self.border_color {
            base.border_color = v;
        }
        if let Some(v) = self.border_width {
            base.border_width = v;
        }
        base
    }
}

/// Overlay over [`AnchorStyle`]: mirrors every field.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnchorOverlay {
    pub core_size: Option<f32>,
    pub core_radius: Option<f32>,
    pub core_color: Option<ColorQuad>,
    pub core_border_color: Option<ColorQuad>,
    pub core_border_width: Option<f32>,
    pub orbit_offset: Option<f32>,
    pub orbit_spacing: Option<f32>,
    pub ring_color: Option<ColorQuad>,
    pub ring_width: Option<f32>,
    pub offered_ring_color: Option<ColorQuad>,
}

impl AnchorOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn core_size(mut self, v: f32) -> Self {
        self.core_size = Some(v);
        self
    }
    pub fn core_radius(mut self, v: f32) -> Self {
        self.core_radius = Some(v);
        self
    }
    pub fn core_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.core_color = Some(v.into());
        self
    }
    pub fn core_border_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.core_border_color = Some(v.into());
        self
    }
    pub fn core_border_width(mut self, v: f32) -> Self {
        self.core_border_width = Some(v);
        self
    }
    pub fn orbit_offset(mut self, v: f32) -> Self {
        self.orbit_offset = Some(v);
        self
    }
    pub fn orbit_spacing(mut self, v: f32) -> Self {
        self.orbit_spacing = Some(v);
        self
    }
    pub fn ring_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.ring_color = Some(v.into());
        self
    }
    pub fn ring_width(mut self, v: f32) -> Self {
        self.ring_width = Some(v);
        self
    }
    pub fn offered_ring_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.offered_ring_color = Some(v.into());
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            core_size: self.core_size.or(other.core_size),
            core_radius: self.core_radius.or(other.core_radius),
            core_color: self.core_color.or(other.core_color),
            core_border_color: self.core_border_color.or(other.core_border_color),
            core_border_width: self.core_border_width.or(other.core_border_width),
            orbit_offset: self.orbit_offset.or(other.orbit_offset),
            orbit_spacing: self.orbit_spacing.or(other.orbit_spacing),
            ring_color: self.ring_color.or(other.ring_color),
            ring_width: self.ring_width.or(other.ring_width),
            offered_ring_color: self.offered_ring_color.or(other.offered_ring_color),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    pub fn resolve_over(&self, mut base: AnchorStyle) -> AnchorStyle {
        if let Some(v) = self.core_size {
            base.core_size = v;
        }
        if let Some(v) = self.core_radius {
            base.core_radius = v;
        }
        if let Some(v) = self.core_color {
            base.core_color = v;
        }
        if let Some(v) = self.core_border_color {
            base.core_border_color = v;
        }
        if let Some(v) = self.core_border_width {
            base.core_border_width = v;
        }
        if let Some(v) = self.orbit_offset {
            base.orbit_offset = v;
        }
        if let Some(v) = self.orbit_spacing {
            base.orbit_spacing = v;
        }
        if let Some(v) = self.ring_color {
            base.ring_color = v;
        }
        if let Some(v) = self.ring_width {
            base.ring_width = v;
        }
        if let Some(v) = self.offered_ring_color {
            base.offered_ring_color = v;
        }
        base
    }
}

/// Overlay over [`SelectionBoxStyle`]. Colors are plain [`Color`]s like the
/// style's; the setters take a quad and keep its `near_start` so a
/// `ColorData` pin can feed them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionBoxOverlay {
    pub fill: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
}

impl SelectionBoxOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fill(mut self, v: impl Into<ColorQuad>) -> Self {
        self.fill = Some(v.into().near_start);
        self
    }
    pub fn border_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_color = Some(v.into().near_start);
        self
    }
    pub fn border_width(mut self, v: f32) -> Self {
        self.border_width = Some(v);
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            fill: self.fill.or(other.fill),
            border_color: self.border_color.or(other.border_color),
            border_width: self.border_width.or(other.border_width),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    pub fn resolve_over(&self, mut base: SelectionBoxStyle) -> SelectionBoxStyle {
        if let Some(v) = self.fill {
            base.fill = v;
        }
        if let Some(v) = self.border_color {
            base.border_color = v;
        }
        if let Some(v) = self.border_width {
            base.border_width = v;
        }
        base
    }
}

/// Overlay over [`CuttingToolStyle`]; color handling as [`SelectionBoxOverlay`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CuttingToolOverlay {
    pub color: Option<Color>,
    pub width: Option<f32>,
}

impl CuttingToolOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.color = Some(v.into().near_start);
        self
    }
    pub fn width(mut self, v: f32) -> Self {
        self.width = Some(v);
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            color: self.color.or(other.color),
            width: self.width.or(other.width),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    pub fn resolve_over(&self, mut base: CuttingToolStyle) -> CuttingToolStyle {
        if let Some(v) = self.color {
            base.color = v;
        }
        if let Some(v) = self.width {
            base.width = v;
        }
        base
    }
}

/// Overlay over [`MinimapStyle`]; color handling as [`SelectionBoxOverlay`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MinimapOverlay {
    pub background: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub node_color: Option<Color>,
    pub selected_node_color: Option<Color>,
    pub viewport_fill: Option<Color>,
    pub viewport_border_color: Option<Color>,
    pub viewport_border_width: Option<f32>,
}

impl MinimapOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn background(mut self, v: impl Into<ColorQuad>) -> Self {
        self.background = Some(v.into().near_start);
        self
    }
    pub fn border_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.border_color = Some(v.into().near_start);
        self
    }
    pub fn border_width(mut self, v: f32) -> Self {
        self.border_width = Some(v);
        self
    }
    pub fn node_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.node_color = Some(v.into().near_start);
        self
    }
    pub fn selected_node_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.selected_node_color = Some(v.into().near_start);
        self
    }
    pub fn viewport_fill(mut self, v: impl Into<ColorQuad>) -> Self {
        self.viewport_fill = Some(v.into().near_start);
        self
    }
    pub fn viewport_border_color(mut self, v: impl Into<ColorQuad>) -> Self {
        self.viewport_border_color = Some(v.into().near_start);
        self
    }
    pub fn viewport_border_width(mut self, v: f32) -> Self {
        self.viewport_border_width = Some(v);
        self
    }

    /// Layers `self` over `other`; `self` wins where set. Stays partial.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            background: self.background.or(other.background),
            border_color: self.border_color.or(other.border_color),
            border_width: self.border_width.or(other.border_width),
            node_color: self.node_color.or(other.node_color),
            selected_node_color: self.selected_node_color.or(other.selected_node_color),
            viewport_fill: self.viewport_fill.or(other.viewport_fill),
            viewport_border_color: self.viewport_border_color.or(other.viewport_border_color),
            viewport_border_width: self.viewport_border_width.or(other.viewport_border_width),
        }
    }

    /// Applies the set fields onto a concrete base, leaving unset fields intact.
    pub fn resolve_over(&self, mut base: MinimapStyle) -> MinimapStyle {
        if let Some(v) = self.background {
            base.background = v;
        }
        if let Some(v) = self.border_color {
            base.border_color = v;
        }
        if let Some(v) = self.border_width {
            base.border_width = v;
        }
        if let Some(v) = self.node_color {
            base.node_color = v;
        }
        if let Some(v) = self.selected_node_color {
            base.selected_node_color = v;
        }
        if let Some(v) = self.viewport_fill {
            base.viewport_fill = v;
        }
        if let Some(v) = self.viewport_border_color {
            base.viewport_border_color = v;
        }
        if let Some(v) = self.viewport_border_width {
            base.viewport_border_width = v;
        }
        base
    }
}
