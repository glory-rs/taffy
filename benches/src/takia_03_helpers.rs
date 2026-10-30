use rand::distr::uniform::SampleRange;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use takia::style::Style as TakiaStyle;

use super::{BuildTree, BuildTreeExt, GenStyle};

pub struct Takia03TreeBuilder<R: Rng, G: GenStyle<TakiaStyle>> {
    rng: R,
    style_generator: G,
    tree: takia_03::Taffy,
    root: takia_03::prelude::Node,
    nodes: Vec<takia_03::prelude::Node>,
}

// Implement the BuildTree trait
impl<R: Rng, G: GenStyle<TakiaStyle>> BuildTree<R, G> for Takia03TreeBuilder<R, G> {
    const NAME: &'static str = "Taffy 0.3";
    type Tree = takia_03::Taffy;
    type Node = takia_03::prelude::Node;

    fn with_rng(mut rng: R, mut style_generator: G) -> Self {
        let mut tree = takia_03::Taffy::new();
        let root = tree.new_leaf(convert_style(style_generator.create_root_style(&mut rng))).unwrap();
        Takia03TreeBuilder { rng, style_generator, tree, root, nodes: vec![root] }
    }

    fn compute_layout_inner(&mut self, available_width: Option<f32>, available_height: Option<f32>) {
        let available_space =
            takia_03::geometry::Size { width: available_width.into(), height: available_height.into() };
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
        let node = self.tree.new_leaf(convert_style(style)).unwrap();
        self.nodes.push(node);
        node
    }

    fn create_container_node(&mut self, children: &[Self::Node]) -> Self::Node {
        let style = self.style_generator.create_container_style(&mut self.rng);
        let node = self.tree.new_with_children(convert_style(style), children).unwrap();
        self.nodes.push(node);
        node
    }

