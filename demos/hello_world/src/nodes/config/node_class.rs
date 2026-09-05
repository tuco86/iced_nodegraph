//! Node Class Node
//!
//! Assigns a node config to exactly one workflow node, the demo's counterpart
//! of `iced_nodegraph::Node::class`: the target's style is the connected config
//! layered over the Catalog's node class.

use std::fmt;

use demo_common::NodeContentStyle;
use iced::{
    Length,
    widget::{column, container, pick_list, row, text},
};
use iced_nodegraph::pin;

use crate::ids::NodeId;
use crate::nodes::{node_title_bar, pin_row, pins, value_display};

/// A workflow node the Node Class node can target, as the pick list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassCandidate {
    pub id: NodeId,
    pub label: String,
}

impl fmt::Display for ClassCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// Creates a Node Class node: one node-config input and a pick list over the
/// workflow nodes it may target.
pub fn node_class_node<'a, Message>(
    theme: &'a iced::Theme,
    has_node_config: bool,
    target: Option<&NodeId>,
    candidates: Vec<ClassCandidate>,
    on_target: impl Fn(Option<NodeId>) -> Message + Clone + 'a,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let config_row = pin_row(
        pin!(
            Left,
            pins::cfg::NODE_CONFIG,
            text("node").size(10),
            Input,
            ::std::any::TypeId::of::<pins::NodeConfigData>()
        ),
        value_display(if has_node_config { "ok" } else { "--" }),
    );

    let selected = target.and_then(|id| candidates.iter().find(|c| &c.id == id).cloned());
    let picker = pick_list(candidates, selected, move |c: ClassCandidate| {
        on_target(Some(c.id))
    })
    .placeholder("target")
    .text_size(10)
    .width(Length::Fill);

    let target_row = row![
        text("target").size(10),
        container(picker).width(Length::Fill)
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let content = column![config_row, target_row].spacing(4);

    column![
        node_title_bar("Node Class", style),
        container(content).padding([8, 10])
    ]
    .width(180.0)
    .into()
}
