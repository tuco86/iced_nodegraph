//! Pin widget for node graph connection points.
//!
//! This module provides the [`NodePin`] widget that wraps content and acts as
//! a connection point for edges. Pins are placed within nodes and can be
//! connected to other pins via dragging.
//!
//! ## Usage
//!
//! Pins are typically created using the [`pin!`] macro for convenience:
//!
//! ```ignore
//! use iced_nodegraph::pin;
//!
//! // Simple pin with just a label
//! pin!(Left, 0, text("Input"), Input)
//!
//! // Pin with a user-defined payload
//! pin!(Right, 1, text("Output"), Output, MyKind::Audio)
//! ```
//!
//! ## Pin Properties
//!
//! - [`PinSide`] - Which edge of the node the pin attaches to (Left, Right, Top, Bottom)
//! - [`PinDirection`] - Whether the pin is an input or output
//! - User info - Optional user-defined payload via [`NodePin::info`]
//!
//! ## Connection Behavior
//!
//! When users drag from a pin, the widget tracks valid drop targets based on:
//! - Pin direction (inputs connect to outputs)
//! - The graph's [`NodeGraph::can_connect`](crate::NodeGraph::can_connect) closure
//! - Visual feedback via pulsing animation on valid targets

use crate::ids::PinId;
use iced::{Element, Event, Length, Point, Rectangle, Size};
use iced_widget::core::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Tree, tree},
};
use std::hash::{Hash, Hasher};

/// Default pin size when no content widget is provided.
const DEFAULT_PIN_SIZE: Size = Size::new(50.0, 20.0);

/// Compute a stable hash for a pin ID (used for pin identity tracking).
fn hash_pin_id<P: Hash>(pin_id: &P) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    pin_id.hash(&mut hasher);
    hasher.finish()
}

/// A reference to a specific pin on a specific node.
///
/// The `usize`-indexed specialization of [`PinRef`](crate::PinRef), used by the
/// default API. Construct with `PinReference::new(node_id, pin_id)`; the fields
/// are `node_id` and `pin_id`.
pub type PinReference = crate::node_graph::PinRef<usize, usize>;

/// Which side of a node this pin attaches to.
/// Determines the tangent direction for edge bezier curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum PinSide {
    /// Pin on the left edge, edges exit to the left.
    #[default]
    Left = 0,
    /// Pin on the right edge, edges exit to the right.
    Right = 1,
    /// Pin on the top edge, edges exit upward.
    Top = 2,
    /// Pin on the bottom edge, edges exit downward.
    Bottom = 3,
    /// Pin placed in a row layout. Edges exit to the right (same as `Right`).
    Row = 4,
}

impl From<PinSide> for u32 {
    fn from(side: PinSide) -> u32 {
        side as u32
    }
}

/// Direction of data flow for a pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PinDirection {
    Input,
    Output,
    #[default]
    Both,
}

/// Read-only view of a pin's semantic info, passed to a node's `pin_style`
/// closure so it can style each pin by direction, user info, or id. The pin
/// itself carries no style; the owning node decides how its pins look.
///
/// `UI` is the user-defined per-pin payload set via [`NodePin::info`]; it
/// defaults to `()` for pins that carry none.
pub struct PinInfo<'a, P, UI = ()> {
    direction: PinDirection,
    pin_id: &'a P,
    info: &'a UI,
}

impl<'a, P, UI> PinInfo<'a, P, UI> {
    pub(crate) fn new(direction: PinDirection, pin_id: &'a P, info: &'a UI) -> Self {
        Self {
            direction,
            pin_id,
            info,
        }
    }

    /// The pin's direction (input / output / both).
    pub fn direction(&self) -> PinDirection {
        self.direction
    }

    /// The pin's user id.
    pub fn pin_id(&self) -> &P {
        self.pin_id
    }

    /// The pin's user-defined payload set via [`NodePin::info`].
    pub fn info(&self) -> &UI {
        self.info
    }
}

/// Read-only view of one endpoint of a candidate connection, passed to
/// [`NodeGraph::can_connect`](crate::NodeGraph::can_connect). Bundles the pin's
/// node id, pin id, direction and user payload.
///
/// `UI` is the user-defined per-pin payload; it defaults to `()`.
pub struct PinEnd<'a, N, P, UI = ()> {
    node_id: &'a N,
    pin_id: &'a P,
    direction: PinDirection,
    info: &'a UI,
}

impl<'a, N, P, UI> PinEnd<'a, N, P, UI> {
    pub(crate) fn new(
        node_id: &'a N,
        pin_id: &'a P,
        direction: PinDirection,
        info: &'a UI,
    ) -> Self {
        Self {
            node_id,
            pin_id,
            direction,
            info,
        }
    }

