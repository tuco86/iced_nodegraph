//! Dirty tracking system for incremental GPU buffer updates.
//!
//! This module provides efficient change detection to avoid unnecessary GPU uploads.
//! Only modified data is written to buffers, significantly reducing CPU-GPU bandwidth.

/// Efficient bit set for tracking dirty indices.
///
/// Uses 64-bit words for compact storage and fast iteration.
#[derive(Debug, Clone, Default)]
pub struct BitSet {
    bits: Vec<u64>,
    len: usize,
}

impl BitSet {
    /// Create a new BitSet with given capacity.
    pub fn new(capacity: usize) -> Self {
        let words = (capacity + 63) / 64;
        Self {
            bits: vec![0; words],
            len: capacity,
        }
    }

    /// Create an empty BitSet (capacity 0).
    pub fn empty() -> Self {
        Self {
            bits: Vec::new(),
            len: 0,
        }
    }

    /// Resize the BitSet to hold at least `new_len` elements.
    /// New elements are initialized to false (0).
    pub fn resize(&mut self, new_len: usize) {
        let words = (new_len + 63) / 64;
        self.bits.resize(words, 0);
        self.len = new_len;
    }

    /// Mark an index as dirty.
    pub fn insert(&mut self, index: usize) {
        if index >= self.len {
            self.resize(index + 1);
        }
        let word = index / 64;
        let bit = index % 64;
        self.bits[word] |= 1 << bit;
    }

    /// Check if an index is dirty.
    pub fn contains(&self, index: usize) -> bool {
        if index >= self.len {
            return false;
        }
        let word = index / 64;
        let bit = index % 64;
        (self.bits[word] & (1 << bit)) != 0
    }

    /// Remove an index from the set.
    pub fn remove(&mut self, index: usize) {
        if index < self.len {
            let word = index / 64;
            let bit = index % 64;
            self.bits[word] &= !(1 << bit);
        }
    }

    /// Check if the set is empty (no dirty indices).
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&w| w == 0)
    }

    /// Count the number of dirty indices.
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Clear all dirty flags.
    pub fn clear(&mut self) {
        for word in &mut self.bits {
            *word = 0;
        }
    }

    /// Mark all indices as dirty.
    pub fn set_all(&mut self, count: usize) {
        self.resize(count);
        let full_words = count / 64;
        for word in &mut self.bits[..full_words] {
            *word = u64::MAX;
        }
        let remaining = count % 64;
        if remaining > 0 && full_words < self.bits.len() {
            self.bits[full_words] = (1u64 << remaining) - 1;
        }
    }

    /// Iterate over all dirty indices.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        let len = self.len;
        self.bits
            .iter()
            .enumerate()
            .flat_map(move |(word_idx, &word)| {
                (0..64).filter_map(move |bit| {
                    let idx = word_idx * 64 + bit;
                    if idx < len && (word & (1 << bit)) != 0 {
                        Some(idx)
                    } else {
                        None
                    }
                })
            })
    }

    /// Get the capacity of the BitSet.
    pub fn capacity(&self) -> usize {
        self.len
    }
}

/// Tracks what has changed in the node graph since last GPU sync.
///
/// The dirty tracking system operates at multiple granularities:
/// - `structure_changed`: Nodes or edges were added/removed (requires full rebuild)
/// - `node_positions`: Per-node position changes (common during drag)
/// - `node_styles`: Per-node style changes (color, border, etc.)
/// - `edges`: Per-edge changes (color, routing, vertices)
/// - `uniforms`: Global state changes (camera, time, colors)
#[derive(Debug, Clone, Default)]
pub struct DirtyFlags {
    /// Structural change: node/edge added or removed.
    /// When true, all buffers need to be rebuilt.
    pub structure_changed: bool,

    /// Which nodes had position changes.
    /// Used for efficient partial updates during drag operations.
    pub node_positions: BitSet,

    /// Which nodes had style changes (color, border, etc.).
    pub node_styles: BitSet,

    /// Which edges changed (color, vertices, etc.).
    pub edges: BitSet,

    /// Uniform data changed (camera, time, global colors).
    pub uniforms: bool,

    /// Edge vertices changed (physics simulation).
    pub edge_vertices: BitSet,
}

