//! Generic ID types for user-defined node and pin identification.
//!
//! The library internally uses `usize` indices for GPU buffer operations,
//! but users can work with their own ID types (UUID, enums, strings, etc.)
//! through a bidirectional translation map.

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

/// Trait for user-defined node identifiers.
///
/// Implement this trait on your own types to use them as node IDs:
/// ```rust
/// use iced_nodegraph::NodeId;
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum MyNodeId {
///     Input,
///     Process,
///     Output,
/// }
///
/// impl NodeId for MyNodeId {}
/// ```
pub trait NodeId: Clone + Eq + Hash + Debug + Send + Sync {}

/// Trait for user-defined pin identifiers.
///
/// Pins are identified within the context of a node, so you typically
/// use a per-node-type enum:
/// ```rust
/// use iced_nodegraph::PinId;
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum MathNodePins {
///     InputA,
///     InputB,
///     Output,
/// }
///
/// impl PinId for MathNodePins {}
/// ```
pub trait PinId: Clone + Eq + Hash + Debug + Send + Sync {}

// Blanket implementations for common types

impl NodeId for usize {}
impl PinId for usize {}

impl NodeId for u32 {}
impl PinId for u32 {}

impl NodeId for u64 {}
impl PinId for u64 {}

impl NodeId for String {}
impl PinId for String {}

impl NodeId for &'static str {}
impl PinId for &'static str {}

// UUID support would require the uuid crate as a dependency
// Users can implement the traits for uuid::Uuid in their own code

/// Bidirectional mapping between user node IDs and internal `usize` indices.
///
/// Used internally by NodeGraph to translate between user-facing IDs
/// and GPU-friendly array indices.
#[derive(Debug, Clone)]
pub(crate) struct IdMap<T: Clone + Eq + Hash> {
    /// User ID → internal index
    to_index: HashMap<T, usize>,
    /// Internal index → user ID
    to_id: Vec<T>,
}

impl<T: Clone + Eq + Hash> Default for IdMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Eq + Hash> IdMap<T> {
    /// Creates an empty ID map.
    pub fn new() -> Self {
        Self {
            to_index: HashMap::new(),
            to_id: Vec::new(),
        }
    }

    /// Registers an ID and returns its internal index.
    ///
    /// If the ID already exists, returns the existing index.
    pub fn register(&mut self, id: T) -> usize {
        if let Some(&idx) = self.to_index.get(&id) {
            return idx;
        }
        let idx = self.to_id.len();
        self.to_id.push(id.clone());
        self.to_index.insert(id, idx);
        idx
    }

    /// Looks up the internal index for a user ID.
    pub fn index(&self, id: &T) -> Option<usize> {
        self.to_index.get(id).copied()
    }

    /// Looks up the user ID for an internal index.
    pub fn id(&self, index: usize) -> Option<&T> {
        self.to_id.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_map_basic() {
        let mut map: IdMap<String> = IdMap::new();

        let idx0 = map.register("first".to_string());
        let idx1 = map.register("second".to_string());

        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
        assert_eq!(map.index(&"first".to_string()), Some(0));
        assert_eq!(map.id(1), Some(&"second".to_string()));
    }

    #[test]
    fn test_id_map_duplicate_register() {
        let mut map: IdMap<u32> = IdMap::new();

        let idx0 = map.register(42);
        let idx1 = map.register(42);

        assert_eq!(idx0, idx1);
        // A distinct id takes the next slot, proving 42 occupied exactly one.
        assert_eq!(map.register(43), 1);
    }
}
