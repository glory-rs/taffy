//! Contains both a high-level interface to Takia using a ready-made node tree, and a set of traits for defining custom node trees.
//!
//! - For documentation on the high-level API, see the [`TakiaTree`] struct.
//! - For documentation on the low-level trait-based API, see the [`traits`] module.

// Submodules
mod cache;
mod layout;
mod node;
pub mod traits;

#[doc(hidden)]
#[cfg(all(debug_assertions, feature = "std"))]
pub use cache::cache_mode_change_evictions;
pub use cache::{Cache, ClearState};
pub use layout::{
    AxisStaticAlign, AxisStaticEdge, AxisStaticPosition, Baselines, CollapsibleMarginSet, Layout, LayoutInput,
    LayoutOutput, OofCandidate, OofCandidates, OofPositioningArea, RequestedAxis, RunMode, ScrollableOverflow,
    SizingMode,
};
pub use node::NodeId;
pub(crate) use traits::LayoutPartialTreeExt;
pub use traits::{LayoutContainingBlock, LayoutPartialTree, PrintTree, RoundTree, TraversePartialTree, TraverseTree};

#[cfg(feature = "flexbox")]
pub use traits::LayoutFlexboxContainer;

#[cfg(feature = "grid")]
pub use traits::LayoutGridContainer;

#[cfg(feature = "block_layout")]
pub use traits::LayoutBlockContainer;

#[cfg(feature = "takia_tree")]
mod takia_tree;
#[cfg(feature = "takia_tree")]
pub use takia_tree::{TakiaError, TakiaResult, TakiaTree};

pub use layout::DetailedLayoutInfo;