    /// The id of the node this pin belongs to.
    pub fn node_id(&self) -> &N {
        self.node_id
    }

    /// The pin's user id.
    pub fn pin_id(&self) -> &P {
        self.pin_id
    }

    /// The pin's direction (input / output / both).
    pub fn direction(&self) -> PinDirection {
        self.direction
    }

    /// The pin's user-defined payload set via [`NodePin::info`].
    pub fn info(&self) -> &UI {
        self.info
    }
}

/// A transparent wrapper used as a marker within `NodeGraph`.
///
/// Generic over `P` (the pin identifier type, e.g. `String`, enum, UUID) and
/// `UI` (the user-defined per-pin payload surfaced to `pin_style`/`can_connect`,
/// defaults to `()`).
pub struct NodePin<'a, P, UI, Message, Theme, Renderer>
where
    P: PinId,
    Renderer: renderer::Renderer,
{
    pub side: PinSide,
    pub direction: PinDirection,
    pub pin_id: P,
    pub user_info: UI,
    pub content: Element<'a, Message, Theme, Renderer>,
    interactions_disabled: bool,
}

impl<'a, P, Message, Theme, Renderer> NodePin<'a, P, (), Message, Theme, Renderer>
where
    P: PinId,
    Renderer: renderer::Renderer,
{
    pub fn new(
        side: PinSide,
        pin_id: P,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            side,
            direction: PinDirection::Both,
            pin_id,
            user_info: (),
            content: content.into(),
            interactions_disabled: false,
        }
    }
}

impl<'a, P, UI, Message, Theme, Renderer> NodePin<'a, P, UI, Message, Theme, Renderer>
where
    P: PinId,
    Renderer: renderer::Renderer,
{
    pub fn direction(mut self, direction: PinDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Attaches a user-defined payload to this pin, surfaced to the node's
    /// `pin_style` closure and the graph's `can_connect` closure as `UI`.
    ///
    /// Changing the payload type also changes the pin's `UI` type parameter.
    ///
    /// # Example
    /// ```rust,ignore
    /// pin!(Left, "value", text("x"), Input).info(MyKind::Scalar)
    /// ```
    pub fn info<UI2>(self, info: UI2) -> NodePin<'a, P, UI2, Message, Theme, Renderer> {
        NodePin {
            side: self.side,
            direction: self.direction,
            pin_id: self.pin_id,
            user_info: info,
            content: self.content,
            interactions_disabled: self.interactions_disabled,
        }
    }

    /// Disables all interactions (drag, drop) for this pin.
    ///
    /// The pin remains visible and edges stay connected, but the user
    /// cannot start new connections or unplug existing ones.
    /// Useful for collapsed sections where pins should be visible but inactive.
    pub fn disable_interactions(mut self) -> Self {
        self.interactions_disabled = true;
        self
    }
}

/// Type-erased pin ID that can be cloned and downcast.
/// Uses Arc to enable Clone without requiring P: Clone.
#[derive(Clone)]
pub(crate) struct AnyPinId(std::sync::Arc<dyn std::any::Any + Send + Sync>);

impl AnyPinId {
    /// Creates a new type-erased pin ID.
    pub fn new<P: PinId + Send + Sync + 'static>(id: P) -> Self {
        Self(std::sync::Arc::new(id))
    }

    /// Attempts to downcast to the original pin ID type.
    pub fn downcast_ref<P: 'static>(&self) -> Option<&P> {
        self.0.downcast_ref()
    }
}

impl std::fmt::Debug for AnyPinId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AnyPinId").field(&"<type-erased>").finish()
    }
}

/// Internal state for a NodePin widget.
///
/// Generic only over `UI` (the user payload), NOT over `P`: the pin ID is kept
/// type-erased as a hash plus an [`AnyPinId`]. Within one graph all pins share
/// the same `UI`, so `find_pins` still matches a single `tree::Tag`.
#[derive(Debug, Clone)]
pub(super) struct NodePinState<UI> {
    /// Hash of the user's pin ID for matching
    pub pin_id_hash: u64,
    /// Type-erased pin ID for reverse lookup
    pub pin_id: AnyPinId,
    pub side: PinSide,
    pub direction: PinDirection,
    pub position: Point,
    /// When true, pin cannot be dragged from or dropped onto
    pub interactions_disabled: bool,
    /// User-defined per-pin payload, surfaced to pin_style / can_connect.
    pub user_info: UI,
}

