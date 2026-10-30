//! Contains [TakiaTree](crate::tree::TakiaTree): the default implementation of [LayoutTree](crate::tree::LayoutTree), and the error type for Takia.
#[cfg(not(feature = "std"))]
use slotmap::SecondaryMap;
#[cfg(feature = "std")]
use slotmap::SparseSecondaryMap as SecondaryMap;
use slotmap::{DefaultKey, SlotMap};

#[cfg(feature = "block_layout")]
use crate::block::BlockContext;
use crate::geometry::Size;
use crate::style::{AvailableSpace, Display, Style};
use crate::sys::DefaultCheapStr;
use crate::tree::{
    Cache, ClearState, Layout, LayoutContainingBlock, LayoutInput, LayoutOutput, LayoutPartialTree, NodeId, PrintTree,
    RoundTree, RunMode, TraversePartialTree, TraverseTree,
};
use crate::util::debug::{debug_log, debug_log_node};
use crate::util::sys::{new_const_children_vec, new_vec_with_capacity, Box, ChildrenVec, Vec};

use crate::compute::{
    compute_cached_layout, compute_hidden_layout, compute_leaf_layout, compute_oof_layout, compute_root_layout,
    round_layout,
};
use crate::CacheTree;

#[cfg(feature = "block_layout")]
use crate::{compute::compute_block_layout, LayoutBlockContainer};
#[cfg(feature = "flexbox")]
use crate::{compute::compute_flexbox_layout, LayoutFlexboxContainer};
#[cfg(feature = "grid")]
use crate::{compute::compute_grid_layout, LayoutGridContainer};

#[cfg(feature = "grid")]
use crate::compute::grid::DetailedGridInfo;
use crate::tree::layout::DetailedLayoutInfo;

/// The error Takia generates on invalid operations
pub type TakiaResult<T> = Result<T, TakiaError>;

/// An error that occurs while trying to access or modify a node's children by index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakiaError {
    /// The parent node does not have a child at `child_index`. It only has `child_count` children
    ChildIndexOutOfBounds {
        /// The parent node whose child was being looked up
        parent: NodeId,
        /// The index that was looked up
        child_index: usize,
        /// The total number of children the parent has
        child_count: usize,
    },
    /// The parent node was not found in the [`TakiaTree`](crate::TakiaTree) instance.
    InvalidParentNode(NodeId),
    /// The child node was not found in the [`TakiaTree`](crate::TakiaTree) instance.
    InvalidChildNode(NodeId),
    /// The supplied node was not found in the [`TakiaTree`](crate::TakiaTree) instance.
    InvalidInputNode(NodeId),
}

impl core::fmt::Display for TakiaError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            TakiaError::ChildIndexOutOfBounds { parent, child_index, child_count } => {
                write!(f, "Index (is {child_index}) should be < child_count ({child_count}) for parent node {parent:?}")
            }
            TakiaError::InvalidParentNode(parent) => {
                write!(f, "Parent Node {parent:?} is not in the TakiaTree instance")
            }
            TakiaError::InvalidChildNode(child) => write!(f, "Child Node {child:?} is not in the TakiaTree instance"),
            TakiaError::InvalidInputNode(node) => write!(f, "Supplied Node {node:?} is not in the TakiaTree instance"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TakiaError {}

/// Global configuration values for a TakiaTree instance
#[derive(Debug, Clone, Copy)]
pub(crate) struct TakiaConfig {
    /// Whether to round layout values
    pub(crate) use_rounding: bool,
}

impl Default for TakiaConfig {
    fn default() -> Self {
        Self { use_rounding: true }
    }
}

/// Layout information for a given [`Node`](crate::node::Node)
///
/// Stored in a [`TakiaTree`].
#[derive(Debug, Clone, PartialEq)]
struct NodeData {
    /// The layout strategy used by this node
    pub(crate) style: Style,

    /// The always unrounded results of the layout computation. We must store this separately from the rounded
    /// layout to avoid errors from rounding already-rounded values. See <https://github.com/DioxusLabs/takia/issues/501>.
    pub(crate) unrounded_layout: Layout,

    /// The final results of the layout computation.
    /// These may be rounded or unrounded depending on what the `use_rounding` config setting is set to.
    pub(crate) final_layout: Layout,

    /// Whether the node has context data associated with it or not
    pub(crate) has_context: bool,

    /// Out-of-flow (absolute/fixed) boxes whose containing block is this node,
    /// as recorded by the out-of-flow positioning pass
    pub(crate) hoisted_children: ChildrenVec<NodeId>,

    /// The cached results of the layout computation
    pub(crate) cache: Cache,

    /// The computation result from layout algorithm
    pub(crate) detailed_layout_info: DetailedLayoutInfo,
}

impl NodeData {
    /// Create the data for a new node
    #[must_use]
    pub const fn new(style: Style) -> Self {
        Self {
            style,
            cache: Cache::new(),
            unrounded_layout: Layout::new(),
            final_layout: Layout::new(),
            has_context: false,
            hoisted_children: new_const_children_vec(),
            detailed_layout_info: DetailedLayoutInfo::None,
        }
    }

    /// Marks a node and all of its ancestors as requiring relayout
    ///
    /// This clears any cached data and signals that the data must be recomputed.
    /// If the node was already marked as dirty, returns true
    #[inline]
    pub fn mark_dirty(&mut self) -> ClearState {
        self.cache.clear()
    }
}

/// An entire tree of UI nodes. The entry point to Takia's high-level API.
///
/// Allows you to build a tree of UI nodes, run Takia's layout algorithms over that tree, and then access the resultant layout.]
#[derive(Debug, Clone)]
pub struct TakiaTree<NodeContext = ()> {
    /// The [`NodeData`] for each node stored in this tree
    nodes: SlotMap<DefaultKey, NodeData>,

    /// Functions/closures that compute the intrinsic size of leaf nodes
    node_context_data: SecondaryMap<DefaultKey, NodeContext>,

    /// The children of each node
    ///
    /// The indexes in the outer vector correspond to the position of the parent [`NodeData`]
    children: SlotMap<DefaultKey, ChildrenVec<NodeId>>,

    /// The parents of each node
    ///
    /// The indexes in the outer vector correspond to the position of the child [`NodeData`]
    parents: SlotMap<DefaultKey, Option<NodeId>>,

    /// Layout mode configuration
    config: TakiaConfig,
}

impl Default for TakiaTree {
    fn default() -> TakiaTree<()> {
        TakiaTree::new()
    }
}

/// Iterator that wraps a slice of nodes, lazily converting them to u64
pub struct TakiaTreeChildIter<'a>(core::slice::Iter<'a, NodeId>);
impl Iterator for TakiaTreeChildIter<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().copied()
    }
}

