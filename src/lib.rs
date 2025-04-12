use node_grapgh::NodeGraph;

mod node;
mod node_grapgh;
mod pin;

pub fn node_graph<'a, Message, Theme, Renderer>() -> NodeGraph<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    NodeGraph::default()
}
