use egui::{Color32, Pos2, Rect, Stroke, Vec2};

use my_story_notes_core::settings::GraphPattern;

use super::camera::{View, to_screen, to_world};

/// World-space spacing between a background pattern's lines or concentric turns. Fixed rather
/// than user-configurable, so the "Graph background" setting stays a single, simple choice of
/// pattern rather than growing a further set of tuning knobs.
const PATTERN_SPACING: f32 = 120.0;

/// Draws `pattern` (if not [`GraphPattern::None`]) behind everything else in the graph view, in
/// `color`. Computed fresh every frame from the currently visible world-space area, so it always
/// covers the canvas regardless of how the camera has panned or zoomed, and is otherwise purely
/// cosmetic — it doesn't feed into the simulation or hit-testing at all.
pub(super) fn draw_background_pattern(
    painter: &egui::Painter,
    canvas_rect: Rect,
    view: &View,
    pattern: GraphPattern,
    color: Color32,
) {
    if pattern == GraphPattern::None {
        return;
    }

    let visible = Rect::from_min_max(
        to_world(canvas_rect, view, canvas_rect.min),
        to_world(canvas_rect, view, canvas_rect.max),
    );

    for [a, b] in pattern_segments(pattern, visible) {
        painter.line_segment(
            [
                to_screen(canvas_rect, view, a),
                to_screen(canvas_rect, view, b),
            ],
            Stroke::new(1.0, color),
        );
    }
}

/// World-space line segments for `pattern`, covering `visible` (with enough margin that nothing
/// is visibly clipped) — the pure geometry [`draw_background_pattern`] then projects to screen
/// space and paints.
fn pattern_segments(pattern: GraphPattern, visible: Rect) -> Vec<[Pos2; 2]> {
    use std::f32::consts::{FRAC_PI_3, PI};

    match pattern {
        GraphPattern::None => Vec::new(),
        GraphPattern::SquareGrid => [0.0, PI / 2.0]
            .into_iter()
            .flat_map(|angle| parallel_line_family(visible, angle, PATTERN_SPACING))
            .collect(),
        GraphPattern::TriangularGrid => [0.0, FRAC_PI_3, 2.0 * FRAC_PI_3]
            .into_iter()
            .flat_map(|angle| parallel_line_family(visible, angle, PATTERN_SPACING))
            .collect(),
        GraphPattern::Rays => ray_segments(visible),
        GraphPattern::Spiral => spiral_segments(visible),
    }
}

/// A family of parallel lines at `angle` (radians from the x-axis), spaced `spacing` apart
/// (measured perpendicular to the lines), covering `visible`.
fn parallel_line_family(visible: Rect, angle: f32, spacing: f32) -> Vec<[Pos2; 2]> {
    let direction = Vec2::angled(angle);
    let normal = Vec2::new(-direction.y, direction.x);
    let center = visible.center();

    // Long enough that every line fully crosses `visible` regardless of angle.
    let half_length = visible.size().length();

    let corner_offsets = [
        visible.left_top(),
        visible.right_top(),
        visible.left_bottom(),
        visible.right_bottom(),
    ]
    .map(|corner| (corner - center).dot(normal));
    let min_offset = corner_offsets.into_iter().fold(f32::INFINITY, f32::min);
    let max_offset = corner_offsets.into_iter().fold(f32::NEG_INFINITY, f32::max);

    let first_line = (min_offset / spacing).floor() as i32;
    let last_line = (max_offset / spacing).ceil() as i32;

    (first_line..=last_line)
        .map(|i| {
            let base = center + normal * (i as f32 * spacing);
            [
                base - direction * half_length,
                base + direction * half_length,
            ]
        })
        .collect()
}

/// The farthest any corner of `visible` gets from the world origin — far enough that rays or a
/// spiral centered on the origin always reach past every edge of `visible`, however the camera
/// has panned or zoomed it relative to the origin.
fn max_corner_distance_from_origin(visible: Rect) -> f32 {
    [
        visible.left_top(),
        visible.right_top(),
        visible.left_bottom(),
        visible.right_bottom(),
    ]
    .iter()
    .map(|corner| corner.to_vec2().length())
    .fold(0.0, f32::max)
}

/// Evenly-spaced line segments radiating out from the world origin, far enough to cross every
/// edge of `visible`.
const RAY_COUNT: usize = 16;

fn ray_segments(visible: Rect) -> Vec<[Pos2; 2]> {
    let radius = max_corner_distance_from_origin(visible);

    (0..RAY_COUNT)
        .map(|i| {
            let angle = i as f32 / RAY_COUNT as f32 * std::f32::consts::TAU;
            [Pos2::ZERO, Pos2::ZERO + Vec2::angled(angle) * radius]
        })
        .collect()
}

/// How many arms [`spiral_segments`] draws — evenly rotated around the origin, so 2 gives the
/// classic "double spiral" look rather than a single arm.
const SPIRAL_ARM_COUNT: usize = 2;

/// A double spiral: [`SPIRAL_ARM_COUNT`] Archimedean spiral arms (radius growing by
/// [`PATTERN_SPACING`] every full turn), all centered on the world origin and evenly rotated
/// around it, each approximated as a polyline out to the same radius [`ray_segments`] reaches —
/// see [`spiral_arm_segments`] for a single arm.
fn spiral_segments(visible: Rect) -> Vec<[Pos2; 2]> {
    let max_radius = max_corner_distance_from_origin(visible);

    (0..SPIRAL_ARM_COUNT)
        .flat_map(|arm| {
            let phase = arm as f32 / SPIRAL_ARM_COUNT as f32 * std::f32::consts::TAU;
            spiral_arm_segments(max_radius, phase)
        })
        .collect()
}

