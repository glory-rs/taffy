# Takia

A CSS block, flexbox and grid layout engine. This maintained fork retains the fractional flex/gap corrections used by Glory.

Takia is maintained by Glory and is based on [Taffy](https://github.com/DioxusLabs/taffy).
The first renamed release preserves the fork's version `0.15.0` and existing maintenance patches.
Original authors, licenses and copyright notices are retained. Upstream projects using the
original library are not represented as users of this renamed fork.

[![CI](https://github.com/glory-rs/takia/actions/workflows/ci.yml/badge.svg)](https://github.com/glory-rs/takia/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/takia.svg)](https://crates.io/crates/takia)
[![docs.rs](https://img.shields.io/docsrs/takia)](https://docs.rs/takia)

## Dependency

```toml
[dependencies]
takia = "=0.15.0"
```

The layout tree type is `TakiaTree`; the built-in tree feature is `takia_tree`. Other algorithm features retain their functionality.

## Branches and releases

- `main` contains this project's maintained implementation, namespace and release tooling.
- `upstream-main` mirrors the upstream default branch exactly. Its scheduled/manual sync never updates `main`.
- Upstream changes are reviewed before integration into maintained `main`.
- Release tags use `takia-v0.15.0`, preserving inherited upstream tags.

See [UPSTREAM.md](UPSTREAM.md) for provenance and [RELEASING.md](RELEASING.md) for
the explicit dependency order, checks and publication procedure. Supported platform
checks are compilation/test evidence and do not imply unperformed device or OS IME acceptance.

## Example

```rust
use takia::prelude::*;

// First create an instance of TakiaTree
let mut tree : TakiaTree<()> = TakiaTree::new();

// Create a tree of nodes using `TakiaTree.new_leaf` and `TakiaTree.new_with_children`.
// These functions both return a node id which can be used to refer to that node
// The Style struct is used to specify styling information
let header_node = tree
    .new_leaf(
        Style {
            size: Size { width: length(800.0), height: length(100.0) },
            ..Default::default()
        },
    ).unwrap();

let body_node = tree
    .new_leaf(
        Style {
            size: Size { width: length(800.0), height: auto() },
            flex_grow: 1.0,
            ..Default::default()
        },
    ).unwrap();

let root_node = tree
    .new_with_children(
        Style {
            flex_direction: FlexDirection::Column,
            size: Size { width: length(800.0), height: length(600.0) },
            ..Default::default()
        },
        &[header_node, body_node],
    )
    .unwrap();

// Call compute_layout on the root of your tree to run the layout algorithm
tree.compute_layout(root_node, Size::MAX_CONTENT).unwrap();

// Inspect the computed layout using `TakiaTree.layout`
assert_eq!(tree.layout(root_node).unwrap().size.width, 800.0);
assert_eq!(tree.layout(root_node).unwrap().size.height, 600.0);
assert_eq!(tree.layout(header_node).unwrap().size.width, 800.0);
assert_eq!(tree.layout(header_node).unwrap().size.height, 100.0);
assert_eq!(tree.layout(body_node).unwrap().size.width, 800.0);
assert_eq!(tree.layout(body_node).unwrap().size.height, 500.0); // This value was not set explicitly, but was computed by Takia

```
