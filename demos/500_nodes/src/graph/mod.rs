mod layout;
mod procedural;

use crate::TypedIds;
use iced_nodegraph::PinRef;

/// An edge connecting two pins.
type Edge = (PinRef<TypedIds>, PinRef<TypedIds>);

pub use procedural::generate_procedural_graph;
