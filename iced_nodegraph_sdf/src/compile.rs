//! Compilation: Drawable + Style -> GPU data.

use std::borrow::Cow;

use iced::Color;

use crate::drawable::Drawable;
use crate::pipeline::types::{GpuDrawEntry, GpuSegment, GpuStyle, GpuVec2, GpuVec4};
use crate::style::{MAX_STOPS, Style, Transfer};

const FLAG_CLOSED: u32 = 1; // entry.flags
const SEG_FLAG_SIGNED: u32 = 1; // segment.flags
const STYLE_FLAG_HAS_PATTERN: u32 = 1;
const STYLE_FLAG_DISTANCE_FIELD: u32 = 2;

/// Compile a drawable and style into GPU data, world-baked. Equivalent to
/// [`compile_drawable_at`] with origin `(0,0)`. The production path goes through
/// [`compile_local_at`] (a `Shape` evaluated to local geometry); this direct
/// drawable compile is retained for the pixel-test oracle harness.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compile_drawable(
    drawable: &Drawable,
    style: &Style,
    z_order: u32,
    segment_base: u32,
    out_segments: &mut Vec<GpuSegment>,
) -> (GpuDrawEntry, GpuStyle) {
    compile_drawable_at(
        drawable,
        style,
        z_order,
        [0.0, 0.0],
        segment_base,
        out_segments,
    )
}

/// Compile a drawable with its geometry stored in a LOCAL frame around `origin`
/// and `origin` carried as the per-segment translate (the v3 keystone). The
/// entry's `bounds` stay world-space (`= local bounds + origin`). With
/// `origin == (0,0)` this is byte-identical to v2's world-baked compile.
///
/// Because a translate preserves distance (`|grad| = 1`), the rendered result
/// is independent of `origin`: two identical shapes at different positions then
/// produce identical local geometry, differing only in the translate - the
/// property dedup relies on. Tilings are analytic (no segments), so `origin` is
/// ignored for them.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compile_drawable_at(
    drawable: &Drawable,
    style: &Style,
    z_order: u32,
    origin: [f32; 2],
    segment_base: u32,
    out_segments: &mut Vec<GpuSegment>,
) -> (GpuDrawEntry, GpuStyle) {
    let segment_start = segment_base + out_segments.len() as u32;

    // Geometry stored local; placement carried in the per-segment translate.
    let local = if origin == [0.0, 0.0] {
        Cow::Borrowed(drawable)
    } else {
        Cow::Owned(drawable.translated(-origin[0], -origin[1]))
    };

    for seg in &local.segments {
        out_segments.push(GpuSegment {
            flags: if seg.signed { SEG_FLAG_SIGNED } else { 0 },
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            endpoints: GpuVec4([seg.start.x, seg.start.y, seg.end.x, seg.end.y]),
            params: GpuVec4([seg.curvature, seg.heading, 0.0, 0.0]),
            arc_range: GpuVec4([seg.arc_start, seg.arc_end, drawable.total_arc_length, 0.0]),
        });
    }

    let mut flags = 0u32;
    if drawable.is_closed {
        flags |= FLAG_CLOSED;
    }

    let entry = GpuDrawEntry {
        entry_type: drawable.drawable_type as u32,
        style_idx: 0,
        z_order,
        flags,
        // World-space AABB: invariant to the local/translate split (the
        // original world bounds = local bounds + origin).
        bounds: GpuVec4(drawable.bounds),
        segment_start,
        segment_count: drawable.segments.len() as u32,
        tiling_type: drawable.tiling_type.map_or(0, |t| t as u32),
        _pad: 0,
        tiling_params: GpuVec4(drawable.tiling_params),
        // Per-instance placement: the segments above are local; the shader adds
        // this back. Identical shapes share segments, differing only here.
        translate: GpuVec2(origin),
        _translate_pad: GpuVec2([0.0, 0.0]),
    };

    let gpu_style = compile_style(style);

    (entry, gpu_style)
}

/// Compile an ALREADY-LOCAL drawable (e.g. a `ShapeCache` entry, evaluated once
/// at the local origin) placed at world `translate`. Geometry is stored verbatim
/// (no shift) and `translate` is carried per-segment; bounds become world-space
/// (`local + translate`). This is the dedup placement path: one cached local
/// shape rendered at N positions, differing only in the translate. Renders
/// pixel-identically to the world-baked `compile_drawable` (proven by the A1
/// translate-invariance gate plus the recipe `evaluate` tests).
pub(crate) fn compile_local_at(
    local: &Drawable,
    style: &Style,
    z_order: u32,
    translate: [f32; 2],
    segment_base: u32,
    out_segments: &mut Vec<GpuSegment>,
) -> (GpuDrawEntry, GpuStyle) {
    let segment_start = segment_base + out_segments.len() as u32;

    for seg in &local.segments {
        out_segments.push(GpuSegment {
            flags: if seg.signed { SEG_FLAG_SIGNED } else { 0 },
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            endpoints: GpuVec4([seg.start.x, seg.start.y, seg.end.x, seg.end.y]),
            params: GpuVec4([seg.curvature, seg.heading, 0.0, 0.0]),
            arc_range: GpuVec4([seg.arc_start, seg.arc_end, local.total_arc_length, 0.0]),
        });
    }

    entry_referencing(local, style, z_order, translate, segment_start)
}

