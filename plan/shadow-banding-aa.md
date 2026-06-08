# Plan: shadow banding AA seams (#15)

Status: implementing. Priority: regression, do first.

## Goal

Remove the light lines that run across the node shadow, without losing the soft
floating edge.

## Root cause

The shadow is three coplanar `Style` entries tiling the distance axis without
overlap (`iced_nodegraph/src/style/sdf.rs`, `shadow_sdf_layers`): `[0,d]`,
`[-d,0]`, `[-1e6,-d]`. Each entry is a separate premultiplied-alpha layer, and
`render_style` antialiases both ends of every entry. At each shared internal
boundary (`0`, `-d`) two abutting entries each ramp through ~50% coverage; the
premultiplied composite of two partial layers of the same colour does not
reconstruct the intended continuous alpha, so the background bleeds through ->
one light line per internal boundary.

## Design

Bands abut seamlessly; a gap would just be a transparent band. So a band needs
no `from..to` range, only an end (`bis`), and the chain starts at distance 0.
With that, every boundary is a single smoothstep and there is nothing to double-
antialias.

For the shadow this collapses to **one band**: an outward glow that starts at
the silhouette (`dist = 0`) and fades to nothing at `dist = shadow_distance`.
A single entry has no internal boundary, so no seam is possible; its one
smoothstep edge is the soft glow. The old interior band (the source of the worst
line at `-d`) is dropped: the node body covers `dist < 0`, so the shadow is an
outside-only effect.

Separate translucent layers cannot represent a varying-alpha profile without
either a seam (tiling) or an overshoot (overlap); only a single entry can. The
shadow needs just one band, so no multi-stop machinery is introduced. If a
future effect ever needs a multi-stop distance ramp it must likewise be one
entry, not a stack of composited bands.

## Implementation

`iced_nodegraph/src/style/sdf.rs` - rewrite `shadow_sdf_layers` to emit a single
band:

```rust
vec![band(1.0, 0.0, 0.0, d)] // full at the silhouette -> 0 at d
```

Update the doc comment (no longer three tiled bands; one outward glow from 0).
No changes to `Style`, `compile.rs`, or the shader: the existing single-band
path already antialiases both edges correctly when there is only one layer.

Note: the visible shadow now starts at full `shadow_color.a` at the silhouette
instead of 0.5, so it reads slightly stronger; tune the default
`shadow_color.a` / `shadow_distance` for parity if needed during the visual
check.

## Tests

- Unit (`iced_nodegraph`): assert `shadow_sdf_layers` returns exactly one band
  with `dist_from == 0.0`, `dist_to == shadow_distance`, near alpha = full
  shadow alpha, far alpha = 0.
- Pixel (`iced_nodegraph_sdf/src/pipeline/pixel_tests.rs`): render a single
  `[0, d]` band over a closed shape, walk a radial scanline outward across the
  silhouette, assert the alpha is monotonically non-increasing (no local
  brightening) and spans full -> 0. Guards against anyone re-introducing an
  interior band.

## Verification checklist

- `cargo test -p iced_nodegraph` and `cargo test -p iced_nodegraph_sdf`
- `cargo check -p iced_nodegraph`
- `cargo check -p iced_nodegraph --target wasm32-unknown-unknown`
- `cargo clippy -p iced_nodegraph -- -D warnings`
- `cargo build -p iced_nodegraph`
- `cargo fmt --all` clean
- Visual: `demo_hello_world`, vary `shadow_distance` / `shadow_color.a` / zoom;
  confirm the lines are gone and the edge stays soft (run manually).

## Constraints

- Pre-release: breaking changes acceptable, but none are needed here.
- Commit `cargo fmt` clean, e.g. `fix(shadow): single outward band removes seams`.
