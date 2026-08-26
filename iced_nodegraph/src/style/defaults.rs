//! Built-in theme-driven default styles.
//!
//! Each `default_*_style` translates the iced [`Theme`] palette into one
//! complete, concrete style. It is both what the widget draws when no closure is
//! set, and the base a user closure overrides via struct-update - so a host
//! closure can always reach everything the default reaches.
//!
//! Every color comes from [`Roles`], the single theme-to-graph mapping, so the
//! same relationships hold in all 22 built-in themes: node surfaces are
//! elevations of the canvas, edges and pins are two rungs of one legibility
//! ladder, and the accents are reserved - `primary` for selection, `danger` for
//! cutting. Geometry (radii, widths, distances) is theme-independent and lives
//! here; the only branch on the palette is light-versus-dark shadow weight,
//! which is a genuine physical difference rather than a hue mapping.
//!
//! The per-element defaults take a status and express its feedback in full: a
//! selected node is not just a recolored border (see [`default_node_style`]), and
//! an edge marked for cutting takes the cutting tool's own color. The two
//! overlay defaults ([`default_selection_box_style`],
//! [`default_cutting_tool_style`]) have no status - the overlay exists only while
//! its gesture is running.
//!
//! ```rust,no_run
//! use iced::{widget::text, Color, Point};
//! use iced_nodegraph::{NodeStyle, default_node_style, node};
//!
//! # #[derive(Debug, Clone)]
//! # enum Message {}
//! # let (pos, body) = (Point::ORIGIN, text("body"));
//! node::<_, usize, (), Message, iced::Renderer>(0, pos, body)
//!     .style(|theme, status| NodeStyle {
//!         fill_color: Color::WHITE.into(),      // user override wins
//!         ..default_node_style(theme, status)   // theme base + status fills the rest
//!     });
//! ```
//!
//! Geometry lives here too, as named constants rather than factors applied to
//! whatever a style happens to hold: a pin's drawn radius is
//! [`PinStyle::radius`](crate::PinStyle::radius) verbatim, and the well it opens
//! in the node body is its own field.

use iced_nodegraph_sdf::Pattern;
use iced_widget::core::{Color, Theme};

use super::roles::Roles;
use super::{
    AnchorStatus, AnchorStyle, CuttingToolStyle, EdgeCurve, EdgeStatus, EdgeStyle, NodeStatus,
    NodeStyle, PinShape, PinStatus, PinStyle, SelectionBoxStyle, ramp,
};

/// Corner radius of a node body, in world units.
const NODE_CORNER_RADIUS: f32 = 5.0;

/// How far a selected node's body is pulled toward the accent.
///
/// Enough to identify the node when its border is off-screen, small enough that
/// hosted content - colored by the application for the theme's background - stays
/// readable on it.
const SELECTION_TINT: f32 = 0.12;

/// Drawn radius of a pin indicator, in world units.
///
/// A mark you aim at, sized to be aimed at: at zoom 1 this is a 10 pixel dot,
/// comfortably inside [`PIN_CLICK_THRESHOLD`] so the visible pin is always
/// smaller than the area that accepts a click, never larger.
const PIN_RADIUS: f32 = 5.0;

/// Radius of the well a pin opens in the node body, in world units.
///
/// [`PIN_CLICK_THRESHOLD`], deliberately: the well is the body stepping aside
/// for a pin, and how far it steps aside should be how far the pin's hit area
/// reaches. That is a property of the interaction, not of how big the mark
/// happens to be drawn - scaling this off [`PIN_RADIUS`] would make restyling a
/// pin silently reshape the node and desync the two.
///
/// It also leaves room for the halo a valid drop target wears, which fills
/// exactly the gap between the two.
const PIN_CUTOUT_RADIUS: f32 = crate::node_graph::PIN_CLICK_THRESHOLD;

