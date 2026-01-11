// Allow dead_code warnings from encase's ShaderType derive macro
#![allow(dead_code)]

use encase::ShaderType;

// Pin flag constants
pub const PIN_FLAG_VALID_TARGET: u32 = 1; // bit 0: valid drop target during edge dragging

/// Global uniforms shared by all primitives.
///
/// Contains camera, viewport, and timing data needed by all shader passes.
#[derive(Clone, Debug, ShaderType)]
pub struct Uniforms {
    pub os_scale_factor: f32, // e.g. 1.0, 1.5
    pub camera_zoom: f32,
    pub camera_position: glam::Vec2,

    pub cursor_position: glam::Vec2, // in world coordinates

    pub num_nodes: u32,
    pub time: f32, // Time in seconds for animations

    pub overlay_type: u32,
    pub overlay_start: glam::Vec2,

    pub overlay_color: glam::Vec4, // Color for active overlay (box select, edge cutting, etc.)

    pub bounds_origin: glam::Vec2, // widget bounds origin in physical pixels
    pub bounds_size: glam::Vec2,   // widget bounds size in physical pixels
}

/// Grid background configuration.
///
/// Read from storage buffer by the grid shader pass.
#[derive(Clone, Debug, ShaderType)]
pub struct Grid {
    /// Pattern type: 0=None, 1=Grid, 2=Hex, 3=Triangle, 4=Dots, 5=Lines, 6=Crosshatch
    pub pattern_type: u32,
    /// Flags: bit 0 = adaptive_zoom, bit 1 = hex_pointy_top
    pub flags: u32,
    /// Minor spacing in world-space pixels
    pub minor_spacing: f32,
    /// Major spacing ratio (major_spacing / minor_spacing), 0 = no major grid
    pub major_ratio: f32,

    /// Line widths: (minor_width, major_width)
    pub line_widths: glam::Vec2,
    /// Opacities: (minor_opacity, major_opacity)
    pub opacities: glam::Vec2,

    /// Primary pattern color (background fill)
    pub primary_color: glam::Vec4,
    /// Secondary pattern color (grid lines)
    pub secondary_color: glam::Vec4,

    /// Pattern-specific params: (dot_radius, line_angle, crosshatch_angle, _padding)
    pub pattern_params: glam::Vec4,

    /// Adaptive zoom thresholds: (min_spacing, max_spacing, fade_range, _padding)
    pub adaptive_params: glam::Vec4,
}

#[derive(Clone, Debug, ShaderType)]
pub struct Node {
    pub position: glam::Vec2,
    pub size: glam::Vec2,
    pub corner_radius: f32,
    pub border_width: f32,
    pub opacity: f32,
    pub pin_start: u32,
    pub pin_count: u32,
    pub shadow_blur: f32,
    pub shadow_offset: glam::Vec2,
    pub fill_color: glam::Vec4,
    pub border_color: glam::Vec4,
    pub shadow_color: glam::Vec4,
    pub glow_color: glam::Vec4,   // Hover glow color (set when hovered)
    pub flags: u32,               // bit 0: hovered, bit 1: selected
    pub glow_radius: f32,         // Hover glow radius in world units
    // Padding for 16-byte array stride alignment (128 bytes total)
    pub _pad0: u32,
    pub _pad1: u32,
}

#[derive(Clone, Debug, ShaderType)]
pub struct Pin {
    pub position: glam::Vec2,
    pub side: u32,
    pub radius: f32,
    pub color: glam::Vec4,
    pub border_color: glam::Vec4,
    pub direction: u32,
    pub shape: u32, // 0=Circle, 1=Square, 2=Diamond, 3=Triangle
    pub border_width: f32,
    pub flags: u32, // bit 0: valid drop target during edge dragging
}

/// Edge with resolved world positions (no index lookups needed in shader).
///
/// Pattern type IDs: 0=Solid, 1=Dashed, 2=Angled, 3=Dotted, 4=DashDotted, 5=Custom
#[derive(Clone, Debug, ShaderType)]
pub struct Edge {
    // Positions and directions
    pub start: glam::Vec2,
    pub end: glam::Vec2,
    pub start_direction: u32, // PinSide: 0=Left, 1=Right, 2=Top, 3=Bottom
    pub end_direction: u32,
    pub edge_type: u32,    // 0=Bezier, 1=Straight, 2=SmoothStep, 3=Step
    pub pattern_type: u32, // 0=solid, 1=dashed, 2=angled, 3=dotted, 4=dash-dotted

    // Stroke colors
    pub start_color: glam::Vec4, // color at source (t=0)
    pub end_color: glam::Vec4,   // color at target (t=1)

