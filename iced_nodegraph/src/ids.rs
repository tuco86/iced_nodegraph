//! Generic ID types for user-defined node, pin, and edge identification.
//!
//! The library internally uses `usize` indices for GPU buffer operations,
//! but users can work with their own ID types (UUID, enums, strings, etc.)
//! through bidirectional translation maps.

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
pub trait NodeId: Clone + Eq + Hash + Debug {}

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
pub trait PinId: Clone + Eq + Hash + Debug {}

/// Trait for user-defined edge identifiers.
///
/// Edge IDs are optional - you can use the library's auto-generated
/// indices if you don't need custom edge tracking.
pub trait EdgeId: Clone + Eq + Hash + Debug {}

// Blanket implementations for common types

impl NodeId for usize {}
impl PinId for usize {}
impl EdgeId for usize {}

impl NodeId for u32 {}
impl PinId for u32 {}
impl EdgeId for u32 {}

impl NodeId for u64 {}
impl PinId for u64 {}
impl EdgeId for u64 {}

impl NodeId for String {}
impl PinId for String {}
impl EdgeId for String {}

// UUID support would require the uuid crate as a dependency
// Users can implement the traits for uuid::Uuid in their own code

/// Bidirectional mapping between user IDs and internal `usize` indices.
///
/// Used internally by NodeGraph to translate between user-facing IDs
/// and GPU-friendly array indices.
#[derive(Debug, Clone)]
pub struct IdMap<T: Clone + Eq + Hash> {
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

    /// Returns the number of registered IDs.
    pub fn len(&self) -> usize {
        self.to_id.len()
    }

    /// Returns true if no IDs are registered.
    pub fn is_empty(&self) -> bool {
        self.to_id.is_empty()
    }

    /// Clears all registered IDs.
    pub fn clear(&mut self) {
        self.to_index.clear();
        self.to_id.clear();
    }

    /// Iterates over all (index, id) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.to_id.iter().enumerate()
    }
}

/// Combined ID maps for all three ID types in a node graph.
#[derive(Debug, Clone)]
pub struct IdMaps<N: NodeId, P: PinId, E: EdgeId> {
    pub nodes: IdMap<N>,
    pub pins: IdMap<(N, P)>,
    pub edges: IdMap<E>,
}

impl<N: NodeId, P: PinId, E: EdgeId> Default for IdMaps<N, P, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N: NodeId, P: PinId, E: EdgeId> IdMaps<N, P, E> {
    pub fn new() -> Self {
        Self {
            nodes: IdMap::new(),
            pins: IdMap::new(),
            edges: IdMap::new(),
        }
    }

    /// Registers a node ID and returns its internal index.
    pub fn register_node(&mut self, id: N) -> usize {
        self.nodes.register(id)
    }

    /// Registers a pin ID (within a node) and returns its global index.
    pub fn register_pin(&mut self, node_id: N, pin_id: P) -> usize {
        self.pins.register((node_id, pin_id))
    }

    /// Registers an edge ID and returns its internal index.
    pub fn register_edge(&mut self, id: E) -> usize {
        self.edges.register(id)
    }

    /// Looks up the internal node index.
    pub fn node_index(&self, id: &N) -> Option<usize> {
        self.nodes.index(id)
    }

    /// Looks up the user node ID from internal index.
    pub fn node_id(&self, index: usize) -> Option<&N> {
        self.nodes.id(index)
    }

    /// Looks up the internal pin index.
    pub fn pin_index(&self, node_id: &N, pin_id: &P) -> Option<usize> {
        self.pins.index(&(node_id.clone(), pin_id.clone()))
    }

    /// Looks up the user (node_id, pin_id) from internal index.
    pub fn pin_id(&self, index: usize) -> Option<&(N, P)> {
        self.pins.id(index)
    }

    /// Looks up the internal edge index.
    pub fn edge_index(&self, id: &E) -> Option<usize> {
        self.edges.index(id)
    }

    /// Looks up the user edge ID from internal index.
    pub fn edge_id(&self, index: usize) -> Option<&E> {
        self.edges.id(index)
    }

    /// Clears all ID mappings.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.pins.clear();
        self.edges.clear();
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
        assert_eq!(map.len(), 1);
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum TestNodeId {
        NodeA,
        NodeB,
    }
    impl NodeId for TestNodeId {}

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum TestPinId {
        Input,
        Output,
    }
    impl PinId for TestPinId {}

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct TestEdgeId(u32);
    impl EdgeId for TestEdgeId {}

    #[test]
    fn test_id_maps_combined() {
        let mut maps: IdMaps<TestNodeId, TestPinId, TestEdgeId> = IdMaps::new();

        let node_a = maps.register_node(TestNodeId::NodeA);
        let node_b = maps.register_node(TestNodeId::NodeB);

        let pin_a_in = maps.register_pin(TestNodeId::NodeA, TestPinId::Input);
        let pin_a_out = maps.register_pin(TestNodeId::NodeA, TestPinId::Output);
        let pin_b_in = maps.register_pin(TestNodeId::NodeB, TestPinId::Input);

        let edge = maps.register_edge(TestEdgeId(1));

        assert_eq!(node_a, 0);
        assert_eq!(node_b, 1);
        assert_eq!(pin_a_in, 0);
        assert_eq!(pin_a_out, 1);
        assert_eq!(pin_b_in, 2);
        assert_eq!(edge, 0);

        assert_eq!(maps.node_id(0), Some(&TestNodeId::NodeA));
        assert_eq!(
            maps.pin_id(1),
            Some(&(TestNodeId::NodeA, TestPinId::Output))
        );
    }
}