// TraversePartialTree impl for TakiaTree
impl<NodeContext> TraversePartialTree for TakiaTree<NodeContext> {
    type ChildIter<'a>
        = TakiaTreeChildIter<'a>
    where
        Self: 'a;

    #[inline(always)]
    fn child_ids(&self, parent_node_id: NodeId) -> Self::ChildIter<'_> {
        TakiaTreeChildIter(self.children[parent_node_id.into()].iter())
    }

    #[inline(always)]
    fn child_count(&self, parent_node_id: NodeId) -> usize {
        self.children[parent_node_id.into()].len()
    }

    #[inline(always)]
    fn get_child_id(&self, parent_node_id: NodeId, id: usize) -> NodeId {
        self.children[parent_node_id.into()][id]
    }
}

// TraverseTree impl for TakiaTree
impl<NodeContext> TraverseTree for TakiaTree<NodeContext> {}

// CacheTree impl for TakiaTree
impl<NodeContext> CacheTree for TakiaTree<NodeContext> {
    fn cache_get(&mut self, node_id: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.nodes[node_id.into()].cache.get(input)
    }

    fn cache_store(&mut self, node_id: NodeId, input: &LayoutInput, layout_output: LayoutOutput) {
        self.nodes[node_id.into()].cache.store(input, layout_output)
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.nodes[node_id.into()].cache.clear();
    }
}

// PrintTree impl for TakiaTree
impl<NodeContext> PrintTree for TakiaTree<NodeContext> {
    #[inline(always)]
    fn get_debug_label(&self, node_id: NodeId) -> &'static str {
        let node = &self.nodes[node_id.into()];
        let display = node.style.display;
        let num_children = self.child_count(node_id);

        match (num_children, display) {
            (_, Display::None) => "NONE",
            (0, _) => "LEAF",
            #[cfg(feature = "block_layout")]
            (_, Display::Block) => "BLOCK",
            #[cfg(feature = "block_layout")]
            (_, Display::FlowRoot) => "FLOW-ROOT",
            #[cfg(feature = "flexbox")]
            (_, Display::Flex) => {
                use crate::FlexDirection;
                match node.style.flex_direction {
                    FlexDirection::Row | FlexDirection::RowReverse => "FLEX ROW",
                    FlexDirection::Column | FlexDirection::ColumnReverse => "FLEX COL",
                }
            }
            #[cfg(feature = "grid")]
            (_, Display::Grid) => "GRID",
        }
    }

    #[inline(always)]
    fn get_final_layout(&self, node_id: NodeId) -> Layout {
        if self.config.use_rounding {
            self.nodes[node_id.into()].final_layout
        } else {
            self.nodes[node_id.into()].unrounded_layout
        }
    }
}

/// View over the Takia tree that holds the tree itself along with a reference to the context
/// and implements LayoutTree. This allows the context to be stored outside of the TakiaTree struct
/// which makes the lifetimes of the context much more flexible.
pub(crate) struct TakiaView<'t, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    /// A reference to the TakiaTree
    pub(crate) takia: &'t mut TakiaTree<NodeContext>,
    /// The context provided for passing to measure functions if layout is run over this struct
    pub(crate) measure_function: MeasureFunction,
}

impl<NodeContext, MeasureFunction> TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    #[inline(always)]
    /// Unified implementation that both `LayoutPartialTree::compute_child_layout`
    /// and `LayoutBlockContainer::compute_block_child_layout` delegate to.
    fn compute_child_layout(
        &mut self,
        node_id: NodeId,
        inputs: LayoutInput,
        #[cfg(feature = "block_layout")] block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        // If RunMode is PerformHiddenLayout then this indicates that an ancestor node is `Display::None`
        // and thus that we should lay out this node using hidden layout regardless of it's own display style.
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            debug_log!("HIDDEN");
            // `round_layout` reaches out-of-flow boxes only through their containing block's
            // hoisted child list, and a box inside a hidden subtree is in no such list. Write the
            // final layout here so that it does not keep the layout from an earlier visible pass.
            self.takia.nodes[node_id.into()].final_layout = Layout::with_order(0);
            return compute_hidden_layout(self, node_id);
        }

        // We run the following wrapped in "compute_cached_layout", which will check the cache for an entry matching the node and inputs and:
        //   - Return that entry if exists
        //   - Else call the passed closure (below) to compute the result
        //
        // If there was no cache match and a new result needs to be computed then that result will be added to the cache
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let display_mode = tree.takia.nodes[node_id.into()].style.display;
            let has_children = tree.child_count(node_id) > 0;

            debug_log!(display_mode);
            debug_log_node!(inputs);

            // Dispatch to a layout algorithm based on the node's display style and whether the node has children or not.
            let mut output = match (display_mode, has_children) {
                (Display::None, _) => compute_hidden_layout(tree, node_id),
                #[cfg(feature = "block_layout")]
                (Display::Block, true) => compute_block_layout(tree, node_id, inputs, block_ctx),
                #[cfg(feature = "block_layout")]
                (Display::FlowRoot, true) => compute_block_layout(tree, node_id, inputs, None),
                #[cfg(feature = "flexbox")]
                (Display::Flex, true) => compute_flexbox_layout(tree, node_id, inputs),
                #[cfg(feature = "grid")]
                (Display::Grid, true) => compute_grid_layout(tree, node_id, inputs),
                (_, false) => {
                    let node_key = node_id.into();
                    let style = &tree.takia.nodes[node_key].style;
                    let has_context = tree.takia.nodes[node_key].has_context;
                    let node_context = has_context.then(|| tree.takia.node_context_data.get_mut(node_key)).flatten();
                    (tree.measure_function)(inputs, node_id, node_context, style)
                }
            };

            // Lay out any out-of-flow candidates for which this node is the containing block.
            // The rest bubble up via `output.oof_candidates`. This runs inside the cache-miss
            // closure so that cached outputs already contain the processed candidate list.
            // Only full layout passes run it: measure passes must not write hoisted box layouts
            // (which would not be rewritten if the final layout pass is a cache hit).
            if inputs.run_mode == RunMode::PerformLayout {
                compute_oof_layout(tree, node_id, &mut output);
            }

            output
        })
    }
}

