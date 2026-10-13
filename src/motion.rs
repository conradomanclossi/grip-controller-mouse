//! Pure stick math: deadzone, response curve, sub-pixel accumulation.

/// Applies a radial deadzone and a power response curve to a stick vector.
/// Returns a vector whose magnitude is in `0.0..=1.0`, zero inside the deadzone.
pub fn shape(x: f32, y: f32, deadzone: f32, exponent: f32) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt();
    if mag <= deadzone || mag == 0.0 {
        return (0.0, 0.0);
    }
    let live = ((mag.min(1.0) - deadzone) / (1.0 - deadzone)).clamp(0.0, 1.0);
    let scaled = live.powf(exponent);
    (x / mag * scaled, y / mag * scaled)
}

/// Accumulates fractional deltas and hands out whole units, keeping the remainder.
#[derive(Default, Debug)]
pub struct Accumulator {
    x: f64,
    y: f64,
}

impl Accumulator {
    pub fn add(&mut self, dx: f64, dy: f64) -> (i32, i32) {
        self.x += dx;
        self.y += dy;
        let (ix, iy) = (self.x.trunc(), self.y.trunc());
        self.x -= ix;
        self.y -= iy;
        (ix as i32, iy as i32)
    }

    pub fn reset(&mut self) {
        self.x = 0.0;
        self.y = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_deadzone_is_zero() {
        assert_eq!(shape(0.05, 0.05, 0.12, 2.0), (0.0, 0.0));
        assert_eq!(shape(0.0, 0.0, 0.0, 1.0), (0.0, 0.0));
    }

    #[test]
    fn full_deflection_is_unit() {
        let (x, y) = shape(1.0, 0.0, 0.12, 2.0);
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        // Corners of square gates exceed 1.0 and must be clamped.
        let (x, y) = shape(1.0, 1.0, 0.12, 2.0);
        assert!(((x * x + y * y).sqrt() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn curve_is_monotonic_and_preserves_direction() {
        let (a, _) = shape(0.4, 0.0, 0.1, 2.0);
        let (b, _) = shape(0.7, 0.0, 0.1, 2.0);
        assert!(0.0 < a && a < b);
        let (x, y) = shape(-0.5, -0.5, 0.1, 1.0);
        assert!(x < 0.0 && y < 0.0 && (x - y).abs() < 1e-6);
    }

    #[test]
    fn accumulator_keeps_remainder() {
        let mut acc = Accumulator::default();
        assert_eq!(acc.add(0.6, -0.6), (0, 0));
        assert_eq!(acc.add(0.6, -0.6), (1, -1));
        assert_eq!(acc.add(2.9, 0.0), (3, 0));
        acc.reset();
        assert_eq!(acc.add(0.5, 0.5), (0, 0));
    }
}
