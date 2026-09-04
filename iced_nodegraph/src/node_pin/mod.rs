//! The [`NodePin`] widget: a connection point inside a node's content.
//!
//! A pin is an invisible wrapper around any element (usually a label). The
//! graph finds every pin in a node's widget tree, draws its indicator on the
//! node border, and lets the user drag edges between pins.
//!
//! ## Usage
//!
//! [`node_pin`] is the builder; [`pin!`] is the shorthand for the common
//! shapes:
//!
//! ```rust
//! use iced::widget::text;
//! use iced_nodegraph::{NodePin, PinDirection, PinSide, node_pin, pin};
//!
//! #[derive(Clone)]
//! enum MyKind {
//!     Audio,
//! }
//!
//! # #[derive(Debug, Clone)]
//! # enum Message {}
//! # type Pin<'a, UI> = NodePin<'a, u32, UI, Message, iced::Renderer>;
//! let input: Pin<'_, ()> = pin!(Left, 0, text("Input"), Input);
//! let output: Pin<'_, MyKind> = pin!(Right, 1, text("Output"), Output, MyKind::Audio);
//! let same: Pin<'_, MyKind> = node_pin(PinSide::Right, 1, text("Output"))
//!     .direction(PinDirection::Output)
//!     .info(MyKind::Audio);
//! ```
//!
//! A pin's id type and payload type must be the graph's
//! [`Ids::PinId`](crate::Ids::PinId) and [`Ids::Payload`](crate::Ids::Payload):
//! the pin is type-erased into the node's content, so the graph finds it by
//! that pair. A pin of another type is a debug-build assertion at the first
//! layout and is ignored in release builds.
//!
//! ## Pin Properties
//!
//! - [`PinSide`] - which edge of the node the pin attaches to, or `Row` for a
//!   pin spanning the node
//! - [`PinDirection`] - input, output or both
//! - Payload - an optional user value via [`NodePin::info`], surfaced to
//!   [`Node::pin_style`](crate::Node::pin_style) and
//!   [`NodeGraph::can_connect`](crate::NodeGraph::can_connect)
//!
//! ## Connection Behavior
//!
//! When users drag from a pin, the widget tracks valid drop targets based on:
//! - Pin direction (inputs connect to outputs)
//! - The graph's [`NodeGraph::can_connect`](crate::NodeGraph::can_connect) closure
//! - `PinStatus::ValidTarget` on every accepting pin, which
//!   [`default_pin_style`](crate::default_pin_style) paints in the theme's
//!   success color with a halo filling its cutout

use std::any::{Any, type_name};

use iced_wgpu::core::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Tree, tree},
};
use iced_widget::core::{Element, Event, Length, Point, Rectangle, Size, Theme};

use crate::ids::{Id, Ids};

/// Default pin size when no content widget is provided.
const DEFAULT_PIN_SIZE: Size = Size::new(50.0, 20.0);

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
    /// Pin spanning the node: an edge may attach on either the left or the
    /// right border, whichever is nearer its other end.
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
    /// Accepts edges only.
    Input,
    /// Emits edges only.
    Output,
    /// Connects to any pin.
    #[default]
    Both,
}

/// Read-only view of a pin's semantic info, passed to a node's `pin_style`
/// closure so it can style each pin by direction, payload, or id. The pin
/// itself carries no style; the owning node decides how its pins look.
pub struct PinInfo<'a, I: Ids> {
    direction: PinDirection,
    pin_id: &'a I::PinId,
    info: &'a I::Payload,
}

