//! Catalog Node
//!
//! The sink of every config chain: one input per `iced_nodegraph::Catalog`
//! class and status. A config arriving on an input becomes that class's overlay
//! for the frame; the row shows "ok" while it is connected.

use std::any::TypeId;
use std::collections::HashSet;

use demo_common::NodeContentStyle;
use iced::widget::{column, container, text};
use iced_nodegraph::pin;

use crate::ids::PinLabel;
use crate::nodes::{node_title_bar, pin_row, pins, value_display};

/// The data-type marker each Catalog input accepts.
fn input_type(label: PinLabel) -> TypeId {
    use pins::cfg;
    match label {
        cfg::NODE_CONFIG | cfg::NODE_SELECTED => TypeId::of::<pins::NodeConfigData>(),
        cfg::PIN_CONFIG | cfg::PIN_VALID_TARGET => TypeId::of::<pins::PinConfigData>(),
        cfg::EDGE_CONFIG | cfg::EDGE_PENDING_CUT | cfg::DRAG_EDGE => {
            TypeId::of::<pins::EdgeConfigData>()
        }
        cfg::ANCHOR | cfg::ANCHOR_HOVERED | cfg::ANCHOR_VALID_TARGET => {
            TypeId::of::<pins::AnchorConfigData>()
        }
        cfg::GRAPH_CONFIG => TypeId::of::<pins::GraphConfigData>(),
        cfg::SELECTION_BOX => TypeId::of::<pins::SelectionBoxConfigData>(),
        cfg::CUTTING_TOOL => TypeId::of::<pins::CuttingToolConfigData>(),
        cfg::MINIMAP => TypeId::of::<pins::MinimapConfigData>(),
        other => unreachable!("not a Catalog input: {other}"),
    }
}

/// Creates the Catalog node; `connected` lists the inputs that received a
/// config this propagation.
pub fn catalog_node<'a, Message>(
    theme: &'a iced::Theme,
    connected: &HashSet<PinLabel>,
) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    let style = NodeContentStyle::output(theme);

    let rows = pins::cfg::CATALOG_INPUTS.iter().map(|&label| {
        let status = if connected.contains(label) {
            "ok"
        } else {
            "--"
        };
        pin_row(
            pin!(Left, label, text(label).size(10), Input, input_type(label)),
            value_display(status),
        )
        .into()
    });

    let content = column(rows).spacing(4);

    column![
        node_title_bar("Catalog", style),
        container(content).padding([8, 10])
    ]
    .width(180.0)
    .into()
}
