use iced::advanced::{
    Clipboard, Layout, Shell, Widget,
    layout, mouse, renderer,
    widget::{tree, Tree},
};
use iced::{
    Element, Event, Length, Rectangle, Size,
};

/// An edge to attach a `NodePinWidget` to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PinSide {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
    Row,
}

/// A transparent wrapper used as a marker within `NodeGraph`.
pub struct NodePin<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    pub side: PinSide,
    pub content: Element<'a, Message, Theme, Renderer>,
}

impl<'a, Message, Theme, Renderer> NodePin<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    pub fn new(
        side: PinSide,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            side,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct NodePinState {
    pub side: PinSide,
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
        })
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let content_layout = self.content.as_widget().layout(&mut tree.children[0], renderer, limits);
        let size = content_layout.size();
        layout::Node::with_children(size, vec![content_layout])
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
