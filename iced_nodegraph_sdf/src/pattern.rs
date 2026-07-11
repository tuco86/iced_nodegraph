//! Pattern modifiers for SDF shapes.
//!
//! Patterns transform the distance field to create effects like
//! dashed lines, arrows, and dots along a curve or shape boundary.

/// Pattern type for stroke rendering.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PatternType {
    /// Solid stroke.
    #[default]
    Solid,
    /// Dashed stroke with straight caps. Angle tilts the caps (0 = perpendicular).
    Dashed { dash: f32, gap: f32, angle: f32 },
    /// Arrow-style angled dashes. Default angle ~33.3 degrees.
    Arrowed { segment: f32, gap: f32, angle: f32 },
    /// Dotted pattern.
    Dotted { spacing: f32, radius: f32 },
    /// Alternating dash-dot pattern.
    DashDotted {
        dash: f32,
        gap: f32,
        dot_radius: f32,
    },
    /// Alternating arrow-dot pattern.
    ArrowDotted {
        segment: f32,
        gap: f32,
        dot_radius: f32,
    },
}

/// Pattern configuration for SDF stroke rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// Whether this is a plain solid stroke (no dashes, dots or arrows).
    pub fn is_solid(&self) -> bool {
        matches!(self.pattern_type, PatternType::Solid)
    }

    /// Solid stroke with given thickness.
    pub fn solid(thickness: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::Solid,
            flow_speed: 0.0,
        }
    }

    /// Dashed stroke with straight caps.
    pub fn dashed(thickness: f32, dash: f32, gap: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::Dashed {
                dash,
                gap,
                angle: 0.0,
            },
            flow_speed: 0.0,
        }
    }

    /// Dashed stroke with angled caps.
    ///
    /// `angle` is clamped to +-1.2 rad (~69 deg): the shader shears dash
    /// coordinates by `tan(angle)` and corrects by `cos(angle)`, both of
    /// which degenerate toward +-pi/2.
    pub fn dashed_angle(thickness: f32, dash: f32, gap: f32, angle: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::Dashed {
                dash,
                gap,
                angle: clamp_cap_angle(angle),
            },
            flow_speed: 0.0,
        }
    }

    /// Arrow-style angled dashes.
    pub fn arrowed(thickness: f32, segment: f32, gap: f32) -> Self {
        let angle = 33.3_f32.to_radians();
        Self {
            thickness,
            pattern_type: PatternType::Arrowed {
                segment,
                gap,
                angle,
            },
            flow_speed: 0.0,
        }
    }

    /// Arrow-style angled dashes with custom angle.
    ///
    /// `angle` is clamped to +-1.2 rad (~69 deg); see [`Pattern::dashed_angle`].
    pub fn arrowed_angle(thickness: f32, segment: f32, gap: f32, angle: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::Arrowed {
                segment,
                gap,
                angle: clamp_cap_angle(angle),
            },
            flow_speed: 0.0,
        }
    }

    /// Dotted pattern.
    pub fn dotted(spacing: f32, radius: f32) -> Self {
        Self {
            thickness: radius * 2.0,
            pattern_type: PatternType::Dotted { spacing, radius },
            flow_speed: 0.0,
        }
    }

    /// Alternating dash-dot pattern.
    pub fn dash_dotted(thickness: f32, dash: f32, gap: f32, dot_radius: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::DashDotted {
                dash,
                gap,
                dot_radius,
            },
            flow_speed: 0.0,
        }
    }

    /// Alternating arrow-dot pattern.
    pub fn arrow_dotted(thickness: f32, segment: f32, gap: f32, dot_radius: f32) -> Self {
        Self {
            thickness,
            pattern_type: PatternType::ArrowDotted {
                segment,
                gap,
                dot_radius,
            },
            flow_speed: 0.0,
        }
    }

    /// Set flow animation speed (world units per second).
    pub fn flow(mut self, speed: f32) -> Self {
        self.flow_speed = speed;
        self
    }

    /// Whether this pattern has active animation.
    pub fn is_animated(&self) -> bool {
        self.flow_speed != 0.0
    }

    /// Convert to GPU format: (pattern_type_id, thickness, param0, param1, param2, flow_speed).
    pub(crate) fn as_gpu(self) -> (u32, f32, f32, f32, f32, f32) {
        match self.pattern_type {
            PatternType::Solid => (0, self.thickness, 0.0, 0.0, 0.0, self.flow_speed),
            PatternType::Dashed { dash, gap, angle } => (
                1,
                self.thickness,
                dash,
                gap,
                clamp_cap_angle(angle),
                self.flow_speed,
            ),
            PatternType::Arrowed {
                segment,
                gap,
                angle,
            } => (
                2,
                self.thickness,
                segment,
                gap,
                clamp_cap_angle(angle),
                self.flow_speed,
            ),
            PatternType::Dotted { spacing, radius } => {
                (3, self.thickness, spacing, radius, 0.0, self.flow_speed)
            }
            PatternType::DashDotted {
                dash,
                gap,
                dot_radius,
            } => (4, self.thickness, dash, gap, dot_radius, self.flow_speed),
            PatternType::ArrowDotted {
                segment,
                gap,
                dot_radius,
            } => (5, self.thickness, segment, gap, dot_radius, self.flow_speed),
        }
    }
}

/// Clamps a dash/arrow cap angle to a range where the shader's `tan(angle)`
/// shear and `cos(angle)` Lipschitz correction stay well-conditioned; at
/// +-pi/2 both degenerate (inf/NaN coordinates, zero-width strokes).
fn clamp_cap_angle(angle: f32) -> f32 {
    const MAX_CAP_ANGLE: f32 = 1.2; // rad, ~69 deg
    if angle.is_nan() {
        return 0.0;
    }
    angle.clamp(-MAX_CAP_ANGLE, MAX_CAP_ANGLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_pattern() {
        let p = Pattern::solid(2.0);
        let (ty, thickness, _, _, _, _) = p.as_gpu();
        assert_eq!(ty, 0);
        assert_eq!(thickness, 2.0);
        assert!(!p.is_animated());
    }

    #[test]
    fn test_dashed_pattern() {
        let p = Pattern::dashed(2.0, 10.0, 5.0);
        let (ty, thickness, dash, gap, angle, _) = p.as_gpu();
        assert_eq!(ty, 1);
        assert_eq!(thickness, 2.0);
        assert_eq!(dash, 10.0);
        assert_eq!(gap, 5.0);
        assert_eq!(angle, 0.0);
    }

    #[test]
    fn test_arrow_dotted_pattern() {
        let p = Pattern::arrow_dotted(2.0, 8.0, 4.0, 1.5);
        let (ty, _, seg, gap, dot_r, _) = p.as_gpu();
        assert_eq!(ty, 5);
        assert_eq!(seg, 8.0);
        assert_eq!(gap, 4.0);
        assert_eq!(dot_r, 1.5);
    }

    #[test]
    fn test_flow_animation() {
        let p = Pattern::solid(2.0).flow(50.0);
        assert!(p.is_animated());
        let (_, _, _, _, _, flow) = p.as_gpu();
        assert_eq!(flow, 50.0);
    }
}
