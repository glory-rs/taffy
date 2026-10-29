use takia::prelude::*;

fn main() -> Result<(), takia::TakiaError> {
    let mut takia: TakiaTree<()> = TakiaTree::new();

    // left
    let child_t1 = takia.new_leaf(Style {
        size: Size { width: Dimension::from_length(5.0), height: Dimension::from_length(5.0) },
        ..Default::default()
    })?;

    let div1 = takia.new_with_children(
        Style {
            size: Size { width: Dimension::from_percent(0.5), height: Dimension::from_percent(1.0) },
            // justify_content: JustifyContent::Center,
            ..Default::default()
        },
        &[child_t1],
    )?;

    // right
    let child_t2 = takia.new_leaf(Style {
        size: Size { width: Dimension::from_length(5.0), height: Dimension::from_length(5.0) },
        ..Default::default()
    })?;

    let div2 = takia.new_with_children(
        Style {
            size: Size { width: Dimension::from_percent(0.5), height: Dimension::from_percent(1.0) },
            // justify_content: JustifyContent::Center,
            ..Default::default()
        },
        &[child_t2],
    )?;

    let container = takia.new_with_children(
        Style {
            size: Size { width: Dimension::from_percent(1.0), height: Dimension::from_percent(1.0) },
            ..Default::default()
        },
        &[div1, div2],
    )?;

    takia.compute_layout(
        container,
        Size { height: AvailableSpace::Definite(100.0), width: AvailableSpace::Definite(100.0) },
    )?;

    println!("node: {:#?}", takia.layout(container)?);

    println!("div1: {:#?}", takia.layout(div1)?);
    println!("div2: {:#?}", takia.layout(div2)?);

    println!("child1: {:#?}", takia.layout(child_t1)?);
    println!("child2: {:#?}", takia.layout(child_t2)?);

    Ok(())
}
