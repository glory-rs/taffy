use takia::prelude::*;

// Creates three 20px x 20px children, evenly spaced 10px apart from each other
// Thus the container is 80px x 20px.

fn main() -> Result<(), takia::TakiaError> {
    let mut takia: TakiaTree<()> = TakiaTree::new();

    let child_style = Style { size: Size { width: length(20.0), height: length(20.0) }, ..Default::default() };
    let child0 = takia.new_leaf(child_style.clone())?;
    let child1 = takia.new_leaf(child_style.clone())?;
    let child2 = takia.new_leaf(child_style.clone())?;

    let root = takia.new_with_children(
        Style { gap: Size { width: length(10.0), height: zero() }, ..Default::default() },
        &[child0, child1, child2],
    )?;

    // Compute layout and print result
    takia.compute_layout(root, Size::MAX_CONTENT)?;
    takia.print_tree(root);

    Ok(())
}
