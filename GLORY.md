# Glory maintenance branch

Upstream: DioxusLabs/takia, commit `eb6450694f33e2b7413e40677f230ff63f0fc194`
(Takia 0.15.0). Package names and the upstream repository layout are retained.

The flexbox patch avoids subtracting fixed gaps twice when the unfrozen grow or
shrink sum is below one. Direct regressions cover both axes and fractional
grow/shrink factors. The patch is safe Rust; the upstream license is unchanged.

Run `cargo test --locked -p takia --lib`. Cargo.lock pins the maintenance test
environment; consumers resolve their own lockfile. Build outputs must live
outside the checkout (Glory uses D:/BuildCache/glory-semantic-ui).