impl DirtyFlags {
    /// Create new DirtyFlags with everything marked dirty.
    pub fn all_dirty(node_count: usize, edge_count: usize, vertex_count: usize) -> Self {
        let mut flags = Self::default();
        flags.mark_all_dirty(node_count, edge_count, vertex_count);
        flags
    }

    /// Mark everything as dirty (full rebuild required).
    pub fn mark_all_dirty(&mut self, node_count: usize, edge_count: usize, vertex_count: usize) {
        self.structure_changed = true;
        self.uniforms = true;
        self.node_positions.set_all(node_count);
        self.node_styles.set_all(node_count);
        self.edges.set_all(edge_count);
        self.edge_vertices.set_all(vertex_count);
    }

    /// Mark a single node's position as dirty.
    pub fn mark_node_position(&mut self, node_id: usize) {
        self.node_positions.insert(node_id);
    }

    /// Mark a single node's style as dirty.
    pub fn mark_node_style(&mut self, node_id: usize) {
        self.node_styles.insert(node_id);
    }

    /// Mark a single edge as dirty.
    pub fn mark_edge(&mut self, edge_id: usize) {
        self.edges.insert(edge_id);
    }

    /// Mark edge vertices as dirty (range of vertex indices).
    pub fn mark_edge_vertices(&mut self, start: usize, count: usize) {
        for i in start..start + count {
            self.edge_vertices.insert(i);
        }
    }

    /// Mark uniforms as dirty.
    pub fn mark_uniforms(&mut self) {
        self.uniforms = true;
    }

    /// Mark structural change (node/edge added or removed).
    pub fn mark_structure_changed(&mut self) {
        self.structure_changed = true;
    }

    /// Check if everything is clean (no updates needed).
    pub fn is_clean(&self) -> bool {
        !self.structure_changed
            && !self.uniforms
            && self.node_positions.is_empty()
            && self.node_styles.is_empty()
            && self.edges.is_empty()
            && self.edge_vertices.is_empty()
    }

    /// Check if any node-related data is dirty.
    pub fn has_node_changes(&self) -> bool {
        self.structure_changed || !self.node_positions.is_empty() || !self.node_styles.is_empty()
    }

    /// Check if any edge-related data is dirty.
    pub fn has_edge_changes(&self) -> bool {
        self.structure_changed || !self.edges.is_empty() || !self.edge_vertices.is_empty()
    }

    /// Clear all dirty flags after GPU sync.
    pub fn clear(&mut self) {
        self.structure_changed = false;
        self.uniforms = false;
        self.node_positions.clear();
        self.node_styles.clear();
        self.edges.clear();
        self.edge_vertices.clear();
    }

