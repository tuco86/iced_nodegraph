use std::num::NonZeroU64;

use encase::ShaderSize;
use iced::{
    Rectangle,
    wgpu::{
        BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
        BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
        BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, Device, FragmentState,
        FrontFace, MultisampleState, PipelineCompilationOptions, PipelineLayout,
        PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, Queue,
        RenderPipeline, RenderPipelineDescriptor, ShaderModule, ShaderModuleDescriptor,
        ShaderSource, ShaderStages, TextureFormat, VertexState,
    },
};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::Pipeline as PipelineTrait;

use crate::node_graph::{effects::Node, euclid::WorldPoint, state::Dragging};
use crate::style::EdgeCurve;

use super::{EdgeData, Layer};

pub(crate) mod buffer;
pub(crate) mod types;

// ============================================================================
// Arc-Length Computation (CPU-side for accurate pattern spacing)
// ============================================================================

/// Computes the arc length of a cubic Bezier curve using adaptive subdivision.
/// Uses Gauss-Legendre quadrature for accurate integration.
fn bezier_arc_length(p0: glam::Vec2, p1: glam::Vec2, p2: glam::Vec2, p3: glam::Vec2) -> f32 {
    // 5-point Gauss-Legendre quadrature
    const WEIGHTS: [f32; 5] = [
        0.236_926_88,
        0.478_628_67,
        0.568_888_9,
        0.478_628_67,
        0.236_926_88,
    ];
    const ABSCISSAE: [f32; 5] = [
        -0.906_179_85,
        -0.538_469_3,
        0.0,
        0.538_469_3,
        0.906_179_85,
    ];

    let mut length = 0.0;
    for i in 0..5 {
        let t = 0.5 * (ABSCISSAE[i] + 1.0); // Map [-1,1] to [0,1]
        let derivative = bezier_derivative(p0, p1, p2, p3, t);
        length += WEIGHTS[i] * derivative.length();
    }
    length * 0.5 // Scale by interval width
}

/// Derivative of cubic Bezier at parameter t.
fn bezier_derivative(
    p0: glam::Vec2,
    p1: glam::Vec2,
    p2: glam::Vec2,
    p3: glam::Vec2,
    t: f32,
) -> glam::Vec2 {
    let t2 = t * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;

    // dB/dt = 3(1-t)²(P1-P0) + 6(1-t)t(P2-P1) + 3t²(P3-P2)
    3.0 * mt2 * (p1 - p0) + 6.0 * mt * t * (p2 - p1) + 3.0 * t2 * (p3 - p2)
}

/// Computes arc length for a straight line.
fn line_arc_length(p0: glam::Vec2, p1: glam::Vec2) -> f32 {
    (p1 - p0).length()
}

/// Computes arc length for an orthogonal (step) path.
fn step_arc_length(p0: glam::Vec2, p3: glam::Vec2, start_dir: u32) -> f32 {
    let is_horizontal = start_dir == 1 || start_dir == 0; // Right or Left
    let mid_x = (p0.x + p3.x) * 0.5;

    if is_horizontal {
        // Horizontal first: |p0 to mid_x| + |mid_x vertical| + |mid_x to p3|
        let h1 = (mid_x - p0.x).abs();
        let v = (p3.y - p0.y).abs();
        let h2 = (p3.x - mid_x).abs();
        h1 + v + h2
    } else {
        // Vertical first
        let mid_y = (p0.y + p3.y) * 0.5;
        let v1 = (mid_y - p0.y).abs();
        let h = (p3.x - p0.x).abs();
        let v2 = (p3.y - mid_y).abs();
        v1 + h + v2
    }
}

/// Computes arc length for a smooth step path (orthogonal with rounded corners).
fn smooth_step_arc_length(p0: glam::Vec2, p3: glam::Vec2, start_dir: u32, radius: f32) -> f32 {
    // Approximate: straight segments + quarter circle arcs
    let base_length = step_arc_length(p0, p3, start_dir);
    // Subtract corner distance, add arc length (2 quarter circles = PI * radius)
    let corner_adjustment = 2.0 * (std::f32::consts::PI * 0.5 * radius - radius);
    (base_length + corner_adjustment).max(0.0)
}