    fn create_node_with_style(&mut self, style: TakiaStyle, children: &[Self::Node]) -> Self::Node {
        let node = self.tree.new_with_children(convert_style(style), children).unwrap();
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

impl<G: GenStyle<TakiaStyle>> BuildTreeExt<G> for Takia03TreeBuilder<ChaCha8Rng, G> {}

fn convert_style(style: takia::style::Style) -> takia_03::style::Style {
    takia_03::style::Style {
        display: convert_display(style.display),
        position: convert_position(style.position),
        inset: convert_rect(style.inset, convert_length_percentage_auto),
        margin: convert_rect(style.margin, convert_length_percentage_auto),
        padding: convert_rect(style.padding, convert_length_percentage),
        border: convert_rect(style.border, convert_length_percentage),
        size: convert_size(style.size, convert_dimension),
        min_size: convert_size(style.min_size, convert_dimension),
        max_size: convert_size(style.max_size, convert_dimension),
        aspect_ratio: style.aspect_ratio,
        gap: convert_size(style.gap, convert_length_percentage),
        // Alignment
        align_items: None,
        align_self: None,
        justify_items: None,
        justify_self: None,
        align_content: None,
        justify_content: None,
        // Flexbox
        flex_direction: convert_flex_direction(style.flex_direction),
        flex_wrap: convert_flex_wrap(style.flex_wrap),
        flex_grow: style.flex_grow,
        flex_shrink: style.flex_shrink,
        flex_basis: convert_dimension(style.flex_basis),
        // Grid
        grid_template_rows: Vec::new(),
        grid_template_columns: Vec::new(),
        grid_auto_rows: Vec::new(),
        grid_auto_columns: Vec::new(),
        grid_auto_flow: takia_03::style::GridAutoFlow::Row,
        grid_row: takia_03::geometry::Line {
            start: takia_03::style::GridPlacement::Auto,
            end: takia_03::style::GridPlacement::Auto,
        },
        grid_column: takia_03::geometry::Line {
            start: takia_03::style::GridPlacement::Auto,
            end: takia_03::style::GridPlacement::Auto,
        },
    }
}

fn convert_rect<T, U, F: Fn(T) -> U>(input: takia::geometry::Rect<T>, map: F) -> takia_03::geometry::Rect<U> {
    takia_03::geometry::Rect {
        left: map(input.left),
        right: map(input.right),
        top: map(input.top),
        bottom: map(input.bottom),
    }
}

fn convert_size<T, U, F: Fn(T) -> U>(input: takia::geometry::Size<T>, map: F) -> takia_03::geometry::Size<U> {
    takia_03::geometry::Size { width: map(input.width), height: map(input.height) }
}

fn convert_point<T, U, F: Fn(T) -> U>(input: takia::geometry::Point<T>, map: F) -> takia_03::geometry::Point<U> {
    takia_03::geometry::Point { x: map(input.x), y: map(input.y) }
}

fn convert_dimension(input: takia::style::Dimension) -> takia_03::style::Dimension {
    let raw = input.into_raw();
    match raw.tag() {
        takia::style::CompactLength::LENGTH_TAG => takia_03::style::Dimension::Points(raw.value()),
        takia::style::CompactLength::PERCENT_TAG => takia_03::style::Dimension::Percent(raw.value()),
        takia::style::CompactLength::AUTO_TAG => takia_03::style::Dimension::Auto,
        _ => panic!("unsupported Dimension variant"),
    }
}

fn convert_length_percentage_auto(input: takia::style::LengthPercentageAuto) -> takia_03::style::LengthPercentageAuto {
    let raw = input.into_raw();
    match raw.tag() {
        takia::style::CompactLength::LENGTH_TAG => takia_03::style::LengthPercentageAuto::Points(raw.value()),
        takia::style::CompactLength::PERCENT_TAG => takia_03::style::LengthPercentageAuto::Percent(raw.value()),
        takia::style::CompactLength::AUTO_TAG => takia_03::style::LengthPercentageAuto::Auto,
        _ => panic!("unsupported LengthPercentageAuto variant"),
    }
}

fn convert_length_percentage(input: takia::style::LengthPercentage) -> takia_03::style::LengthPercentage {
    let raw = input.into_raw();
    match raw.tag() {
        takia::style::CompactLength::LENGTH_TAG => takia_03::style::LengthPercentage::Points(raw.value()),
        takia::style::CompactLength::PERCENT_TAG => takia_03::style::LengthPercentage::Percent(raw.value()),
        _ => panic!("unsupported LengthPercentage variant"),
    }
}

fn convert_display(input: takia::style::Display) -> takia_03::style::Display {
    match input {
        takia::style::Display::None => takia_03::style::Display::None,
        takia::style::Display::Flex => takia_03::style::Display::Flex,
        takia::style::Display::Grid => takia_03::style::Display::Grid,
        takia::style::Display::Block => panic!("Block layout not implemented in takia 0.3"),
        takia::style::Display::FlowRoot => panic!("Block layout not implemented in takia 0.3"),
    }
}

fn convert_position(input: takia::style::Position) -> takia_03::style::Position {
    match input {
        takia::style::Position::Static => takia_03::style::Position::Relative,
        takia::style::Position::Relative => takia_03::style::Position::Relative,
        takia::style::Position::Absolute => takia_03::style::Position::Absolute,
        takia::style::Position::Fixed => takia_03::style::Position::Absolute,
        takia::style::Position::Sticky => takia_03::style::Position::Relative,
    }
}

fn convert_flex_direction(input: takia::style::FlexDirection) -> takia_03::style::FlexDirection {
    match input {
        takia::style::FlexDirection::Row => takia_03::style::FlexDirection::Row,
        takia::style::FlexDirection::Column => takia_03::style::FlexDirection::Column,
        takia::style::FlexDirection::RowReverse => takia_03::style::FlexDirection::RowReverse,
        takia::style::FlexDirection::ColumnReverse => takia_03::style::FlexDirection::ColumnReverse,
    }
}

fn convert_flex_wrap(input: takia::style::FlexWrap) -> takia_03::style::FlexWrap {
    match input {
        takia::style::FlexWrap::NoWrap => takia_03::style::FlexWrap::NoWrap,
        takia::style::FlexWrap::Wrap => takia_03::style::FlexWrap::Wrap,
        takia::style::FlexWrap::WrapReverse => takia_03::style::FlexWrap::WrapReverse,
    }
}