impl<'a, I: Ids> PinInfo<'a, I> {
    pub(crate) fn new(direction: PinDirection, pin_id: &'a I::PinId, info: &'a I::Payload) -> Self {
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
    pub fn pin_id(&self) -> &I::PinId {
        self.pin_id
    }

    /// The pin's payload set via [`NodePin::info`].
    pub fn info(&self) -> &I::Payload {
        self.info
    }
}

/// Read-only view of one endpoint of a candidate connection, passed to
/// [`NodeGraph::can_connect`](crate::NodeGraph::can_connect). Bundles the pin's
/// node id, pin id, direction and payload.
pub struct PinEnd<'a, I: Ids> {
    node_id: &'a I::NodeId,
    pin_id: &'a I::PinId,
    direction: PinDirection,
    info: &'a I::Payload,
    is_occupied: bool,
}

// Hand-written so `PinEnd` stays `Copy` for any `I` (it only holds shared
// references); a derive would demand `Copy` of the ids and stop `can_connect`
// helpers from passing it to several predicates by value.
impl<I: Ids> Clone for PinEnd<'_, I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I: Ids> Copy for PinEnd<'_, I> {}

impl<'a, I: Ids> PinEnd<'a, I> {
    pub(crate) fn new(
        node_id: &'a I::NodeId,
        pin_id: &'a I::PinId,
        direction: PinDirection,
        info: &'a I::Payload,
        is_occupied: bool,
    ) -> Self {
        Self {
            node_id,
            pin_id,
            direction,
            info,
            is_occupied,
        }
    }

    /// The id of the node this pin belongs to.
    pub fn node_id(&self) -> &I::NodeId {
        self.node_id
    }

    /// The pin's user id.
    pub fn pin_id(&self) -> &I::PinId {
        self.pin_id
    }

    /// The pin's direction (input / output / both).
    pub fn direction(&self) -> PinDirection {
        self.direction
    }

    /// The pin's payload set via [`NodePin::info`].
    pub fn info(&self) -> &I::Payload {
        self.info
    }

    /// Whether this pin already holds at least one edge.
    ///
    /// The edge currently being dragged is excluded, so a connection re-routed
    /// back onto its own input reports that input as free. See
    /// [`input_not_occupied`](crate::connection::input_not_occupied).
    pub fn is_occupied(&self) -> bool {
        self.is_occupied
    }
}

/// A connection point inside a node's content: an invisible wrapper that
/// marks where the graph draws a pin and where an edge attaches.
///
/// `P` is the pin id type and `UI` the payload type; both are inferred from
/// the values given and must equal the graph's `Ids::PinId` / `Ids::Payload`.
pub struct NodePin<'a, P, UI, Message, Renderer>
where
    P: Id,
    Renderer: renderer::Renderer,
{
    side: PinSide,
    direction: PinDirection,
    pin_id: P,
    user_info: UI,
    content: Element<'a, Message, Theme, Renderer>,
    interactions_disabled: bool,
}

