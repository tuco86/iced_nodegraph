# Continue prompt — iced_nodegraph (branch `sdf-v3`)

Paste this into a fresh session to pick up where we left off.

---

We are on branch `sdf-v3`. Last pushed commit: `04f5b4c`. The branch will be
**squash-merged** later, so messy intermediate commits are fine. Commit directly
on `sdf-v3`, fmt-clean, NO co-author trailer. Do NOT launch the GUI — the user
runs it and reports pixels (the agent cannot see pixels).

Read these memories first:
`sdf_intra_primitive_zorder` (NEW, critical for the render path),
`sdf_cull_exact_seg_box`, `arc_is_all_you_need`, `feedback_tests_first`,
`ci_fmt_gate`, `no_coauthor_trailer`.

## What is DONE and pushed (all gates green: widget 133 tests, sdf 142 +2
doctests modulo one PRE-EXISTING flaky test, clippy -D warnings, wasm32 check,
build, fmt)

- **Below-nodes layers consolidated into ONE batched SDF draw** (commit
  `8e63d4c`). Previously the grid (cached via bg_cache), node shadows, and edges
  were THREE separate draws. Now they are one `bg_layer` `SdfPrimitive` in
  `widget.rs` (the graph-background), drawn over full `layout.bounds()`. Node
  fill / foreground / iced-content layers are UNCHANGED; only the node shadow
  moved from its own batch into the graph-background. The grid is no longer
  `.background()`-cached (bg_cache is now inert; folding the grid in was the
  user's explicit call). `GraphInfo.timings` collapsed `shadows`+`edges` into one
  `background` op (Vec content change, not an API break).
- **CRITICAL gotcha learned + memory `sdf_intra_primitive_zorder`**: within ONE
  `SdfPrimitive`, the FIRST-pushed entry composites in FRONT (the cull sorts a
  tile's slots ascending by push index, the fragment blends front-to-back so
  slot 0 = front). Only ACROSS separate draw calls is it painter's order. So the
  bg_layer pushes FRONT-TO-BACK: edge strokes (z2) -> edge+node shadows (z1) ->
  grid (z0, last). Getting this inverted made the grid draw OVER the edges (the
  user caught it); now fixed. The existing foreground batch already follows this
  convention (border before pins; `border_sdf_layers` is "front-to-back").
- **Demo instrumented** (commit `04f5b4c`): `demos/500_nodes` now shows
  `cam: (x, y)  zoom: z` in the stats panel via `.on_pan(...)` (uncontrolled
  camera, report-only, no behavior change). Use it to read off repro coords.

## REVERTED / dead ends

- Tried making `Shape::Bezier` cacheable (so pan/zoom would stop re-arc-splining
  edges). It had ZERO effect on `sdf_prepare` time, so reverted. Lesson: the
  ~43ms `sdf_prepare` at 500 nodes is NOT dominated by bezier eval — it is the
  per-frame SEGMENT COMPILATION / buffer build of ~2500 entries (every edge is
  compiled to GpuSegments each frame regardless of eval caching). This is a
  SEPARATE perf item, not the flicker, and not yet addressed.

## OPEN BUG (the user's actual target): pan/zoom node "float collapse"

In 500_nodes, panning/zooming makes the NODES (fill/border) render wrong; the
grid and edges stay sharp. Deterministic per camera position: stop at the spot
and the display error PERSISTS (it "flickers" because you pass through many such
spots while moving). Confirmed facts from the user:

- **Only nodes collapse** (the per-node CLIPPED SDF path: each node's fill and
  foreground are drawn in their own `clipped_shape_bounds` rect with a
  clip-specific `layer_camera` offset). Grid/edges use the precise full-bounds
  path and are fine.
- **Trigger = zooming OUT; zooming IN fixes it.** Repro captured at
  **cam (-327.7, -132.0), zoom 0.24131** in 500_nodes (read off the new cam line).
- **The user says: "the CLIPS are the wrong SIZE — even the debug heatmap grid
  renders wrong there."** So the per-node clip rect (or the tile grid derived
  from it in `prepare`) is wrong when zoomed out.
- Coords are MODEST (zoom 0.24, cam ~-300), so this is NOT large-magnitude f32
  cancellation. It is a degeneracy/logic bug at normal coordinates.
- IQ Field debug "doesn't help at that zoom level."

### Analysis already done (so don't redo it)

- `world_bbox_to_screen_bounds` (widget.rs ~125) and `camera.layer_transformation`
  (camera.rs ~298) are PROVABLY consistent: both reduce to
  `screen = (world + position)*zoom + viewport_origin*(1 - zoom)`. Clip width =
  `world_width*zoom + const_padding`. The clip FORMULA is therefore correct in
  exact arithmetic — the bug is almost certainly NOT in those two functions.
- `layer_camera` (widget.rs ~92): `cx = camera_position.x + (widget_origin.x*(1
  - zoom) - clip.x)/zoom`. The `clip.x` cancels analytically against the shader's
  `bounds_origin` term; at these modest coords the residual is sub-pixel. Not it.

### Strongest leads for next session (do these)

1. **Empirically dump the actual clip dims.** Add temporary `eprintln!` (or a
   debug overlay) in `widget.rs` printing, per visible node at the repro camera,
   the `fill_clip`/`fg_clip` `Rectangle` (x,y,w,h) and the `grid_cols`/`grid_rows`
   the SDF `prepare` derives (`bounds.width*scale/TILE_SIZE`). Compare against the
   expected node screen size. The user already SEES wrong-size clips, so the
   numbers will show which clip is wrong and by how much — that pins the formula.
2. **Sub-tile-clip rounding.** At zoom 0.24 a node is ~12px; `grid_cols =
   ceil(w*scale/16).max(1)` rounds a sub-2-tile clip up, so the tile grid is
   larger than the clip and the heatmap overshoots. Check whether this rounding
   (or a clip narrower than one physical tile) degenerates the cull/heatmap.
3. **Shared tile-buffer region indexing.** At zoom-out there are ~600 per-node
   primitives (fill+foreground for ~300 in-view nodes) all writing one shared
   `tile_counts`/`tile_entries` buffer via per-primitive `tile_base`. Re-check the
   `tile_base` accumulation and the mid-frame buffer-growth copy in
   `primitive.rs` `prepare` (~747-784) for an off-by/region-overlap that only
   manifests with many primitives (i.e. zoomed out).
4. Reproduce headless (user's tests-first rule): the `pixel_tests.rs` harness +
   `SdfPipeline` path (e.g. `render_prims` at ~1267) can render several clipped
   primitives through ONE pipeline; build a scene of small per-node clips at
   zoom 0.24 and assert the fill fills the node rect, not more/less.

## Pre-push checklist (all must pass)
`cargo test -p iced_nodegraph`; `cargo test -p iced_nodegraph_sdf` (the
`overflowing_tile_keeps_nearest_not_first` pixel test is PRE-EXISTING flaky ~15%
in the parallel run — passes in isolation, unrelated to changes);
`cargo check -p iced_nodegraph` (native + `--target wasm32-unknown-unknown`);
`cargo clippy -p iced_nodegraph -p iced_nodegraph_sdf -- -D warnings`;
`cargo build`; `cargo fmt --all`. The `block v0.1.6` future-incompat note is a
pre-existing transitive-dep warning, ignore it.

## Working agreement
SDF is the highest-risk subsystem: minimal changes, validate, no over-
engineering. Reproduce bugs as automated tests first. Commit fmt-clean on
`sdf-v3`, no co-author trailer. The user runs the GUI and reports pixels.
