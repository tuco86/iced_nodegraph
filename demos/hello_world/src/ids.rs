//! ID generation for nodes and edges using NanoID.
//!
//! Uses a custom 58-character alphabet that excludes visually ambiguous characters
//! (0/O, 1/l/I) for better readability and copy-paste reliability.

use iced_nodegraph::Ids;
use nanoid::nanoid;

/// Custom alphabet: 57 chars, URL-safe, no ambiguous characters (0/O, 1/l/I excluded).
/// 57^10 = 3.6e17 possible IDs - collision probability negligible.
const ALPHABET: [char; 57] = [
    '2', '3', '4', '5', '6', '7', '8', '9', // 8 digits (no 0, 1)
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'N', // 13 uppercase (no I)
    'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', // 11 more uppercase (no O)
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', // 11 lowercase
    'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y',
    'z', // 14 more lowercase (no l)
];

/// Fixed length for all generated IDs.
const ID_LENGTH: usize = 10;

/// Type alias for node identifiers.
pub type NodeId = String;

/// Type alias for edge identifiers.
pub type EdgeId = String;

/// Type alias for pin labels (unique within a node).
/// Uses &'static str for compile-time pin labels defined as constants.
pub type PinLabel = &'static str;

/// The demo's id vocabulary for [`iced_nodegraph`]: nanoid strings for nodes
/// and edges, `&'static str` pin labels, and a [`std::any::TypeId`] per pin as
/// the data-type marker connections are validated against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HelloIds;

impl Ids for HelloIds {
    type NodeId = NodeId;
    type PinId = PinLabel;
    type EdgeId = EdgeId;
    type AnchorId = usize;
    type Payload = std::any::TypeId;
}

/// An edge in memory: the two endpoints it wires, by node id and pin label,
/// and the anchors it wraps.
///
/// The route is a set of anchor ids in no particular order - the widget derives
/// which way round the cable meets them - so `update` only ever adds to it or
/// removes from it.
///
/// Distinct from `persistence::SavedEdge`, which carries owned `String` pin
/// labels because a label read back from disk is not `'static`.
#[derive(Debug, Clone)]
pub struct EdgeData {
    pub from_node: NodeId,
    pub from_pin: PinLabel,
    pub to_node: NodeId,
    pub to_pin: PinLabel,
    pub route: Vec<usize>,
}

impl EdgeData {
    /// An unrouted edge between two pins.
    pub fn new(from_node: NodeId, from_pin: PinLabel, to_node: NodeId, to_pin: PinLabel) -> Self {
        Self {
            from_node,
            from_pin,
            to_node,
            to_pin,
            route: Vec::new(),
        }
    }
}

/// Generates a new unique node ID.
pub fn generate_node_id() -> NodeId {
    nanoid!(ID_LENGTH, &ALPHABET)
}

/// Generates a new unique edge ID.
pub fn generate_edge_id() -> EdgeId {
    nanoid!(ID_LENGTH, &ALPHABET)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_id_length() {
        let id = generate_node_id();
        assert_eq!(id.len(), ID_LENGTH);
    }

    /// Validates that an ID has the correct format.
    pub fn is_valid_id(id: &str) -> bool {
        id.len() == ID_LENGTH && id.chars().all(|c| ALPHABET.contains(&c))
    }

    #[test]
    fn test_id_alphabet() {
        for _ in 0..1000 {
            let id = generate_node_id();
            assert!(is_valid_id(&id), "Invalid ID generated: {}", id);
        }
    }

    #[test]
    fn test_id_uniqueness() {
        let mut ids = HashSet::new();
        for _ in 0..10_000 {
            let id = generate_node_id();
            assert!(ids.insert(id.clone()), "Duplicate ID: {}", id);
        }
    }

    #[test]
    fn test_node_and_edge_ids_different() {
        // Both functions use same algorithm, but we verify they work independently
        let node_id = generate_node_id();
        let edge_id = generate_edge_id();
        assert_eq!(node_id.len(), edge_id.len());
        assert!(is_valid_id(&node_id));
        assert!(is_valid_id(&edge_id));
    }

    #[test]
    fn test_validation_rejects_invalid() {
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("short"));
        assert!(!is_valid_id("wayyyyyyyyyytoolong"));
        assert!(!is_valid_id("contains0O")); // Contains excluded chars
        assert!(!is_valid_id("contains1l"));
        assert!(!is_valid_id("containsII"));
    }
}