/// Computes the total arc length for an edge based on its curve type.
fn compute_edge_arc_length(
    start: glam::Vec2,
    end: glam::Vec2,
    start_dir: u32,
    end_dir: u32,
    curve: EdgeCurve,
) -> f32 {
    match curve {
        EdgeCurve::Line => line_arc_length(start, end),
        EdgeCurve::Orthogonal => step_arc_length(start, end, start_dir),
        EdgeCurve::OrthogonalSmooth { radius } => {
            smooth_step_arc_length(start, end, start_dir, radius)
        }
        EdgeCurve::BezierCubic | EdgeCurve::BezierQuadratic => {
            // Compute control points based on pin directions
            let (p1, p2) = compute_bezier_control_points(start, end, start_dir, end_dir);
            bezier_arc_length(start, p1, p2, end)
        }
    }
}

/// Computes Bezier control points from start/end positions and pin directions.
fn compute_bezier_control_points(
    start: glam::Vec2,
    end: glam::Vec2,
    start_dir: u32,
    end_dir: u32,
) -> (glam::Vec2, glam::Vec2) {
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    let tension = (dx.max(dy) * 0.5).max(50.0);

    let dir_to_vec = |dir: u32| -> glam::Vec2 {
        match dir {
            0 => glam::Vec2::new(-1.0, 0.0), // Left
            1 => glam::Vec2::new(1.0, 0.0),  // Right
            2 => glam::Vec2::new(0.0, -1.0), // Top
            3 => glam::Vec2::new(0.0, 1.0),  // Bottom
            _ => glam::Vec2::new(1.0, 0.0),
        }
    };

    let p1 = start + dir_to_vec(start_dir) * tension;
    let p2 = end + dir_to_vec(end_dir) * tension;

    (p1, p2)
}

pub struct Pipeline {
    uniforms: Buffer,
    nodes: buffer::Buffer<types::Node>,
    pins: buffer::Buffer<types::Pin>,
    edges: buffer::Buffer<types::Edge>,

    pipeline_background: RenderPipeline,
    pipeline_edges: RenderPipeline,
    pipeline_nodes_fill: RenderPipeline, // Background: node fill + shadow
    #[allow(dead_code)] // Kept for potential future use, borders now rendered by NodePrimitive
    pipeline_nodes_border: RenderPipeline,
    pipeline_pins: RenderPipeline,
    pipeline_overlay: RenderPipeline,

    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,

    /// Cached buffer generations to avoid recreating bind groups unnecessarily.
    /// This is critical for WebGPU/WASM where bind group creation can exhaust memory.
    bind_group_generations: (u64, u64, u64),
}

impl PipelineTrait for Pipeline {
    fn new(
        device: &iced::wgpu::Device,
        _queue: &iced::wgpu::Queue,
        format: iced::wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_shader(device, format, None)
    }
}

