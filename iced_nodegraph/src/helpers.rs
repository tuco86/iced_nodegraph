//! Helper utilities for common node graph operations.
//!
//! This module provides convenience functions for:
//! - Cloning/duplicating nodes with proper edge remapping
//! - Deleting nodes with automatic edge cleanup
//! - Selection management helpers

use crate::PinReference;
use iced::Point;
use std::collections::{HashMap, HashSet};

/// Result of a clone operation containing new nodes and remapped edges.
#[derive(Debug, Clone)]
pub struct CloneResult<T> {
    /// The cloned nodes with their new positions.
    /// Each entry is `(new_index, position, cloned_data)`.
    pub nodes: Vec<(usize, Point, T)>,
    /// Edges between the cloned nodes (remapped to new indices).
    pub internal_edges: Vec<(PinReference, PinReference)>,
}

/// Clone a set of nodes with an offset, remapping internal edges.
///
/// This function helps implement Ctrl+D style duplication:
/// - Creates copies of selected nodes at offset positions
/// - Remaps edges that connect cloned nodes to each other
/// - Does not copy edges that connect to non-cloned nodes
///
/// # Arguments
///
/// * `source_indices` - Indices of nodes to clone
/// * `offset` - Position offset for cloned nodes (e.g., `Vector::new(50.0, 50.0)`)
/// * `node_count` - Current total node count (new nodes start at this index)
/// * `get_node` - Function to get node data: `|index| -> Option<(Point, T)>`
/// * `edges` - All current edges in the graph
///
/// # Returns
///
/// A `CloneResult` containing the new nodes and their internal edges.
///
/// # Example
///
/// ```rust,ignore
/// use iced_nodegraph::helpers::clone_nodes;
/// use iced::Vector;
///
/// let result = clone_nodes(
///     &[0, 2],  // Clone nodes 0 and 2
///     Vector::new(50.0, 50.0),
///     self.nodes.len(),
///     |i| self.nodes.get(i).map(|(pos, data)| (*pos, data.clone())),
///     &self.edges,
/// );
///
/// // Add cloned nodes
/// for (new_idx, pos, data) in result.nodes {
///     self.nodes.push((pos, data));
/// }
///
/// // Add internal edges
/// self.edges.extend(result.internal_edges);
/// ```
pub fn clone_nodes<T, F>(
    source_indices: &[usize],
    offset: iced::Vector,
    node_count: usize,
    get_node: F,
    edges: &[(PinReference, PinReference)],
) -> CloneResult<T>
where
    F: Fn(usize) -> Option<(Point, T)>,
{
    // Build mapping from old index to new index
    let index_map: HashMap<usize, usize> = source_indices
        .iter()
        .enumerate()
        .map(|(i, &old_idx)| (old_idx, node_count + i))
        .collect();

    // Clone nodes with offset positions
    let nodes: Vec<_> = source_indices
        .iter()
        .filter_map(|&idx| {
            get_node(idx).map(|(pos, data)| {
                let new_pos = Point::new(pos.x + offset.x, pos.y + offset.y);
                (*index_map.get(&idx).unwrap(), new_pos, data)
            })
        })
        .collect();

    // Remap internal edges (edges where both endpoints are in the cloned set)
    let source_set: HashSet<_> = source_indices.iter().copied().collect();
    let internal_edges: Vec<_> = edges
        .iter()
        .filter(|(from, to)| source_set.contains(&from.node_id) && source_set.contains(&to.node_id))
        .map(|(from, to)| {
            let new_from = PinReference::new(*index_map.get(&from.node_id).unwrap(), from.pin_id);
            let new_to = PinReference::new(*index_map.get(&to.node_id).unwrap(), to.pin_id);
            (new_from, new_to)
        })
        .collect();

    CloneResult {
        nodes,
        internal_edges,
    }
}

/// Result of a delete operation with index remapping information.
#[derive(Debug, Clone)]
pub struct DeleteResult {
    /// Indices to remove (sorted in descending order for safe removal).
    pub indices_to_remove: Vec<usize>,
    /// Function to remap old indices to new indices after deletion.
    /// Returns `None` if the node was deleted.
    remap: HashMap<usize, usize>,
}

impl DeleteResult {
    /// Remap an old node index to its new index after deletion.
    ///
    /// Returns `None` if the node was deleted.
    pub fn remap(&self, old_index: usize) -> Option<usize> {
        self.remap.get(&old_index).copied()
    }

