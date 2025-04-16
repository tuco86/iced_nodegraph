use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Size, Vector,
    advanced::{
        Clipboard, Layout, Shell, layout, mouse, renderer,
        widget::{self, Tree, tree},
    },
    border,
};

use super::{
    NodeGraph,
    euclid::IntoIced,
    state::{Dragging, NodeGraphState},
};
use crate::{
    PinSide,
    node_grapgh::euclid::{IntoEuclid, ScreenPoint, WorldPoint},
    node_pin::NodePinState,
};

impl<Message, Theme, Renderer> iced::advanced::Widget<Message, Theme, Renderer>
    for NodeGraph<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<NodeGraphState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(NodeGraphState::default())
    }

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
            .map(|((position, element), node_tree)| {
                element
                    .as_widget()
                    .layout(node_tree, renderer, &limits)
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
        let state = tree.state.downcast_ref::<NodeGraphState>();
        let graph_move_offset = if let Dragging::Graph(origin) = state.dragging {
            cursor
                .position()
                .map(|cursor_position| cursor_position - origin.into_iced())
        } else {
            None
        }
        .unwrap_or(Vector::ZERO);
        // println!("dragging: {:?} cursor_position {:?} graph_move_offset: {:?}", state.dragging, cursor.position(), graph_move_offset);
        state
            .camera
            .with_extra_offset(graph_move_offset)
            .draw_with::<_, Renderer>(renderer, viewport, cursor, |renderer, viewport, cursor| {
                // println!("camera: {:?}", state.camera);
                // println!("cursor: {:?}", cursor);
                // println!("viewport: {:?}", viewport);
                // println!("state.offset: {:?}", state.offset);
                // println!("state.zoom: {:?}", state.zoom);

                for (node_index, (((_position, element), tree), layout)) in self
                    .elements_iter()
                    .zip(&tree.children)
                    .zip(layout.children())
                    .enumerate()
                {
                    let node_move_offset =
                        if let Dragging::Node(dragging_node_index, origin) = state.dragging {
                            cursor
                                .position()
                                .filter(|_| dragging_node_index == node_index)
                                .map(|cursor_position| cursor_position - origin.into_iced())
                        } else {
                            None
                        }
                        .unwrap_or(Vector::ZERO);
                    renderer.with_translation(node_move_offset, |renderer| {
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
                            .draw(tree, renderer, theme, style, layout, cursor, &viewport);

                        let pins = find_pins(tree, layout);
                        // let pins: Vec<(&NodePinState, Layout<'_>)> = vec![];

                        // println!("pins: {:?}", pins.len());

                        // find node_pin elements in layouy children
                        for (pin_index, pin_state, pin_layout, _) in pins {
                            // println!("pin_index: {:?}", pin_index);
                            // use renderer.fill_quad to draw a circle around a point at the center of the pin but moved to the border of the node.
                            let pin_radius = 5.0;
                            let pin_size = Size::new(pin_radius * 2.0, pin_radius * 2.0);
                            let pin_offset =
                                Vector::new(-pin_size.width / 2.0, -pin_size.height / 2.0);
                            let (a, b) = pin_positions(pin_state.side, layout.bounds());
                            for pin_position in [a, b] {
                                let pin_rectangle =
                                    Rectangle::new(pin_position + pin_offset, pin_size);
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
                        }
                    });
                }
            });
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
            element
                .as_widget()
                .operate(node_tree, node_layout, renderer, operation);
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        screen_cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<NodeGraphState>();
        let graph_move_offset = if let Dragging::Graph(origin) = state.dragging {
            screen_cursor
                .position()
                .map(|cursor_position| cursor_position - origin.into_iced())
        } else {
            None
        }
        .unwrap_or(Vector::ZERO);
        state
            .camera
            .with_extra_offset(graph_move_offset)
            .update_with(viewport, screen_cursor, |viewport, world_cursor| {
                let state = tree.state.downcast_mut::<NodeGraphState>();
                // println!("camera: {:?}", state.camera);
                // println!("cursor: {:?}", cursor);
                // println!("viewport: {:?}", viewport);
                // println!("state.offset: {:?}", state.offset);
                // println!("state.zoom: {:?}", state.zoom);

                if state.dragging != Dragging::None {
                    match event {
                        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    }
                }

                match state.dragging {
                    Dragging::None => {}
                    Dragging::Graph(origin) => match event {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                            if let Some(cursor_position) = screen_cursor.position() {
                                let cursor_position: ScreenPoint = cursor_position.into_euclid();
                                let offset = cursor_position - origin;
                                state.camera.translate_screen(offset);
                            }
                            state.dragging = Dragging::None;
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    },
                    Dragging::Node(node_index, origin) => match event {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                            if let Some(cursor_position) = world_cursor.position() {
                                let cursor_position = cursor_position.into_euclid();
                                let offset = cursor_position - origin;
                                self.nodes[node_index].0 =
                                    self.nodes[node_index].0 + offset.into_iced();
                            }
                            state.dragging = Dragging::None;
                            shell.capture_event();
                            shell.invalidate_layout();
                            shell.request_redraw();
                        }
                        _ => {}
                    },
                    Dragging::Edge(_, _, point) => match event {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                            state.dragging = Dragging::None;
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    },
                }

                for (((_, element), tree), layout) in self
                    .elements_iter_mut()
                    .zip(&mut tree.children)
                    .zip(layout.children())
                {
                    element.as_widget_mut().update(
                        tree,
                        event,
                        layout,
                        world_cursor,
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
                    Event::Mouse(mouse::Event::WheelScrolled { delta, .. }) => {
                        if let Some(cursor_pos) = screen_cursor.position() {
                            let cursor_pos: ScreenPoint = cursor_pos.into_euclid();

                            let scroll_amount = match delta {
                                mouse::ScrollDelta::Pixels { y, .. } => *y,
                                mouse::ScrollDelta::Lines { y, .. } => *y * 10.0,
                            };

                            let zoom_delta = scroll_amount / 100.0;

                            state.camera.zoom_at(cursor_pos, zoom_delta);
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                        if let Some(cursor_position) = world_cursor.position() {
                            // check bounds for pins
                            for (node_index, (node_layout, node_tree)) in
                                layout.children().zip(&mut tree.children).enumerate()
                            {
                                for (pin_index, _, _, (a, b)) in find_pins(node_tree, node_layout) {
                                    let distance = a
                                        .distance(cursor_position)
                                        .min(b.distance(cursor_position));
                                    if distance < 5.0 {
                                        println!(
                                            "clicked pin {:?} on node {:?} at {:?}",
                                            pin_index, node_index, cursor_position
                                        );
                                        let state = tree.state.downcast_mut::<NodeGraphState>();
                                        state.dragging = Dragging::Edge(
                                            node_index,
                                            pin_index,
                                            cursor_position.into_euclid(),
                                        );
                                        shell.capture_event();
                                        return;
                                    }
                                }
                            }
                            // check bounds for nodes
                            for (node_index, node_layout) in layout.children().enumerate() {
                                if world_cursor.is_over(node_layout.bounds()) {
                                    println!(
                                        "clicked node {:?} at {:?}",
                                        node_index, cursor_position
                                    );
                                    let state = tree.state.downcast_mut::<NodeGraphState>();
                                    state.dragging =
                                        Dragging::Node(node_index, cursor_position.into_euclid());
                                    shell.capture_event();
                                    return;
                                }
                            }
                        }
                        if let Some(cursor_position) = screen_cursor.position() {
                            // else drag the whole graph
                            println!("clicked graph at {:?}", cursor_position);
                            let state = tree.state.downcast_mut::<NodeGraphState>();
                            state.dragging = Dragging::Graph(cursor_position.into_euclid());
                            shell.capture_event();
                            return;
                        }
                    }
                    _ => {}
                }
            });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if let Some(cursor_position) = cursor.position() {
            let state = tree.state.downcast_ref::<NodeGraphState>();
            let cursor_position = state.camera.screen_to_world(cursor_position);

            for (_, state, _, (a, b)) in find_pins(tree, layout) {
                let distance = a
                    .into_euclid()
                    .distance_to(cursor_position)
                    .min(b.into_euclid().distance_to(cursor_position));
                if distance < 5.0 {
                    return match state.side {
                        PinSide::Row => mouse::Interaction::Crosshair,
                        PinSide::Left | PinSide::Right => mouse::Interaction::ResizingHorizontally,
                        PinSide::Top | PinSide::Bottom => mouse::Interaction::ResizingVertically,
                    };
                }
            }

            for (((_, element), tree), layout) in self
                .elements_iter()
                .zip(&tree.children)
                .zip(layout.children())
            {
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

            let state = tree.state.downcast_ref::<NodeGraphState>();
            match state.dragging {
                Dragging::None => mouse::Interaction::default(),
                Dragging::Graph(_) => mouse::Interaction::Grabbing,
                Dragging::Node(_, _) => mouse::Interaction::Grabbing,
                Dragging::Edge(_, _, _) => mouse::Interaction::Grabbing,
            }
        } else {
            mouse::Interaction::default()
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

pub fn node_graph<'a, Message, Theme, Renderer>() -> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    NodeGraph::default()
}

//// Helper function to find all NodePin elements in the tree - OF A Node!!!
fn find_pins<'a>(
    tree: &'a Tree,
    layout: Layout<'a>,
) -> Vec<(usize, &'a NodePinState, Layout<'a>, (Point, Point))> {
    let mut flat = Vec::new();
    let mut pin_index = 0;
    inner_find_pins(&mut flat, &mut pin_index, tree, layout, tree, layout);
    flat
}

fn inner_find_pins<'a>(
    flat: &mut Vec<(usize, &'a NodePinState, Layout<'a>, (Point, Point))>,
    pin_index: &mut usize,
    node_tree: &'a Tree,
    node_layout: Layout<'a>,
    pin_tree: &'a Tree,
    pin_layout: Layout<'a>,
) {
    if pin_tree.tag == tree::Tag::of::<NodePinState>() {
        println!("found pin: {:?}", pin_tree.tag);
        let pin_state = pin_tree.state.downcast_ref::<NodePinState>();
        let node_bounds = node_layout.bounds();
        let pin_positions = pin_positions(pin_state.side, node_bounds);
        flat.push((*pin_index, pin_state, pin_layout, pin_positions));
        *pin_index += 1;
    }

    for (child_layout, child_tree) in pin_layout.children().zip(&pin_tree.children) {
        inner_find_pins(
            flat,
            pin_index,
            node_tree,
            node_layout,
            child_tree,
            child_layout,
        );
    }
}

fn pin_positions(side: PinSide, node_bounds: Rectangle) -> (Point, Point) {
    if side == PinSide::Row {
        (
            pin_position(PinSide::Left, node_bounds),
            pin_position(PinSide::Right, node_bounds),
        )
    } else {
        let position = pin_position(side, node_bounds);
        (position, position)
    }
}

fn pin_position(side: PinSide, node_bounds: Rectangle) -> Point {
    match side {
        PinSide::Row => panic!("Row pin is supposed to be handled separately"),
        PinSide::Left => Point::new(node_bounds.x + 0.5, node_bounds.y + 0.5),
        PinSide::Right => Point::new(node_bounds.x + node_bounds.width - 0.5, node_bounds.y + 0.5),
        PinSide::Top => Point::new(node_bounds.x + 0.5, node_bounds.y + 0.5),
        PinSide::Bottom => Point::new(
            node_bounds.x + 0.5,
            node_bounds.y + node_bounds.height - 0.5,
        ),
    }
}