/// Complete theme-derived node style, with the selected look expressed in full
/// rather than as a border tweak.
///
/// A node is an OPAQUE card one elevation step above the canvas. Opacity is not a
/// styling knob here: a translucent body lets the grid and the edges running
/// behind it show through the content, which is the single loudest way to make a
/// node read as an overlay instead of an object.
///
/// A selected node reads as *brought forward*: its body is tinted toward the
/// accent, the border takes the accent and gains a translucent halo, and the drop
/// shadow deepens - reinforcing the z-promotion the widget already applies. All
/// of it is style-level (color bands and a stop chain), so switching selection
/// does not touch node geometry or the shape cache.
///
/// Override any of it by returning your own [`NodeStyle`] from the node's
/// `style` closure; the closure receives the [`NodeStatus`], so a host is not
/// limited to recoloring the border.
pub fn default_node_style(theme: &Theme, status: NodeStatus) -> NodeStyle {
    let roles = Roles::of(theme);

    // Shadow weight is genuinely light/dark dependent: a black shadow barely
    // registers against a dark canvas, so the dark variant leans on the border
    // for silhouette and keeps the shadow as a hint of depth, while the light
    // variant lets the shadow do the separating.
    let (shadow_alpha, shadow_distance) = if roles.is_dark {
        (0.38, 7.0)
    } else {
        (0.22, 9.0)
    };

    let base = NodeStyle {
        fill_color: roles.body.into(),
        corner_radius: NODE_CORNER_RADIUS,
        opacity: 1.0,
        border_color: roles.border.into(),
        border_pattern: Pattern::solid(1.0),
        border_outline_width: 0.0,
        border_outline_color: Color::TRANSPARENT.into(),
        shadow_color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
        shadow_distance,
        shadow_offset: (0.0, 3.0),
    };

    match status {
        NodeStatus::Idle => base,
        NodeStatus::Selected => NodeStyle {
            fill_color: ramp::blend(roles.body, roles.accent, SELECTION_TINT).into(),
            border_color: roles.accent.into(),
            border_pattern: Pattern::solid(2.0),
            // An outward band on the silhouette, so the halo reads at any zoom
            // without moving the outline.
            border_outline_width: 3.0,
            border_outline_color: Color {
                a: 0.28,
                ..roles.accent
            }
            .into(),
            shadow_distance: shadow_distance * 1.6,
            ..base
        },
    }
}

/// Complete theme-derived pin style, with the valid-target state expressed as a
/// color change plus a halo.
///
/// An idle pin is a MARK: it takes the wire's ladder one rung brighter rather
/// than the selection accent, so "this node is selected" and "this is a
/// connection point" never resolve to the same color, and so a theme whose
/// `primary` collides with its background still has visible pins. A filled dot
/// needs no border, exactly as iced's slider handle carries none.
///
/// A valid drop target is the one moment a pin earns an accent, and it gets its
/// own: `success` reads as "this connection would be accepted", leaving
/// `primary` to selection and `danger` to cutting. The halo is a translucent
/// ring drawn outside the indicator, filling the pin's cutout exactly.
///
/// The feedback is a still image, not motion, and every field it touches is a
/// color band. Both pin states resolve to the same indicator recipe and the same
/// node silhouette, so a drag repaints what the SDF renderer already has resident
/// rather than making it rebuild a shape per frame.
pub fn default_pin_style(theme: &Theme, status: PinStatus) -> PinStyle {
    let roles = Roles::of(theme);

    let base = PinStyle {
        color: roles.terminal.into(),
        radius: PIN_RADIUS,
        shape: PinShape::Circle,
        cutout_radius: PIN_CUTOUT_RADIUS,
        border_color: Color::TRANSPARENT.into(),
        border_width: 0.0,
    };

    match status {
        PinStatus::Idle => base,
        PinStatus::ValidTarget => PinStyle {
            color: roles.valid.into(),
            border_color: Color {
                a: 0.4,
                ..roles.valid
            }
            .into(),
            border_width: PIN_CUTOUT_RADIUS - PIN_RADIUS,
            // The cutout is geometry: holding it across statuses keeps one node
            // silhouette in the shape cache instead of one per drag state.
            ..base
        },
    }
}