// TraversePartialTree impl for TakiaView
impl<NodeContext, MeasureFunction> TraversePartialTree for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    type ChildIter<'a>
        = TakiaTreeChildIter<'a>
    where
        Self: 'a;

    #[inline(always)]
    fn child_ids(&self, parent_node_id: NodeId) -> Self::ChildIter<'_> {
        self.takia.child_ids(parent_node_id)
    }

    #[inline(always)]
    fn child_count(&self, parent_node_id: NodeId) -> usize {
        self.takia.child_count(parent_node_id)
    }

    #[inline(always)]
    fn get_child_id(&self, parent_node_id: NodeId, child_index: usize) -> NodeId {
        self.takia.get_child_id(parent_node_id, child_index)
    }
}

// TraverseTree impl for TakiaView
impl<NodeContext, MeasureFunction> TraverseTree for TakiaView<'_, NodeContext, MeasureFunction> where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput
{
}

// LayoutPartialTree impl for TakiaView
impl<NodeContext, MeasureFunction> LayoutPartialTree for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type CustomIdent = DefaultCheapStr;

    #[inline(always)]
    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.takia.nodes[node_id.into()].style
    }

    #[inline(always)]
    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.takia.nodes[node_id.into()].unrounded_layout = *layout;
    }

    #[inline(always)]
    fn resolve_calc_value(&self, _val: *const (), _basis: f32) -> f32 {
        0.0
    }

    #[inline(always)]
    fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.compute_child_layout(
            node_id,
            inputs,
            #[cfg(feature = "block_layout")]
            None,
        )
    }
}

impl<NodeContext, MeasureFunction> LayoutContainingBlock for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    type OofItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_oof_item_style(&self, node_id: NodeId) -> Self::OofItemStyle<'_> {
        &self.takia.nodes[node_id.into()].style
    }

    #[inline(always)]
    fn clear_hoisted_children(&mut self, node_id: NodeId) {
        self.takia.nodes[node_id.into()].hoisted_children.clear();
    }

    #[inline(always)]
    fn add_hoisted_children(&mut self, node_id: NodeId, hoisted: &[NodeId]) {
        self.takia.nodes[node_id.into()].hoisted_children.extend_from_slice(hoisted);
    }

    #[inline(always)]
    fn get_detailed_layout_info(&self, node_id: NodeId) -> &DetailedLayoutInfo {
        &self.takia.nodes[node_id.into()].detailed_layout_info
    }
}

impl<NodeContext, MeasureFunction> CacheTree for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    fn cache_get(&mut self, node_id: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.takia.nodes[node_id.into()].cache.get(input)
    }

    fn cache_store(&mut self, node_id: NodeId, input: &LayoutInput, layout_output: LayoutOutput) {
        self.takia.nodes[node_id.into()].cache.store(input, layout_output)
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.takia.nodes[node_id.into()].cache.clear();
    }
}

#[cfg(feature = "block_layout")]
impl<NodeContext, MeasureFunction> LayoutBlockContainer for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_block_container_style(&self, node_id: NodeId) -> Self::BlockContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    #[inline(always)]
    fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }

    #[inline(always)]
    fn compute_block_child_layout(
        &mut self,
        node_id: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        self.compute_child_layout(node_id, inputs, block_ctx)
    }
}

#[cfg(feature = "flexbox")]
impl<NodeContext, MeasureFunction> LayoutFlexboxContainer for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.takia.nodes[node_id.into()].style
    }

    #[inline(always)]
    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.takia.nodes[child_node_id.into()].style
    }
}

#[cfg(feature = "grid")]
impl<NodeContext, MeasureFunction> LayoutGridContainer for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
        &self.takia.nodes[node_id.into()].style
    }

    #[inline(always)]
    fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
        &self.takia.nodes[child_node_id.into()].style
    }

    #[inline(always)]
    fn set_detailed_grid_info(&mut self, node_id: NodeId, detailed_grid_info: DetailedGridInfo) {
        self.takia.nodes[node_id.into()].detailed_layout_info = DetailedLayoutInfo::Grid(Box::new(detailed_grid_info));
    }
}

// RoundTree impl for TakiaView
impl<NodeContext, MeasureFunction> RoundTree for TakiaView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    #[inline(always)]
    fn get_unrounded_layout(&self, node: NodeId) -> Layout {
        self.takia.nodes[node.into()].unrounded_layout
    }

    #[inline(always)]
    fn set_final_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.takia.nodes[node_id.into()].final_layout = *layout;
    }

    #[inline(always)]
    fn is_out_of_flow(&self, node_id: NodeId) -> bool {
        let node = &self.takia.nodes[node_id.into()];
        node.style.position.is_out_of_flow() && node.style.display != crate::style::Display::None
    }

    #[inline(always)]
    fn hoisted_child_count(&self, node_id: NodeId) -> usize {
        self.takia.nodes[node_id.into()].hoisted_children.len()
    }

    #[inline(always)]
    fn get_hoisted_child_id(&self, node_id: NodeId, index: usize) -> NodeId {
        self.takia.nodes[node_id.into()].hoisted_children[index]
    }
}

