//! Composable predicates for [`NodeGraph::can_connect`](crate::NodeGraph::can_connect).
//!
//! `can_connect` is authoritative: setting it REPLACES the built-in validation, so
//! a closure that only checks pin payloads would also re-allow same-direction and
//! self-node connections. These helpers let a closure opt the built-in rules back in
//! one call at a time, and [`default_can_connect`] bundles the set the widget applies
//! when no `can_connect` is set.
//!
//! ```rust,ignore
//! use iced_nodegraph::connection::{default_can_connect, direction_ok};
//!
//! // Keep every built-in rule, add a payload check:
//! ng.can_connect(|from, to| default_can_connect(from, to) && from.info() == to.info());
//!
//! // Or pick individual rules (here: direction only, allowing a second edge per input):
//! ng.can_connect(direction_ok);
//! ```

use crate::node_pin::{PinDirection, PinEnd};

/// Returns `true` if the two pin directions are compatible: `Output` to `Input`
/// (either order) or any pairing involving `Both`.
pub fn direction_ok<N, P, UI>(from: PinEnd<'_, N, P, UI>, to: PinEnd<'_, N, P, UI>) -> bool {
    matches!(
        (from.direction(), to.direction()),
        (PinDirection::Both, _)
            | (_, PinDirection::Both)
            | (PinDirection::Output, PinDirection::Input)
            | (PinDirection::Input, PinDirection::Output)
    )
}

/// Returns `true` if the two pins live on different nodes, rejecting a node wiring
/// back into itself.
pub fn not_same_node<N, P, UI>(from: PinEnd<'_, N, P, UI>, to: PinEnd<'_, N, P, UI>) -> bool
where
    N: PartialEq,
{
    from.node_id() != to.node_id()
}

/// Returns `true` unless `to` is an `Input` pin that already holds an edge.
///
/// Enforces "one edge per input" - the rule that bites hosts, since a single drag
/// fires `on_connect` on every snap. Inputs are single-slot; `Output` and `Both`
/// pins fan out and always pass. The edge currently being dragged does not count as
/// occupying, so re-routing a connection back onto its own input still works.
pub fn input_not_occupied<N, P, UI>(to: PinEnd<'_, N, P, UI>) -> bool {
    !(matches!(to.direction(), PinDirection::Input) && to.is_occupied())
}

/// The built-in connection rule: [`direction_ok`] and [`not_same_node`] and
/// [`input_not_occupied`].
///
/// This is exactly what the widget applies when no `can_connect` is set. Compose it
/// to keep every built-in rule while adding your own, e.g.
/// `ng.can_connect(|from, to| default_can_connect(from, to) && my_check(from, to))`.
/// To allow a second edge per input (replace-on-drop), omit this and use
/// `direction_ok`/`not_same_node` directly, deduplicating by input in `on_connect`.
pub fn default_can_connect<N, P, UI>(from: PinEnd<'_, N, P, UI>, to: PinEnd<'_, N, P, UI>) -> bool
where
    N: PartialEq,
{
    direction_ok(from, to) && not_same_node(from, to) && input_not_occupied(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(
        node: &'static usize,
        dir: PinDirection,
        occupied: bool,
    ) -> PinEnd<'static, usize, usize> {
        // Pin id and payload are irrelevant to these predicates.
        PinEnd::new(node, &0, dir, &(), occupied)
    }

    #[test]
    fn direction_ok_matches_output_input_and_both() {
        let out = pin(&0, PinDirection::Output, false);
        let inp = pin(&1, PinDirection::Input, false);
        let both = pin(&2, PinDirection::Both, false);
        assert!(direction_ok(out, inp));
        assert!(direction_ok(inp, out));
        assert!(direction_ok(out, both));
        assert!(!direction_ok(out, out)); // output -> output
        assert!(!direction_ok(inp, inp)); // input -> input
    }

    #[test]
    fn not_same_node_rejects_self_wiring() {
        let a = pin(&0, PinDirection::Output, false);
        let b = pin(&0, PinDirection::Input, false);
        let c = pin(&1, PinDirection::Input, false);
        assert!(!not_same_node(a, b));
        assert!(not_same_node(a, c));
    }

    #[test]
    fn input_not_occupied_only_limits_inputs() {
        let busy_in = pin(&0, PinDirection::Input, true);
        let free_in = pin(&0, PinDirection::Input, false);
        let busy_out = pin(&0, PinDirection::Output, true);
        let busy_both = pin(&0, PinDirection::Both, true);
        assert!(!input_not_occupied(busy_in));
        assert!(input_not_occupied(free_in));
        assert!(input_not_occupied(busy_out)); // outputs fan out
        assert!(input_not_occupied(busy_both)); // both is not single-slot
    }

    #[test]
    fn default_bundles_all_three() {
        let out = pin(&0, PinDirection::Output, false);
        let free_in = pin(&1, PinDirection::Input, false);
        let busy_in = pin(&1, PinDirection::Input, true);
        let same_node_in = pin(&0, PinDirection::Input, false);
        assert!(default_can_connect(out, free_in));
        assert!(!default_can_connect(out, busy_in)); // occupied input
        assert!(!default_can_connect(out, same_node_in)); // self-wiring
    }
}
