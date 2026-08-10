mod common;

use egui::{Pos2, Vec2};
use my_story_notes::graph;

#[test]
fn two_disconnected_cliques_settle_into_separate_tight_clusters() {
    // Two independent "blobs" of four notes each: every note within a blob links to every other
    // note in the same blob, but there are no links between the two blobs at all — see
    // `tests/fixtures/two_blobs_project.mystorynotes`. Each blob should settle as a tight
    // cluster, and the two blobs should settle clearly apart from each other rather than merging
    // into one cloud.
    let project = common::fixture("two_blobs_project.mystorynotes");
    let positions = graph::settle(&project);

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

    assert!(
        within_blob_avg < across_blob_avg,
        "notes within a blob should average closer together than notes across blobs: \
         within={within_blob_avg}, across={across_blob_avg}"
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