impl<'a, P, Message, Renderer> NodePin<'a, P, (), Message, Renderer>
where
    P: Id,
    Renderer: renderer::Renderer,
{
    /// Creates a pin on `side` with the given id wrapping `content`. Direction
    /// is [`PinDirection::Both`] and the payload `()` until set.
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

impl<'a, P, UI, Message, Renderer> NodePin<'a, P, UI, Message, Renderer>
where
    P: Id,
    Renderer: renderer::Renderer,
{
    /// Sets whether the pin accepts edges, emits them, or both.
    pub fn direction(mut self, direction: PinDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Attaches a payload to this pin, surfaced to the node's `pin_style`
    /// closure and the graph's `can_connect` closure.
    ///
    /// Changing the payload type also changes the pin's `UI` type parameter,
    /// which must equal the graph's `Ids::Payload`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use iced::widget::text;
    /// use iced_nodegraph::{NodePin, pin};
    ///
    /// #[derive(Clone)]
    /// enum MyKind {
    ///     Scalar,
    /// }
    ///
    /// # #[derive(Debug, Clone)]
    /// # enum Message {}
    /// let scalar: NodePin<'_, &str, MyKind, Message, iced::Renderer> =
    ///     pin!(Left, "value", text("x"), Input).info(MyKind::Scalar);
    /// ```
    pub fn info<UI2>(self, info: UI2) -> NodePin<'a, P, UI2, Message, Renderer> {
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

/// Internal state for a NodePin widget.
///
/// Generic over `P` (the pin id) and `UI` (the payload). Within one graph all
/// pins share the same pair, so `find_pins` downcasts to a single concrete
/// `NodePinState`. The pin id is stored directly: matching an edge endpoint is
/// exact equality, and recovering the user's id is just a borrow.
#[derive(Debug, Clone)]
pub(crate) struct NodePinState<P, UI> {
    /// The user's pin id.
    pub pin_id: P,
    pub side: PinSide,
    pub direction: PinDirection,
    pub position: Point,
    /// When true, pin cannot be dragged from or dropped onto
    pub interactions_disabled: bool,
    /// The payload, surfaced to pin_style / can_connect.
    pub user_info: UI,
}

/// The tree state every pin registers, whatever its `P` and `UI`.
///
/// One tag for all pins lets the graph tell a pin of the wrong type from no
/// pin at all: it downcasts the boxed state to its own `NodePinState` and
/// reports the type name of anything else.
pub(crate) struct PinSlot {
    state: Box<dyn Any>,
    type_name: &'static str,
}

impl PinSlot {
    fn new<P: 'static, UI: 'static>(state: NodePinState<P, UI>) -> Self {
        Self {
            state: Box::new(state),
            type_name: type_name::<NodePinState<P, UI>>(),
        }
    }

    /// The pin state, when it is a `NodePinState<P, UI>`.
    pub(crate) fn get<P: 'static, UI: 'static>(&self) -> Option<&NodePinState<P, UI>> {
        self.state.downcast_ref()
    }

    /// The type name of the state held, for the mismatch assertion.
    pub(crate) fn type_name(&self) -> &'static str {
        self.type_name
    }

    fn get_mut<P: 'static, UI: 'static>(&mut self) -> &mut NodePinState<P, UI> {
        self.state
            .downcast_mut()
            .expect("a pin's tree state is the state it registered")
    }
}

impl<'a, P, UI, Message, Renderer> Widget<Message, Theme, Renderer>
    for NodePin<'a, P, UI, Message, Renderer>
where
    P: Id,
    UI: Clone + 'static,
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PinSlot>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(PinSlot::new(NodePinState {
            pin_id: self.pin_id.clone(),
            side: self.side,
            direction: self.direction,
            position: Point::new(0.0, 0.0),
            interactions_disabled: self.interactions_disabled,
            user_info: self.user_info.clone(),
        }))
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
            let state = tree.state.downcast_mut::<PinSlot>().get_mut::<P, UI>();
            state.pin_id = self.pin_id.clone();
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
        // Reconcile through `Tree::diff_children`, never by calling the content's
        // own `diff` directly: only the former compares widget tags and rebuilds
        // state whose type changed. Diffing the content directly left a stale
        // state of the wrong type in place whenever the pin's content widget type
        // changed at the same tree position -- which happens as soon as a node is
        // removed from the middle of the graph, since node trees are reconciled by
        // position. The next access downcast that state and panicked.
        tree.diff_children(std::slice::from_ref(&self.content));
    }
}

impl<'a, P, UI, Message, Renderer> From<NodePin<'a, P, UI, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    P: Id,
    UI: Clone + 'static,
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
{
    fn from(widget: NodePin<'a, P, UI, Message, Renderer>) -> Self {
        Element::new(widget)
    }
}

/// Creates a [`NodePin`] on `side` with the given id wrapping `content`.
pub fn node_pin<'a, P, Message, Renderer>(
    side: PinSide,
    pin_id: P,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> NodePin<'a, P, (), Message, Renderer>
where
    P: Id,
    Renderer: renderer::Renderer,
{
    NodePin::new(side, pin_id, content)
}