#[allow(clippy::iter_cloned_collect)] // due to no-std support, we need to use `iter_cloned` instead of `collect`
impl<NodeContext> TakiaTree<NodeContext> {
    /// Creates a new [`TakiaTree`]
    ///
    /// The default capacity of a [`TakiaTree`] is 16 nodes.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(16)
    }

    /// Creates a new [`TakiaTree`] that can store `capacity` nodes before reallocation
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        TakiaTree {
            // TODO: make this method const upstream,
            // so constructors here can be const
            nodes: SlotMap::with_capacity(capacity),
            children: SlotMap::with_capacity(capacity),
            parents: SlotMap::with_capacity(capacity),
            node_context_data: SecondaryMap::with_capacity(capacity),
            config: TakiaConfig::default(),
        }
    }

    /// Enable rounding of layout values. Rounding is enabled by default.
    pub fn enable_rounding(&mut self) {
        self.config.use_rounding = true;
    }

    /// Disable rounding of layout values. Rounding is enabled by default.
    pub fn disable_rounding(&mut self) {
        self.config.use_rounding = false;
    }

    /// Creates and adds a new unattached leaf node to the tree, and returns the node of the new node
    pub fn new_leaf(&mut self, layout: Style) -> TakiaResult<NodeId> {
        let id = self.nodes.insert(NodeData::new(layout));
        let _ = self.children.insert(new_vec_with_capacity(0));
        let _ = self.parents.insert(None);

        Ok(id.into())
    }

    /// Creates and adds a new unattached leaf node to the tree, and returns the [`NodeId`] of the new node
    ///
    /// Creates and adds a new leaf node with a supplied context
    pub fn new_leaf_with_context(&mut self, layout: Style, context: NodeContext) -> TakiaResult<NodeId> {
        let mut data = NodeData::new(layout);
        data.has_context = true;

        let id = self.nodes.insert(data);
        self.node_context_data.insert(id, context);

        let _ = self.children.insert(new_vec_with_capacity(0));
        let _ = self.parents.insert(None);

        Ok(id.into())
    }

    /// Creates and adds a new node, which may have any number of `children`
    pub fn new_with_children(&mut self, layout: Style, children: &[NodeId]) -> TakiaResult<NodeId> {
        let id = NodeId::from(self.nodes.insert(NodeData::new(layout)));

        for child in children {
            self.parents[(*child).into()] = Some(id);
        }

        let _ = self.children.insert(children.iter().copied().collect::<_>());
        let _ = self.parents.insert(None);

        Ok(id)
    }

    /// Drops all nodes in the tree
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.children.clear();
        self.parents.clear();
        // A context belongs to the node that owns it. Leaving it behind keeps arbitrary
        // user data -- for a measure function, a boxed closure and everything it
        // captures -- alive after the node it belonged to is gone.
        self.node_context_data.clear();
    }

    /// Remove a specific node from the tree and drop it
    ///
    /// Returns the id of the node removed.
    pub fn remove(&mut self, node: NodeId) -> TakiaResult<NodeId> {
        let key = node.into();
        if let Some(parent) = self.parents[key] {
            if let Some(children) = self.children.get_mut(parent.into()) {
                children.retain(|f| *f != node);
            }
            self.mark_dirty(parent)?;
        }

        // Remove "parent" references to a node when removing that node
        if let Some(children) = self.children.get(key) {
            for child in children.iter().copied() {
                self.parents[child.into()] = None;
            }
        }

        let _ = self.children.remove(key);
        let _ = self.parents.remove(key);
        let _ = self.nodes.remove(key);
        let _ = self.node_context_data.remove(key);

        Ok(node)
    }

    /// Sets the context data associated with the node
    #[inline]
    pub fn set_node_context(&mut self, node: NodeId, measure: Option<NodeContext>) -> TakiaResult<()> {
        let key = node.into();
        if let Some(measure) = measure {
            self.nodes[key].has_context = true;
            self.node_context_data.insert(key, measure);
        } else {
            self.nodes[key].has_context = false;
            self.node_context_data.remove(key);
        }

        self.mark_dirty(node)?;

        Ok(())
    }

    /// Gets a reference to the the context data associated with the node
    #[inline]
    pub fn get_node_context(&self, node: NodeId) -> Option<&NodeContext> {
        self.node_context_data.get(node.into())
    }

    /// Gets a mutable reference to the the context data associated with the node
    #[inline]
    pub fn get_node_context_mut(&mut self, node: NodeId) -> Option<&mut NodeContext> {
        self.node_context_data.get_mut(node.into())
    }

    /// Gets mutable references to the the context data associated with the nodes. All keys must be valid and disjoint, otherwise None is returned.
    pub fn get_disjoint_node_context_mut<const N: usize>(
        &mut self,
        keys: [NodeId; N],
    ) -> Option<[&mut NodeContext; N]> {
        self.node_context_data.get_disjoint_mut(keys.map(|k| k.into()))
    }

    /// Adds a `child` node under the supplied `parent`
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> TakiaResult<()> {
        let parent_key = parent.into();
        let child_key = child.into();
        self.parents[child_key] = Some(parent);
        self.children[parent_key].push(child);
        self.mark_dirty(parent)?;

        Ok(())
    }

    /// Inserts a `child` node at the given `child_index` under the supplied `parent`, shifting all children after it to the right.
    pub fn insert_child_at_index(&mut self, parent: NodeId, child_index: usize, child: NodeId) -> TakiaResult<()> {
        let parent_key = parent.into();

        let child_count = self.children[parent_key].len();
        if child_index > child_count {
            return Err(TakiaError::ChildIndexOutOfBounds { parent, child_index, child_count });
        }

        self.parents[child.into()] = Some(parent);
        self.children[parent_key].insert(child_index, child);
        self.mark_dirty(parent)?;

        Ok(())
    }

    /// Directly sets the `children` of the supplied `parent`
    pub fn set_children(&mut self, parent: NodeId, children: &[NodeId]) -> TakiaResult<()> {
        let parent_key = parent.into();

        // Remove node as parent from all its current children.
        for child in &self.children[parent_key] {
            self.parents[(*child).into()] = None;
        }

        // Build up relation node <-> child
        for &child in children {
            // Remove child from previous parent
            if let Some(previous_parent) = self.parents[child.into()] {
                self.remove_child(previous_parent, child).unwrap();
            }
            self.parents[child.into()] = Some(parent);
        }

        let parent_children = &mut self.children[parent_key];
        parent_children.clear();
        children.iter().for_each(|child| parent_children.push(*child));

        self.mark_dirty(parent)?;

        Ok(())
    }

    /// Removes the `child` of the parent `node`
    ///
    /// The child is not removed from the tree entirely, it is simply no longer attached to its previous parent.
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) -> TakiaResult<NodeId> {
        let index = self.children[parent.into()].iter().position(|n| *n == child).unwrap();
        self.remove_child_at_index(parent, index)
    }

    /// Removes the child at the given `index` from the `parent`
    ///
    /// The child is not removed from the tree entirely, it is simply no longer attached to its previous parent.
    pub fn remove_child_at_index(&mut self, parent: NodeId, child_index: usize) -> TakiaResult<NodeId> {
        let parent_key = parent.into();
        let child_count = self.children[parent_key].len();
        if child_index >= child_count {
            return Err(TakiaError::ChildIndexOutOfBounds { parent, child_index, child_count });
        }

        let child = self.children[parent_key].remove(child_index);
        self.parents[child.into()] = None;

        self.mark_dirty(parent)?;

        Ok(child)
    }

    /// Removes children at the given range from the `parent`
    ///
    /// Children are not removed from the tree entirely, they are simply no longer attached to their previous parent.
    ///
    /// Function will panic if given range is invalid. See [`core::slice::range`]
    pub fn remove_children_range<R>(&mut self, parent: NodeId, range: R) -> TakiaResult<()>
    where
        R: core::ops::RangeBounds<usize>,
    {
        let parent_key = parent.into();
        for child in self.children[parent_key].drain(range) {
            self.parents[child.into()] = None;
        }

        self.mark_dirty(parent)?;
        Ok(())
    }

    /// Replaces the child at the given `child_index` from the `parent` node with the new `child` node
    ///
    /// The child is not removed from the tree entirely, it is simply no longer attached to its previous parent.
    pub fn replace_child_at_index(
        &mut self,
        parent: NodeId,
        child_index: usize,
        new_child: NodeId,
    ) -> TakiaResult<NodeId> {
        let parent_key = parent.into();

        let child_count = self.children[parent_key].len();
        if child_index >= child_count {
            return Err(TakiaError::ChildIndexOutOfBounds { parent, child_index, child_count });
        }

        self.parents[new_child.into()] = Some(parent);
        let old_child = core::mem::replace(&mut self.children[parent_key][child_index], new_child);
        self.parents[old_child.into()] = None;

        self.mark_dirty(parent)?;

        Ok(old_child)
    }

    /// Returns the child node of the parent `node` at the provided `child_index`
    #[inline]
    pub fn child_at_index(&self, parent: NodeId, child_index: usize) -> TakiaResult<NodeId> {
        let parent_key = parent.into();
        let child_count = self.children[parent_key].len();
        if child_index >= child_count {
            return Err(TakiaError::ChildIndexOutOfBounds { parent, child_index, child_count });
        }

        Ok(self.children[parent_key][child_index])
    }

    /// Returns the total number of nodes in the tree
    #[inline]
    pub fn total_node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the `NodeId` of the parent node of the specified node (if it exists)
    ///
    /// - Return None if the specified node has no parent
    /// - Panics if the specified node does not exist
    #[inline]
    pub fn parent(&self, child_id: NodeId) -> Option<NodeId> {
        self.parents[child_id.into()]
    }

    /// Returns a list of children that belong to the parent node
    pub fn children(&self, parent: NodeId) -> TakiaResult<Vec<NodeId>> {
        Ok(self.children[parent.into()].clone())
    }

    /// Returns the out-of-flow (absolute/fixed) boxes whose containing block is `node`, as
    /// recorded by the last layout. These boxes are laid out by `node` rather than by their
    /// parent, and their [`Layout::location`] is relative to `node`.
    pub fn hoisted_children(&self, node: NodeId) -> TakiaResult<&[NodeId]> {
        Ok(&self.nodes[node.into()].hoisted_children)
    }

    /// Sets the [`Style`] of the provided `node`
    #[inline]
    pub fn set_style(&mut self, node: NodeId, style: Style) -> TakiaResult<()> {
        self.nodes[node.into()].style = style;
        self.mark_dirty(node)?;
        Ok(())
    }

    /// Gets the [`Style`] of the provided `node`
    #[inline]
    pub fn style(&self, node: NodeId) -> TakiaResult<&Style> {
        Ok(&self.nodes[node.into()].style)
    }

    /// Return this node layout relative to its parent
    #[inline]
    pub fn layout(&self, node: NodeId) -> TakiaResult<&Layout> {
        if self.config.use_rounding {
            Ok(&self.nodes[node.into()].final_layout)
        } else {
            Ok(&self.nodes[node.into()].unrounded_layout)
        }
    }

    /// Returns this node layout with unrounded values relative to its parent.
    #[inline]
    pub fn unrounded_layout(&self, node: NodeId) -> &Layout {
        &self.nodes[node.into()].unrounded_layout
    }

    /// Get the "detailed layout info" for a node.
    ///
    /// Currently this is only implemented for CSS Grid containers where it contains
    /// the computed size of each grid track and the computed placement of each grid item
    #[inline]
    pub fn detailed_layout_info(&self, node_id: NodeId) -> &DetailedLayoutInfo {
        &self.nodes[node_id.into()].detailed_layout_info
    }

    /// Marks the layout of this node and its ancestors as outdated
    pub fn mark_dirty(&mut self, node: NodeId) -> TakiaResult<()> {
        fn mark_dirty_recursive(
            nodes: &mut SlotMap<DefaultKey, NodeData>,
            parents: &SlotMap<DefaultKey, Option<NodeId>>,
            node_key: DefaultKey,
        ) {
            match nodes[node_key].mark_dirty() {
                ClearState::AlreadyEmpty => {
                    // Node was already marked as dirty.
                    // No need to visit ancestors
                    // as they should be marked as dirty already.
                }
                ClearState::Cleared => {
                    if let Some(Some(node)) = parents.get(node_key) {
                        mark_dirty_recursive(nodes, parents, (*node).into());
                    }
                }
            }
        }

        mark_dirty_recursive(&mut self.nodes, &self.parents, node.into());

        Ok(())
    }

    /// Indicates whether the layout of this node needs to be recomputed
    #[inline]
    pub fn dirty(&self, node: NodeId) -> TakiaResult<bool> {
        Ok(self.nodes[node.into()].cache.is_empty())
    }

    /// Updates the stored layout of the provided `node` and its children
    pub fn compute_layout_with_measure<MeasureFunction>(
        &mut self,
        node_id: NodeId,
        available_space: Size<AvailableSpace>,
        measure_function: MeasureFunction,
    ) -> Result<(), TakiaError>
    where
        MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
    {
        let use_rounding = self.config.use_rounding;
        let mut takia_view = TakiaView { takia: self, measure_function };
        compute_root_layout(&mut takia_view, node_id, available_space);
        if use_rounding {
            round_layout(&mut takia_view, node_id);
        }
        Ok(())
    }

    /// Updates the stored layout of the provided `node` and its children
    pub fn compute_layout(&mut self, node: NodeId, available_space: Size<AvailableSpace>) -> Result<(), TakiaError> {
        self.compute_layout_with_measure(node, available_space, |inputs, _, _, style| {
            compute_leaf_layout(inputs, style, |_, _| 0.0, |_, _| Size::ZERO)
        })
    }

    /// Prints a debug representation of the tree's layout
    #[cfg(feature = "std")]
    pub fn print_tree(&mut self, root: NodeId) {
        crate::util::print_tree(self, root)
    }

    /// Returns an instance of LayoutTree representing the TakiaTree
    #[cfg(test)]
    pub(crate) fn as_layout_tree(&mut self) -> impl LayoutPartialTree + CacheTree + '_ {
        TakiaView {
            takia: self,
            measure_function: |inputs, _, _, style| compute_leaf_layout(inputs, style, |_, _| 0.0, |_, _| Size::ZERO),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::style::{Dimension, Display, FlexDirection};
    use crate::style_helpers::*;
    use crate::util::sys;

    fn size_measure_function(
        inputs: LayoutInput,
        _node_id: NodeId,
        node_context: Option<&mut Size<f32>>,
        style: &Style,
    ) -> LayoutOutput {
        compute_leaf_layout(
            inputs,
            style,
            |_, _| 0.0,
            |known_dimensions, _available_space| {
                known_dimensions.unwrap_or(node_context.cloned().unwrap_or(Size::ZERO))
            },
        )
    }

    #[test]
    fn new_should_allocate_default_capacity() {
        const DEFAULT_CAPACITY: usize = 16; // This is the capacity defined in the `impl Default`
        let takia: TakiaTree<()> = TakiaTree::new();

        assert!(takia.children.capacity() >= DEFAULT_CAPACITY);
        assert!(takia.parents.capacity() >= DEFAULT_CAPACITY);
        assert!(takia.nodes.capacity() >= DEFAULT_CAPACITY);
    }

    #[test]
    fn test_with_capacity() {
        const CAPACITY: usize = 8;
        let takia: TakiaTree<()> = TakiaTree::with_capacity(CAPACITY);

        assert!(takia.children.capacity() >= CAPACITY);
        assert!(takia.parents.capacity() >= CAPACITY);
        assert!(takia.nodes.capacity() >= CAPACITY);
    }

    #[test]
    fn test_new_leaf() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let res = takia.new_leaf(Style::default());
        assert!(res.is_ok());
        let node = res.unwrap();

        // node should be in the takia tree and have no children
        assert!(takia.child_count(node) == 0);
    }

    #[test]
    fn new_leaf_with_context() {
        let mut takia: TakiaTree<Size<f32>> = TakiaTree::new();

        let res = takia.new_leaf_with_context(Style::default(), Size::ZERO);
        assert!(res.is_ok());
        let node = res.unwrap();

        // node should be in the takia tree and have no children
        assert!(takia.child_count(node) == 0);
    }

    /// Test that new_with_children works as expected
    #[test]
    fn test_new_with_children() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1]).unwrap();

        // node should have two children
        assert_eq!(takia.child_count(node), 2);
        assert_eq!(takia.children(node).unwrap()[0], child0);
        assert_eq!(takia.children(node).unwrap()[1], child1);
    }

    #[test]
    fn remove_node_should_remove() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let node = takia.new_leaf(Style::default()).unwrap();

        let _ = takia.remove(node).unwrap();
    }

    #[test]
    fn remove_node_should_detach_hierarchy() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        // Build a linear tree layout: <0> <- <1> <- <2>
        let node2 = takia.new_leaf(Style::default()).unwrap();
        let node1 = takia.new_with_children(Style::default(), &[node2]).unwrap();
        let node0 = takia.new_with_children(Style::default(), &[node1]).unwrap();

        // Both node0 and node1 should have 1 child nodes
        assert_eq!(takia.children(node0).unwrap().as_slice(), &[node1]);
        assert_eq!(takia.children(node1).unwrap().as_slice(), &[node2]);

        // Disconnect the tree: <0> <2>
        let _ = takia.remove(node1).unwrap();

        // Both remaining nodes should have no child nodes
        assert!(takia.children(node0).unwrap().is_empty());
        assert!(takia.children(node2).unwrap().is_empty());
    }

    #[test]
    fn remove_last_node() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let parent = takia.new_leaf(Style::default()).unwrap();
        let child = takia.new_leaf(Style::default()).unwrap();
        takia.add_child(parent, child).unwrap();

        takia.remove(child).unwrap();
        takia.remove(parent).unwrap();
    }

    // Related to: https://github.com/DioxusLabs/takia/issues/998
    #[test]
    fn remove_node_marks_parent_dirty() {
        use crate::prelude::*;

        let mut takia: TakiaTree<()> = TakiaTree::new();

        // The root's height comes entirely from its child's 1px bottom border
        let child = takia
            .new_leaf(Style { border: Rect { bottom: length(1f32), ..Rect::zero() }, ..Default::default() })
            .unwrap();
        let root = takia.new_with_children(Style::default(), &[child]).unwrap();

        takia.compute_layout(root, Size::MIN_CONTENT).unwrap();
        assert_eq!(takia.layout(root).unwrap().size.height, 1f32);

        takia.remove(child).unwrap();
        assert_eq!(takia.dirty(root), Ok(true));

        // The root no longer has any children, so its height should shrink back to zero
        takia.compute_layout(root, Size::MIN_CONTENT).unwrap();
        assert_eq!(takia.layout(root).unwrap().size.height, 0f32);
    }

    #[test]
    fn set_measure() {
        let mut takia: TakiaTree<Size<f32>> = TakiaTree::new();
        let node = takia.new_leaf_with_context(Style::default(), Size { width: 200.0, height: 200.0 }).unwrap();
        takia.compute_layout_with_measure(node, Size::MAX_CONTENT, size_measure_function).unwrap();
        assert_eq!(takia.layout(node).unwrap().size.width, 200.0);

        takia.set_node_context(node, Some(Size { width: 100.0, height: 100.0 })).unwrap();
        takia.compute_layout_with_measure(node, Size::MAX_CONTENT, size_measure_function).unwrap();
        assert_eq!(takia.layout(node).unwrap().size.width, 100.0);
    }

    #[test]
    fn set_measure_of_previously_unmeasured_node() {
        let mut takia: TakiaTree<Size<f32>> = TakiaTree::new();
        let node = takia.new_leaf(Style::default()).unwrap();
        takia.compute_layout_with_measure(node, Size::MAX_CONTENT, size_measure_function).unwrap();
        assert_eq!(takia.layout(node).unwrap().size.width, 0.0);

        takia.set_node_context(node, Some(Size { width: 100.0, height: 100.0 })).unwrap();
        takia.compute_layout_with_measure(node, Size::MAX_CONTENT, size_measure_function).unwrap();
        assert_eq!(takia.layout(node).unwrap().size.width, 100.0);
    }

    /// Test that adding `add_child()` works
    #[test]
    fn add_child() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let node = takia.new_leaf(Style::default()).unwrap();
        assert_eq!(takia.child_count(node), 0);

        let child0 = takia.new_leaf(Style::default()).unwrap();
        takia.add_child(node, child0).unwrap();
        assert_eq!(takia.child_count(node), 1);

        let child1 = takia.new_leaf(Style::default()).unwrap();
        takia.add_child(node, child1).unwrap();
        assert_eq!(takia.child_count(node), 2);
    }

    #[test]
    fn insert_child_at_index() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let child2 = takia.new_leaf(Style::default()).unwrap();

        let node = takia.new_leaf(Style::default()).unwrap();
        assert_eq!(takia.child_count(node), 0);

        takia.insert_child_at_index(node, 0, child0).unwrap();
        assert_eq!(takia.child_count(node), 1);
        assert_eq!(takia.children(node).unwrap()[0], child0);

        takia.insert_child_at_index(node, 0, child1).unwrap();
        assert_eq!(takia.child_count(node), 2);
        assert_eq!(takia.children(node).unwrap()[0], child1);
        assert_eq!(takia.children(node).unwrap()[1], child0);

        takia.insert_child_at_index(node, 1, child2).unwrap();
        assert_eq!(takia.child_count(node), 3);
        assert_eq!(takia.children(node).unwrap()[0], child1);
        assert_eq!(takia.children(node).unwrap()[1], child2);
        assert_eq!(takia.children(node).unwrap()[2], child0);
    }

    #[test]
    fn set_children() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1]).unwrap();

        assert_eq!(takia.child_count(node), 2);
        assert_eq!(takia.children(node).unwrap()[0], child0);
        assert_eq!(takia.children(node).unwrap()[1], child1);

        let child2 = takia.new_leaf(Style::default()).unwrap();
        let child3 = takia.new_leaf(Style::default()).unwrap();
        takia.set_children(node, &[child2, child3]).unwrap();

        assert_eq!(takia.child_count(node), 2);
        assert_eq!(takia.children(node).unwrap()[0], child2);
        assert_eq!(takia.children(node).unwrap()[1], child3);
    }

    /// Test that removing a child works
    #[test]
    fn remove_child() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1]).unwrap();

        assert_eq!(takia.child_count(node), 2);

        takia.remove_child(node, child0).unwrap();
        assert_eq!(takia.child_count(node), 1);
        assert_eq!(takia.children(node).unwrap()[0], child1);

        takia.remove_child(node, child1).unwrap();
        assert_eq!(takia.child_count(node), 0);
    }

    #[test]
    fn remove_child_at_index() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1]).unwrap();

        assert_eq!(takia.child_count(node), 2);

        takia.remove_child_at_index(node, 0).unwrap();
        assert_eq!(takia.child_count(node), 1);
        assert_eq!(takia.children(node).unwrap()[0], child1);

        takia.remove_child_at_index(node, 0).unwrap();
        assert_eq!(takia.child_count(node), 0);
    }

    #[test]
    fn remove_children_range() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let child2 = takia.new_leaf(Style::default()).unwrap();
        let child3 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1, child2, child3]).unwrap();

        assert_eq!(takia.child_count(node), 4);

        takia.remove_children_range(node, 1..=2).unwrap();
        assert_eq!(takia.child_count(node), 2);
        assert_eq!(takia.children(node).unwrap(), [child0, child3]);
        for child in [child0, child3] {
            assert_eq!(takia.parent(child), Some(node));
        }
        for child in [child1, child2] {
            assert_eq!(takia.parent(child), None);
        }
    }

    // Related to: https://github.com/DioxusLabs/takia/issues/510
    #[test]
    fn remove_child_updates_parents() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let parent = takia.new_leaf(Style::default()).unwrap();
        let child = takia.new_leaf(Style::default()).unwrap();

        takia.add_child(parent, child).unwrap();

        takia.remove(parent).unwrap();

        // Once the parent is removed this shouldn't panic.
        assert!(takia.set_children(child, &[]).is_ok());
    }

    #[test]
    fn replace_child_at_index() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();

        let node = takia.new_with_children(Style::default(), &[child0]).unwrap();
        assert_eq!(takia.child_count(node), 1);
        assert_eq!(takia.children(node).unwrap()[0], child0);

        takia.replace_child_at_index(node, 0, child1).unwrap();
        assert_eq!(takia.child_count(node), 1);
        assert_eq!(takia.children(node).unwrap()[0], child1);
    }
    #[test]
    fn test_child_at_index() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let child2 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1, child2]).unwrap();

        assert!(if let Ok(result) = takia.child_at_index(node, 0) { result == child0 } else { false });
        assert!(if let Ok(result) = takia.child_at_index(node, 1) { result == child1 } else { false });
        assert!(if let Ok(result) = takia.child_at_index(node, 2) { result == child2 } else { false });
    }
    #[test]
    fn test_child_count() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1]).unwrap();

        assert!(takia.child_count(node) == 2);
        assert!(takia.child_count(child0) == 0);
        assert!(takia.child_count(child1) == 0);
    }

    #[allow(clippy::vec_init_then_push)]
    #[test]
    fn test_children() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1]).unwrap();

        let mut children = sys::Vec::new();
        children.push(child0);
        children.push(child1);

        let children_result = takia.children(node).unwrap();
        assert_eq!(children_result, children);

        assert!(takia.children(child0).unwrap().is_empty());
    }
    #[test]
    fn test_set_style() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let node = takia.new_leaf(Style::default()).unwrap();
        assert_eq!(takia.style(node).unwrap().display, Display::Flex);

        takia.set_style(node, Style { display: Display::None, ..Style::default() }).unwrap();
        assert_eq!(takia.style(node).unwrap().display, Display::None);
    }
    #[test]
    fn test_style() {
        let mut takia: TakiaTree<()> = TakiaTree::new();

        let style = Style { display: Display::None, flex_direction: FlexDirection::RowReverse, ..Default::default() };

        let node = takia.new_leaf(style.clone()).unwrap();

        let res = takia.style(node);
        assert!(res.is_ok());
        assert!(res.unwrap() == &style);
    }
    #[test]
    fn test_layout() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let node = takia.new_leaf(Style::default()).unwrap();

        // TODO: Improve this test?
        let res = takia.layout(node);
        assert!(res.is_ok());
    }

    #[test]
    fn test_mark_dirty() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child0 = takia.new_leaf(Style::default()).unwrap();
        let child1 = takia.new_leaf(Style::default()).unwrap();
        let node = takia.new_with_children(Style::default(), &[child0, child1]).unwrap();

        takia.compute_layout(node, Size::MAX_CONTENT).unwrap();

        assert_eq!(takia.dirty(child0), Ok(false));
        assert_eq!(takia.dirty(child1), Ok(false));
        assert_eq!(takia.dirty(node), Ok(false));

        takia.mark_dirty(node).unwrap();
        assert_eq!(takia.dirty(child0), Ok(false));
        assert_eq!(takia.dirty(child1), Ok(false));
        assert_eq!(takia.dirty(node), Ok(true));

        takia.compute_layout(node, Size::MAX_CONTENT).unwrap();
        takia.mark_dirty(child0).unwrap();
        assert_eq!(takia.dirty(child0), Ok(true));
        assert_eq!(takia.dirty(child1), Ok(false));
        assert_eq!(takia.dirty(node), Ok(true));
    }

    #[test]
    fn compute_layout_should_produce_valid_result() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let node_result = takia.new_leaf(Style {
            size: Size { width: Dimension::from_length(10f32), height: Dimension::from_length(10f32) },
            ..Default::default()
        });
        assert!(node_result.is_ok());
        let node = node_result.unwrap();
        let layout_result = takia.compute_layout(
            node,
            Size { width: AvailableSpace::Definite(100.), height: AvailableSpace::Definite(100.) },
        );
        assert!(layout_result.is_ok());
    }

    #[test]
    fn make_sure_layout_location_is_top_left() {
        use crate::prelude::*;

        let mut takia: TakiaTree<()> = TakiaTree::new();

        let node = takia
            .new_leaf(Style {
                size: Size { width: Dimension::from_percent(1f32), height: Dimension::from_percent(1f32) },
                ..Default::default()
            })
            .unwrap();

        let root = takia
            .new_with_children(
                Style {
                    size: Size { width: Dimension::from_length(100f32), height: Dimension::from_length(100f32) },
                    padding: Rect {
                        left: length(10f32),
                        right: length(20f32),
                        top: length(30f32),
                        bottom: length(40f32),
                    },
                    ..Default::default()
                },
                &[node],
            )
            .unwrap();

        takia.compute_layout(root, Size::MAX_CONTENT).unwrap();

        // If Layout::location represents top-left coord, 'node' location
        // must be (due applied 'root' padding): {x: 10, y: 30}.
        //
        // It's important, since result will be different for each other
        // coordinate space:
        // - bottom-left:  {x: 10, y: 40}
        // - top-right:    {x: 20, y: 30}
        // - bottom-right: {x: 20, y: 40}
        let layout = takia.layout(node).unwrap();
        assert_eq!(layout.location.x, 10f32);
        assert_eq!(layout.location.y, 30f32);
    }

    #[test]
    fn set_children_reparents() {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child = takia.new_leaf(Style::default()).unwrap();
        let old_parent = takia.new_with_children(Style::default(), &[child]).unwrap();

        let new_parent = takia.new_leaf(Style::default()).unwrap();
        takia.set_children(new_parent, &[child]).unwrap();

        assert!(takia.children(old_parent).unwrap().is_empty());
    }

    /// A context that increments a counter when dropped. Each test owns its own static
    /// counter so the two can run in parallel.
    struct DropCounted(&'static core::sync::atomic::AtomicUsize);

    impl Drop for DropCounted {
        fn drop(&mut self) {
            self.0.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn remove_drops_the_nodes_context() {
        static DROPS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
        let mut takia: TakiaTree<DropCounted> = TakiaTree::new();
        let node = takia.new_leaf_with_context(Style::DEFAULT, DropCounted(&DROPS)).unwrap();

        takia.remove(node).unwrap();

        assert_eq!(
            DROPS.load(core::sync::atomic::Ordering::SeqCst),
            1,
            "removing a node must drop its context rather than strand it in node_context_data"
        );
    }

    #[test]
    fn clear_drops_every_nodes_context() {
        static DROPS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
        let mut takia: TakiaTree<DropCounted> = TakiaTree::new();
        for _ in 0..8 {
            takia.new_leaf_with_context(Style::DEFAULT, DropCounted(&DROPS)).unwrap();
        }

        takia.clear();

        assert_eq!(
            DROPS.load(core::sync::atomic::Ordering::SeqCst),
            8,
            "clear() drops all nodes, so it must drop their contexts too"
        );
    }
}
