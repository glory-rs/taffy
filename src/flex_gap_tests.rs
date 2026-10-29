//! Local regression for CSS Flexbox 9.7 fractional factors and fixed gaps.
use crate::prelude::*;

fn verify(grow: bool) {
    let sets: &[&[f32]] = &[
        &[0.0, 0.0],
        &[0.25, 0.25],
        &[0.125, 0.25],
        &[0.0, 0.25],
        &[0.1, 0.8],
        &[0.5, 0.5],
        &[1.0, 2.0],
        &[0.1, 0.1, 0.1],
    ];
    for row in [true, false] {
        for gap in [0.0, 10.0, 25.0] {
            for factors in sets {
                let mut tree: TakiaTree<()> = TakiaTree::new();
                tree.disable_rounding();
                let main = if row { 300.0 } else { 200.0 };
                let base = if grow {
                    if row {
                        50.0
                    } else {
                        30.0
                    }
                } else if row {
                    250.0
                } else {
                    180.0
                };
                // The final gap case also exercises outer flex bases/margins.
                let margin = if gap == 25.0 { 5.0 } else { 0.0 };
                let mut children = Vec::new();
                for &factor in *factors {
                    children.push(
                        tree.new_leaf(Style {
                            size: Size {
                                width: length(if row { base } else { 20.0 }),
                                height: length(if row { 20.0 } else { base }),
                            },
                            min_size: Size { width: length(0.0), height: length(0.0) },
                            margin: Rect {
                                left: length(margin),
                                right: length(margin),
                                top: length(margin),
                                bottom: length(margin),
                            },
                            flex_grow: if grow { factor } else { 0.0 },
                            flex_shrink: if grow { 1.0 } else { factor },
                            ..Style::default()
                        })
                        .unwrap(),
                    );
                }
                let root = tree
                    .new_with_children(
                        Style {
                            flex_direction: if row { FlexDirection::Row } else { FlexDirection::Column },
                            size: Size { width: length(300.0), height: length(200.0) },
                            gap: Size { width: length(gap), height: length(gap) },
                            ..Style::default()
                        },
                        &children,
                    )
                    .unwrap();
                tree.compute_layout(root, Size::MAX_CONTENT).unwrap();
                let sum: f32 = factors.iter().sum();
                let free = main - factors.len() as f32 * (base + 2.0 * margin) - (factors.len() - 1) as f32 * gap;
                for (&id, &factor) in children.iter().zip(*factors) {
                    let delta = if sum == 0.0 { 0.0 } else { free * sum.min(1.0) * factor / sum };
                    let layout = tree.layout(id).unwrap();
                    let actual = if row { layout.size.width } else { layout.size.height };
                    assert!(
                        (actual - base - delta).abs() < 0.02,
                        "grow={grow} row={row} gap={gap} factors={factors:?}: actual={actual}, expected={}",
                        base + delta
                    );
                }
            }
        }
    }
}
#[test]
fn fractional_grow_uses_gap_adjusted_free_space_once() {
    verify(true);
}
#[test]
fn fractional_shrink_uses_gap_adjusted_free_space_once() {
    verify(false);
}
