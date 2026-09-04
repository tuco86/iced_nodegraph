//! The id vocabulary of one application's graph.
//!
//! Nodes, pins, edges and anchors carry the host's own id types, and a pin can
//! carry a payload. [`Ids`] bundles those five types so a graph names them once,
//! on a marker type, instead of on every widget, builder and callback. [`Id`]
//! is the bound each id type has to meet; it is implemented for every type that
//! can be cloned, compared, hashed, printed and sent, so a newtype, an enum or a
//! `uuid::Uuid` needs no impl at all.
//!
//! [`Indexed`] is the built-in vocabulary: `usize` for nodes, pins and anchors,
//! no edge id and no pin payload. A graph over it needs no type annotation.
//!
//! ```rust
//! use std::any::TypeId;
//!
//! use iced_nodegraph::Ids;
//!
//! #[derive(Clone, Debug, PartialEq, Eq, Hash)]
//! enum NodeKind {
//!     Source,
//!     Sink,
//! }
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! struct AppIds;
//!
//! impl Ids for AppIds {
//!     type NodeId = NodeKind;
//!     type PinId = &'static str;
//!     type EdgeId = u64;
//!     type AnchorId = usize;
//!     type Payload = TypeId;
//! }
//! ```

use std::fmt::Debug;
use std::hash::Hash;

/// The bound every node, pin, edge and anchor id meets.
///
/// Implemented for every type that satisfies the supertraits: `Clone + Eq +
/// Hash` for lookups and comparison, `Debug` for the duplicate-id assertion,
/// `Send + Sync + 'static` because an id travels in a `Message`.
pub trait Id: Clone + Eq + Hash + Debug + Send + Sync + 'static {}

impl<T: Clone + Eq + Hash + Debug + Send + Sync + 'static> Id for T {}

/// The id and payload types of one graph, named once on a marker type.
///
/// The marker is a unit struct the host declares; the supertraits let every
/// type generic over it derive `Clone`, `PartialEq`, `Hash` and `Debug`. Name it
/// once when building the graph (`NodeGraph::<AppIds, _, _>::new()`) and in the
/// messages that carry a [`PinRef`](crate::PinRef); everything else infers.
///
/// `EdgeId` is `()` for a host whose edges carry no identity of their own.
/// `Payload` rides on each pin through [`NodePin::info`](crate::NodePin::info)
/// and is handed back to [`Node::pin_style`](crate::Node::pin_style) and
/// [`NodeGraph::can_connect`](crate::NodeGraph::can_connect); `()` when pins
/// carry none. A pin's `PinId` and `Payload` types must match the graph's:
/// the pin is found in the widget tree by that pair, and a mismatch is a
/// debug-build assertion at the first layout.
pub trait Ids: Copy + Eq + Hash + Debug + Send + Sync + 'static {
    /// Identifies a node; unique among the graph's nodes.
    type NodeId: Id;
    /// Identifies a pin within its node.
    type PinId: Id;
    /// Identifies an edge; `()` when edges carry no id.
    type EdgeId: Id;
    /// Identifies a routing anchor; its own id space, independent of nodes.
    type AnchorId: Id;
    /// Per-pin payload surfaced to `pin_style` and `can_connect`.
    type Payload: Clone + 'static;
}

/// The built-in vocabulary: `usize` node, pin and anchor ids, no edge id, no
/// pin payload. The default `I` of every generic type in this crate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Indexed;

impl Ids for Indexed {
    type NodeId = usize;
    type PinId = usize;
    type EdgeId = ();
    type AnchorId = usize;
    type Payload = ();
}
