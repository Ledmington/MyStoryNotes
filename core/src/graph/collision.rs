//! Hard circle-overlap resolution, shared by the real-time simulation ([`super::simulation`]) and
//! the one-shot initial layout ([`super::layout`]) — see [`resolve_overlaps`]. Pure geometry: no
//! notion of notes, edges, velocity, or the manuscript note. A caller that needs something
//! excluded from collision (e.g. the manuscript note, which still occupies a slot in every
//! per-note array to stay aligned with [`crate::project::Project::notes`] but is never drawn)
//! passes a radius of `0.0` for it, which excludes it entirely, both as pusher and pushed.

use crate::math::{Pos2, Vec2};

/// Below this center-to-center distance, two circles are treated as coincident, needing a fixed
/// nudge direction rather than a well-defined one — mirrors `simulation::lj_force`'s identical
/// handling of the same case.
const COINCIDENT_EPSILON: f32 = 1e-4;

/// Pushes every pair of circles (`positions[i]` with radius `radii[i]`) that overlap apart along
/// their connecting axis until none do, or `iterations` relaxation passes have run, whichever
/// comes first. Each overlapping pair is split evenly: both circles move by half the overlap. A
/// zero (or negative) radius excludes that index from every check, both as pusher and pushed.
/// Returns whether any position moved at all, across every iteration.
pub(super) fn resolve_overlaps(positions: &mut [Pos2], radii: &[f32], iterations: usize) -> bool {
    let mut moved_at_all = false;

    for _ in 0..iterations {
        let mut moved = false;

        for i in 0..positions.len() {
            if radii[i] <= 0.0 {
                continue;
            }

            for j in (i + 1)..positions.len() {
                if radii[j] <= 0.0 {
                    continue;
                }

                let delta = positions[i] - positions[j];
                let min_dist = radii[i] + radii[j];
                let dist = delta.length();

                if dist >= min_dist {
                    continue;
                }

                moved = true;
                let direction = if dist > COINCIDENT_EPSILON {
                    delta / dist
                } else {
                    Vec2::new(1.0, 0.0)
                };
                let correction = direction * ((min_dist - dist) / 2.0);

                positions[i] += correction;
                positions[j] -= correction;
            }
        }

        moved_at_all |= moved;
        if !moved {
            break;
        }
    }

    moved_at_all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_overlaps_separates_two_overlapping_circles() {
        let mut positions = [Pos2::new(-10.0, 0.0), Pos2::new(10.0, 0.0)];
        let radii = [50.0, 50.0]; // sum 100, currently 20 apart

        let moved = resolve_overlaps(&mut positions, &radii, 10);

        assert!(moved);
        let distance = positions[0].distance(positions[1]);
        assert!(
            distance >= 100.0 - 0.01,
            "expected >= 100px apart, got {distance}"
        );
    }

    #[test]
    fn resolve_overlaps_leaves_already_separated_circles_untouched() {
        let mut positions = [Pos2::new(-100.0, 0.0), Pos2::new(100.0, 0.0)];
        let radii = [50.0, 50.0];

        let moved = resolve_overlaps(&mut positions, &radii, 10);

        assert!(!moved);
        assert_eq!(positions, [Pos2::new(-100.0, 0.0), Pos2::new(100.0, 0.0)]);
    }

    #[test]
    fn resolve_overlaps_splits_the_correction_evenly() {
        let mut positions = [Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0)];
        let radii = [50.0, 50.0];

        resolve_overlaps(&mut positions, &radii, 1);

        // Midpoint should be unchanged: both circles moved the same distance in opposite
        // directions.
        let midpoint_x = (positions[0].x + positions[1].x) / 2.0;
        assert!(
            (midpoint_x - 5.0).abs() < 0.01,
            "midpoint drifted: {midpoint_x}"
        );
    }

    #[test]
    fn resolve_overlaps_ignores_a_zero_radius_circle() {
        let mut positions = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0)];
        let radii = [0.0, 50.0];

        let moved = resolve_overlaps(&mut positions, &radii, 10);

        assert!(!moved);
        assert_eq!(positions, [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0)]);
    }

    #[test]
    fn resolve_overlaps_nudges_coincident_centers_apart_deterministically() {
        let mut positions = [Pos2::new(5.0, 5.0), Pos2::new(5.0, 5.0)];
        let radii = [20.0, 20.0];

        resolve_overlaps(&mut positions, &radii, 10);

        assert!(positions[0].distance(positions[1]) >= 40.0 - 0.01);
    }

    #[test]
    fn resolve_overlaps_converges_for_a_chain_of_three_mutually_overlapping_circles() {
        let mut positions = [
            Pos2::new(0.0, 0.0),
            Pos2::new(5.0, 0.0),
            Pos2::new(10.0, 0.0),
        ];
        let radii = [30.0, 30.0, 30.0];

        resolve_overlaps(&mut positions, &radii, 50);

        for i in 0..3 {
            for j in (i + 1)..3 {
                let distance = positions[i].distance(positions[j]);
                assert!(
                    distance >= radii[i] + radii[j] - 0.01,
                    "{i} and {j} still overlap: {distance}px apart"
                );
            }
        }
    }
}