    /// Resize dirty tracking to accommodate new counts.
    /// Call this when the number of nodes/edges changes.
    pub fn resize(&mut self, node_count: usize, edge_count: usize, vertex_count: usize) {
        self.node_positions.resize(node_count);
        self.node_styles.resize(node_count);
        self.edges.resize(edge_count);
        self.edge_vertices.resize(vertex_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // BitSet tests
    #[test]
    fn test_bitset_new_is_empty() {
        let set = BitSet::new(100);
        assert!(set.is_empty());
        assert_eq!(set.count(), 0);
        assert_eq!(set.capacity(), 100);
    }

    #[test]
    fn test_bitset_insert_and_contains() {
        let mut set = BitSet::new(100);
        set.insert(0);
        set.insert(50);
        set.insert(99);

        assert!(set.contains(0));
        assert!(set.contains(50));
        assert!(set.contains(99));
        assert!(!set.contains(1));
        assert!(!set.contains(49));
        assert_eq!(set.count(), 3);
    }

    #[test]
    fn test_bitset_auto_resize() {
        let mut set = BitSet::new(10);
        set.insert(100); // Beyond initial capacity

        assert!(set.contains(100));
        assert!(set.capacity() >= 101);
    }

    #[test]
    fn test_bitset_remove() {
        let mut set = BitSet::new(100);
        set.insert(50);
        assert!(set.contains(50));

        set.remove(50);
        assert!(!set.contains(50));
    }

    #[test]
    fn test_bitset_clear() {
        let mut set = BitSet::new(100);
        set.insert(10);
        set.insert(20);
        set.insert(30);
        assert_eq!(set.count(), 3);

        set.clear();
        assert!(set.is_empty());
        assert_eq!(set.count(), 0);
    }

    #[test]
    fn test_bitset_set_all() {
        let mut set = BitSet::new(0);
        set.set_all(10);

        for i in 0..10 {
            assert!(set.contains(i), "Index {} should be set", i);
        }
        assert!(!set.contains(10));
        assert_eq!(set.count(), 10);
    }

    #[test]
    fn test_bitset_iter() {
        let mut set = BitSet::new(100);
        set.insert(5);
        set.insert(15);
        set.insert(65); // Second word

        let indices: Vec<_> = set.iter().collect();
        assert_eq!(indices, vec![5, 15, 65]);
    }

    #[test]
    fn test_bitset_cross_word_boundary() {
        let mut set = BitSet::new(200);
        set.insert(63); // Last bit of first word
        set.insert(64); // First bit of second word
        set.insert(127); // Last bit of second word
        set.insert(128); // First bit of third word

        assert!(set.contains(63));
        assert!(set.contains(64));
        assert!(set.contains(127));
        assert!(set.contains(128));
        assert_eq!(set.count(), 4);
    }

    // DirtyFlags tests
    #[test]
    fn test_dirty_flags_default_is_clean() {
        let flags = DirtyFlags::default();
        assert!(flags.is_clean());
    }

    #[test]
    fn test_dirty_flags_mark_all_dirty() {
        let mut flags = DirtyFlags::default();
        flags.mark_all_dirty(10, 5, 20);

        assert!(!flags.is_clean());
        assert!(flags.structure_changed);
        assert!(flags.uniforms);
        assert_eq!(flags.node_positions.count(), 10);
        assert_eq!(flags.node_styles.count(), 10);
        assert_eq!(flags.edges.count(), 5);
        assert_eq!(flags.edge_vertices.count(), 20);
    }

    #[test]
    fn test_dirty_flags_mark_node_position() {
        let mut flags = DirtyFlags::default();
        flags.mark_node_position(5);

        assert!(!flags.is_clean());
        assert!(flags.has_node_changes());
        assert!(!flags.structure_changed);
        assert!(flags.node_positions.contains(5));
    }

    #[test]
    fn test_dirty_flags_mark_edge() {
        let mut flags = DirtyFlags::default();
        flags.mark_edge(3);

        assert!(!flags.is_clean());
        assert!(flags.has_edge_changes());
        assert!(flags.edges.contains(3));
    }

    #[test]
    fn test_dirty_flags_mark_edge_vertices() {
        let mut flags = DirtyFlags::default();
        flags.mark_edge_vertices(10, 5); // Vertices 10, 11, 12, 13, 14

        assert!(flags.edge_vertices.contains(10));
        assert!(flags.edge_vertices.contains(14));
        assert!(!flags.edge_vertices.contains(9));
        assert!(!flags.edge_vertices.contains(15));
    }

    #[test]
    fn test_dirty_flags_clear() {
        let mut flags = DirtyFlags::default();
        flags.mark_all_dirty(10, 5, 20);
        flags.clear();

        assert!(flags.is_clean());
        assert!(!flags.structure_changed);
        assert!(!flags.uniforms);
    }

    #[test]
    fn test_dirty_flags_has_node_changes() {
        let mut flags = DirtyFlags::default();

        assert!(!flags.has_node_changes());

        flags.mark_node_position(0);
        assert!(flags.has_node_changes());

        flags.clear();
        flags.mark_node_style(0);
        assert!(flags.has_node_changes());

        flags.clear();
        flags.mark_structure_changed();
        assert!(flags.has_node_changes());
    }

    #[test]
    fn test_dirty_flags_has_edge_changes() {
        let mut flags = DirtyFlags::default();

        assert!(!flags.has_edge_changes());

        flags.mark_edge(0);
        assert!(flags.has_edge_changes());

        flags.clear();
        flags.mark_edge_vertices(0, 1);
        assert!(flags.has_edge_changes());

        flags.clear();
        flags.mark_structure_changed();
        assert!(flags.has_edge_changes());
    }
}
