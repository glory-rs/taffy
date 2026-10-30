use rand::distr::uniform::SampleRange;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use takia::style::Style as TakiaStyle;
use takia::tree::NodeId as TakiaNodeId;
use takia::TakiaTree;

use super::{BuildTree, BuildTreeExt, GenStyle};

pub struct TakiaTreeBuilder<R: Rng, G: GenStyle<TakiaStyle>> {
    rng: R,
    style_generator: G,
    tree: TakiaTree,
    root: TakiaNodeId,
    nodes: Vec<TakiaNodeId>,
}

// Implement the BuildTree trait
impl<R: Rng, G: GenStyle<TakiaStyle>> BuildTree<R, G> for TakiaTreeBuilder<R, G> {
    const NAME: &'static str = "Takia";
    type Tree = TakiaTree;
    type Node = TakiaNodeId;

    fn with_rng(mut rng: R, mut style_generator: G) -> Self {
        let mut tree = TakiaTree::new();
        let root = tree.new_leaf(style_generator.create_root_style(&mut rng)).unwrap();
        TakiaTreeBuilder { rng, style_generator, tree, root, nodes: vec![root] }
    }

    fn compute_layout_inner(&mut self, available_width: Option<f32>, available_height: Option<f32>) {
        let available_space = takia::geometry::Size { width: available_width.into(), height: available_height.into() };
        self.tree.compute_layout(self.root, available_space).unwrap();
    }

    fn mark_all_dirty(&mut self) {
        for &node in &self.nodes {
            self.tree.mark_dirty(node).unwrap();
        }
    }

    fn random_usize(&mut self, range: impl SampleRange<usize>) -> usize {
        self.rng.random_range(range)
    }

    fn create_leaf_node(&mut self) -> Self::Node {
        let style = self.style_generator.create_leaf_style(&mut self.rng);
        let node = self.tree.new_leaf(style).unwrap();
        self.nodes.push(node);
        node
    }

    fn create_container_node(&mut self, children: &[Self::Node]) -> Self::Node {
        let style = self.style_generator.create_container_style(&mut self.rng);
        let node = self.tree.new_with_children(style, children).unwrap();
        self.nodes.push(node);
        node
    }

    fn create_node_with_style(&mut self, style: TakiaStyle, children: &[Self::Node]) -> Self::Node {
        let node = self.tree.new_with_children(style, children).unwrap();
        self.nodes.push(node);
        node
    }

    fn total_node_count(&mut self) -> usize {
        self.tree.total_node_count()
    }

    fn set_root_children(&mut self, children: &[Self::Node]) {
        self.tree.set_children(self.root, children).unwrap();
    }

    fn into_tree_and_root(self) -> (Self::Tree, Self::Node) {
        (self.tree, self.root)
    }
}

impl<G: GenStyle<TakiaStyle>> BuildTreeExt<G> for TakiaTreeBuilder<ChaCha8Rng, G> {}
