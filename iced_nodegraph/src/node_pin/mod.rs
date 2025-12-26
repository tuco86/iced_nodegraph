use iced::{Color, Element, Event, Length, Point, Rectangle, Size};
use iced_widget::core::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Tree, tree},
};

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
pub struct NodePin<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    pub side: PinSide,
    pub direction: PinDirection,
    pub pin_type: String,
    pub color: Color,
    pub content: Element<'a, Message, Theme, Renderer>,
}

impl<'a, Message, Theme, Renderer> NodePin<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    pub fn new(side: PinSide, content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            side,
            direction: PinDirection::Both,
            pin_type: String::from("any"),
            color: Color::from_rgb(0.5, 0.5, 0.5),
            content: content.into(),
        }
    }

    pub fn direction(mut self, direction: PinDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn pin_type(mut self, pin_type: impl Into<String>) -> Self {
        self.pin_type = pin_type.into();
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct NodePinState {
    pub side: PinSide,
    pub direction: PinDirection,
    pub pin_type: String,
    pub color: Color,
    pub position: Point,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for NodePin<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + 'a,
    Theme: 'a,
    Message: 'a,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<NodePinState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(NodePinState {
            side: self.side,
            direction: self.direction,
            pin_type: self.pin_type.clone(),
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
            let state = tree.state.downcast_mut::<NodePinState>();
            state.side = self.side;
            state.direction = self.direction;
            state.pin_type = self.pin_type.clone();
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

impl<'a, Message, Theme, Renderer> From<NodePin<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
    Theme: 'a,
{
    fn from(widget: NodePin<'a, Message, Theme, Renderer>) -> Self {
        Element::new(widget)
    }
}

pub fn node_pin<'a, Message, Theme, Renderer>(
    side: PinSide,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> NodePin<'a, Message, Theme, Renderer>
where
    Renderer: iced_widget::core::renderer::Renderer,
{
    NodePin::new(side, content)
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
/// // Full syntax: side, content, direction, pin_type, color
/// pin!(Right, text("output"), Output, "email", Color::from_rgb(0.3, 0.7, 0.9))
///
/// // Without color (uses default gray)
/// pin!(Left, text("input"), Input, "string")
///
/// // Minimal (side + content only, defaults: Both direction, "any" type)
/// pin!(Right, text("data"))
/// ```
#[macro_export]
macro_rules! pin {
    // Full: side, content, direction, type, color
    ($side:ident, $content:expr, $dir:ident, $pin_type:expr, $color:expr) => {
        $crate::node_pin($crate::PinSide::$side, $content)
            .direction($crate::PinDirection::$dir)
            .pin_type($pin_type)
            .color($color)
    };

    // Without color: side, content, direction, type
    ($side:ident, $content:expr, $dir:ident, $pin_type:expr) => {
        $crate::node_pin($crate::PinSide::$side, $content)
            .direction($crate::PinDirection::$dir)
            .pin_type($pin_type)
    };

    // Minimal: side, content only
    ($side:ident, $content:expr) => {
        $crate::node_pin($crate::PinSide::$side, $content)
    };
}
