# Plan: SDF style as a distance-stop chain (architecture)

Status: implementing. Supersedes the shadow-only patch (#15) - that fixed the
symptom; this fixes the model that produced it.

## The architecture problem

A continuous cross-section profile (e.g. a node body: fill inside, border ring,
outline, then nothing) is today built as several separate `Style` entries that
each cover one band `[dist_from, dist_to]` and are composited back-to-front with
premultiplied alpha. `render_style` antialiases both ends of every band, so at a
boundary shared by two abutting bands each contributes ~50% coverage and the
premultiplied composite of two partial layers does not reconstruct the intended
single alpha -> a seam. The shadow showed it first; it is latent anywhere bands
of one shape stack (fill|border, border|outline, shadow bands).

## The model

A `Style`'s cross-section is ONE chain of distance stops, evaluated in a single
fragment pass - never composited from separate entries. No premultiply between
bands, so no seam is possible by construction.

```rust
pub struct Stop { pub dist: f32, pub start: Color, pub end: Color } // start/end = arc 0..1
pub struct Style { pub stops: Vec<Stop>, pub pattern: Option<Pattern>, pub distance_field: bool }
```

Stops are ascending by `dist`. Evaluation at signed distance `d` (negative
inside the shape):

- `d <= stops[0].dist`: hold `stops[0]` colour (clamped).
- between consecutive stops: `smoothstep`-blend their colours. The transition
  window is `[d_i, d_{i+1}]`, widened to a minimum of `aa` (one pixel) centred on
  its midpoint so a zero-width step (`d_i == d_{i+1}`) is a crisp antialiased
  edge while a wide band is a soft gradient. This is the single AA mechanism.
- `d >= stops[last].dist`: hold `stops[last]` colour (clamped).

A region disappears by ending the chain at a transparent stop; a gap is a
transparent stop between two opaque ones. There is no separate per-band clip:
"start at 0, only a `bis`, every transition smoothstep, gaps as transparent
bands" falls out of this.

Idioms:
- solid fill: `[(0, c), (0, transparent)]` - opaque inside, AA silhouette at 0.
- node shadow: `[(0, c), (d, transparent)]` - full inside the (offset)
  silhouette (held below stop 0), fading to 0 at `d`.
- outward-only glow (`Style::shadow` primitive): `[(0, transparent), (0, c),
  (d, transparent)]` - nothing inside, appears at the silhouette, fades to 0.
- clipped band `[from,to]` (border ring): `[(from, transp), (from, c), (to, c),
  (to, transp)]`.

Patterned strokes keep the existing pattern path (abs-distance + one smoothstep);
they use `stops[0]` for the arc-gradient colour. A pattern entry cannot merge
with band stops, so it stays its own entry.

## Merge strategy (where the seam actually dies)

Same-geometry solid bands MUST share one chain, or separate entries still
composite and seam:

- Node body (one silhouette): fill + border ring + border outline -> one chain.
- Pin: fill + border -> one chain.
- Node shadow (offset silhouette): its own chain (outward glow).
- Edge: patterned stroke stays its own entry; the solid border ring + outline +
  background merge into one band-chain; shadow its own. Stroke|border boundary
  remains two entries (different distances, thin patterned stroke) - acceptable,
  note as follow-up if it ever shows.

`MAX_STOPS = 8` on the GPU side covers the merged node body; assert on overflow.

## GPU + shader

- `GpuStyle` (pipeline/types.rs + WGSL): replace `near_*/far_*` + `dist_from/to`
  with `stop_start: [vec4;8]`, `stop_end: [vec4;8]`, `stop_dist: [vec4;2]`
  (4 dists packed per vec4), `stop_count: u32`. Keep pattern + flags fields.
- `compile_style` (compile.rs): fill the arrays from `style.stops`, pad unused,
  set `stop_count`; clamp/assert `<= MAX_STOPS`.
- `render_style` (shader.wgsl): distance-field and pattern branches unchanged
  except they read `stop[0]`; the band branch becomes the clamped-hold +
  min-`aa` smoothstep chain loop above, output premultiplied once.
- `extent`: `max stop dist` (closed) / `max(max_dist, -min_dist)` (open).
  `is_fill`: no pattern and `stops[0]` alpha > 0.

## Call sites to port (full blast radius already inventoried)

- `iced_nodegraph_sdf/src/style.rs`: all constructors (`solid`, `stroke`,
  `arc_gradient`, `arc_gradient_stroke`, `shadow`, `blur`, `distance_field`,
  `with_pattern`, `dist_range`, `expand`, `uniform`) rebuilt on stops; keep
  signatures so callers compile. `is_fill`/`extent` updated. Unit tests updated.
- `iced_nodegraph/src/style/sdf.rs`: `quad_style`/`quad_stroke` build stop
  chains; `shadow_sdf_layers` -> single outward-glow chain; merge node
  `fill_sdf_style` + `border_sdf_layers` into one chained style; pin likewise;
  edge `sdf_layers` merges its solid bands.
- `iced_nodegraph_sdf/src/pipeline/pixel_tests.rs`: 5 shadow/style literals ->
  stop chains. `examples/basic` `build_style` literal.
- Demos read `ColorQuad` (upper-crate type, unrelated) - not affected.

## Verification

Pixel tests are the oracle: porting the constructors must keep every existing
pixel test green (behaviour-preserving), then the seam test stays green and a new
merged-body test asserts continuous alpha across the fill|border boundary.
Full pre-push checklist (test, check native + wasm, clippy -D warnings, build,
fmt) for both crates. Visual pass in demo_hello_world (manual).

## Constraints

Pre-release: breaking the `Style` field API is fine. Keep constructor method
signatures stable to bound the blast radius. fmt-clean commits.
