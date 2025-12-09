mod calendar;
mod email_parser;
mod email_trigger;
mod filter;

pub use calendar::calendar_node;
pub use email_parser::email_parser_node;
pub use email_trigger::email_trigger_node;
pub use filter::filter_node;

use iced::Theme;

/// Creates a node element based on the node type name.
pub fn node<'a, Message>(node_type: &str, theme: &'a Theme) -> iced::Element<'a, Message>
where
    Message: Clone + 'a,
{
    match node_type {
        "email_trigger" => email_trigger_node(theme),
        "email_parser" => email_parser_node(theme),
        "filter" => filter_node(theme),
        "calendar" => calendar_node(theme),
        _ => email_trigger_node(theme), // fallback
    }
}
