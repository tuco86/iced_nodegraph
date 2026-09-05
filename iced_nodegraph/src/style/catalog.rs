//! The [`Catalog`] trait a theme implements to host a [`NodeGraph`], and the
//! boxed closure types [`iced_widget::core::Theme`] uses as its classes.
//!
//! The shape is iced's own (`iced_widget::button::Catalog`): the widget stores
//! a class per styled thing, the theme resolves a class plus status into the
//! concrete style at draw time. For `iced::Theme` every class is a boxed
//! closure, which is what the `.style(|theme, status| ..)` builders box up.
//! A host with its own theme type picks its own class types and resolves them
//! itself.
//!
//! [`NodeGraph`]: crate::NodeGraph

use iced_widget::core::Theme;

use super::{
    AnchorStatus, AnchorStyle, CuttingToolStyle, EdgeStatus, EdgeStyle, GraphStyle, MinimapStyle,
    NodeStatus, NodeStyle, PinStatus, PinStyle, SelectionBoxStyle, default_anchor_style,
    default_cutting_tool_style, default_edge_style, default_graph_style, default_minimap_style,
    default_node_style, default_pin_style, default_selection_box_style,
};
use crate::ids::Ids;
use crate::node_pin::PinInfo;

/// A styling function for a [`Node`](crate::Node): theme + status -> style.
pub type NodeStyleFn<'a, Theme> = Box<dyn Fn(&Theme, NodeStatus) -> NodeStyle + 'a>;

/// A styling function for the pins of a [`Node`](crate::Node): theme + this
/// pin's info + the other endpoint's info (the drag source during an edge
/// drag, else `None`) + status -> style.
pub type PinStyleFn<'a, Theme, I> =
    Box<dyn Fn(&Theme, &PinInfo<'_, I>, Option<&PinInfo<'_, I>>, PinStatus) -> PinStyle + 'a>;

/// A styling function for an [`Edge`](crate::Edge): theme + status + both
/// endpoint infos in draw order (start = output side, end = input side) ->
/// style.
pub type EdgeStyleFn<'a, Theme, I> =
    Box<dyn Fn(&Theme, EdgeStatus, PinInfo<'_, I>, PinInfo<'_, I>) -> EdgeStyle + 'a>;

/// A styling function for the edge being dragged: theme + the source pin's
/// info -> style. A freshly dragged edge has no status.
pub type DragEdgeStyleFn<'a, Theme, I> = Box<dyn Fn(&Theme, PinInfo<'_, I>) -> EdgeStyle + 'a>;

/// A styling function for an [`Anchor`](crate::Anchor): theme + status ->
/// style.
pub type AnchorStyleFn<'a, Theme> = Box<dyn Fn(&Theme, AnchorStatus) -> AnchorStyle + 'a>;

/// A styling function for the canvas.
pub type GraphStyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> GraphStyle + 'a>;

/// A styling function for the selection box.
pub type SelectionBoxStyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> SelectionBoxStyle + 'a>;

/// A styling function for the edge-cutting trail.
pub type CuttingToolStyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> CuttingToolStyle + 'a>;

/// A styling function for the minimap overlay.
pub type MinimapStyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> MinimapStyle + 'a>;

/// The theme's styling of everything a [`NodeGraph`](crate::NodeGraph) draws.
///
/// One trait rather than one per element because the graph is one widget: a
/// theme that can host it must answer for nodes, pins, edges, anchors and the
/// graph's own chrome together. The shape per element is iced's
/// (`iced_widget::button::Catalog`): a class type the widget stores, a default
/// class, and a resolver from class (plus status) to the concrete style.
///
/// The `.style(..)` builders (`Node::style`, `Edge::style`,
/// `NodeGraph::graph_style`, ...) need the matching class to implement
/// `From<*StyleFn<'_, Theme, ..>>`; the `.class(..)` builders take any class
/// directly.
pub trait Catalog {
    /// The class of a [`Node`](crate::Node).
    type NodeClass<'a>;
    /// The class of a node's pins.
    type PinClass<'a, I: Ids>;
    /// The class of an [`Edge`](crate::Edge).
    type EdgeClass<'a, I: Ids>;
    /// The class of the edge being dragged.
    type DragEdgeClass<'a, I: Ids>;
    /// The class of an [`Anchor`](crate::Anchor).
    type AnchorClass<'a>;
    /// The class of the canvas.
    type GraphClass<'a>;
    /// The class of the selection box.
    type SelectionBoxClass<'a>;
    /// The class of the edge-cutting trail.
    type CuttingToolClass<'a>;
    /// The class of the minimap overlay.
    type MinimapClass<'a>;

