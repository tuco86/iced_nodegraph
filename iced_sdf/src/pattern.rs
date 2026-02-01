//! Pattern modifiers for SDF shapes.
//!
//! Patterns transform the SDF distance field to create effects like
//! dashed lines, arrows, and dots. They work on any SDF shape.

/// Pattern type for stroke rendering.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PatternType {
    /// Solid stroke.
    #[default]
    Solid,
    /// Dashed stroke with round caps.
    Dashed {
        /// Length of each dash segment.
        dash: f32,
        /// Gap between dashes.
        gap: f32,
    },
    /// Arrowed/angled dashes (like marching ants).
    Arrowed {
        /// Length of each segment.
        segment: f32,
        /// Gap between segments.
        gap: f32,
        /// Angle in degrees for the shear effect.
        angle: f32,
    },
    /// Dotted pattern.
    Dotted {
        /// Spacing between dots.
        spacing: f32,
        /// Dot radius.
        radius: f32,
    },
}

/// Pattern configuration for SDF rendering.
#[derive(Debug, Clone, Copy)]
pub struct Pattern {
    /// Stroke thickness.
    pub thickness: f32,
    /// Pattern type.
    pub pattern_type: PatternType,
    /// Flow animation speed (world units per second, 0 = no animation).
    pub flow_speed: f32,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            thickness: 1.0,
            pattern_type: PatternType::Solid,
            flow_speed: 0.0,
        }
    }
}

impl Pattern {
    /// Create a solid pattern with given thickness.
    pub fn solid(thickness: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::Solid,
            flow_speed: 0.0,
        }
    }

    /// Create a dashed pattern.
    pub fn dashed(thickness: f32, dash: f32, gap: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::Dashed { dash, gap },
            flow_speed: 0.0,
        }
    }

    /// Create an arrowed pattern.
    pub fn arrowed(thickness: f32, segment: f32, gap: f32, angle: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::Arrowed { segment, gap, angle },
            flow_speed: 0.0,
        }
    }

    /// Create a dotted pattern.
    pub fn dotted(spacing: f32, radius: f32) -> Self {
        Self {
            thickness: radius * 2.0,
            pattern_type: PatternType::Dotted { spacing, radius },
            flow_speed: 0.0,
        }
    }

    /// Set flow animation speed.
    pub fn flow(mut self, speed: f32) -> Self {
        self.flow_speed = speed;
        self
    }

    /// Convert to GPU format (packed into 4 floats).
    /// Format: (pattern_type, param0, param1, param2)
    pub fn to_gpu(&self) -> (u32, f32, f32, f32, f32, f32) {
        match self.pattern_type {
            PatternType::Solid => (0, self.thickness, 0.0, 0.0, 0.0, self.flow_speed),
            PatternType::Dashed { dash, gap } => (1, self.thickness, dash, gap, 0.0, self.flow_speed),
            PatternType::Arrowed { segment, gap, angle } => {
                (2, self.thickness, segment, gap, angle.to_radians(), self.flow_speed)
            }
            PatternType::Dotted { spacing, radius } => {
                (3, self.thickness, spacing, radius, 0.0, self.flow_speed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_pattern() {
        let p = Pattern::solid(2.0);
        let (ty, thickness, _, _, _, _) = p.to_gpu();
        assert_eq!(ty, 0);
        assert_eq!(thickness, 2.0);
    }

    #[test]
    fn test_dashed_pattern() {
        let p = Pattern::dashed(2.0, 10.0, 5.0);
        let (ty, thickness, dash, gap, _, _) = p.to_gpu();
        assert_eq!(ty, 1);
        assert_eq!(thickness, 2.0);
        assert_eq!(dash, 10.0);
        assert_eq!(gap, 5.0);
    }

    #[test]
    fn test_flow_animation() {
        let p = Pattern::solid(2.0).flow(50.0);
        let (_, _, _, _, _, flow) = p.to_gpu();
        assert_eq!(flow, 50.0);
    }
}