    // Stroke parameters
    pub thickness: f32,
    pub curve_length: f32, // pre-computed arc length for proper parameterization
    pub dash_length: f32,  // dashed/angled: segment, dotted: spacing
    pub gap_length: f32,   // dashed/angled: gap, dotted: radius

    // Animation and pattern
    pub flow_speed: f32,     // world units per second, 0.0 = no animation
    pub dash_cap: u32,       // 0=butt, 1=round, 2=square, 3=angled
    pub dash_cap_angle: f32, // angle in radians for angled caps
    pub pattern_angle: f32,  // angle in radians for Angled pattern

    // Flags and border params
    pub flags: u32, // bit 0: animated, bit 1: glow, bit 2: pulse, bit 3: pending cut
    pub border_width: f32, // border/outline thickness
    pub border_gap: f32, // gap between stroke and border
    pub shadow_blur: f32, // shadow blur radius

    // Border color
    pub border_color: glam::Vec4,

    // Shadow
    pub shadow_color: glam::Vec4,
    pub shadow_offset: glam::Vec2,
    // Padding for 16-byte array stride alignment (160 bytes total)
    pub _pad0: f32,
    pub _pad1: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use encase::ShaderSize;

    #[test]
    fn test_node_shader_size() {
        // WGSL Node struct is 128 bytes (32 x 4-byte fields)
        assert_eq!(Node::SHADER_SIZE.get(), 128, "Node size mismatch");
    }

    #[test]
    fn test_pin_shader_size() {
        // WGSL Pin struct is 64 bytes (16 x 4-byte fields)
        assert_eq!(Pin::SHADER_SIZE.get(), 64, "Pin size mismatch");
    }

    #[test]
    fn test_edge_shader_size() {
        // WGSL Edge struct is 160 bytes (40 x 4-byte fields)
        assert_eq!(Edge::SHADER_SIZE.get(), 160, "Edge size mismatch");
    }

    #[test]
    fn test_uniforms_shader_size() {
        // WGSL Uniforms - validate it's reasonable
        let size = Uniforms::SHADER_SIZE.get();
        assert!(size > 0, "Uniforms size should be positive");
        assert!(size % 16 == 0, "Uniforms size should be 16-byte aligned");
    }

    fn create_test_node(id: u32) -> Node {
        Node {
            position: glam::Vec2::new(id as f32 * 100.0, 0.0),
            size: glam::Vec2::new(200.0, 150.0),
            corner_radius: 8.0,
            border_width: 2.0,
            opacity: 1.0,
            pin_start: id * 2,
            pin_count: 2,
            shadow_blur: 10.0,
            shadow_offset: glam::Vec2::new(2.0, 2.0),
            fill_color: glam::Vec4::new(0.2, 0.2, 0.2, 1.0),
            border_color: glam::Vec4::new(0.5, 0.5, 0.5, 1.0),
            shadow_color: glam::Vec4::new(0.0, 0.0, 0.0, 0.5),
            glow_color: glam::Vec4::new(0.3, 0.6, 1.0, 0.3),
            flags: id,
            glow_radius: 8.0,
            _pad0: 0,
            _pad1: 0,
        }
    }

    /// Helper to serialize items at correct offsets (like buffer.rs does)
    fn serialize_items<T: ShaderType + ShaderSize + encase::internal::WriteInto>(
        items: &[T],
    ) -> Vec<u8> {
        let item_size = T::SHADER_SIZE.get() as usize;
        let total_size = items.len() * item_size;
        let mut bytes = vec![0u8; total_size];

        for (i, item) in items.iter().enumerate() {
            let offset = i * item_size;
            let slice = &mut bytes[offset..offset + item_size];
            let mut writer = encase::StorageBuffer::new(slice);
            writer.write(item).expect("write failed");
        }

        bytes
    }

    #[test]
    fn test_array_serialization_correct_size() {
        let nodes = vec![
            create_test_node(0),
            create_test_node(1),
            create_test_node(2),
        ];
        let bytes = serialize_items(&nodes);

        // Each node is 128 bytes. For 3 nodes we expect 384 bytes.
        let expected_size = 3 * Node::SHADER_SIZE.get() as usize;
        assert_eq!(
            bytes.len(),
            expected_size,
            "Array should be {} bytes for 3 nodes, got {}",
            expected_size,
            bytes.len()
        );
    }