    /// The class a node gets without [`Node::class`](crate::Node::class).
    fn default_node<'a>() -> Self::NodeClass<'a>;
    /// Resolves a node's style.
    fn node(&self, class: &Self::NodeClass<'_>, status: NodeStatus) -> NodeStyle;

    /// The class a node's pins get without
    /// [`Node::pin_class`](crate::Node::pin_class).
    fn default_pin<'a, I: Ids>() -> Self::PinClass<'a, I>;
    /// Resolves a pin's style. `pin` is the pin being drawn; `other` is the
    /// drag source while an edge drag is in flight, else `None`.
    fn pin<I: Ids>(
        &self,
        class: &Self::PinClass<'_, I>,
        pin: &PinInfo<'_, I>,
        other: Option<&PinInfo<'_, I>>,
        status: PinStatus,
    ) -> PinStyle;

    /// The class an edge gets without [`Edge::class`](crate::Edge::class).
    fn default_edge<'a, I: Ids>() -> Self::EdgeClass<'a, I>;
    /// Resolves an edge's style. `from` and `to` are the endpoints in draw
    /// order (output side first).
    fn edge<I: Ids>(
        &self,
        class: &Self::EdgeClass<'_, I>,
        status: EdgeStatus,
        from: PinInfo<'_, I>,
        to: PinInfo<'_, I>,
    ) -> EdgeStyle;

    /// The class the dragged edge gets without
    /// [`NodeGraph::dragging_edge_class`](crate::NodeGraph::dragging_edge_class).
    fn default_drag_edge<'a, I: Ids>() -> Self::DragEdgeClass<'a, I>;
    /// Resolves the dragged edge's style from the pin the drag started on.
    fn drag_edge<I: Ids>(
        &self,
        class: &Self::DragEdgeClass<'_, I>,
        source: PinInfo<'_, I>,
    ) -> EdgeStyle;

    /// The class an anchor gets without [`Anchor::class`](crate::Anchor::class).
    fn default_anchor<'a>() -> Self::AnchorClass<'a>;
    /// Resolves an anchor's style.
    fn anchor(&self, class: &Self::AnchorClass<'_>, status: AnchorStatus) -> AnchorStyle;

    /// The class the canvas gets without
    /// [`NodeGraph::graph_class`](crate::NodeGraph::graph_class).
    fn default_graph<'a>() -> Self::GraphClass<'a>;
    /// Resolves the canvas style.
    fn graph(&self, class: &Self::GraphClass<'_>) -> GraphStyle;

    /// The class the selection box gets without
    /// [`NodeGraph::selection_box_class`](crate::NodeGraph::selection_box_class).
    fn default_selection_box<'a>() -> Self::SelectionBoxClass<'a>;
    /// Resolves the selection box style.
    fn selection_box(&self, class: &Self::SelectionBoxClass<'_>) -> SelectionBoxStyle;

    /// The class the cutting trail gets without
    /// [`NodeGraph::cutting_tool_class`](crate::NodeGraph::cutting_tool_class).
    fn default_cutting_tool<'a>() -> Self::CuttingToolClass<'a>;
    /// Resolves the cutting trail style.
    fn cutting_tool(&self, class: &Self::CuttingToolClass<'_>) -> CuttingToolStyle;

    /// The class the minimap gets without
    /// [`NodeGraph::minimap_class`](crate::NodeGraph::minimap_class).
    fn default_minimap<'a>() -> Self::MinimapClass<'a>;
    /// Resolves the minimap style.
    fn minimap(&self, class: &Self::MinimapClass<'_>) -> MinimapStyle;
}

impl Catalog for Theme {
    type NodeClass<'a> = NodeStyleFn<'a, Self>;
    type PinClass<'a, I: Ids> = PinStyleFn<'a, Self, I>;
    type EdgeClass<'a, I: Ids> = EdgeStyleFn<'a, Self, I>;
    type DragEdgeClass<'a, I: Ids> = DragEdgeStyleFn<'a, Self, I>;
    type AnchorClass<'a> = AnchorStyleFn<'a, Self>;
    type GraphClass<'a> = GraphStyleFn<'a, Self>;
    type SelectionBoxClass<'a> = SelectionBoxStyleFn<'a, Self>;
    type CuttingToolClass<'a> = CuttingToolStyleFn<'a, Self>;
    type MinimapClass<'a> = MinimapStyleFn<'a, Self>;

    fn default_node<'a>() -> Self::NodeClass<'a> {
        Box::new(default_node_style)
    }

    fn node(&self, class: &Self::NodeClass<'_>, status: NodeStatus) -> NodeStyle {
        class(self, status)
    }

    fn default_pin<'a, I: Ids>() -> Self::PinClass<'a, I> {
        Box::new(|theme, _pin, _other, status| default_pin_style(theme, status))
    }

    fn pin<I: Ids>(
        &self,
        class: &Self::PinClass<'_, I>,
        pin: &PinInfo<'_, I>,
        other: Option<&PinInfo<'_, I>>,
        status: PinStatus,
    ) -> PinStyle {
        class(self, pin, other, status)
    }

    fn default_edge<'a, I: Ids>() -> Self::EdgeClass<'a, I> {
        Box::new(|theme, status, _from, _to| default_edge_style(theme, status))
    }

    fn edge<I: Ids>(
        &self,
        class: &Self::EdgeClass<'_, I>,
        status: EdgeStatus,
        from: PinInfo<'_, I>,
        to: PinInfo<'_, I>,
    ) -> EdgeStyle {
        class(self, status, from, to)
    }

    fn default_drag_edge<'a, I: Ids>() -> Self::DragEdgeClass<'a, I> {
        Box::new(|theme, _source| default_edge_style(theme, EdgeStatus::Idle))
    }

    fn drag_edge<I: Ids>(
        &self,
        class: &Self::DragEdgeClass<'_, I>,
        source: PinInfo<'_, I>,
    ) -> EdgeStyle {
        class(self, source)
    }

    fn default_anchor<'a>() -> Self::AnchorClass<'a> {
        Box::new(default_anchor_style)
    }

    fn anchor(&self, class: &Self::AnchorClass<'_>, status: AnchorStatus) -> AnchorStyle {
        class(self, status)
    }

    fn default_graph<'a>() -> Self::GraphClass<'a> {
        Box::new(default_graph_style)
    }

    fn graph(&self, class: &Self::GraphClass<'_>) -> GraphStyle {
        class(self)
    }

    fn default_selection_box<'a>() -> Self::SelectionBoxClass<'a> {
        Box::new(default_selection_box_style)
    }

    fn selection_box(&self, class: &Self::SelectionBoxClass<'_>) -> SelectionBoxStyle {
        class(self)
    }

    fn default_cutting_tool<'a>() -> Self::CuttingToolClass<'a> {
        Box::new(default_cutting_tool_style)
    }

    fn cutting_tool(&self, class: &Self::CuttingToolClass<'_>) -> CuttingToolStyle {
        class(self)
    }

    fn default_minimap<'a>() -> Self::MinimapClass<'a> {
        Box::new(default_minimap_style)
    }

    fn minimap(&self, class: &Self::MinimapClass<'_>) -> MinimapStyle {
        class(self)
    }
}
