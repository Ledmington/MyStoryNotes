use super::{ConnectionId, Edge};
use crate::math::{Pos2, Vec2};
use crate::project::NoteId;

const TAU: f32 = std::f32::consts::TAU;

/// Lays out every note for a fresh graph: each connected component (see [`connected_components`]
/// — two notes belong to the same one if a link goes either way between them, matching how the
/// rest of the app treats "connected") gets its own circular cluster, each cluster's own node
/// order chosen by a deterministic shuffle refined to reduce edge crossings (see
/// [`place_component`]); the clusters' centers are then arranged around a shared circle sized so
/// they don't overlap (see [`meta_radius`]); and finally, notes with no links at all are placed
/// on an outer circle around everything else. Not guaranteed to find a crossing-free order for
/// any one cluster — one doesn't always exist — just a reasonable starting point for the
/// real-time simulation to refine. Fully deterministic: the same notes and links always produce
/// the same layout.
pub(super) fn initial_layout(node_count: usize, edges: &[Edge]) -> Vec<Pos2> {
    if node_count == 0 {
        return Vec::new();
    }
    // A lone node has nothing to be arranged relative to, and starting it off-center only makes
    // it visibly crawl toward the origin under the centering force every frame after.
    if node_count == 1 {
        return vec![Pos2::ZERO];
    }

    let mut positions = vec![Pos2::ZERO; node_count];

    let (clustered, isolated): (Vec<Vec<NoteId>>, Vec<Vec<NoteId>>) =
        connected_components(node_count, edges)
            .into_iter()
            .partition(|component| component.len() > 1);

    if clustered.is_empty() {
        // Nothing linked to anything: every note is its own "isolated" component, so just ring
        // them all around the origin, same as the graph always used to look before there was a
        // concept of separate clusters.
        place_ring(
            &isolated.into_iter().flatten().collect::<Vec<_>>(),
            Pos2::ZERO,
            component_radius(node_count),
            &mut positions,
        );
        return positions;
    }

    let cluster_radii: Vec<f32> = clustered
        .iter()
        .map(|component| component_radius(component.len()))
        .collect();
    let centers_radius = meta_radius(&cluster_radii);

    for (index, component) in clustered.iter().enumerate() {
        let angle = index as f32 / clustered.len() as f32 * TAU;
        let center = Pos2::ZERO + centers_radius * Vec2::new(angle.cos(), angle.sin());
        place_component(component, edges, center, &mut positions);
    }

    if !isolated.is_empty() {
        let isolated_nodes: Vec<NoteId> = isolated.into_iter().flatten().collect();
        let max_cluster_radius = cluster_radii.into_iter().fold(0.0f32, f32::max);
        let outer_radius =
            centers_radius + max_cluster_radius + component_radius(isolated_nodes.len());

        place_ring(&isolated_nodes, Pos2::ZERO, outer_radius, &mut positions);
    }

    positions
}

/// Every note's connected component: two notes are in the same one if there's a link between
/// them in either direction, directly or by way of other notes. A note with no links at all
/// forms a component of its own. Order within each component, and the order components are
/// returned in, is determined purely by [`NoteId`] (so, indirectly, by note name — see
/// [`crate::project::Project::notes`]'s sort order), making the whole thing deterministic.
fn connected_components(node_count: usize, edges: &[Edge]) -> Vec<Vec<NoteId>> {
    let adjacency = undirected_adjacency(node_count, edges);
    let mut visited = vec![false; node_count];
    let mut components = Vec::new();

    for start in (0..node_count).map(NoteId::from) {
        if visited[usize::from(start)] {
            continue;
        }

        let mut component = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start);
        visited[usize::from(start)] = true;

        while let Some(node) = queue.pop_front() {
            component.push(node);

            for &neighbor in &adjacency[usize::from(node)] {
                if !visited[usize::from(neighbor)] {
                    visited[usize::from(neighbor)] = true;
                    queue.push_back(neighbor);
                }
            }
        }

        components.push(component);
    }

    components
}

/// Adjacency lists treating every edge as undirected (a link either way makes two notes
/// "connected") and ignoring self-links, which don't connect a note to anything else.
fn undirected_adjacency(node_count: usize, edges: &[Edge]) -> Vec<Vec<NoteId>> {
    let mut adjacency: Vec<Vec<NoteId>> = vec![Vec::new(); node_count];

    for edge in edges {
        if edge.from != edge.to {
            adjacency[usize::from(edge.from)].push(edge.to);
            adjacency[usize::from(edge.to)].push(edge.from);
        }
    }

    adjacency
}

/// A radius scaled to how many notes need to fit around it without crowding — used both for one
/// cluster's own circle and (with `size` set to how many there are) the outer ring of isolated
/// notes.
fn component_radius(size: usize) -> f32 {
    120.0 + 20.0 * size as f32
}

