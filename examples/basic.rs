use takia::prelude::*;

fn main() -> Result<(), takia::TakiaError> {
    let mut takia: TakiaTree<()> = TakiaTree::new();

    let child = takia.new_leaf(Style {
        size: Size { width: Dimension::from_percent(0.5), height: Dimension::AUTO },
        ..Default::default()
    })?;

    let node = takia.new_with_children(
        Style {
            size: Size { width: Dimension::from_length(100.0), height: Dimension::from_length(100.0) },
            justify_content: JustifyContent::CENTER,
            ..Default::default()
        },
        &[child],
    )?;

    println!("Compute layout with 100x100 viewport:");
    takia.compute_layout(
        node,
        Size { height: AvailableSpace::Definite(100.0), width: AvailableSpace::Definite(100.0) },
    )?;
    println!("node: {:#?}", takia.layout(node)?);
    println!("child: {:#?}", takia.layout(child)?);

    println!("Compute layout with undefined (infinite) viewport:");
    takia.compute_layout(node, Size::MAX_CONTENT)?;
    println!("node: {:#?}", takia.layout(node)?);
    println!("child: {:#?}", takia.layout(child)?);

    Ok(())
}
