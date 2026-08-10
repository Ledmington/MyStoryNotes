mod common;

use egui::{Pos2, Vec2};
use my_story_notes::graph;
use my_story_notes::settings::SimulationSettings;

#[test]
fn two_disconnected_cliques_settle_into_separate_tight_clusters() {
    // Two independent "blobs" of four notes each: every note within a blob links to every other
    // note in the same blob, but there are no links between the two blobs at all — see
    // `tests/fixtures/two_blobs_project.mystorynotes`. Each blob should settle as a tight
    // cluster, and the two blobs should settle clearly apart from each other rather than merging
    // into one cloud.
    let project = common::fixture("two_blobs_project.mystorynotes");
    let positions = graph::settle(&project, &SimulationSettings::default());

    let blob_a = &positions[0..4];
    let blob_b = &positions[4..8];

    let centroid_a = centroid(blob_a);
    let centroid_b = centroid(blob_b);
    let separation = (centroid_a - centroid_b).length();
    let spread_a = spread(blob_a, centroid_a);
    let spread_b = spread(blob_b, centroid_b);

    assert!(
        separation > spread_a.max(spread_b),
        "the two blobs should settle clearly apart, not merged into one cloud: \
         separation={separation}, spread_a={spread_a}, spread_b={spread_b}"
    );

    let within_blob_avg = (avg_pairwise(blob_a) + avg_pairwise(blob_b)) / 2.0;
    let across_blob_avg = avg_across(blob_a, blob_b);

    // A generous bound on the connected-pair equilibrium distance itself, not just its ratio to
    // the across-blob distance: a four-note clique can't settle every pairwise distance at
    // exactly R_STRONG (a planar clique's best case is a square, whose two diagonals are longer
    // than its four sides), but it also shouldn't settle many times farther out than that.
    assert!(
        within_blob_avg < 250.0,
        "notes within a tightly-linked blob should settle close together, not balloon outward: \
         within={within_blob_avg}"
    );
    assert!(
        across_blob_avg > within_blob_avg * 1.5,
        "notes within a blob should average noticeably closer together than notes across blobs, \
         not just marginally: within={within_blob_avg}, across={across_blob_avg}"
    );
}

#[test]
fn star_topology_keeps_hub_close_to_leaves_without_exploding() {
    // A single hub note linked out to six leaves, none of which link to each other or back to
    // the hub — see `tests/fixtures/star_project.mystorynotes`. Regression coverage (at the
    // public-API level) for a real bug where the angular-balance force had no distance falloff,
    // magnitude clamp, or normalization by how many neighbors it was spread across, so a
    // well-linked hub alone was enough to make the whole graph expand without ever settling.
    let project = common::fixture("star_project.mystorynotes");
    let positions = graph::settle(&project, &SimulationSettings::default());

    assert!(
        positions.iter().all(|p| p.x.is_finite() && p.y.is_finite()),
        "the star should settle to finite positions, not explode outward"
    );

    let hub = positions[0];
    let leaves = &positions[1..];

    let hub_leaf_avg: f32 = leaves
        .iter()
        .map(|leaf| (*leaf - hub).length())
        .sum::<f32>()
        / leaves.len() as f32;

    assert!(
        hub_leaf_avg < 5000.0,
        "hub and leaves settled implausibly far apart: {hub_leaf_avg}px"
    );

    let leaf_leaf_avg = avg_pairwise(leaves);

    assert!(
        hub_leaf_avg < leaf_leaf_avg,
        "connected hub-leaf pairs should average closer together than unconnected leaf-leaf \
         pairs: hub_leaf_avg={hub_leaf_avg}, leaf_leaf_avg={leaf_leaf_avg}"
    );
}

/// The average of a slice of points.
fn centroid(points: &[Pos2]) -> Pos2 {
    let sum = points.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2());
    (sum / points.len() as f32).to_pos2()
}

/// The average distance from `center` to each point in `points`.
fn spread(points: &[Pos2], center: Pos2) -> f32 {
    points.iter().map(|p| (*p - center).length()).sum::<f32>() / points.len() as f32
}

/// The average pairwise distance between every two distinct points in `points`.
fn avg_pairwise(points: &[Pos2]) -> f32 {
    let mut total = 0.0;
    let mut count = 0;

    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            total += (points[i] - points[j]).length();
            count += 1;
        }
    }

    total / count as f32
}

/// The average distance between every point in `a` and every point in `b`.
fn avg_across(a: &[Pos2], b: &[Pos2]) -> f32 {
    let total: f32 = a
        .iter()
        .flat_map(|p| b.iter().map(move |q| (*p - *q).length()))
        .sum();

    total / (a.len() * b.len()) as f32
}
