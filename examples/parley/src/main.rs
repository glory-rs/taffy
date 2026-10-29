mod image;
use image::{image_measure_function, ImageContext};
use parley::{style::StyleProperty, FontContext, Layout, LayoutContext, LineHeight};
use takia::prelude::*;

pub const LOREM_IPSUM : &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

struct ParleyTextContext {
    layout: Layout<[u8; 4]>,
}

impl ParleyTextContext {
    fn new(
        text: &str,
        font_size: f32,
        line_height: f32,
        layout_context: &mut LayoutContext,
        font_context: &mut FontContext,
    ) -> Self {
        let mut builder = layout_context.ranged_builder(font_context, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(line_height)));

        Self { layout: builder.build(text) }
    }

    fn measure(
        &mut self,
        known_dimensions: takia::Size<Option<f32>>,
        available_space: takia::Size<takia::AvailableSpace>,
    ) -> takia::Size<f32> {
        let width = known_dimensions.width.unwrap_or_else(|| {
            let widths = self.layout.calculate_content_widths();
            match available_space.width {
                AvailableSpace::MinContent => widths.min,
                AvailableSpace::MaxContent => widths.max,
                AvailableSpace::Definite(limit) => limit.min(widths.max).max(widths.min),
            }
            .ceil()
        });

        self.layout.break_all_lines(Some(width));
        let height = known_dimensions.height.unwrap_or_else(|| self.layout.height());

        takia::Size { width, height }
    }
}

#[allow(clippy::large_enum_variant)]
enum NodeContext {
    Text(ParleyTextContext),
    Image(ImageContext),
}

impl NodeContext {
    /// Constructor for a text node context
    fn text(
        text: &str,
        font_size: f32,
        line_height: f32,
        layout_context: &mut LayoutContext,
        font_context: &mut FontContext,
    ) -> Self {
        NodeContext::Text(ParleyTextContext::new(text, font_size, line_height, layout_context, font_context))
    }

    /// Constructor for an image node context
    fn image(width: f32, height: f32) -> Self {
        NodeContext::Image(ImageContext { width, height })
    }
}

fn measure_function(
    known_dimensions: takia::Size<Option<f32>>,
    available_space: takia::Size<takia::AvailableSpace>,
    node_context: Option<&mut NodeContext>,
) -> Size<f32> {
    if let Size { width: Some(width), height: Some(height) } = known_dimensions {
        return Size { width, height };
    }

    match node_context {
        None => Size::ZERO,
        Some(NodeContext::Text(text_context)) => text_context.measure(known_dimensions, available_space),
        Some(NodeContext::Image(image_context)) => image_measure_function(known_dimensions, image_context),
    }
}

fn main() -> Result<(), takia::TakiaError> {
    let mut font_context = FontContext::new();
    let mut layout_context = LayoutContext::new();
    let mut takia: TakiaTree<NodeContext> = TakiaTree::new();

    let text_node = takia.new_leaf_with_context(
        Style::default(),
        NodeContext::text(LOREM_IPSUM, 14.0, 16.0, &mut layout_context, &mut font_context),
    )?;

    let image_node = takia.new_leaf_with_context(Style::default(), NodeContext::image(400.0, 300.0))?;

    let root = takia.new_with_children(
        Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            size: Size { width: length(200.0), height: auto() },
            ..Default::default()
        },
        &[text_node, image_node],
    )?;

    // Compute layout and print result
    takia.compute_layout_with_measure(root, Size::MAX_CONTENT, |inputs, _node_id, node_context, style| {
        takia::compute_leaf_layout(
            inputs,
            style,
            |_, _| 0.0,
            |known_dimensions, available_space| measure_function(known_dimensions, available_space, node_context),
        )
    })?;
    takia.print_tree(root);

    Ok(())
}