    /// Remap edges, filtering out any that reference deleted nodes.
    pub fn remap_edges(
        &self,
        edges: &[(PinReference, PinReference)],
    ) -> Vec<(PinReference, PinReference)> {
        edges
            .iter()
            .filter_map(|(from, to)| {
                let new_from_node = self.remap(from.node_id)?;
                let new_to_node = self.remap(to.node_id)?;
                Some((
                    PinReference::new(new_from_node, from.pin_id),
                    PinReference::new(new_to_node, to.pin_id),
                ))
            })
            .collect()
    }
}

/// Prepare deletion of nodes with automatic edge cleanup and index remapping.
///
/// This function helps implement Delete key functionality:
/// - Computes which edges need to be removed
/// - Provides index remapping for remaining nodes
/// - Returns indices in safe removal order (descending)
///
/// # Arguments
///
/// * `delete_indices` - Indices of nodes to delete
/// * `node_count` - Current total node count
///
/// # Returns
///
/// A `DeleteResult` with removal indices and remapping logic.
///
/// # Example
///
/// ```rust,ignore
/// use iced_nodegraph::helpers::delete_nodes;
///
/// let result = delete_nodes(&[1, 3], self.nodes.len());
///
/// // Remap edges first (before removing nodes)
/// self.edges = result.remap_edges(&self.edges);
///
/// // Remove nodes in descending order
/// for idx in &result.indices_to_remove {
///     self.nodes.remove(*idx);
/// }
///
/// // Remap selection
/// self.selection = self.selection
///     .iter()
///     .filter_map(|&i| result.remap(i))
///     .collect();
/// ```
pub fn delete_nodes(delete_indices: &[usize], node_count: usize) -> DeleteResult {
    let delete_set: HashSet<_> = delete_indices.iter().copied().collect();

    // Compute remapping: for each surviving node, what's its new index?
    let mut remap = HashMap::new();
    let mut new_idx = 0;
    for old_idx in 0..node_count {
        if !delete_set.contains(&old_idx) {
            remap.insert(old_idx, new_idx);
            new_idx += 1;
        }
    }

    // Sort indices in descending order for safe Vec removal
    let mut indices_to_remove: Vec<_> = delete_indices.to_vec();
    indices_to_remove.sort_by(|a, b| b.cmp(a));
    indices_to_remove.dedup();

    DeleteResult {
        indices_to_remove,
        remap,
    }
}

/// Helper for managing node selection state.
///
/// Provides common selection operations with consistent behavior.
#[derive(Debug, Clone, Default)]
pub struct SelectionHelper {
    selected: HashSet<usize>,
}

impl SelectionHelper {
    /// Create a new empty selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create selection from an iterator of indices.
    pub fn from_iter(iter: impl IntoIterator<Item = usize>) -> Self {
        Self {
            selected: iter.into_iter().collect(),
        }
    }

    /// Check if a node is selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    /// Get the number of selected nodes.
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// Check if selection is empty.
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// Toggle selection of a node (for Ctrl+click).
    pub fn toggle(&mut self, index: usize) {
        if self.selected.contains(&index) {
            self.selected.remove(&index);
        } else {
            self.selected.insert(index);
        }
    }

    /// Add a node to selection (for Shift+click).
    pub fn add(&mut self, index: usize) {
        self.selected.insert(index);
    }

    /// Set selection to a single node (for regular click).
    pub fn set_single(&mut self, index: usize) {
        self.selected.clear();
        self.selected.insert(index);
    }

    /// Set selection to multiple nodes (for box select).
    pub fn set_multiple(&mut self, indices: impl IntoIterator<Item = usize>) {
        self.selected = indices.into_iter().collect();
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.selected.clear();
    }

    /// Get selected indices as a slice-compatible Vec.
    pub fn to_vec(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }

    /// Get reference to the underlying HashSet.
    pub fn as_set(&self) -> &HashSet<usize> {
        &self.selected
    }

    /// Remap indices after node deletion.
    pub fn remap(&mut self, delete_result: &DeleteResult) {
        self.selected = self
            .selected
            .iter()
            .filter_map(|&i| delete_result.remap(i))
            .collect();
    }

    /// Extend selection with cloned node indices.
    pub fn extend_with_clones<T>(&mut self, clone_result: &CloneResult<T>) {
        for (new_idx, _, _) in &clone_result.nodes {
            self.selected.insert(*new_idx);
        }
    }
}

