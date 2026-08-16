use egui::{Color32, Pos2, Rect, Stroke, Vec2};

use my_story_notes_core::settings::GraphPattern;

use super::camera::{View, to_screen, to_world};

/// World-space spacing between a background pattern's lines, circles, wave rows, or dots. Fixed
/// rather than user-configurable, so the "Graph background" setting stays a single, simple choice
/// of pattern rather than growing a further set of tuning knobs.
const PATTERN_SPACING: f32 = 120.0;

/// On-screen radius of a dot in [`GraphPattern::SquareDots`] or [`GraphPattern::TriangularDots`],
/// in pixels — fixed regardless of zoom, same as every pattern's 1px line width.
const DOT_RADIUS: f32 = 1.5;

/// The straight-line geometry a background pattern reduces to: either a set of line segments
/// (every grid, [`GraphPattern::Rays`], and every curved pattern once tessellated into a
/// polyline) or a set of points (the dot patterns) — [`draw_background_pattern`] paints each
/// kind differently.
enum PatternGeometry {
    Lines(Vec<[Pos2; 2]>),
    Dots(Vec<Pos2>),
}

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

    match pattern_geometry(pattern, visible, view.zoom) {
        PatternGeometry::Lines(segments) => {
            for [a, b] in segments {
                painter.line_segment(
                    [
                        to_screen(canvas_rect, view, a),
                        to_screen(canvas_rect, view, b),
                    ],
                    Stroke::new(1.0, color),
                );
            }
        }
        PatternGeometry::Dots(points) => {
            for point in points {
                painter.circle_filled(to_screen(canvas_rect, view, point), DOT_RADIUS, color);
            }
        }
    }
}

