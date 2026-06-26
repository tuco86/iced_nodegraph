# Continue prompt — iced_nodegraph (branch `sdf-v3`)

Paste this into a fresh session to pick up where we left off.

---

We are on branch `sdf-v3`. Last pushed commit: `aa4ba53`
("feat(sdf): arc-only segments + biarc collapse fix"). The branch will be
**squash-merged** later, so messy intermediate commits are fine.

Read these memories first for full context:
`.claude/projects/.../memory/project_sdf_v3_step0.md`,
`project_zoomout_pan_greyout.md`, `feedback_no_slot_overflow_theory.md`.
(They live under the user's `~/.claude/projects/C--workspace-iced-nodegraph/memory/`.)

## What is DONE and pushed (all gates green: sdf 142 tests + 2 doctests, widget
suites, clippy -D warnings, wasm32 check, cargo build, fmt all clean)

- **Arc-only segment migration complete (CPU + GPU).** Every segment is now ONE
  arc primitive: `start/end + signed curvature (+heading)`; `curvature==0` = line,
  `start==end` = point. No more `segment_type/geom0/geom1`, no GPU cubic. Cubics
  are arc-splined on the CPU via `biarc::cubic_to_arcs` (wired into `curve.rs`
  `build_drawable` and `boolean.rs`). GPU `GpuSegment`/`eval_segment`/
  `seg_box_interval` reconstruct the arc from endpoints+curvature
  (`arc_from_endpoints`, mirrors `segment::arc_params`). `sd_bezier` + all bezier
  helpers deleted from `shader.wgsl`. CPU reference field is `segment::seg_sdf`.
- **Gray-block / edge-box blob bug FIXED** (the cull over-inclusion): the arc cull
  splits wide arcs into sub-chords so the concave interior stays empty. User
  confirmed visually: gray blocks gone, most edges crisp.
- **"Edge suddenly becomes a straight line on move/drag" FIXED**: root cause was
  `biarc::circle_through` returning a ~1e9-radius circle for a near-collinear
  S-curve; the deviation gate accepted it (f32 cancellation at huge radius) and
  `from_center_arc` collapsed it to a zero-length point. Fix: reject giant
  (>1e5) / non-finite radii in `circle_through` so the fitter splits. Gate:
  `drawable::curved_edge_never_collapses_to_a_line`.
- Per-tile slot cap is at **128** (SLOT_STRIDE 256). Do NOT raise it / do NOT blame
  slot overflow for glitches — see `feedback_no_slot_overflow_theory.md`.

NOT YET committed elsewhere: nothing pending. The migration has NOT been
visually signed off beyond the two items above; the 0.2.0 cutover is still the
user's call.

## OPEN BUG (next investigation): zoom-out pan greyout

In 500_nodes, panning while zoomed far out greys the nodes behind a
semi-transparent veil; pan/pixel-dependent, "grows toward the right". Findings so
far (see `project_zoomout_pan_greyout.md`):
- The stats PANEL does NOT dim -> it is node-graph content, not a window-level veil.
- User observation: **"nodes go grey, the background grid stays sharp."**
- Ruled out (headless): node fills + cull are robust to world X = 1e7 (tiled,
  multi-node); edges robust to 40k; bg-cache not stale during pan.
- Found a REAL but separate latent bug: the tiling background (`p % spacing`,
  raw world_p) washes out past world X ~1.7e7 (2^24 f32 limit). But 500_nodes
  coords are only ~100..3000, so this is NOT the seen symptom. Worth fixing
  anyway (fold camera mod spacing in f64).
- LEADING suspect for the actual veil: the **hosted node-content zoom/pan
  transform**. iced has no per-widget zoom; node children are drawn via
  `renderer.with_transformation(camera.layer_transformation())` (widget.rs).
  "Nodes grey, grid sharp" points here. Not yet isolated.

## NEXT TASK the user asked for (do this first at work)

Instead of one-off layer toggles, build a **per-layer debug MATRIX** in the
500_nodes demo's debug panel. Rows = SDF layers (background/tiling, shadows,
node fill, foreground, edges, pins). Columns (per layer, independently
toggleable):
1. **visible** (show/hide the layer)
2. **small-tile color** — fine-tile heatmap, green->red by slot usage
   (`DEBUG_TILE_HEATMAP`)
3. **big-tile color** — regional/workgroup-tile visualization
4. **iq-field** — distance-field view (`DEBUG_DISTANCE_FIELD`)
5. **hover-debug** — hovered-tile slot inspector (`DEBUG_HOVERED_TILE`)

The current demo has a flat "Tile Debug" checkbox group
(Edges/Shadows/Node Fill/Foreground/IQ Field/Hovered Tile) in `stats_panel`
(`demos/500_nodes/src/lib.rs`). `DebugFlags` live on `SdfPrimitive` in
`iced_nodegraph_sdf/src/primitive.rs`; the shader honours
`DEBUG_TILE_HEATMAP / DEBUG_DISTANCE_FIELD / DEBUG_HOVERED_TILE`. A per-layer
matrix needs per-layer debug-flag plumbing (each layer's primitive carries its
own flags) plus a "visible" gate. This matrix is also the tool to finally
localize the greyout.

## Working agreement reminders
- SDF is the highest-risk subsystem: minimal changes, validate, no over-
  engineering. Commit fmt-clean. Commit directly on `sdf-v3`. Do not launch the
  GUI (the user runs it). The user can see pixels; the agent cannot.