/// One arm of an Archimedean spiral (radius growing by [`PATTERN_SPACING`] every full turn),
/// starting at the world origin and rotated by `phase` radians, approximated as a polyline out to
/// `max_radius`.
fn spiral_arm_segments(max_radius: f32, phase: f32) -> Vec<[Pos2; 2]> {
    /// Radians per polyline segment — small enough for the curve to look smooth.
    const STEP: f32 = 0.2;

    let growth_per_radian = PATTERN_SPACING / std::f32::consts::TAU;

    let mut segments = Vec::new();
    let mut theta = STEP;
    let mut prev = Pos2::ZERO;

    while growth_per_radian * theta <= max_radius {
        let point = Pos2::ZERO + Vec2::angled(theta + phase) * (growth_per_radian * theta);
        segments.push([prev, point]);
        prev = point;
        theta += STEP;
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_line_family_produces_horizontal_lines_covering_the_visible_rect() {
        let visible = Rect::from_min_max(Pos2::new(-5.0, 0.0), Pos2::new(5.0, 25.0));
        let spacing = 10.0;

        let segments = parallel_line_family(visible, 0.0, spacing);

        let mut ys: Vec<f32> = segments
            .iter()
            .map(|&[a, b]| {
                assert_eq!(a.y, b.y, "an angle-0 line should be horizontal");
                assert!(
                    (a.x - b.x).abs() >= visible.width(),
                    "each line should fully cross the visible rect's width"
                );
                a.y
            })
            .collect();
        ys.sort_by(f32::total_cmp);

        assert!(
            ys.first().is_some_and(|&y| y <= visible.min.y)
                && ys.last().is_some_and(|&y| y >= visible.max.y),
            "the lines should cover the visible rect's whole height: {ys:?}"
        );

        for pair in ys.windows(2) {
            assert!(
                (pair[1] - pair[0] - spacing).abs() < 1e-4,
                "consecutive lines should be exactly `spacing` apart: {ys:?}"
            );
        }
    }

    #[test]
    fn ray_segments_all_start_at_the_origin_and_reach_past_the_visible_corners() {
        let visible = Rect::from_min_max(Pos2::new(50.0, 50.0), Pos2::new(200.0, 300.0));

        let segments = ray_segments(visible);

        assert_eq!(segments.len(), RAY_COUNT);

        let max_corner_distance = max_corner_distance_from_origin(visible);
        for [start, end] in segments {
            assert_eq!(start, Pos2::ZERO);
            assert!(
                end.to_vec2().length() >= max_corner_distance - 1e-3,
                "a ray should reach at least as far as the farthest visible corner"
            );
        }
    }

    #[test]
    fn spiral_arm_segments_form_a_continuous_polyline_with_growing_radius() {
        let max_radius = 150.0;

        let segments = spiral_arm_segments(max_radius, 0.0);
        assert!(!segments.is_empty());

        assert_eq!(
            segments[0][0],
            Pos2::ZERO,
            "the spiral should start at the origin"
        );

        let mut last_radius = 0.0f32;
        for window in segments.windows(2) {
            assert_eq!(
                window[0][1], window[1][0],
                "consecutive segments should share an endpoint"
            );
        }
        for &[_, point] in &segments {
            let radius = point.to_vec2().length();
            assert!(
                radius > last_radius,
                "the spiral's radius should grow monotonically"
            );
            last_radius = radius;
        }

        assert!(
            last_radius <= max_radius,
            "the spiral shouldn't overshoot the requested radius"
        );
    }

    #[test]
    fn spiral_segments_draws_two_arms_offset_by_half_a_turn() {
        let visible = Rect::from_min_max(Pos2::new(-100.0, -100.0), Pos2::new(100.0, 100.0));
        let max_radius = max_corner_distance_from_origin(visible);

        let segments = spiral_segments(visible);
        let one_arm = spiral_arm_segments(max_radius, 0.0);

        assert_eq!(
            segments.len(),
            one_arm.len() * SPIRAL_ARM_COUNT,
            "each arm should contribute the same number of segments"
        );

        let (first_arm, second_arm) = segments.split_at(one_arm.len());

        // The second arm should trace the same radius profile as the first, just rotated by half
        // a turn (PI radians) around the origin.
        for (&[_, a], &[_, b]) in first_arm.iter().zip(second_arm) {
            assert!(
                (a.to_vec2().length() - b.to_vec2().length()).abs() < 1e-3,
                "both arms should reach the same radius at each step"
            );

            let angle_diff =
                (b.to_vec2().angle() - a.to_vec2().angle()).rem_euclid(std::f32::consts::TAU);
            assert!(
                (angle_diff - std::f32::consts::PI).abs() < 1e-3,
                "the second arm should be rotated exactly half a turn from the first: {angle_diff}"
            );
        }
    }

    #[test]
    fn pattern_segments_is_empty_only_for_none() {
        let visible = Rect::from_min_max(Pos2::new(-50.0, -50.0), Pos2::new(50.0, 50.0));

        assert!(pattern_segments(GraphPattern::None, visible).is_empty());
        assert!(!pattern_segments(GraphPattern::SquareGrid, visible).is_empty());
        assert!(!pattern_segments(GraphPattern::TriangularGrid, visible).is_empty());
        assert!(!pattern_segments(GraphPattern::Rays, visible).is_empty());
        assert!(!pattern_segments(GraphPattern::Spiral, visible).is_empty());
    }
}
