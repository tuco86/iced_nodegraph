use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Shadow, Size, Vector,
    advanced::{
        Clipboard, Layout, Shell, layout, mouse, overlay, renderer,
        widget::{self, Tree, tree},
    },
    border,
};

use crate::node;

use super::NodeGraph;

impl<Message, Theme, Renderer> iced::advanced::Widget<Message, Theme, Renderer>
    for NodeGraph<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    fn size(&self) -> Size<Length> {
        self.size
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(self.size.width).height(self.size.height);
        let size = limits.resolve(self.size.width, self.size.height, Size::ZERO);
        let nodes = self
            .elements_iter()
            .zip(&mut tree.children)
            .map(|((position, element), state)| {
                element
                    .as_widget()
                    .layout(state, renderer, &limits)
                    .move_to(position)
            })
            .collect();
        layout::Node::with_children(size, nodes)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for (node_index, (((_position, element), state), layout)) in self
            .elements_iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
        {
            let offset = self
                .dragging_node
                .filter(|(i, o)| *i == node_index)
                .map_or(Vector::ZERO, |(_, o)| cursor.position().unwrap() - o);
            renderer.with_translation(offset, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: layout.bounds(),
                        border: border::Border {
                            color: Color::WHITE,
                            width: 1.0,
                            radius: border::Radius::new(5.0),
                        },
                        ..Default::default()
                    },
                    Background::Color(Color::from_rgb(0.1, 0.15, 0.13)),
                );

                element
                    .as_widget()
                    .draw(state, renderer, theme, style, layout, cursor, viewport);
            });
        }
    }

    fn size_hint(&self) -> Size<Length> {
        self.size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn state(&self) -> tree::State {
        tree::State::None
    }

    fn children(&self) -> Vec<Tree> {
        self.elements_iter()
            .map(|(_, element)| Tree::new(element))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let children: Vec<&Element<'_, Message, Theme, Renderer>> =
            self.elements_iter().map(|(_, e)| e).collect();
        tree.diff_children(&children);
    }

    fn operate(
        &self,
        _state: &mut Tree,
        _layout: Layout<'_>,
        _renderer: &Renderer,
        _operation: &mut dyn widget::Operation,
    ) {
    }

    fn update(
        &mut self,
        _state: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if self.dragging_node.is_none() =>
            {
                self.dragging_node =
                    layout
                        .children()
                        .enumerate()
                        .find_map(|(node_index, layout)| {
                            if cursor.is_over(layout.bounds()) {
                                if let Some(origin) = cursor.position() {
                                    println!("clicked node {:?} at {:?}", node_index, origin);
                                    shell.capture_event();
                                    Some((node_index, origin))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        });
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if self.dragging_node.is_some() =>
            {
                let (node_index, origin) = self.dragging_node.take().unwrap();
                let offset = cursor.position().unwrap() - origin;
                self.nodes[node_index].0 = self.nodes[node_index].0 + offset;
                println!(
                    "dropped node {:?} at {:?}",
                    node_index, self.nodes[node_index].0
                );
                shell.invalidate_layout();
                shell.request_redraw();
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if self.dragging_node.is_some() => {
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        for ((_, element), layout) in self.elements_iter().zip(layout.children()) {
            let bounds = layout.bounds();
            if cursor.is_over(bounds) {
                let interaction = element
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer);
                return if interaction != mouse::Interaction::default() {
                    interaction
                } else {
                    mouse::Interaction::Grabbing
                };
            }
        }
        mouse::Interaction::default()
    }

    fn overlay<'a>(
        &'a mut self,
        _state: &'a mut Tree,
        _layout: Layout<'_>,
        _renderer: &Renderer,
        _translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        None
    }
}

impl<'a, Message, Theme, Renderer> From<NodeGraph<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer + 'a,
    Message: 'static,
    Theme: 'a,
{
    fn from(grid: NodeGraph<'a, Message, Theme, Renderer>) -> Self {
        Element::new(grid)
    }
}