impl<'a, P, UI, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for NodePin<'a, P, UI, Message, Theme, Renderer>
where
    P: PinId + 'static,
    UI: Clone + 'static,
    Renderer: renderer::Renderer + 'a,
    Theme: 'a,
    Message: 'a,
{
    fn tag(&self) -> tree::Tag {
        // Same tag for all pins sharing UI - enables consistent pin finding
        tree::Tag::of::<NodePinState<UI>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(NodePinState {
            pin_id_hash: hash_pin_id(&self.pin_id),
            pin_id: AnyPinId::new(self.pin_id.clone()),
            side: self.side,
            direction: self.direction,
            position: Point::new(0.0, 0.0),
            interactions_disabled: self.interactions_disabled,
            user_info: self.user_info.clone(),
        })
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        if let Some(content_tree) = tree.children.first_mut() {
            let content_layout =
                self.content
                    .as_widget_mut()
                    .layout(content_tree, renderer, limits);
            let size = content_layout.size();
            layout::Node::with_children(size, vec![content_layout])
        } else {
            layout::Node::new(DEFAULT_PIN_SIZE)
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        {
            let state = tree.state.downcast_mut::<NodePinState<UI>>();
            state.pin_id_hash = hash_pin_id(&self.pin_id);
            state.pin_id = AnyPinId::new(self.pin_id.clone());
            state.side = self.side;
            state.direction = self.direction;
            state.position = layout.bounds().center();
            state.interactions_disabled = self.interactions_disabled;
            state.user_info = self.user_info.clone();
        }
        if let Some((child_layout, child_tree)) = layout.children().zip(&mut tree.children).next() {
            self.content.as_widget_mut().update(
                child_tree,
                event,
                child_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some((child_layout, child_tree)) = layout.children().zip(&tree.children).next() {
            self.content.as_widget().draw(
                child_tree,
                renderer,
                theme,
                style,
                child_layout,
                cursor,
                viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if let Some((content_tree, content_layout)) =
            tree.children.first().zip(layout.children().next())
        {
            self.content.as_widget().mouse_interaction(
                content_tree,
                content_layout,
                cursor,
                viewport,
                renderer,
            )
        } else {
            mouse::Interaction::default()
        }
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn diff(&self, tree: &mut Tree) {
        if let Some(content_tree) = tree.children.first_mut() {
            self.content.as_widget().diff(content_tree);
        } else {
            tree.children.push(Tree::new(&self.content));
        }
    }
}

impl<'a, P, UI, Message, Theme, Renderer> From<NodePin<'a, P, UI, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    P: PinId + 'static,
    UI: Clone + 'static,
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
    Theme: 'a,
{
    fn from(widget: NodePin<'a, P, UI, Message, Theme, Renderer>) -> Self {
        Element::new(widget)
    }
}

pub fn node_pin<'a, P, Message, Theme, Renderer>(
    side: PinSide,
    pin_id: P,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> NodePin<'a, P, (), Message, Theme, Renderer>
where
    P: PinId,
    Renderer: iced_widget::core::renderer::Renderer,
{
    NodePin::new(side, pin_id, content)
}

/// Macro for creating pins with concise syntax.
///
/// The pin widget is an invisible wrapper that marks where a connection point
/// should be placed. The content element (typically a text label) is passed through.
///
/// # Examples
///
/// Pins carry no style of their own; the owning node colors and shapes them via
/// [`Node::pin_style`](crate::Node::pin_style), keyed on the pin's direction,
/// user info or id.
///
/// ```rust,ignore
/// use iced_nodegraph::pin;
/// use iced::widget::text;
///
/// // Full syntax: side, pin_id, content, direction, user info
/// pin!(Right, "output", text("output"), Output, MyKind::Email)
///
/// // With direction only (connects to anything)
/// pin!(Right, "data", text("data"), Output)
///
/// // Minimal (side, pin_id, content only, defaults: Both direction, no info)
/// pin!(Right, "data", text("data"))
/// ```
#[macro_export]
macro_rules! pin {
    // With user info: side, pin_id, content, direction, info
    ($side:ident, $pin_id:expr, $content:expr, $dir:ident, $info:expr) => {
        $crate::node_pin($crate::PinSide::$side, $pin_id, $content)
            .direction($crate::PinDirection::$dir)
            .info($info)
    };

    // Direction only: side, pin_id, content, direction
    ($side:ident, $pin_id:expr, $content:expr, $dir:ident) => {
        $crate::node_pin($crate::PinSide::$side, $pin_id, $content)
            .direction($crate::PinDirection::$dir)
    };

    // Minimal: side, pin_id, content only
    ($side:ident, $pin_id:expr, $content:expr) => {
        $crate::node_pin($crate::PinSide::$side, $pin_id, $content)
    };
}