/// World-space geometry for `pattern`, covering `visible` (with enough margin that nothing is
/// visibly clipped) — the pure geometry [`draw_background_pattern`] then projects to screen space
/// and paints. `zoom` (screen pixels per world unit) only affects the patterns approximated by a
/// polyline ([`GraphPattern::Spiral`], [`GraphPattern::ConcentricCircles`],
/// [`GraphPattern::WaveLines`]), fine enough to look smooth at the current zoom; every other
/// pattern is already made of dead-straight lines or fixed-size dots, which stay correct at any
/// zoom.
fn pattern_geometry(pattern: GraphPattern, visible: Rect, zoom: f32) -> PatternGeometry {
    use std::f32::consts::{FRAC_PI_3, PI};

    match pattern {
        GraphPattern::None => PatternGeometry::Lines(Vec::new()),
        GraphPattern::SquareGrid => PatternGeometry::Lines(
            [0.0, PI / 2.0]
                .into_iter()
                .flat_map(|angle| parallel_line_family(visible, angle, PATTERN_SPACING))
                .collect(),
        ),
        GraphPattern::TriangularGrid => PatternGeometry::Lines(
            [0.0, FRAC_PI_3, 2.0 * FRAC_PI_3]
                .into_iter()
                .flat_map(|angle| parallel_line_family(visible, angle, PATTERN_SPACING))
                .collect(),
        ),
        GraphPattern::HexagonalGrid => PatternGeometry::Lines(hex_grid_segments(visible)),
        GraphPattern::Rays => PatternGeometry::Lines(ray_segments(visible)),
        GraphPattern::Spiral => PatternGeometry::Lines(spiral_segments(visible, zoom)),
        GraphPattern::ConcentricCircles => {
            PatternGeometry::Lines(concentric_circle_segments(visible, zoom))
        }
        GraphPattern::WaveLines => PatternGeometry::Lines(wave_line_segments(visible, zoom)),
        GraphPattern::SquareDots => PatternGeometry::Dots(square_dot_grid(visible)),
        GraphPattern::TriangularDots => PatternGeometry::Dots(triangular_dot_grid(visible)),
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

/// Center-to-corner size of each hexagon in [`GraphPattern::HexagonalGrid`], chosen so adjacent
/// hexagon centers are [`PATTERN_SPACING`] apart horizontally — the same density as the two line
/// grids.
const HEX_SIZE: f32 = PATTERN_SPACING * 2.0 / 3.0;

/// The 6 edges of a flat-top hexagon of `size` (center-to-corner distance) centered on `center`.
fn hexagon_edges(center: Pos2, size: f32) -> [[Pos2; 2]; 6] {
    let vertex = |i: usize| center + Vec2::angled(i as f32 * std::f32::consts::FRAC_PI_3) * size;
    std::array::from_fn(|i| [vertex(i), vertex((i + 1) % 6)])
}

/// A honeycomb of flat-top [`HEX_SIZE`] hexagons (see [`hexagon_edges`]) tiling `visible`, laid
/// out on the standard offset hex grid: columns [`HEX_SIZE`] * 1.5 apart, alternating columns
/// staggered vertically by half a row. Shared edges between adjacent hexagons are emitted twice
/// (once per hexagon), which overdraws slightly but keeps this — and [`hexagon_edges`] — simple.
fn hex_grid_segments(visible: Rect) -> Vec<[Pos2; 2]> {
    let horiz_spacing = 1.5 * HEX_SIZE;
    let vert_spacing = 3f32.sqrt() * HEX_SIZE;
    let margin = HEX_SIZE;

    let min_col = ((visible.min.x - margin) / horiz_spacing).floor() as i32;
    let max_col = ((visible.max.x + margin) / horiz_spacing).ceil() as i32;

    (min_col..=max_col)
        .flat_map(|col| {
            let x = col as f32 * horiz_spacing;
            let row_offset = if col.rem_euclid(2) == 1 {
                vert_spacing / 2.0
            } else {
                0.0
            };

            let min_row = ((visible.min.y - margin - row_offset) / vert_spacing).floor() as i32;
            let max_row = ((visible.max.y + margin - row_offset) / vert_spacing).ceil() as i32;

            (min_row..=max_row).flat_map(move |row| {
                let y = row as f32 * vert_spacing + row_offset;
                hexagon_edges(Pos2::new(x, y), HEX_SIZE)
            })
        })
        .collect()
}

/// The farthest any corner of `visible` gets from the world origin — far enough that rays, a
/// spiral, or concentric circles centered on the origin always reach past every edge of
/// `visible`, however the camera has panned or zoomed it relative to the origin.
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

/// Target on-screen length, in pixels, for one polyline segment of any pattern approximating a
/// smooth curve ([`GraphPattern::Spiral`], [`GraphPattern::ConcentricCircles`],
/// [`GraphPattern::WaveLines`]) — small enough that the curve still reads as smooth rather than
/// faceted.
const CURVE_TARGET_SEGMENT_PIXELS: f32 = 6.0;

/// How many arms [`spiral_segments`] draws — evenly rotated around the origin, so 2 gives the
/// classic "double spiral" look rather than a single arm.
const SPIRAL_ARM_COUNT: usize = 2;

/// A double spiral: [`SPIRAL_ARM_COUNT`] Archimedean spiral arms (radius growing by
/// [`PATTERN_SPACING`] every full turn), all centered on the world origin and evenly rotated
/// around it, each approximated as a polyline out to the same radius [`ray_segments`] reaches —
/// see [`spiral_arm_segments`] for a single arm.
fn spiral_segments(visible: Rect, zoom: f32) -> Vec<[Pos2; 2]> {
    let max_radius = max_corner_distance_from_origin(visible);

    (0..SPIRAL_ARM_COUNT)
        .flat_map(|arm| {
            let phase = arm as f32 / SPIRAL_ARM_COUNT as f32 * std::f32::consts::TAU;
            spiral_arm_segments(max_radius, phase, zoom)
        })
        .collect()
}

/// Upper bound on [`spiral_arm_segments`]'s adaptive angular step — without it, the step implied
/// by [`CURVE_TARGET_SEGMENT_PIXELS`] would blow up near the origin (where the radius, and so the
/// implied step, approaches infinity), leaving only a handful of segments to draw the spiral's
/// tightest turns.
const SPIRAL_MAX_STEP: f32 = 0.2;

/// Hard cap on how many segments [`spiral_arm_segments`] will ever generate, regardless of how
/// small the target chord length implies the step should be — an extreme zoom-out on a very large
/// canvas could otherwise imply an unreasonable number of vanishingly short segments. At that
/// point the spiral is already far finer than the eye can resolve, so simply stopping short of
/// `max_radius` once the budget runs out is an invisible trade-off, not a visible truncation.
const SPIRAL_MAX_SEGMENTS_PER_ARM: usize = 3_000;

/// One arm of an Archimedean spiral (radius growing by [`PATTERN_SPACING`] every full turn),
/// starting at the world origin and rotated by `phase` radians, approximated as a polyline out to
/// `max_radius` — see [`CURVE_TARGET_SEGMENT_PIXELS`] for how finely.
fn spiral_arm_segments(max_radius: f32, phase: f32, zoom: f32) -> Vec<[Pos2; 2]> {
    let growth_per_radian = PATTERN_SPACING / std::f32::consts::TAU;
    let target_chord = CURVE_TARGET_SEGMENT_PIXELS / zoom;

    let mut segments = Vec::new();
    let mut theta = 0.0f32;
    let mut prev = Pos2::ZERO;

    while segments.len() < SPIRAL_MAX_SEGMENTS_PER_ARM {
        // A spiral's local curvature is dominated by its circumferential motion rather than its
        // slow radial growth, so the angular step needed for a chord of about `target_chord`
        // world units is approximately `target_chord / radius` — capped above by
        // `SPIRAL_MAX_STEP` alone (dividing by a radius of exactly zero, at the very center,
        // yields `f32::INFINITY`, which `f32::min` correctly clamps down to it).
        let radius_here = growth_per_radian * theta;
        let step = (target_chord / radius_here).min(SPIRAL_MAX_STEP);
        theta += step;

        let radius = growth_per_radian * theta;
        if radius > max_radius {
            break;
        }

        let point = Pos2::ZERO + Vec2::angled(theta + phase) * radius;
        segments.push([prev, point]);
        prev = point;
    }

    segments
}

/// Floor on how many segments approximate one [`GraphPattern::ConcentricCircles`] ring — without
/// it, a very small on-screen circle (a small radius, or a very zoomed-out view) could otherwise
/// be approximated by only a handful of segments and read as a visible polygon rather than a
/// circle.
const CIRCLE_MIN_SEGMENTS: usize = 12;

/// Concentric rings, [`PATTERN_SPACING`] apart, centered on the world origin, out to the farthest
/// corner of `visible` — each ring a regular polygon fine enough (see
/// [`CURVE_TARGET_SEGMENT_PIXELS`]) to read as a circle.
fn concentric_circle_segments(visible: Rect, zoom: f32) -> Vec<[Pos2; 2]> {
    let max_radius = max_corner_distance_from_origin(visible);
    let ring_count = (max_radius / PATTERN_SPACING).floor() as u32;

    (1..=ring_count)
        .flat_map(|ring| circle_segments(ring as f32 * PATTERN_SPACING, zoom))
        .collect()
}

/// A single ring of `radius`, centered on the world origin, approximated as a regular polygon —
/// see [`CURVE_TARGET_SEGMENT_PIXELS`] and [`CIRCLE_MIN_SEGMENTS`] for how finely.
fn circle_segments(radius: f32, zoom: f32) -> Vec<[Pos2; 2]> {
    let circumference_pixels = std::f32::consts::TAU * radius * zoom;
    let count = ((circumference_pixels / CURVE_TARGET_SEGMENT_PIXELS).round() as usize)
        .max(CIRCLE_MIN_SEGMENTS);

    (0..count)
        .map(|i| {
            let angle = |step: usize| step as f32 / count as f32 * std::f32::consts::TAU;
            [
                Pos2::ZERO + Vec2::angled(angle(i)) * radius,
                Pos2::ZERO + Vec2::angled(angle(i + 1)) * radius,
            ]
        })
        .collect()
}

/// Peak vertical displacement of [`GraphPattern::WaveLines`] from its row's baseline, world
/// units. Modest relative to [`PATTERN_SPACING`] (the distance between rows, reused as the wave's
/// wavelength too) so consecutive wave rows read as distinct, non-overlapping lines.
const WAVE_AMPLITUDE: f32 = 24.0;

/// Horizontal, sinusoidal lines [`PATTERN_SPACING`] apart, covering `visible` — each one a
/// polyline sampled finely enough (see [`CURVE_TARGET_SEGMENT_PIXELS`]) to look smooth.
fn wave_line_segments(visible: Rect, zoom: f32) -> Vec<[Pos2; 2]> {
    let margin = PATTERN_SPACING;
    let min_row = ((visible.min.y - margin) / PATTERN_SPACING).floor() as i32;
    let max_row = ((visible.max.y + margin) / PATTERN_SPACING).ceil() as i32;

    let start_x = visible.min.x - margin;
    let end_x = visible.max.x + margin;
    let step = CURVE_TARGET_SEGMENT_PIXELS / zoom;

    (min_row..=max_row)
        .flat_map(|row| wave_row_segments(row as f32 * PATTERN_SPACING, start_x, end_x, step))
        .collect()
}

/// One [`GraphPattern::WaveLines`] row, sinusoidal around `base_y`, sampled from `start_x` to
/// `end_x` every `step` world units.
fn wave_row_segments(base_y: f32, start_x: f32, end_x: f32, step: f32) -> Vec<[Pos2; 2]> {
    let point_at = |x: f32| {
        let offset = WAVE_AMPLITUDE * (x / PATTERN_SPACING * std::f32::consts::TAU).sin();
        Pos2::new(x, base_y + offset)
    };

    let sample_count = ((end_x - start_x) / step).ceil().max(1.0) as usize;

    let mut segments = Vec::with_capacity(sample_count);
    let mut prev = point_at(start_x);

    for i in 1..=sample_count {
        let x = (start_x + i as f32 * step).min(end_x);
        let point = point_at(x);
        segments.push([prev, point]);
        prev = point;
    }

    segments
}

/// Grid points [`PATTERN_SPACING`] apart in both directions, covering `visible`.
fn square_dot_grid(visible: Rect) -> Vec<Pos2> {
    let margin = PATTERN_SPACING;
    let min_col = ((visible.min.x - margin) / PATTERN_SPACING).floor() as i32;
    let max_col = ((visible.max.x + margin) / PATTERN_SPACING).ceil() as i32;
    let min_row = ((visible.min.y - margin) / PATTERN_SPACING).floor() as i32;
    let max_row = ((visible.max.y + margin) / PATTERN_SPACING).ceil() as i32;

    (min_row..=max_row)
        .flat_map(|row| {
            let y = row as f32 * PATTERN_SPACING;
            (min_col..=max_col).map(move |col| Pos2::new(col as f32 * PATTERN_SPACING, y))
        })
        .collect()
}

/// Grid points on a triangular lattice (every point equidistant, [`PATTERN_SPACING`] apart, from
/// its six neighbors), covering `visible` — rows [`PATTERN_SPACING`] * `sqrt(3)/2` apart, with
/// alternating rows offset horizontally by half [`PATTERN_SPACING`].
fn triangular_dot_grid(visible: Rect) -> Vec<Pos2> {
    let row_spacing = PATTERN_SPACING * 3f32.sqrt() / 2.0;
    let margin = PATTERN_SPACING;
    let min_row = ((visible.min.y - margin) / row_spacing).floor() as i32;
    let max_row = ((visible.max.y + margin) / row_spacing).ceil() as i32;

    (min_row..=max_row)
        .flat_map(|row| {
            let y = row as f32 * row_spacing;
            let x_offset = if row.rem_euclid(2) == 1 {
                PATTERN_SPACING / 2.0
            } else {
                0.0
            };

            let min_col = ((visible.min.x - margin - x_offset) / PATTERN_SPACING).floor() as i32;
            let max_col = ((visible.max.x + margin - x_offset) / PATTERN_SPACING).ceil() as i32;

            (min_col..=max_col)
                .map(move |col| Pos2::new(col as f32 * PATTERN_SPACING + x_offset, y))
        })
        .collect()
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
    fn hexagon_edges_are_all_the_same_length_and_share_endpoints() {
        let center = Pos2::new(3.0, -2.0);
        let size = 40.0;

        let edges = hexagon_edges(center, size);

        for &[a, b] in &edges {
            assert!(
                ((a - center).length() - size).abs() < 1e-3,
                "every vertex should be exactly `size` from the center"
            );
            assert!(
                ((b - a).length() - size).abs() < 1e-3,
                "a regular hexagon's edges should be as long as its own circumradius"
            );
        }

        for window in edges
            .iter()
            .chain(edges.first())
            .collect::<Vec<_>>()
            .windows(2)
        {
            assert_eq!(
                window[0][1], window[1][0],
                "consecutive edges should share an endpoint, forming a closed loop"
            );
        }
    }

    #[test]
    fn hex_grid_segments_covers_the_visible_rect() {
        let visible = Rect::from_min_max(Pos2::new(-50.0, -50.0), Pos2::new(250.0, 250.0));

        let segments = hex_grid_segments(visible);
        assert!(!segments.is_empty());

        // Every corner of `visible` should fall within (or very near) at least one hexagon,
        // i.e. no gaps in the tiling — approximated here by checking that some vertex of the
        // grid lies close to each corner's hexagon-sized neighborhood.
        for corner in [visible.left_top(), visible.right_bottom()] {
            let nearest = segments
                .iter()
                .flat_map(|&[a, b]| [a, b])
                .map(|point| (point - corner).length())
                .fold(f32::INFINITY, f32::min);
            assert!(
                nearest < HEX_SIZE * 2.0,
                "corner {corner:?} has no nearby hex grid geometry (nearest point {nearest}away)"
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

        let segments = spiral_arm_segments(max_radius, 0.0, 1.0);
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

        let segments = spiral_segments(visible, 1.0);
        let one_arm = spiral_arm_segments(max_radius, 0.0, 1.0);

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
    fn spiral_arm_segments_bounds_each_segments_on_screen_length_regardless_of_radius() {
        // Large enough that a fixed angular step would produce a visibly long, straight-looking
        // chord on the outer rings.
        let max_radius = 3000.0;
        let zoom = 1.0;

        let segments = spiral_arm_segments(max_radius, 0.0, zoom);

        for &[a, b] in &segments {
            let screen_chord_length = (b - a).length() * zoom;
            assert!(
                screen_chord_length <= CURVE_TARGET_SEGMENT_PIXELS * 1.5,
                "segment from {a:?} to {b:?} is {screen_chord_length}px on screen, \
                 wider than the {CURVE_TARGET_SEGMENT_PIXELS}px target"
            );
        }
    }

    #[test]
    fn spiral_arm_segments_uses_coarser_world_space_steps_when_zoomed_out() {
        let max_radius = 500.0;

        let zoomed_in = spiral_arm_segments(max_radius, 0.0, 4.0);
        let zoomed_out = spiral_arm_segments(max_radius, 0.0, 0.25);

        assert!(
            zoomed_out.len() < zoomed_in.len(),
            "a more zoomed-out view should need fewer, coarser world-space segments to look \
             just as smooth on screen: {} zoomed-out segments vs {} zoomed-in",
            zoomed_out.len(),
            zoomed_in.len()
        );
    }

    #[test]
    fn circle_segments_forms_a_closed_polygon_at_a_constant_radius() {
        let radius = 200.0;

        let segments = circle_segments(radius, 1.0);
        assert!(segments.len() >= CIRCLE_MIN_SEGMENTS);

        for &[a, b] in &segments {
            assert!((a.to_vec2().length() - radius).abs() < 1e-2);
            assert!((b.to_vec2().length() - radius).abs() < 1e-2);
        }

        for window in segments.windows(2) {
            assert_eq!(
                window[0][1], window[1][0],
                "consecutive segments should share an endpoint"
            );
        }
        assert!(
            (segments.last().unwrap()[1] - segments.first().unwrap()[0]).length() < 1e-3,
            "the ring should close on itself"
        );
    }

    #[test]
    fn circle_segments_never_drops_below_the_minimum_segment_count() {
        // A tiny radius and a very zoomed-out view both push the target chord count near zero.
        let segments = circle_segments(1.0, 0.1);
        assert!(segments.len() >= CIRCLE_MIN_SEGMENTS);
    }

    #[test]
    fn concentric_circle_segments_covers_rings_out_to_the_visible_corners() {
        let visible = Rect::from_min_max(Pos2::new(-50.0, -50.0), Pos2::new(250.0, 250.0));
        let max_radius = max_corner_distance_from_origin(visible);

        let segments = concentric_circle_segments(visible, 1.0);
        assert!(!segments.is_empty());

        let farthest = segments
            .iter()
            .flat_map(|&[a, b]| [a, b])
            .map(|point| point.to_vec2().length())
            .fold(0.0, f32::max);

        assert!(
            farthest >= max_radius - PATTERN_SPACING,
            "the outermost ring should reach close to the visible corners"
        );
    }

    #[test]
    fn wave_row_segments_stays_within_its_amplitude_of_the_baseline() {
        let base_y = 60.0;

        let segments = wave_row_segments(base_y, -100.0, 100.0, 5.0);
        assert!(!segments.is_empty());

        for &[a, b] in &segments {
            assert!((a.y - base_y).abs() <= WAVE_AMPLITUDE + 1e-3);
            assert!((b.y - base_y).abs() <= WAVE_AMPLITUDE + 1e-3);
        }

        for window in segments.windows(2) {
            assert_eq!(
                window[0][1], window[1][0],
                "consecutive segments should share an endpoint"
            );
        }
        assert_eq!(segments.first().unwrap()[0].x, -100.0);
        assert_eq!(segments.last().unwrap()[1].x, 100.0);
    }

    #[test]
    fn wave_line_segments_covers_every_row_across_the_visible_rect() {
        let visible = Rect::from_min_max(Pos2::new(-10.0, -10.0), Pos2::new(10.0, 250.0));

        let segments = wave_line_segments(visible, 1.0);
        assert!(!segments.is_empty());

        let min_x = segments
            .iter()
            .flat_map(|&[a, b]| [a.x, b.x])
            .fold(f32::INFINITY, f32::min);
        let max_x = segments
            .iter()
            .flat_map(|&[a, b]| [a.x, b.x])
            .fold(f32::NEG_INFINITY, f32::max);

        assert!(min_x <= visible.min.x);
        assert!(max_x >= visible.max.x);
    }

    #[test]
    fn square_dot_grid_covers_the_visible_rect_on_a_regular_lattice() {
        let visible = Rect::from_min_max(Pos2::new(-5.0, -5.0), Pos2::new(245.0, 245.0));

        let dots = square_dot_grid(visible);
        assert!(!dots.is_empty());

        for dot in &dots {
            assert!(
                (dot.x / PATTERN_SPACING).fract().abs() < 1e-3
                    || (dot.x / PATTERN_SPACING).fract().abs() > 1.0 - 1e-3
            );
            assert!(
                (dot.y / PATTERN_SPACING).fract().abs() < 1e-3
                    || (dot.y / PATTERN_SPACING).fract().abs() > 1.0 - 1e-3
            );
        }

        assert!(dots.iter().any(|dot| dot.x <= visible.min.x));
        assert!(dots.iter().any(|dot| dot.x >= visible.max.x));
        assert!(dots.iter().any(|dot| dot.y <= visible.min.y));
        assert!(dots.iter().any(|dot| dot.y >= visible.max.y));
    }

    #[test]
    fn triangular_dot_grid_every_point_is_pattern_spacing_from_its_nearest_neighbor() {
        let visible = Rect::from_min_max(Pos2::new(-5.0, -5.0), Pos2::new(245.0, 245.0));

        let dots = triangular_dot_grid(visible);
        assert!(!dots.is_empty());

        for &dot in &dots {
            let nearest = dots
                .iter()
                .filter(|&&other| other != dot)
                .map(|&other| (other - dot).length())
                .fold(f32::INFINITY, f32::min);

            assert!(
                (nearest - PATTERN_SPACING).abs() < 1e-2,
                "point {dot:?}'s nearest neighbor is {nearest} away, expected {PATTERN_SPACING}"
            );
        }
    }

    #[test]
    fn pattern_geometry_is_empty_only_for_none() {
        // Big enough that at least one ring of `GraphPattern::ConcentricCircles` (the first at
        // radius `PATTERN_SPACING`) actually falls within it, unlike every other pattern here
        // this one has nothing to draw in a visible rect entirely closer to the origin than its
        // first ring.
        let visible = Rect::from_min_max(Pos2::new(-150.0, -150.0), Pos2::new(150.0, 150.0));

        let is_empty = |pattern| match pattern_geometry(pattern, visible, 1.0) {
            PatternGeometry::Lines(segments) => segments.is_empty(),
            PatternGeometry::Dots(points) => points.is_empty(),
        };

        assert!(is_empty(GraphPattern::None));
        assert!(!is_empty(GraphPattern::SquareGrid));
        assert!(!is_empty(GraphPattern::TriangularGrid));
        assert!(!is_empty(GraphPattern::HexagonalGrid));
        assert!(!is_empty(GraphPattern::Rays));
        assert!(!is_empty(GraphPattern::Spiral));
        assert!(!is_empty(GraphPattern::ConcentricCircles));
        assert!(!is_empty(GraphPattern::WaveLines));
        assert!(!is_empty(GraphPattern::SquareDots));
        assert!(!is_empty(GraphPattern::TriangularDots));
    }
}
