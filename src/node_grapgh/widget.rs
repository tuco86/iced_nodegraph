use iced::{
    advanced::{
        layout, mouse, renderer, widget::{self, tree, Tree}, Clipboard, Layout, Shell
    }, border, Background, Color, Element, Event, Length, Point, Rectangle, Size, Vector
};

use crate::{node_pin::NodePinState, PinSide};
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
                .filter(|(i, _)| *i == node_index)
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

                let pins = find_pins(state, layout);
                // let pins: Vec<(&NodePinState, Layout<'_>)> = vec![];

                // find node_pin elements in layouy children
                for (pin_state, pin_layout) in pins
                {
                    // use renderer.fill_quad to draw a circle around a point at the center of the pin but moved to the border of the node.
                    let pin_center = pin_layout.bounds().center();
                    let node_bounds = layout.bounds();
                    let pin_radius = 5.0;
                    let pin_size = Size::new(pin_radius * 2.0, pin_radius * 2.0);
                    let pin_offset = Vector::new(-pin_size.width / 2.0, -pin_size.height / 2.0);
                    let pin_position = match pin_state.side {
                        PinSide::Row | // TODO: handle row pin correctly (a pin to the left and right of the node)
                        PinSide::Left => Point::new(node_bounds.x + 0.5, pin_center.y),
                        PinSide::Right => Point::new(node_bounds.x + node_bounds.width - 0.5, pin_center.y),
                        PinSide::Top => Point::new(pin_center.x, node_bounds.y + 0.5),
                        PinSide::Bottom => Point::new(pin_center.x, node_bounds.y + node_bounds.height - 0.5),
                    };
                    let pin_rectangle = Rectangle::new(pin_position + pin_offset, pin_size);
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: pin_rectangle,
                            border: border::Border {
                                color: Color::WHITE,
                                width: 1.0,
                                radius: border::Radius::new(pin_radius),
                            },
                            ..Default::default()
                        },
                        Background::Color(Color::from_rgb(0.1, 0.15, 0.13)),
                    );
                }
            });
        }
    }

    fn size_hint(&self) -> Size<Length> {
        self.size()
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
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        for (((_, element), node_tree), node_layout) in self
            .elements_iter()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            element.as_widget().operate(
                node_tree,
                node_layout,
                renderer,
                operation,
            );
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
        for (((_, element), tree), layout) in self.elements_iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            element.as_widget_mut().update(
                tree,
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }

        if shell.is_event_captured() {
            return;
        }
        
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
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        for (((_, element), tree), layout) in self.elements_iter().zip(&tree.children).zip(layout.children()) {
            let bounds = layout.bounds();
            if cursor.is_over(bounds) {
                let interaction = element
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer);
                if interaction != mouse::Interaction::None {
                    return interaction;
                }
            }
        }

        if self.dragging_node.is_some() {
            mouse::Interaction::Grabbing
        } else if self.dragging_edge.is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::Idle
        }
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

//// Helper function to find all NodePin elements in the tree
fn find_pins<'a>(
    tree: &'a Tree,
    layout: Layout<'a>,
) -> Vec<(&'a NodePinState, Layout<'a>)> {
    let mut flat = Vec::new();
    inner_find_pins(&mut flat, tree, layout);
    flat
}

fn inner_find_pins<'a>(
    flat: &mut Vec<(&'a NodePinState, Layout<'a>)>,
    tree: &'a Tree,
    layout: Layout<'a>,
) {
    if tree.tag == tree::Tag::of::<NodePinState>() {
        match tree.state {
            tree::State::None => {}
            tree::State::Some(ref state) => {
                let state = state.downcast_ref::<NodePinState>().expect("what?");
                flat.push((state, layout));
            }
        }
        // let state = tree.state.downcast_ref::<NodePinState>();
        // flat.push((state, layout));
    }

    for (child_tree, child_layout) in tree.children.iter().zip(layout.children()) {
        inner_find_pins(flat, child_tree, child_layout);
    }
}