/// Complete theme-derived anchor style.
///
/// The core is furniture, not a mark: a 6 unit dot on the node recipe (body
/// fill, border silhouette), because it is a thing you grab rather than a thing
/// you aim at, while an occupied orbit's ring takes the wire color because it is
/// the path a cable runs on. `Selected` mirrors the node's accent border, and
/// `ValidTarget` mirrors the pin's `success`-derived valid color, so the three
/// accents keep meaning exactly one thing each across the whole widget.
///
/// `orbit_offset`/`orbit_spacing` are geometry, not paint: they are the radii
/// cables are laid tangent to, resolved before the path is built, so changing
/// them reshapes the cable rather than recoloring it. Orbit 0 stands 8 units
/// clear of the core's edge, and the spacing is wide enough that two wraps read
/// as separate strands at zoom 1.
pub fn default_anchor_style(theme: &Theme, status: AnchorStatus) -> AnchorStyle {
    let roles = Roles::of(theme);

    let base = AnchorStyle {
        core_size: crate::node_graph::DEFAULT_CORE_SIZE,
        core_radius: 3.0,
        core_color: roles.body.into(),
        core_border_color: roles.border.into(),
        core_border_width: 1.0,
        orbit_offset: crate::node_graph::DEFAULT_ORBIT_OFFSET,
        orbit_spacing: crate::node_graph::DEFAULT_ORBIT_SPACING,
        ring_color: Color {
            a: 0.35,
            ..roles.wire
        }
        .into(),
        ring_width: 1.0,
    };

    match status {
        AnchorStatus::Idle => base,
        AnchorStatus::Hovered => AnchorStyle {
            core_border_color: Color {
                a: 0.6,
                ..roles.accent
            }
            .into(),
            core_border_width: 1.5,
            ..base
        },
        AnchorStatus::ValidTarget => AnchorStyle {
            core_color: roles.valid.into(),
            ring_color: Color {
                a: 0.7,
                ..roles.valid
            }
            .into(),
            ..base
        },
    }
}

/// Complete theme-derived edge style with status feedback: `Idle` is a 2px solid
/// stroke in the theme's wire color; `PendingCut` tints the stroke with the
/// theme's edge-cutting color.
///
/// The default stroke is a single concrete color. To make an edge follow its
/// connected pins (e.g. a port-typed color), build the gradient from each
/// endpoint's [`PinInfo`](crate::PinInfo) in the edge `style` closure and
/// struct-update over this base.
pub fn default_edge_style(theme: &Theme, status: EdgeStatus) -> EdgeStyle {
    let roles = Roles::of(theme);
    // Unused-color sentinel for the off fields (border, outlines, shadow).
    let none = Color::TRANSPARENT;
    let base = EdgeStyle {
        stroke_color: roles.wire.into(),
        pattern: Pattern::solid(2.0),
        stroke_outline_width: 0.0,
        stroke_outline_color: none.into(),
        border_color: none.into(),
        border_width: 0.0,
        border_gap: 0.5,
        border_outline_width: 0.0,
        border_outline_color: none.into(),
        border_background: none.into(),
        shadow_color: none.into(),
        shadow_expand: 0.0,
        shadow_blur: 0.0,
        shadow_offset: (0.0, 0.0),
        curve: EdgeCurve::BezierCubic,
    };

    match status {
        EdgeStatus::Idle => base,
        EdgeStatus::PendingCut => EdgeStyle {
            // An edge marked for cutting takes the cutting tool's own color, so
            // the trail and its victims read as one gesture.
            stroke_color: roles.danger.into(),
            ..base
        },
    }
}

/// Theme-derived style of the selection box.
///
/// The accent hue at two alphas: a translucent wash so nodes stay legible
/// underneath, and an opaque-enough outline to read against both.
pub fn default_selection_box_style(theme: &Theme) -> SelectionBoxStyle {
    let accent = Roles::of(theme).accent;
    SelectionBoxStyle {
        fill: Color { a: 0.15, ..accent },
        border_color: Color { a: 0.75, ..accent },
        border_width: 1.5,
    }
}

