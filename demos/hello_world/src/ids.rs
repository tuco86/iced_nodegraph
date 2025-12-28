//! ID generation for nodes and edges using NanoID.
//!
//! Uses a custom 58-character alphabet that excludes visually ambiguous characters
//! (0/O, 1/l/I) for better readability and copy-paste reliability.

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
