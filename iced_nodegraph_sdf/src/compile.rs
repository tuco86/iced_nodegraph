//! Compilation: Drawable + Style -> GPU data.

use iced::Color;

use crate::drawable::Drawable;
use crate::pipeline::types::{GpuDrawEntry, GpuSegment, GpuStyle, GpuVec2, GpuVec4};
use crate::style::{MAX_STOPS, Style, Transfer};

pub(crate) const FLAG_CLOSED: u32 = 1; // entry.flags
/// `entry.entry_type` of an infinite analytic tiling (`DrawableType::Tiling`).
pub(crate) const ENTRY_TILING: u32 = 2;
const SEG_FLAG_SIGNED: u32 = 1; // segment.flags
const STYLE_FLAG_HAS_PATTERN: u32 = 1;
/// The style-independent per-shape metadata an entry needs beyond the segment
/// range: everything [`entry_from_meta`] reads that would otherwise require the
/// evaluated [`Drawable`]. The pipeline stores this alongside a resident
/// segment range (plan/arena-residency.md), so an instance of an
/// already-resident shape builds its entry WITHOUT re-evaluating the shape -
/// for a non-cacheable stroke (an edge) that skips the whole biarc fit.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EntryMeta {
    pub entry_type: u32,
    /// Entry flags (bit 0 = closed contour).
    pub flags: u32,
    pub segment_count: u32,
    pub tiling_type: u32,
    pub tiling_params: [f32; 4],
}

/// Extracts the [`EntryMeta`] of an evaluated local drawable.
pub(crate) fn entry_meta(local: &Drawable) -> EntryMeta {
    EntryMeta {
        entry_type: local.drawable_type as u32,
        flags: if local.is_closed { FLAG_CLOSED } else { 0 },
        segment_count: local.segments.len() as u32,
        tiling_type: local.tiling_type.map_or(0, |t| t as u32),
        tiling_params: local.tiling_params,
    }
}

/// Build a command from shape metadata placed at `translate`, referencing the
/// segment range at `segment_start`. The metadata path of [`entry_referencing`]:
/// callers holding a resident [`EntryMeta`] need no evaluated drawable at all.
pub(crate) fn entry_from_meta(
    meta: &EntryMeta,
    style: &Style,
    z_order: u32,
    translate: [f32; 2],
    segment_start: u32,
) -> (GpuDrawEntry, GpuStyle) {
    let entry = GpuDrawEntry {
        entry_type: meta.entry_type,
        style_idx: 0,
        z_order,
        flags: meta.flags,
        segment_start,
        segment_count: meta.segment_count,
        tiling_type: meta.tiling_type,
        _pad: 0,
        tiling_params: GpuVec4(meta.tiling_params),
        translate: GpuVec2(translate),
        _translate_pad: GpuVec2([0.0, 0.0]),
    };
    (entry, compile_style(style))
}

/// Compile an ALREADY-LOCAL drawable (e.g. a `ShapeCache` entry, evaluated once
/// at the local origin) placed at world `translate`. Geometry is stored verbatim
/// (no shift) and `translate` is carried per-segment; bounds become world-space
/// (`local + translate`). This is the dedup placement path: one cached local
/// shape rendered at N positions, differing only in the translate.
///
/// Because a translate preserves distance (`|grad| = 1`), the rendered result is
/// independent of `translate`: two identical shapes at different positions share
/// identical local geometry, differing only in the per-segment translate - the
/// property dedup relies on.
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
    entry_from_meta(&entry_meta(local), style, z_order, translate, segment_start)
}

fn c2v(c: Color) -> GpuVec4 {
    GpuVec4::new(c.r, c.g, c.b, c.a)
}

fn compile_style(style: &Style) -> GpuStyle {
    let mut flags = 0u32;

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
    use crate::pattern::Pattern;
    use crate::shape::{PathSeg, Shape};
    use std::f32::consts::FRAC_PI_2;

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
        assert_eq!(full.flags, refd.flags);
        assert_eq!(full.entry_type, refd.entry_type);
    }

    /// `Shape::path` needs no dedicated wiring here: this module is generic
    /// over an already-evaluated `Drawable`, so the routing-anchor cable
    /// (Line then Arc) compiles through the same `compile_local_at` as any
    /// other non-cacheable open stroke (an edge) - one command, one segment
    /// range, unclosed.
    #[test]
    fn path_compiles_as_single_open_stroke_entry() {
        let local = Shape::path(
            [0.0, 0.0],
            [
                PathSeg::Line { to: [40.0, 0.0] },
                PathSeg::Arc {
                    center: [40.0, 10.0],
                    radius: 10.0,
                    sweep: FRAC_PI_2,
                },
            ],
        )
        .evaluate();
        let style = Style::stroke(iced::Color::WHITE, Pattern::solid(2.0));

        let mut segs = Vec::new();
        let (entry, _) = compile_local_at(&local, &style, 0, [0.0, 0.0], 0, &mut segs);

        assert_eq!(
            entry.segment_count, 2,
            "one drawable, one command, one GPU segment per PathSeg"
        );
        assert_eq!(segs.len(), 2);
        assert_eq!(entry.flags, 0, "an open path is never a closed contour");
    }
}