/// Theme-derived style of the edge-cutting trail.
///
/// Cutting is destructive, so it paints in the theme's `danger` color rather
/// than the accent.
pub fn default_cutting_tool_style(theme: &Theme) -> CuttingToolStyle {
    CuttingToolStyle {
        color: Roles::of(theme).danger,
        width: 3.0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::ColorQuad;
    use super::*;

    /// Selection is expressed on four independent channels, not just the border,
    /// and every one of them is style-level: switching selection must not cost a
    /// geometry rebuild. Asserted across every theme, since a channel that
    /// collapses does so in the palette that made its accent awkward, not in
    /// `Theme::Dark`.
    #[test]
    fn selected_node_reads_as_brought_forward() {
        for theme in Theme::ALL {
            let idle = default_node_style(theme, NodeStatus::Idle);
            let sel = default_node_style(theme, NodeStatus::Selected);

            assert_ne!(
                sel.border_color, idle.border_color,
                "{theme}: the border must leave the neutral ramp for the accent",
            );
            assert_ne!(
                sel.fill_color, idle.fill_color,
                "{theme}: the body must carry the selection when the border is \
                 off-screen",
            );
            assert!(
                sel.border_outline_width > 0.0,
                "{theme}: a halo ring distinguishes selection at any zoom",
            );
            assert!(
                sel.shadow_distance > idle.shadow_distance,
                "{theme}: the shadow deepens, reinforcing the z-promotion",
            );

            // Geometry-affecting fields must match, or the shape cache gains an
            // entry per selection state.
            assert_eq!(sel.corner_radius, idle.corner_radius);
            assert_eq!(sel.shadow_offset, idle.shadow_offset);
        }
    }

    /// A node is an object, not an overlay. A translucent body shows the grid and
    /// the edges running behind it straight through the content, which is the one
    /// change that makes every theme look cheap at once - so no theme may opt out.
    #[test]
    fn a_node_body_is_opaque_in_every_theme() {
        for theme in Theme::ALL {
            for status in [NodeStatus::Idle, NodeStatus::Selected] {
                assert_eq!(
                    default_node_style(theme, status).opacity,
                    1.0,
                    "{theme}: a node body must be opaque",
                );
            }
        }
    }

    /// Pins are marks, not accents. Sharing `primary` with selection would make
    /// "this node is selected" and "this is a connection point" the same color,
    /// and would hide the pins of any theme whose `primary` collides with its
    /// background. Holds in both pin states: a valid target has its own accent.
    #[test]
    fn a_pin_never_borrows_the_selection_accent() {
        for theme in Theme::ALL {
            let selected = default_node_style(theme, NodeStatus::Selected).border_color;
            for status in [PinStatus::Idle, PinStatus::ValidTarget] {
                assert_ne!(
                    default_pin_style(theme, status).color,
                    selected,
                    "{theme}: a {status:?} pin wears the selection accent",
                );
            }
        }
    }

    /// A drop target you cannot see is a drag you have to guess at. `ValidTarget`
    /// must differ from `Idle` in fill AND wear a halo, in every theme - the
    /// status argument is not decoration on the signature.
    #[test]
    fn a_valid_drop_target_is_visible_in_every_theme() {
        for theme in Theme::ALL {
            let idle = default_pin_style(theme, PinStatus::Idle);
            let valid = default_pin_style(theme, PinStatus::ValidTarget);

            assert_ne!(
                valid.color, idle.color,
                "{theme}: a valid drop target paints like an idle pin",
            );
            assert!(
                valid.border_width > 0.0,
                "{theme}: a valid drop target has no halo",
            );
        }
    }

    /// Valid-target feedback must be paint only.
    ///
    /// Every geometry-bearing pin field has to survive a status change: `radius`
    /// and `shape` decide the indicator's recipe, `cutout_radius` the node
    /// silhouette that is punched around it, and both are content-addressed
    /// shapes held across frames. A default that moved any of them would rebuild
    /// and re-cache a shape per drag state, on every node in the graph, for the
    /// duration of a drag. The remaining fields are color bands, which are free.
    #[test]
    fn valid_target_feedback_costs_no_geometry() {
        for theme in Theme::ALL {
            let idle = default_pin_style(theme, PinStatus::Idle);
            let valid = default_pin_style(theme, PinStatus::ValidTarget);

            assert_eq!(valid.radius, idle.radius, "{theme}: the indicator resizes");
            assert_eq!(valid.shape, idle.shape, "{theme}: the indicator reshapes");
            assert_eq!(
                valid.cutout_radius, idle.cutout_radius,
                "{theme}: the node silhouette changes with pin status",
            );
        }
    }

    /// The well has to be wider than the mark in it, or the pin overruns the hole
    /// and sits on the body's own border instead of in a socket. The halo is sized
    /// to fill exactly that gap.
    #[test]
    fn a_pin_fits_inside_the_well_it_opens() {
        let idle = default_pin_style(&Theme::Dark, PinStatus::Idle);
        assert!(
            idle.cutout_radius > idle.radius,
            "the pin overruns its well"
        );

        let valid = default_pin_style(&Theme::Dark, PinStatus::ValidTarget);
        assert_eq!(
            valid.radius + valid.border_width,
            valid.cutout_radius,
            "the valid-target halo must reach the rim of the well, no further",
        );
    }

    /// An edge marked for cutting must take the cutting tool's own color, so the
    /// trail and the edges it will destroy read as one gesture.
    #[test]
    fn pending_cut_tints_stroke_with_the_cutting_tool_color() {
        let t = Theme::Dark;
        let o = default_edge_style(&t, EdgeStatus::PendingCut);
        assert_eq!(
            o.stroke_color,
            ColorQuad::solid(default_cutting_tool_style(&t).color)
        );
    }
}