/// A radius for the circle that clusters' own centers are placed on (see [`initial_layout`]),
/// picked so that no two clusters' circles overlap even in the worst case where every cluster
/// were as large as the biggest one — a simple, conservative bound rather than an exact
/// circle-packing solution. Zero for zero or one cluster, which need no separation from anything.
fn meta_radius(cluster_radii: &[f32]) -> f32 {
    if cluster_radii.len() <= 1 {
        return 0.0;
    }

    let max_radius = cluster_radii.iter().copied().fold(0.0f32, f32::max);
    let half_angle_between_adjacent_clusters = std::f32::consts::PI / cluster_radii.len() as f32;

    max_radius / half_angle_between_adjacent_clusters.sin()
}

/// Lays out one connected component's notes on a circle around `center`, in whichever order (see
/// [`shuffled`] and [`refine_order`]) minimizes how many of its own edges cross each other as
/// chords of that circle — components never share edges with each other, so this is entirely
/// independent per component.
fn place_component(component: &[NoteId], edges: &[Edge], center: Pos2, positions: &mut [Pos2]) {
    if component.len() == 1 {
        positions[usize::from(component[0])] = center;
        return;
    }

    let radius = component_radius(component.len());
    let local_edges = local_edges(component, edges);

    // Seeded by this component's own lowest note id, so different components don't all start
    // from the identical shuffle pattern relative to their own local indices, while staying
    // entirely deterministic.
    let seed = usize::from(component[0]) as u64;
    let starting_order = shuffled((0..component.len()).map(NoteId::from).collect(), seed);
    let order = refine_order(starting_order, &local_edges);

    for (slot, &local_node) in order.iter().enumerate() {
        let angle = slot as f32 / component.len() as f32 * TAU;
        let global_node = component[usize::from(local_node)];
        positions[usize::from(global_node)] = center + radius * Vec2::new(angle.cos(), angle.sin());
    }
}

/// Places `nodes` evenly around a circle of `radius` centered on `center` — used for the outer
/// ring of notes with no links at all, and as the whole-graph fallback when there are no links
/// anywhere.
fn place_ring(nodes: &[NoteId], center: Pos2, radius: f32, positions: &mut [Pos2]) {
    for (slot, &node) in nodes.iter().enumerate() {
        let angle = slot as f32 / nodes.len() as f32 * TAU;
        positions[usize::from(node)] = center + radius * Vec2::new(angle.cos(), angle.sin());
    }
}

/// `edges` restricted to `component` and renumbered to local indices (`component[i]` becomes
/// local id `i`), so [`refine_order`] and [`count_crossings`] — which only care about a
/// consistent `0..order.len()` numbering, not what it means globally — can be reused unchanged
/// for a single component's own subgraph. Filtering on `edge.from` alone is enough: every edge
/// touching this component touches only this component, since components are themselves built
/// from these same edges.
fn local_edges(component: &[NoteId], edges: &[Edge]) -> Vec<Edge> {
    let local_index_of = |global: NoteId| -> NoteId {
        NoteId::from(
            component
                .iter()
                .position(|&node| node == global)
                .expect("edge endpoint should belong to this component"),
        )
    };

    edges
        .iter()
        .filter(|edge| component.contains(&edge.from))
        .map(|edge| Edge {
            from: local_index_of(edge.from),
            to: local_index_of(edge.to),
        })
        .collect()
}

/// Refines `order` (a circular arrangement of `0..order.len()`) by a capped number of
/// pairwise-swap attempts — a 2-opt-style local search — each kept only if it strictly reduces
/// the total crossing count against `edges`.
fn refine_order(mut order: Vec<NoteId>, edges: &[Edge]) -> Vec<NoteId> {
    let node_count = order.len();

    if edges.is_empty() || node_count < 3 {
        return order;
    }

    const MAX_PASSES: usize = 20;
    const MAX_ATTEMPTS: usize = 4_000;

    let mut attempts_left = MAX_ATTEMPTS;
    let mut crossings = count_crossings(&order, edges);

    'passes: for _ in 0..MAX_PASSES {
        let mut improved = false;

        for a in 0..node_count {
            for b in (a + 1)..node_count {
                if attempts_left == 0 {
                    break 'passes;
                }
                attempts_left -= 1;

                order.swap(a, b);
                let candidate = count_crossings(&order, edges);

                if candidate < crossings {
                    crossings = candidate;
                    improved = true;
                } else {
                    order.swap(a, b);
                }
            }
        }

        if !improved || crossings == 0 {
            break;
        }
    }

    order
}

/// Counts how many pairs of edges cross as chords of a circle, given `order[slot] = node index`
/// at that circular position. Two edges with four distinct endpoints cross iff exactly one
/// endpoint of the second falls strictly between the first's two endpoints going around the
/// circle.
fn count_crossings(order: &[NoteId], edges: &[Edge]) -> usize {
    let mut slot_of = vec![0usize; order.len()];
    for (slot, &node) in order.iter().enumerate() {
        slot_of[usize::from(node)] = slot;
    }

    let mut crossings = 0;

    for i in (0..edges.len()).map(ConnectionId::from) {
        let (a, b) = (
            slot_of[usize::from(edges[usize::from(i)].from)],
            slot_of[usize::from(edges[usize::from(i)].to)],
        );
        if a == b {
            continue;
        }

        for edge in &edges[usize::from(i) + 1..] {
            let (c, d) = (
                slot_of[usize::from(edge.from)],
                slot_of[usize::from(edge.to)],
            );

            if c == d || c == a || c == b || d == a || d == b {
                continue;
            }

            if between(a, b, c) != between(a, b, d) {
                crossings += 1;
            }
        }
    }

    crossings
}

