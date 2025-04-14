use iced::Element;
pub use node_grapgh::NodeGraph;
pub use node_pin::{NodePin, PinSide};

mod node;
mod node_grapgh;
mod node_pin;

pub fn node_graph<'a, Message, Theme, Renderer>() -> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    NodeGraph::default()
}

pub fn node_pin<'a, Message, Theme, Renderer>(side: PinSide, content: impl Into<Element<'a, Message, Theme, Renderer>>) -> NodePin<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    NodePin {
        side,
        content: content.into(),
    }
}