/// Build a command for `local` placed at `translate` that REFERENCES an existing
/// segment range (`segment_start`), pushing NO segments. This is the GPU
/// instancing path: when an identical shape's segments are already in the buffer
/// this frame, every further instance is one tiny command pointing at the shared
/// range, so 500 identical nodes upload one shape's segments, not 500 copies.
pub(crate) fn entry_referencing(
    local: &Drawable,
    style: &Style,
    z_order: u32,
    translate: [f32; 2],
    segment_start: u32,
) -> (GpuDrawEntry, GpuStyle) {
    let mut flags = 0u32;
    if local.is_closed {
        flags |= FLAG_CLOSED;
    }

    let lb = local.bounds;
    let entry = GpuDrawEntry {
        entry_type: local.drawable_type as u32,
        style_idx: 0,
        z_order,
        flags,
        bounds: GpuVec4([
            lb[0] + translate[0],
            lb[1] + translate[1],
            lb[2] + translate[0],
            lb[3] + translate[1],
        ]),
        segment_start,
        segment_count: local.segments.len() as u32,
        tiling_type: local.tiling_type.map_or(0, |t| t as u32),
        _pad: 0,
        tiling_params: GpuVec4(local.tiling_params),
        translate: GpuVec2(translate),
        _translate_pad: GpuVec2([0.0, 0.0]),
    };

    (entry, compile_style(style))
}

fn c2v(c: Color) -> GpuVec4 {
    GpuVec4::new(c.r, c.g, c.b, c.a)
}

fn compile_style(style: &Style) -> GpuStyle {
    let mut flags = 0u32;
    if style.distance_field {
        flags |= STYLE_FLAG_DISTANCE_FIELD;
    }

    let (pattern_type, pattern_thickness, p0, p1, p2, flow_speed) = match &style.pattern {
        Some(p) => {
            flags |= STYLE_FLAG_HAS_PATTERN;
            p.as_gpu()
        }
        None => (0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    debug_assert!(
        style.stops.len() <= MAX_STOPS,
        "style has {} stops, max is {MAX_STOPS}",
        style.stops.len(),
    );
    let mut stop_start = [GpuVec4::ZERO; MAX_STOPS];
    let mut stop_end = [GpuVec4::ZERO; MAX_STOPS];
    let mut stop_dist = [GpuVec4::ZERO; MAX_STOPS / 4];
    for (i, s) in style.stops.iter().take(MAX_STOPS).enumerate() {
        stop_start[i] = c2v(s.start);
        stop_end[i] = c2v(s.end);
        stop_dist[i / 4].0[i % 4] = s.dist;
    }

    let (transfer_type, transfer_param) = match style.transfer {
        Transfer::Linear => (0u32, 0.0),
        Transfer::Smoothstep => (1, 0.0),
        Transfer::Gamma(g) => (2, g),
    };

    GpuStyle {
        stop_start,
        stop_end,
        stop_dist,
        stop_count: style.stops.len().min(MAX_STOPS) as u32,
        flags,
        pattern_type,
        pattern_thickness,
        pattern_param0: p0,
        pattern_param1: p1,
        pattern_param2: p2,
        flow_speed,
        transfer_type,
        transfer_param,
        _transfer_pad0: 0,
        _transfer_pad1: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::Curve;

    /// The dedup placement path (`compile_local_at` on a cached LOCAL drawable)
    /// produces byte-equivalent GPU data to localizing the world-baked drawable
    /// (`compile_drawable_at`), which the A1 gate already proved renders
    /// pixel-identically to v2. This closes the chain for live recipe entries.
    #[test]
    fn compile_local_at_matches_localized_world() {
        let local = Curve::rounded_rect([0.0, 0.0], [40.0, 25.0], 6.0);
        let t = [300.0, -120.0];
        let world = local.translated(t[0], t[1]);
        let style = Style::solid(iced::Color::WHITE);

        let mut segs_local = Vec::new();
        let (e_local, _) = compile_local_at(&local, &style, 0, t, 0, &mut segs_local);
        let mut segs_world = Vec::new();
        let (e_world, _) = compile_drawable_at(&world, &style, 0, t, 0, &mut segs_world);

        assert_eq!(segs_local.len(), segs_world.len());
        for (a, b) in segs_local.iter().zip(segs_world.iter()) {
            for i in 0..4 {
                assert!((a.endpoints.0[i] - b.endpoints.0[i]).abs() < 1e-3);
                assert!((a.params.0[i] - b.params.0[i]).abs() < 1e-3);
            }
        }
        // The per-instance translate now lives on the entry, equal for both.
        assert_eq!(e_local.translate.0, e_world.translate.0);
        for i in 0..4 {
            assert!(
                (e_local.bounds.0[i] - e_world.bounds.0[i]).abs() < 1e-3,
                "world bounds differ at {i}",
            );
        }
    }

    /// A command that REFERENCES a shared segment range (GPU instancing) is
    /// byte-identical to the full compile's command - so a second instance can
    /// skip the segment upload and still render the same.
    #[test]
    fn entry_referencing_matches_full_command() {
        let local = Curve::rounded_rect([0.0, 0.0], [40.0, 25.0], 6.0);
        let t = [100.0, 50.0];
        let style = Style::solid(iced::Color::WHITE);

        let mut segs = Vec::new();
        let (full, _) = compile_local_at(&local, &style, 3, t, 0, &mut segs);
        let (refd, _) = entry_referencing(&local, &style, 3, t, 0);

        assert_eq!(full.segment_start, refd.segment_start);
        assert_eq!(full.segment_count, refd.segment_count);
        assert_eq!(full.translate.0, refd.translate.0);
        assert_eq!(full.bounds.0, refd.bounds.0);
        assert_eq!(full.flags, refd.flags);
        assert_eq!(full.entry_type, refd.entry_type);
    }
}
