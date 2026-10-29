#[cfg(test)]
mod position_variants {
    use takia::prelude::*;
    use takia::{NodeId, Point, TakiaTree};

    fn leaf_with_inset(position: Position, display: Display) -> (TakiaTree<()>, NodeId) {
        let mut takia: TakiaTree<()> = TakiaTree::new();
        let child = takia
            .new_leaf(Style {
                position,
                size: Size { width: length(10.0), height: length(10.0) },
                inset: Rect { left: length(5.0), right: auto(), top: length(7.0), bottom: auto() },
                ..Default::default()
            })
            .unwrap();
        let root = takia
            .new_with_children(
                Style { display, size: Size { width: length(100.0), height: length(100.0) }, ..Default::default() },
                &[child],
            )
            .unwrap();
        takia.compute_layout(root, Size::MAX_CONTENT).unwrap();
        (takia, child)
    }

    #[test]
    fn static_and_sticky_ignore_inset() {
        for display in [Display::Block, Display::Flex, Display::Grid] {
            for position in [Position::Static, Position::Sticky] {
                let (takia, child) = leaf_with_inset(position, display);
                let layout = takia.layout(child).unwrap();
                assert_eq!(layout.location, Point { x: 0.0, y: 0.0 }, "{position:?} in {display:?}");
            }
            let (takia, child) = leaf_with_inset(Position::Relative, display);
            assert_eq!(takia.layout(child).unwrap().location, Point { x: 5.0, y: 7.0 }, "Relative in {display:?}");
        }
    }

    #[test]
    fn sticky_is_positioned_but_in_flow() {
        assert!(Position::Sticky.is_positioned());
        assert!(!Position::Sticky.is_out_of_flow());
        assert!(!Position::Static.is_positioned());
    }
}
