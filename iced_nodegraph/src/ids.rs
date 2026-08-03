//! Id traits for user-defined node and pin identification.
//!
//! Nodes and pins carry the user's own id type directly. These traits collect
//! the bounds the widget needs on those types: `Clone + Eq + Hash` to look them
//! up and compare them, `Debug` for the duplicate-id assertion, and
//! `Send + Sync` because an id travels in a `Message`.
//!
//! Blanket impls cover the integer, `String` and `&'static str` cases. For any
//! other type - a newtype, an enum, a `uuid::Uuid` - write the one-line impl
//! yourself.

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
