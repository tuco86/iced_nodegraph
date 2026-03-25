//! Compilation: Drawable + Style -> GPU data.

use iced::Color;

use crate::drawable::Drawable;
use crate::pipeline::types::{GpuDrawEntry, GpuSegment, GpuStyle, GpuVec4};
use crate::style::{Fill, Style};

const FLAG_CLOSED: u32 = 1; // entry.flags
// style.flags:
const FLAG_GRADIENT: u32 = 1;
const FLAG_ARC_GRADIENT: u32 = 2;
const FLAG_HAS_PATTERN: u32 = 4;
const FLAG_DISTANCE_FIELD: u32 = 8;
const STYLE_FLAG_CLOSED: u32 = 16; // propagated from drawable.is_closed

/// Compile a drawable and style into GPU data.
///
/// Pushes segments into `out_segments` and returns (draw_entry, style).
pub(crate) fn compile_drawable(
    drawable: &Drawable,
    style: &Style,
    z_order: u32,
    segment_base: u32,
    out_segments: &mut Vec<GpuSegment>,
) -> (GpuDrawEntry, GpuStyle) {
    let segment_start = segment_base + out_segments.len() as u32;

    // Emit segments
    for seg in &drawable.segments {
        out_segments.push(GpuSegment {
            segment_type: seg.segment_type as u32,
            _pad0: 0, _pad1: 0, _pad2: 0,
            geom0: GpuVec4(seg.geom0),
            geom1: GpuVec4(seg.geom1),
            arc_range: GpuVec4([
                seg.arc_start,
                seg.arc_end,
                drawable.total_arc_length,
                0.0,
            ]),
        });
    }

    let segment_count = drawable.segments.len() as u32;

    let mut flags = 0u32;
    if drawable.is_closed { flags |= FLAG_CLOSED; }

    let entry = GpuDrawEntry {
        entry_type: drawable.drawable_type as u32,
        style_idx: 0, // Set by caller
        z_order,
        flags,
        bounds: GpuVec4(drawable.bounds),
        segment_start,
        segment_count,
        tiling_type: drawable.tiling_type.map_or(0, |t| t as u32),
        _pad: 0,
        tiling_params: GpuVec4(drawable.tiling_params),
    };

    let mut gpu_style = compile_style(style);
    if drawable.is_closed {
        gpu_style.flags |= STYLE_FLAG_CLOSED;
    }

    (entry, gpu_style)
}

fn color_to_vec4(c: Color) -> GpuVec4 {
    // Pass sRGB values directly (same as legacy), shader operates in sRGB space
    GpuVec4::new(c.r, c.g, c.b, c.a)
}

fn compile_style(style: &Style) -> GpuStyle {
    let mut flags = 0u32;

    let (color, gradient_color, gradient_angle) = match style.fill {
        Fill::Solid(c) => (color_to_vec4(c), GpuVec4::ZERO, 0.0),
        Fill::Gradient { start, end, angle } => {
            flags |= FLAG_GRADIENT;
            (color_to_vec4(start), color_to_vec4(end), angle)
        }
        Fill::ArcLengthGradient { start, end } => {
            flags |= FLAG_GRADIENT | FLAG_ARC_GRADIENT;
            (color_to_vec4(start), color_to_vec4(end), 0.0)
        }
        Fill::DistanceField => {
            flags |= FLAG_DISTANCE_FIELD;
            // IQ default: orange outside, blue inside
            (color_to_vec4(Color::from_rgb(0.9, 0.6, 0.3)),
             color_to_vec4(Color::from_rgb(0.65, 0.85, 1.0)),
             0.0)
        }
    };

    let (pattern_type, pattern_thickness, p0, p1, p2, flow_speed) = match &style.pattern {
        Some(p) => {
            flags |= FLAG_HAS_PATTERN;
            p.as_gpu()
        }
        None => (0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    let (outline_thickness, outline_color) = match &style.outline {
        Some(o) => (o.thickness, color_to_vec4(o.color)),
        None => (0.0, GpuVec4::ZERO),
    };

    GpuStyle {
        color,
        gradient_color,
        gradient_angle,
        flags,
        expand: style.expand,
        blur: style.blur,
        pattern_type,
        pattern_thickness,
        pattern_param0: p0,
        pattern_param1: p1,
        pattern_param2: p2,
        flow_speed,
        outline_thickness,
        _pad0: 0.0,
        outline_color,
    }
}