/// Shorthand for [`node_pin`] with the side and direction named bare.
///
/// Pins carry no style of their own; the owning node colors and shapes them via
/// [`Node::pin_style`](crate::Node::pin_style), keyed on the pin's direction,
/// payload or id.
///
/// # Examples
///
/// ```rust
/// use iced::widget::text;
/// use iced_nodegraph::{NodePin, pin};
///
/// #[derive(Clone)]
/// enum MyKind {
///     Email,
/// }
///
/// # #[derive(Debug, Clone)]
/// # enum Message {}
/// # type Pin<'a, UI> = NodePin<'a, &'static str, UI, Message, iced::Renderer>;
/// // Full syntax: side, pin_id, content, direction, payload
/// let full: Pin<'_, MyKind> = pin!(Right, "output", text("output"), Output, MyKind::Email);
///
/// // With direction only (connects to anything)
/// let directed: Pin<'_, ()> = pin!(Right, "data", text("data"), Output);
///
/// // Minimal (side, pin_id, content only, defaults: Both direction, no payload)
/// let minimal: Pin<'_, ()> = pin!(Right, "data", text("data"));
/// ```
#[macro_export]
macro_rules! pin {
    // With payload: side, pin_id, content, direction, info
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::{text, text_input};

    /// A pin whose content widget changed type must be handed a FRESH content
    /// tree. Node trees are reconciled by position, so removing a node from the
    /// middle of a graph shifts every following node onto a sibling's tree: a
    /// pin holding a `text_input` can land on the tree of a pin holding a
    /// `text`. Reconciling the content directly (instead of through
    /// `Tree::diff`) skipped the tag comparison and kept the stale state, and
    /// the next access downcast it to the wrong type and panicked.
    #[test]
    fn content_tree_is_rebuilt_when_the_content_type_changes() {
        let stateful: Element<'_, (), Theme, iced::Renderer> =
            node_pin(PinSide::Left, 0usize, text_input("", "x")).into();
        let mut tree = Tree::new(&stateful);
        let stateful_tag = tree.children[0].tag;

        let stateless: Element<'_, (), Theme, iced::Renderer> =
            node_pin(PinSide::Left, 0usize, text("y")).into();
        tree.diff(&stateless);

        assert_eq!(tree.children.len(), 1);
        assert_ne!(tree.children[0].tag, stateful_tag);
        assert_eq!(tree.children[0].tag, Tree::new(&stateless).children[0].tag);
    }

    /// The pin's own state survives a content change: the pin tag is the same,
    /// so only the content subtree is replaced.
    #[test]
    fn the_pin_keeps_its_own_state_across_a_content_change() {
        let before: Element<'_, (), Theme, iced::Renderer> =
            node_pin(PinSide::Left, 7usize, text("a")).into();
        let mut tree = Tree::new(&before);
        let after: Element<'_, (), Theme, iced::Renderer> =
            node_pin(PinSide::Left, 7usize, text_input("", "b")).into();
        tree.diff(&after);

        assert_eq!(tree.tag, tree::Tag::of::<PinSlot>());
        let slot = tree.state.downcast_ref::<PinSlot>();
        assert_eq!(slot.get::<usize, ()>().map(|s| s.pin_id), Some(7));
    }

    /// Every pin shares one tag, so a pin of another id type lands on the same
    /// slot and is recognisable as the wrong pin rather than as no pin.
    #[test]
    fn a_pin_of_another_type_is_told_apart_by_its_slot() {
        let pin: Element<'_, (), Theme, iced::Renderer> =
            node_pin(PinSide::Left, "named", text("a")).into();
        let tree = Tree::new(&pin);

        assert_eq!(tree.tag, tree::Tag::of::<PinSlot>());
        let slot = tree.state.downcast_ref::<PinSlot>();
        assert!(slot.get::<usize, ()>().is_none());
        assert!(slot.get::<&'static str, ()>().is_some());
        assert!(slot.type_name().contains("NodePinState"));
    }
}