impl Pipeline {
    pub fn new_with_shader(
        device: &Device,
        format: TextureFormat,
        custom_shader_wgsl: Option<&str>,
    ) -> Self {
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("uniform buffer"),
            size: <types::Uniforms as ShaderSize>::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let nodes = buffer::Buffer::new(
            device,
            Some("nodes buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let pins = buffer::Buffer::new(
            device,
            Some("pins buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let edges = buffer::Buffer::new(
            device,
            Some("edges buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );

        let bind_group_layout = create_bind_group_layout(device);
        let bind_group = create_bind_group(
            device,
            &bind_group_layout,
            uniforms.as_entire_binding(),
            nodes.as_entire_binding(),
            pins.as_entire_binding(),
            edges.as_entire_binding(),
        );

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Node Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        // Use custom shader if provided, otherwise use default
        let shader_source = custom_shader_wgsl.unwrap_or(include_str!("shader.wgsl"));
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("node shaders"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        // Create all 5 pipelines
        let pipeline_background = create_pipeline_custom(
            device,
            format,
            &layout,
            &module,
            "vs_background",
            "fs_background",
            "background",
        );
        let pipeline_edges = create_pipeline_custom(
            device, format, &layout, &module, "vs_edge", "fs_edge", "edges",
        );
        let pipeline_nodes_fill = create_pipeline_custom(
            device,
            format,
            &layout,
            &module,
            "vs_node",
            "fs_node_fill",
            "nodes_fill",
        );
        let pipeline_nodes_border = create_pipeline_custom(
            device,
            format,
            &layout,
            &module,
            "vs_node",
            "fs_node",
            "nodes_border",
        );
        let pipeline_pins =
            create_pipeline_custom(device, format, &layout, &module, "vs_pin", "fs_pin", "pins");
        let pipeline_overlay = create_pipeline_custom(
            device,
            format,
            &layout,
            &module,
            "vs_overlay",
            "fs_overlay",
            "overlay",
        );

        Self {
            uniforms,
            nodes,
            pins,
            edges,
            pipeline_background,
            pipeline_edges,
            pipeline_nodes_fill,
            pipeline_nodes_border,
            pipeline_pins,
            pipeline_overlay,
            bind_group_layout,
            bind_group,
            bind_group_generations: (0, 0, 0),
        }
    }

    pub fn update_new(
        &mut self,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle<f32>,
        viewport: &Viewport,
        camera_zoom: f32,
        camera_position: WorldPoint,
        cursor_position: WorldPoint,
        time: f32,
        dragging: &Dragging,
        nodes: &[Node],
        edges: &[EdgeData],
        edge_color: glam::Vec4,
        background_color: glam::Vec4,
        border_color: glam::Vec4,
        fill_color: glam::Vec4,
        drag_edge_color: glam::Vec4,
        drag_edge_valid_color: glam::Vec4,
        selected_nodes: &std::collections::HashSet<usize>,
        selected_edge_color: glam::Vec4,
        edge_thickness: f32,
        layer: Layer,
        valid_drop_targets: &std::collections::HashSet<(usize, usize)>,
        background_style: &crate::style::BackgroundStyle,
    ) {
        let mut pin_start = 0;
        let num_nodes = self.nodes.update(
            device,
            queue,
            nodes.iter().map(|node| {
                let (pin_start, pin_count) = {
                    let count = node.pins.len() as u32;
                    let start = pin_start;
                    pin_start += count;
                    (start, count)
                };
                types::Node {
                    position: glam::Vec2::new(node.position.x, node.position.y),
                    size: glam::Vec2::new(node.size.width, node.size.height),
                    corner_radius: node.corner_radius,
                    border_width: node.border_width,
                    opacity: node.opacity,
                    pin_start,
                    pin_count,
                    shadow_blur: node.shadow_blur,
                    shadow_offset: glam::Vec2::new(node.shadow_offset.0, node.shadow_offset.1),
                    fill_color: glam::Vec4::new(
                        node.fill_color.r,
                        node.fill_color.g,
                        node.fill_color.b,
                        node.fill_color.a,
                    ),
                    border_color: glam::Vec4::new(
                        node.border_color.r,
                        node.border_color.g,
                        node.border_color.b,
                        node.border_color.a,
                    ),
                    shadow_color: glam::Vec4::new(
                        node.shadow_color.r,
                        node.shadow_color.g,
                        node.shadow_color.b,
                        node.shadow_color.a,
                    ),
                    flags: node.flags,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                }
            }),
        );

        // Check if we're in edge dragging mode
        let is_edge_dragging = matches!(
            dragging,
            Dragging::Edge(_, _, _) | Dragging::EdgeOver(_, _, _, _)
        );

        let num_pins = self.pins.update(
            device,
            queue,
            nodes
                .iter()
                .enumerate()
                .flat_map(|(node_id, node)| {
                    node.pins
                        .iter()
                        .enumerate()
                        .map(move |(pin_id, pin)| (node_id, pin_id, pin))
                })
                .map(|(node_id, pin_id, pin)| {
                    use crate::node_pin::PinDirection;
                    use crate::style::PinShape;

                    let pin_direction = match pin.direction {
                        PinDirection::Input => 0,
                        PinDirection::Output => 1,
                        PinDirection::Both => 2,
                    };
                    let pin_color =
                        glam::Vec4::new(pin.color.r, pin.color.g, pin.color.b, pin.color.a);

                    // Use precomputed valid_drop_targets from widget state
                    // This was computed once at drag-start using the can_connect callback
                    let flags =
                        if is_edge_dragging && valid_drop_targets.contains(&(node_id, pin_id)) {
                            types::PIN_FLAG_VALID_TARGET
                        } else {
                            0
                        };

                    types::Pin {
                        position: glam::Vec2::new(pin.offset.x, pin.offset.y),
                        color: pin_color,
                        border_color: glam::Vec4::new(
                            pin.border_color.r,
                            pin.border_color.g,
                            pin.border_color.b,
                            pin.border_color.a,
                        ),
                        side: pin.side,
                        radius: pin.radius,
                        direction: pin_direction,
                        shape: match pin.shape {
                            PinShape::Circle => 0,
                            PinShape::Square => 1,
                            PinShape::Diamond => 2,
                            PinShape::Triangle => 3,
                        },
                        border_width: pin.border_width,
                        flags,
                    }
                }),
        );

        // Extract pending cuts for edge cutting highlight
        let pending_cuts = if let Dragging::EdgeCutting { pending_cuts, .. } = dragging {
            Some(pending_cuts)
        } else {
            None
        };

        let num_edges = self.edges.update(
            device,
            queue,
            edges.iter().enumerate().map(|(edge_idx, edge_data)| {
                // Get source and target nodes/pins
                let from_node = &nodes[edge_data.from_node];
                let to_node = &nodes[edge_data.to_node];
                let from_pin = &from_node.pins[edge_data.from_pin];
                let to_pin = &to_node.pins[edge_data.to_pin];

                // Pin offsets are already absolute world positions (not relative to node)
                // This is because widget.rs adds the node position when creating pins
                let start_pos = glam::Vec2::new(from_pin.offset.x, from_pin.offset.y);
                let end_pos = glam::Vec2::new(to_pin.offset.x, to_pin.offset.y);

                // Highlight edges where both ends are selected
                let is_highlighted = selected_nodes.contains(&edge_data.from_node)
                    && selected_nodes.contains(&edge_data.to_node);

                // Check if edge is pending cut
                let is_pending_cut = pending_cuts
                    .as_ref()
                    .map(|cuts| cuts.contains(&edge_idx))
                    .unwrap_or(false);

                let style = &edge_data.style;

                // Get stroke layer (default to transparent if None)
                let stroke = style.stroke.as_ref();
                let (stroke_start, stroke_end, thickness) = stroke
                    .map(|s| (s.start_color, s.end_color, s.width))
                    .unwrap_or((iced::Color::TRANSPARENT, iced::Color::TRANSPARENT, 2.0));

                // Resolve edge gradient colors:
                // - TRANSPARENT (alpha < 0.01) = use pin color at that end
                // - Explicit color = use it
                // - Selected edges override everything with selection color
                let (start_color, end_color) = if is_highlighted {
                    // Selected edges use selection color (solid)
                    (selected_edge_color, selected_edge_color)
                } else {
                    // Resolve start color: explicit or pin color
                    let start = if stroke_start.a > 0.01 {
                        glam::Vec4::new(
                            stroke_start.r,
                            stroke_start.g,
                            stroke_start.b,
                            stroke_start.a,
                        )
                    } else {
                        glam::Vec4::new(
                            from_pin.color.r,
                            from_pin.color.g,
                            from_pin.color.b,
                            from_pin.color.a,
                        )
                    };

                    // Resolve end color: explicit or pin color
                    let end = if stroke_end.a > 0.01 {
                        glam::Vec4::new(stroke_end.r, stroke_end.g, stroke_end.b, stroke_end.a)
                    } else {
                        glam::Vec4::new(
                            to_pin.color.r,
                            to_pin.color.g,
                            to_pin.color.b,
                            to_pin.color.a,
                        )
                    };

                    (start, end)
                };

                // Extract pattern info from stroke
                let (
                    pattern_type,
                    dash_length,
                    gap_length,
                    dash_cap,
                    dash_cap_angle,
                    pattern_angle,
                ) = stroke
                    .map(|s| {
                        let pattern_type = s.pattern.type_id();
                        let (param1, param2) = s.pattern.params();
                        let cap_type = s.dash_cap.type_id();
                        let cap_angle = s.dash_cap.angle();
                        let pattern_angle = s.pattern.angle();
                        (
                            pattern_type,
                            param1,
                            param2,
                            cap_type,
                            cap_angle,
                            pattern_angle,
                        )
                    })
                    .unwrap_or((0, 0.0, 0.0, 0, 0.0, 0.0));

                // Compute arc length on CPU for accurate pattern spacing
                let start_vec = glam::Vec2::new(start_pos.x, start_pos.y);
                let end_vec = glam::Vec2::new(end_pos.x, end_pos.y);
                let curve_length = compute_edge_arc_length(
                    start_vec,
                    end_vec,
                    from_pin.side,
                    to_pin.side,
                    style.curve,
                );

                // Build flags: bit 0 = has motion, bit 3 = pending cut
                let mut flags = style.flags();
                if is_pending_cut {
                    flags |= 8; // bit 3 for pending cut highlight
                }

                // Extract border layer info
                let (border_width, border_gap, border_color) = style
                    .border
                    .as_ref()
                    .map(|b| {
                        (
                            b.width,
                            b.gap,
                            glam::Vec4::new(b.color.r, b.color.g, b.color.b, b.color.a),
                        )
                    })
                    .unwrap_or((0.0, 0.0, glam::Vec4::ZERO));

                // Extract shadow layer info
                let (shadow_blur, shadow_color, shadow_offset) = style
                    .shadow
                    .as_ref()
                    .map(|s| {
                        (
                            s.blur,
                            glam::Vec4::new(s.color.r, s.color.g, s.color.b, s.color.a),
                            glam::Vec2::new(s.offset.0, s.offset.1),
                        )
                    })
                    .unwrap_or((0.0, glam::Vec4::ZERO, glam::Vec2::ZERO));

                // Determine if edge is "reversed" (from Input to Output instead of Output to Input)
                let is_reversed = {
                    use crate::node_pin::PinDirection;
                    matches!(
                        (from_pin.direction, to_pin.direction),
                        (PinDirection::Input, PinDirection::Output)
                    )
                };

                // Animation direction: positive speed moves pattern Output→Input
                // For reversed edges, flip the speed to maintain consistent visual flow
                let base_speed = style.motion_speed();
                let flow_speed = if is_reversed { base_speed } else { -base_speed };

                // Arrow direction: flip pattern_angle for reversed edges
                // This keeps arrows pointing in consistent direction regardless of how edge was drawn
                let pattern_angle = if is_reversed {
                    -pattern_angle
                } else {
                    pattern_angle
                };

                types::Edge {
                    start: start_pos,
                    end: end_pos,
                    start_direction: from_pin.side,
                    end_direction: to_pin.side,
                    edge_type: style.curve.type_id(),
                    pattern_type,
                    start_color,
                    end_color,
                    thickness,
                    curve_length,
                    dash_length,
                    gap_length,
                    flow_speed,
                    dash_cap,
                    dash_cap_angle,
                    pattern_angle,
                    flags,
                    border_width,
                    border_gap,
                    shadow_blur,
                    border_color,
                    shadow_color,
                    shadow_offset,
                    _pad0: 0.0,
                    _pad1: 0.0,
                }
            }),
        );

        let dragging_type: u32 = match dragging {
            Dragging::None => 0,
            Dragging::Graph(_) => 1,
            Dragging::Node(_, _) => 2,
            Dragging::Edge(_, _, _) => 3,
            Dragging::EdgeOver(_, _, _, _) => 4,
            Dragging::BoxSelect(_, _) => 5,
            Dragging::GroupMove(_) => 6,
            Dragging::EdgeCutting { .. } => 7,
        };

        // Only BoxSelect and EdgeCutting need the origin point for overlay rendering
        let dragging_edge_from_origin = match dragging {
            Dragging::BoxSelect(start, _end) => *start,
            Dragging::EdgeCutting { trail, .. } => {
                trail.first().copied().unwrap_or(WorldPoint::zero())
            }
            _ => WorldPoint::zero(),
        };

        let scale = viewport.scale_factor();
        let uniforms = types::Uniforms {
            os_scale_factor: scale,
            camera_zoom,
            camera_position: glam::Vec2::new(camera_position.x, camera_position.y),
            border_color,
            fill_color,
            edge_color,
            background_color,
            drag_edge_color,
            drag_edge_valid_color,
            cursor_position: glam::Vec2::new(cursor_position.x, cursor_position.y),
            num_nodes,
            num_pins,
            num_edges,
            time,
            overlay_type: dragging_type,
            overlay_start: glam::Vec2::new(
                dragging_edge_from_origin.x,
                dragging_edge_from_origin.y,
            ),
            // Theme-derived visual parameters (computed in Rust, no hardcodes in shader)
            grid_color: glam::Vec4::new(
                border_color.x * 1.3,
                border_color.y * 1.3,
                border_color.z * 1.3,
                1.0,
            ),
            hover_glow_color: glam::Vec4::new(0.5, 0.7, 1.0, 1.0), // Soft blue glow
            selection_box_color: glam::Vec4::new(0.3, 0.6, 1.0, 1.0), // Selection blue
            edge_cutting_color: glam::Vec4::new(1.0, 0.3, 0.3, 1.0), // Warning red
            hover_glow_radius: 6.0,
            edge_thickness,
            render_mode: match layer {
                Layer::Background => 0,
                Layer::Foreground => 1,
            },
            viewport_size: glam::Vec2::new(
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ),
            bounds_origin: glam::Vec2::new(bounds.x * scale, bounds.y * scale),
            bounds_size: glam::Vec2::new(bounds.width * scale, bounds.height * scale),

            // Background pattern configuration
            bg_pattern_type: background_style.pattern.type_id(),
            bg_flags: (if background_style.adaptive_zoom {
                1u32
            } else {
                0
            }) | (if background_style.hex_pointy_top {
                2u32
            } else {
                0
            }),
            bg_minor_spacing: background_style.minor_spacing,
            bg_major_ratio: background_style
                .major_spacing
                .map(|m| m / background_style.minor_spacing)
                .unwrap_or(0.0),
            bg_line_widths: glam::Vec2::new(
                background_style.minor_width,
                background_style.major_width,
            ),
            bg_opacities: glam::Vec2::new(
                background_style.minor_opacity,
                background_style.major_opacity,
            ),
            bg_primary_color: glam::Vec4::new(
                background_style.primary_color.r,
                background_style.primary_color.g,
                background_style.primary_color.b,
                background_style.primary_color.a,
            ),
            bg_secondary_color: glam::Vec4::new(
                background_style.secondary_color.r,
                background_style.secondary_color.g,
                background_style.secondary_color.b,
                background_style.secondary_color.a,
            ),
            bg_pattern_params: glam::Vec4::new(
                background_style.dot_radius,
                background_style.line_angle,
                background_style.crosshatch_angle,
                0.0, // padding
            ),
            bg_adaptive_params: glam::Vec4::new(
                background_style.adaptive_min_spacing,
                background_style.adaptive_max_spacing,
                background_style.adaptive_fade_range,
                0.0, // padding
            ),
        };

        // Write uniforms using encase for proper layout
        let mut uniform_buffer = encase::UniformBuffer::new(Vec::new());
        uniform_buffer
            .write(&uniforms)
            .expect("Failed to write uniforms");
        queue.write_buffer(&self.uniforms, 0, uniform_buffer.as_ref());

        // Only recreate bind group if buffer generations changed.
        // This is critical for WebGPU/WASM where bind group creation can exhaust GPU memory.
        let current_generations = (
            self.nodes.generation(),
            self.pins.generation(),
            self.edges.generation(),
        );
        if current_generations != self.bind_group_generations {
            self.bind_group = create_bind_group(
                device,
                &self.bind_group_layout,
                self.uniforms.as_entire_binding(),
                self.nodes.as_entire_binding(),
                self.pins.as_entire_binding(),
                self.edges.as_entire_binding(),
            );
            self.bind_group_generations = current_generations;
        }
    }

    pub fn render_pass(&self, pass: &mut iced::wgpu::RenderPass<'_>, layer: Layer) {
        let num_nodes = self.nodes.len();
        let num_pins = self.pins.len();
        let num_edges = self.edges.len();

        pass.set_bind_group(0, &self.bind_group, &[]);

        match layer {
            Layer::Background => {
                // Pass 1: Background grid (fullscreen)
                pass.set_pipeline(&self.pipeline_background);
                pass.draw(0..3, 0..1);

                // Pass 2: Edges (instanced - behind nodes)
                if num_edges > 0 {
                    pass.set_pipeline(&self.pipeline_edges);
                    pass.draw(0..6, 0..num_edges as u32);
                }

                // Pass 3: Node fills (instanced) - fs_node_fill shader
                if num_nodes > 0 {
                    pass.set_pipeline(&self.pipeline_nodes_fill);
                    pass.draw(0..6, 0..num_nodes as u32);
                }

                // Pass 4: Pin indicators (instanced)
                if num_pins > 0 {
                    pass.set_pipeline(&self.pipeline_pins);
                    pass.draw(0..6, 0..num_pins as u32);
                }
            }
            Layer::Foreground => {
                // NOTE: Node borders are now rendered by the new NodePrimitive system.
                // This old primitive is only used for overlays (box select, edge cutting).

                // Draw box select / edge cutting overlay
                pass.set_pipeline(&self.pipeline_overlay);
                pass.draw(0..6, 0..1);
            }
        }
    }
}

fn create_pipeline_custom(
    device: &Device,
    format: TextureFormat,
    layout: &PipelineLayout,
    module: &ShaderModule,
    vs_entry: &str,
    fs_entry: &str,
    label: &str,
) -> RenderPipeline {
    let fragment_targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: VertexState {
            module,
            entry_point: Some(vs_entry),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(),
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(FragmentState {
            module,
            entry_point: Some(fs_entry),
            targets: &fragment_targets,
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}

fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Node Pipeline Bind Group Layout"),
        entries: &[
            // Binding 0: Uniforms (uniform buffer)
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(<types::Uniforms as ShaderSize>::SHADER_SIZE),
                },
                count: None,
            },
            // Binding 1: Nodes (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::Node as ShaderSize>::SHADER_SIZE.get() * 10)
                            .expect("Node SHADER_SIZE * 10 must be non-zero"),
                    ),
                },
                count: None,
            },
            // Binding 2: Pins (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::Pin as ShaderSize>::SHADER_SIZE.get() * 10)
                            .expect("Pin SHADER_SIZE * 10 must be non-zero"),
                    ),
                },
                count: None,
            },
            // Binding 3: Edges (storage buffer, read-only)
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(<types::Edge as ShaderSize>::SHADER_SIZE.get() * 10)
                            .expect("Edge SHADER_SIZE * 10 must be non-zero"),
                    ),
                },
                count: None,
            },
        ],
    })
}

fn create_bind_group(
    device: &Device,
    bind_group_layout: &BindGroupLayout,
    uniforms: BindingResource,
    nodes: BindingResource,
    pins: BindingResource,
    edges: BindingResource,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("Node Pipeline Bind Group"),
        layout: bind_group_layout,
        entries: &[
            // Entry 0: Uniforms
            BindGroupEntry {
                binding: 0,
                resource: uniforms,
            },
            // Entry 1: Nodes
            BindGroupEntry {
                binding: 1,
                resource: nodes,
            },
            // Entry 2: Pins
            BindGroupEntry {
                binding: 2,
                resource: pins,
            },
            // Entry 3: Edges
            BindGroupEntry {
                binding: 3,
                resource: edges,
            },
        ],
    })
}
