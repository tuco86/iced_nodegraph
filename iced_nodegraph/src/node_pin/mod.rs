use crate::ids::PinId;
use iced::{Color, Element, Event, Length, Point, Rectangle, Size};
use iced_widget::core::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Tree, tree},
};
use std::any::TypeId;

/// A reference to a specific pin on a specific node.
///
/// This provides type-safe identification of pins for edge connections,
/// replacing error-prone `(usize, usize)` tuples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinReference {
    /// The index of the node containing this pin
    pub node_id: usize,
    /// The index of the pin within the node
    pub pin_id: usize,
}

impl PinReference {
    /// Creates a new pin reference.
    pub fn new(node_id: usize, pin_id: usize) -> Self {
        Self { node_id, pin_id }
    }
}

/// Convert from generic PinRef<usize, usize> to PinReference for backwards compatibility.
impl From<crate::node_grapgh::PinRef<usize, usize>> for PinReference {
    fn from(pin_ref: crate::node_grapgh::PinRef<usize, usize>) -> Self {
        Self {
            node_id: pin_ref.node_id,
            pin_id: pin_ref.pin_id,
        }
    }
}

/// Convert from PinReference to generic PinRef<usize, usize>.
impl From<PinReference> for crate::node_grapgh::PinRef<usize, usize> {
    fn from(pin_ref: PinReference) -> Self {
        Self {
            node_id: pin_ref.node_id,
            pin_id: pin_ref.pin_id,
        }
    }
}

/// An edge to attach a `NodePinWidget` to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum PinSide {
    #[default]
    Left = 0,
    Right = 1,
    Top = 2,
    Bottom = 3,
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

/// A transparent wrapper used as a marker within `NodeGraph`.
///
/// Generic over `P` which is the pin identifier type (e.g., `String`, enum, UUID).
pub struct NodePin<'a, P, Message, Theme, Renderer>
where
    P: PinId,
    Renderer: renderer::Renderer,
{
    pub side: PinSide,
    pub direction: PinDirection,
    pub pin_id: P,
    pub data_type: TypeId,
    pub color: Color,
    pub content: Element<'a, Message, Theme, Renderer>,
}

impl<'a, P, Message, Theme, Renderer> NodePin<'a, P, Message, Theme, Renderer>
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
            data_type: TypeId::of::<()>(), // Default: untyped
            color: Color::from_rgb(0.5, 0.5, 0.5),
            content: content.into(),
        }
    }

    pub fn direction(mut self, direction: PinDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Sets the data type of this pin for connection matching.
    ///
    /// Only pins with the same `TypeId` can connect to each other.
    ///
    /// # Example
    /// ```rust,ignore
    /// pin!(Left, "value", text("x"), Input).data_type::<f32>()
    /// ```
    pub fn data_type<T: 'static>(mut self) -> Self {
        self.data_type = TypeId::of::<T>();
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// Type-erased pin ID that can be cloned and downcast.
/// Uses Arc to enable Clone without requiring P: Clone.
#[derive(Clone)]
pub struct AnyPinId(std::sync::Arc<dyn std::any::Any + Send + Sync>);

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
/// This is NOT generic over P - it stores a hash of the pin_id.
/// This ensures consistent tree::Tag matching regardless of the user's pin ID type.
/// The hash is computed when the state is created/updated.
#[derive(Debug, Clone)]
pub(super) struct NodePinState {
    /// Hash of the user's pin ID for matching
    pub pin_id_hash: u64,
    /// Type-erased pin ID for reverse lookup
    pub pin_id: AnyPinId,
    pub side: PinSide,
    pub direction: PinDirection,
    /// TypeId of the data this pin carries - used for connection matching
    pub data_type: TypeId,
    pub color: Color,
    pub position: Point,
}

impl<'a, P, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for NodePin<'a, P, Message, Theme, Renderer>
where
    P: PinId + 'static,
    Renderer: renderer::Renderer + 'a,
    Theme: 'a,
    Message: 'a,
{
    fn tag(&self) -> tree::Tag {
        // Always the same tag regardless of P - enables consistent pin finding
        tree::Tag::of::<NodePinState>()
    }

    fn state(&self) -> tree::State {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.pin_id, &mut hasher);

        tree::State::new(NodePinState {
            pin_id_hash: hasher.finish(),
            pin_id: AnyPinId::new(self.pin_id.clone()),
            side: self.side,
            direction: self.direction,
            data_type: self.data_type,
            color: self.color,
            position: Point::new(0.0, 0.0),
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
            layout::Node::new(Size::new(50.0, 20.0)) // Default pin size
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
            use std::hash::Hasher;
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&self.pin_id, &mut hasher);

            let state = tree.state.downcast_mut::<NodePinState>();
            state.pin_id_hash = hasher.finish();
            state.pin_id = AnyPinId::new(self.pin_id.clone());
            state.side = self.side;
            state.direction = self.direction;
            state.data_type = self.data_type;
            state.color = self.color;
            state.position = layout.bounds().center();
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
        let content_tree = &tree.children[0];
        let content_layout = layout.children().next().unwrap();
        self.content.as_widget().mouse_interaction(
            content_tree,
            content_layout,
            cursor,
            viewport,
            renderer,
        )
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

impl<'a, P, Message, Theme, Renderer> From<NodePin<'a, P, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    P: PinId + 'static,
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
    Theme: 'a,
{
    fn from(widget: NodePin<'a, P, Message, Theme, Renderer>) -> Self {
        Element::new(widget)
    }
}

pub fn node_pin<'a, P, Message, Theme, Renderer>(
    side: PinSide,
    pin_id: P,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> NodePin<'a, P, Message, Theme, Renderer>
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
/// ```rust,ignore
/// use iced_nodegraph::pin;
/// use iced::Color;
/// use iced::widget::text;
///
/// // Full syntax: side, pin_id, content, direction, data_type, color
/// pin!(Right, "output", text("output"), Output, Email, Color::from_rgb(0.3, 0.7, 0.9))
///
/// // With type only (uses default gray color)
/// pin!(Left, "input", text("input"), Input, f32)
///
/// // With direction only (untyped, connects to anything)
/// pin!(Right, "data", text("data"), Output)
///
/// // Minimal (side, pin_id, content only, defaults: Both direction, untyped)
/// pin!(Right, "data", text("data"))
/// ```
#[macro_export]
macro_rules! pin {
    // Full: side, pin_id, content, direction, type, color
    ($side:ident, $pin_id:expr, $content:expr, $dir:ident, $data_type:ty, $color:expr) => {
        $crate::node_pin($crate::PinSide::$side, $pin_id, $content)
            .direction($crate::PinDirection::$dir)
            .data_type::<$data_type>()
            .color($color)
    };

    // With type: side, pin_id, content, direction, type
    ($side:ident, $pin_id:expr, $content:expr, $dir:ident, $data_type:ty) => {
        $crate::node_pin($crate::PinSide::$side, $pin_id, $content)
            .direction($crate::PinDirection::$dir)
            .data_type::<$data_type>()
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
