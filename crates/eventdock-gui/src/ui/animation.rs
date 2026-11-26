/// Compute the blend factor for exponential convergence over a time step.
pub(super) fn exponential_blend(
    rate: f32,
    dt: f32,
) -> f32 {
    1.0 - (-rate * dt).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_time_preserves_position() {
        assert_eq!(exponential_blend(30.0, 0.0), 0.0);
    }

    #[test]
    fn blend_increases_with_time() {
        let short = exponential_blend(30.0, 0.01);
        let long = exponential_blend(30.0, 0.1);
        assert!(short > 0.0 && short < long && long < 1.0);
    }

    #[test]
    fn subdividing_time_preserves_convergence() {
        let half = exponential_blend(30.0, 0.01);
        let full = exponential_blend(30.0, 0.02);
        assert!((half + (1.0 - half) * half - full).abs() < 1e-6);
    }
}
