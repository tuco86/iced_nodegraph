//! Compilation: Drawable + Style -> GPU data.

use iced::Color;

use crate::drawable::Drawable;
use crate::pipeline::types::{GpuDrawEntry, GpuSegment, GpuStyle, GpuVec4};
use crate::style::Style;

const FLAG_CLOSED: u32 = 1; // entry.flags
const STYLE_FLAG_HAS_PATTERN: u32 = 1;
const STYLE_FLAG_DISTANCE_FIELD: u32 = 2;
const STYLE_FLAG_CLOSED: u32 = 4;

/// Compile a drawable and style into GPU data.
pub(crate) fn compile_drawable(
    drawable: &Drawable,
    style: &Style,
    z_order: u32,
    segment_base: u32,
    out_segments: &mut Vec<GpuSegment>,
) -> (GpuDrawEntry, GpuStyle) {
    let segment_start = segment_base + out_segments.len() as u32;

    for seg in &drawable.segments {
        out_segments.push(GpuSegment {
            segment_type: seg.segment_type as u32,
            _pad0: 0, _pad1: 0, _pad2: 0,
            geom0: GpuVec4(seg.geom0),
            geom1: GpuVec4(seg.geom1),
            arc_range: GpuVec4([seg.arc_start, seg.arc_end, drawable.total_arc_length, 0.0]),
        });
    }

    let mut flags = 0u32;
    if drawable.is_closed { flags |= FLAG_CLOSED; }

    let entry = GpuDrawEntry {
        entry_type: drawable.drawable_type as u32,
        style_idx: 0,
        z_order,
        flags,
        bounds: GpuVec4(drawable.bounds),
        segment_start,
        segment_count: drawable.segments.len() as u32,
        tiling_type: drawable.tiling_type.map_or(0, |t| t as u32),
        _pad: 0,
        tiling_params: GpuVec4(drawable.tiling_params),
    };

    let mut gpu_style = compile_style(style);
    if drawable.is_closed { gpu_style.flags |= STYLE_FLAG_CLOSED; }

    (entry, gpu_style)
}

fn c2v(c: Color) -> GpuVec4 { GpuVec4::new(c.r, c.g, c.b, c.a) }

fn compile_style(style: &Style) -> GpuStyle {
    let mut flags = 0u32;
    if style.distance_field { flags |= STYLE_FLAG_DISTANCE_FIELD; }

    let (pattern_type, pattern_thickness, p0, p1, p2, flow_speed) = match &style.pattern {
        Some(p) => { flags |= STYLE_FLAG_HAS_PATTERN; p.as_gpu() }
        None => (0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    GpuStyle {
        near_start: c2v(style.near_start),
        near_end: c2v(style.near_end),
        far_start: c2v(style.far_start),
        far_end: c2v(style.far_end),
        dist_from: style.dist_from,
        dist_to: style.dist_to,
        flags,
        pattern_type,
        pattern_thickness,
        pattern_param0: p0,
        pattern_param1: p1,
        pattern_param2: p2,
        flow_speed,
        _pad0: 0.0, _pad1: 0.0, _pad2: 0.0,
    }
}