impl From<Vec<usize>> for SelectionHelper {
    fn from(indices: Vec<usize>) -> Self {
        Self::from_iter(indices)
    }
}

impl From<&[usize]> for SelectionHelper {
    fn from(indices: &[usize]) -> Self {
        Self::from_iter(indices.iter().copied())
    }
}

impl<'a> IntoIterator for &'a SelectionHelper {
    type Item = &'a usize;
    type IntoIter = std::collections::hash_set::Iter<'a, usize>;

    fn into_iter(self) -> Self::IntoIter {
        self.selected.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_nodes_basic() {
        let nodes: Vec<(Point, &str)> = vec![
            (Point::new(0.0, 0.0), "A"),
            (Point::new(100.0, 0.0), "B"),
            (Point::new(200.0, 0.0), "C"),
        ];
        let edges = vec![
            (PinReference::new(0, 0), PinReference::new(1, 0)),
            (PinReference::new(1, 0), PinReference::new(2, 0)),
        ];

        let result = clone_nodes(
            &[0, 1],
            iced::Vector::new(50.0, 50.0),
            nodes.len(),
            |i| nodes.get(i).map(|(p, d)| (*p, *d)),
            &edges,
        );

        // Should have 2 cloned nodes
        assert_eq!(result.nodes.len(), 2);

        // New indices should be 3 and 4
        assert_eq!(result.nodes[0].0, 3);
        assert_eq!(result.nodes[1].0, 4);

        // Positions should be offset
        assert_eq!(result.nodes[0].1, Point::new(50.0, 50.0));
        assert_eq!(result.nodes[1].1, Point::new(150.0, 50.0));

        // Should have 1 internal edge (0->1 becomes 3->4)
        assert_eq!(result.internal_edges.len(), 1);
        assert_eq!(result.internal_edges[0].0.node_id, 3);
        assert_eq!(result.internal_edges[0].1.node_id, 4);
    }

    #[test]
    fn test_delete_nodes_remapping() {
        let result = delete_nodes(&[1, 3], 5);

        // Should have indices in descending order
        assert_eq!(result.indices_to_remove, vec![3, 1]);

        // Remapping: 0->0, 2->1, 4->2
        assert_eq!(result.remap(0), Some(0));
        assert_eq!(result.remap(1), None); // deleted
        assert_eq!(result.remap(2), Some(1));
        assert_eq!(result.remap(3), None); // deleted
        assert_eq!(result.remap(4), Some(2));
    }

    #[test]
    fn test_delete_nodes_edge_remapping() {
        let edges = vec![
            (PinReference::new(0, 0), PinReference::new(1, 0)), // will be removed
            (PinReference::new(0, 0), PinReference::new(2, 0)), // 0->0, 2->1
            (PinReference::new(2, 0), PinReference::new(4, 0)), // 2->1, 4->2
        ];

        let result = delete_nodes(&[1, 3], 5);
        let remapped = result.remap_edges(&edges);

        assert_eq!(remapped.len(), 2);
        assert_eq!(remapped[0].0.node_id, 0);
        assert_eq!(remapped[0].1.node_id, 1);
        assert_eq!(remapped[1].0.node_id, 1);
        assert_eq!(remapped[1].1.node_id, 2);
    }

    #[test]
    fn test_selection_helper_toggle() {
        let mut sel = SelectionHelper::new();

        sel.toggle(1);
        assert!(sel.is_selected(1));

        sel.toggle(1);
        assert!(!sel.is_selected(1));
    }

    #[test]
    fn test_selection_helper_set_single() {
        let mut sel = SelectionHelper::from_iter([1, 2, 3]);

        sel.set_single(5);

        assert_eq!(sel.len(), 1);
        assert!(sel.is_selected(5));
        assert!(!sel.is_selected(1));
    }

    #[test]
    fn test_selection_helper_remap() {
        let mut sel = SelectionHelper::from_iter([0, 2, 4]);
        let delete_result = delete_nodes(&[1, 3], 5);

        sel.remap(&delete_result);

        // 0->0, 2->1, 4->2
        assert!(sel.is_selected(0));
        assert!(sel.is_selected(1));
        assert!(sel.is_selected(2));
        assert_eq!(sel.len(), 3);
    }
}
