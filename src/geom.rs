//! Small floating-point geometry helpers shared by rasterisation and the app icon.

/// Return the shortest distance from a point to a line segment.
#[inline]
pub fn distance_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let vx = bx - ax;
    let vy = by - ay;
    let wx = px - ax;
    let wy = py - ay;
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= 1e-9 {
        0.0
    } else {
        ((wx * vx + wy * vy) / len2).clamp(0.0, 1.0)
    };
    let dx = wx - vx * t;
    let dy = wy - vy * t;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::distance_to_segment;

    #[test]
    fn distance_handles_projection_and_degenerate_segments() {
        assert!((distance_to_segment(2.0, 1.0, 0.0, 0.0, 4.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((distance_to_segment(2.0, 0.0, 0.0, 0.0, 0.0, 0.0) - 2.0).abs() < 1e-6);
    }
}