/// Whether slot `x` lies strictly between `a` and `b` going around the circle from `a` to `b` in
/// increasing-slot order (wrapping past the end).
fn between(a: usize, b: usize, x: usize) -> bool {
    if a < b {
        a < x && x < b
    } else {
        x > a || x < b
    }
}

/// A deterministic Fisher-Yates shuffle of `items`, seeded by `seed` (see [`Splitmix64`]) —
/// [`place_component`]'s starting point before [`refine_order`] untangles it, so a cluster starts
/// from an arbitrary-looking arrangement rather than one already organized by (and so biased
/// toward) a traversal order.
fn shuffled(mut items: Vec<NoteId>, seed: u64) -> Vec<NoteId> {
    let mut rng = Splitmix64(seed);

    for i in (1..items.len()).rev() {
        let j = rng.next_below(i + 1);
        items.swap(i, j);
    }

    items
}

/// A small, seedable pseudo-random generator (the SplitMix64 algorithm), used only to give each
/// connected component's initial node order (see [`shuffled`]) a fixed, reproducible "random"
/// starting point — deterministic so the same project always lays out identically, rather than
/// pulling in a general-purpose random-number crate for this one internal use.
struct Splitmix64(u64);

impl Splitmix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A pseudo-random value in `0..bound`.
    fn next_below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::NoteId;

    fn edge(from: usize, to: usize) -> Edge {
        Edge {
            from: NoteId::from(from),
            to: NoteId::from(to),
        }
    }

    #[test]
    fn initial_layout_places_a_lone_node_at_the_origin() {
        assert_eq!(initial_layout(1, &[]), vec![Pos2::ZERO]);
    }

    #[test]
    fn initial_layout_is_deterministic() {
        let edges = vec![edge(0, 1), edge(1, 2), edge(3, 4)];
        assert_eq!(initial_layout(6, &edges), initial_layout(6, &edges));
    }

    #[test]
    fn connected_components_groups_notes_linked_in_either_direction() {
        // 0 <-> 1 (both directions), 2 -> 3 (one direction only), 4 alone.
        let edges = vec![edge(0, 1), edge(1, 0), edge(2, 3)];

        let mut components = connected_components(5, &edges);
        for component in &mut components {
            component.sort();
        }
        components.sort_by_key(|component| component[0]);

        assert_eq!(
            components,
            vec![
                vec![NoteId::from(0), NoteId::from(1)],
                vec![NoteId::from(2), NoteId::from(3)],
                vec![NoteId::from(4)],
            ]
        );
    }

    #[test]
    fn isolated_notes_end_up_farther_from_the_center_than_clustered_ones() {
        // A pair of linked notes, plus a third with no links at all.
        let edges = vec![edge(0, 1)];

        let positions = initial_layout(3, &edges);

        let isolated_distance = positions[2].distance(Pos2::ZERO);
        let clustered_distance = positions[0]
            .distance(Pos2::ZERO)
            .max(positions[1].distance(Pos2::ZERO));

        assert!(
            isolated_distance > clustered_distance,
            "isolated note at {:?} should be farther from the center than the clustered pair \
             (distances {isolated_distance} vs {clustered_distance})",
            positions[2]
        );
    }

    #[test]
    fn two_clusters_do_not_overlap() {
        // Two separate pairs, each internally linked but with no link between the pairs.
        let edges = vec![edge(0, 1), edge(2, 3)];

        let positions = initial_layout(4, &edges);

        let midpoint = |a: Pos2, b: Pos2| Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let first_cluster_center = midpoint(positions[0], positions[1]);
        let second_cluster_center = midpoint(positions[2], positions[3]);

        let center_distance = first_cluster_center.distance(second_cluster_center);
        let combined_radii = component_radius(2) * 2.0;

        assert!(
            center_distance >= combined_radii,
            "clusters at {first_cluster_center:?} and {second_cluster_center:?} (distance \
             {center_distance}) should be at least {combined_radii} apart to not overlap"
        );
    }

    #[test]
    fn refine_order_finds_a_planar_cycle() {
        // A 5-cycle (0-2-4-1-3-0) described with edges out of index order, so the naive index
        // order would cross, but *some* circular order (the cycle order itself) never does.
        let edges = vec![edge(0, 2), edge(2, 4), edge(4, 1), edge(1, 3), edge(3, 0)];
        let order = (0..5).map(NoteId::from).collect();

        let refined = refine_order(order, &edges);

        assert_eq!(count_crossings(&refined, &edges), 0);
    }
}