    #[test]
    fn test_node_position_at_correct_offsets() {
        let nodes = vec![
            create_test_node(0),
            create_test_node(1),
            create_test_node(2),
        ];
        let bytes = serialize_items(&nodes);
        let stride = Node::SHADER_SIZE.get() as usize; // 112 bytes

        // Check position.x at each node's offset (position is first field)
        for (i, node) in nodes.iter().enumerate() {
            let offset = i * stride;
            let pos_x_bytes = &bytes[offset..offset + 4];
            let pos_x = f32::from_le_bytes(pos_x_bytes.try_into().unwrap());
            assert_eq!(
                pos_x, node.position.x,
                "Node {} position.x mismatch at offset {}: expected {}, got {}",
                i, offset, node.position.x, pos_x
            );
        }
    }

    #[test]
    fn test_node_flags_at_correct_offsets() {
        let nodes = vec![
            create_test_node(0),
            create_test_node(1),
            create_test_node(2),
        ];
        let bytes = serialize_items(&nodes);
        let stride = Node::SHADER_SIZE.get() as usize; // 128 bytes

        // Calculate flags offset within Node struct:
        // position: 8, size: 8, corner_radius: 4, border_width: 4, opacity: 4,
        // pin_start: 4, pin_count: 4, shadow_blur: 4, shadow_offset: 8,
        // fill_color: 16, border_color: 16, shadow_color: 16, glow_color: 16
        // = 8+8+4+4+4+4+4+4+8+16+16+16+16 = 112 bytes, then flags at 112
        let flags_offset_in_struct = 112;

        for (i, node) in nodes.iter().enumerate() {
            let offset = i * stride + flags_offset_in_struct;
            let flags_bytes = &bytes[offset..offset + 4];
            let flags = u32::from_le_bytes(flags_bytes.try_into().unwrap());
            assert_eq!(
                flags, node.flags,
                "Node {} flags mismatch at offset {}: expected {}, got {}",
                i, offset, node.flags, flags
            );
        }
    }

    fn create_test_pin(id: u32) -> Pin {
        Pin {
            position: glam::Vec2::new(id as f32 * 50.0, 100.0),
            side: id % 4,
            radius: 6.0,
            color: glam::Vec4::new(0.8, 0.2, 0.2, 1.0),
            border_color: glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            direction: id % 3,
            shape: id % 4,
            border_width: 1.0,
            flags: id,
        }
    }

    #[test]
    fn test_pin_array_serialization() {
        let pins = vec![
            create_test_pin(0),
            create_test_pin(1),
            create_test_pin(2),
            create_test_pin(3),
        ];
        let bytes = serialize_items(&pins);
        let stride = Pin::SHADER_SIZE.get() as usize; // 64 bytes

        assert_eq!(bytes.len(), 4 * 64, "4 pins should be 256 bytes");

        // Check position.x for each pin
        for (i, pin) in pins.iter().enumerate() {
            let offset = i * stride;
            let pos_x_bytes = &bytes[offset..offset + 4];
            let pos_x = f32::from_le_bytes(pos_x_bytes.try_into().unwrap());
            assert_eq!(pos_x, pin.position.x, "Pin {} position.x mismatch", i);
        }
    }

    fn create_test_edge(id: u32) -> Edge {
        Edge {
            start: glam::Vec2::new(id as f32 * 100.0, 0.0),
            end: glam::Vec2::new(id as f32 * 100.0 + 200.0, 100.0),
            start_direction: 1,
            end_direction: 0,
            edge_type: 0,
            pattern_type: 0,
            start_color: glam::Vec4::new(0.5, 0.5, 0.5, 1.0),
            end_color: glam::Vec4::new(0.5, 0.5, 0.5, 1.0),
            thickness: 2.0,
            curve_length: 250.0,
            dash_length: 10.0,
            gap_length: 5.0,
            flow_speed: 0.0,
            dash_cap: 0,
            dash_cap_angle: 0.0,
            pattern_angle: 0.0,
            flags: id,
            border_width: 0.0,
            border_gap: 0.0,
            shadow_blur: 0.0,
            border_color: glam::Vec4::ZERO,
            shadow_color: glam::Vec4::ZERO,
            shadow_offset: glam::Vec2::ZERO,
            _pad0: 0.0,
            _pad1: 0.0,
        }
    }

    #[test]
    fn test_edge_array_serialization() {
        let edges = vec![create_test_edge(0), create_test_edge(1)];
        let bytes = serialize_items(&edges);
        let stride = Edge::SHADER_SIZE.get() as usize; // 160 bytes

        assert_eq!(bytes.len(), 2 * 160, "2 edges should be 320 bytes");

        // Check start.x for each edge
        for (i, edge) in edges.iter().enumerate() {
            let offset = i * stride;
            let start_x_bytes = &bytes[offset..offset + 4];
            let start_x = f32::from_le_bytes(start_x_bytes.try_into().unwrap());
            assert_eq!(start_x, edge.start.x, "Edge {} start.x mismatch", i);
        }
    }
}
