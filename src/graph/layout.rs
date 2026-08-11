use egui::Pos2;

use super::{ConnectionId, Edge};
use crate::project::NoteId;

/// Places `node_count` nodes on a circle, in a circular order chosen by a bounded deterministic
/// local search to reduce how many edges (as chords of the circle) cross each other. Not
/// guaranteed to find a crossing-free order — one doesn't always exist — just a reasonable
/// starting point for the real-time simulation to refine.
pub(super) fn initial_layout(node_count: usize, edges: &[Edge]) -> Vec<Pos2> {
    if node_count == 0 {
        return Vec::new();
    }

    let order = crossing_minimized_order(node_count, edges);
    let radius = 120.0 + 20.0 * node_count as f32;

    let mut positions = vec![Pos2::ZERO; node_count];
    for (slot, &node) in order.iter().enumerate() {
        let angle = slot as f32 / node_count as f32 * std::f32::consts::TAU;
        positions[usize::from(node)] = Pos2::new(radius * angle.cos(), radius * angle.sin());
    }

    positions
}

/// A circular order of `0..node_count`, seeded with a BFS traversal (so linked notes start out
/// close together) and then refined by a capped number of pairwise-swap attempts, each kept only
/// if it strictly reduces the total crossing count.
fn crossing_minimized_order(node_count: usize, edges: &[Edge]) -> Vec<NoteId> {
    let mut order = bfs_order(node_count, edges);

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

/// A traversal order that tends to place linked notes near each other: BFS from node 0, then any
/// nodes unreachable from it appended the same way, breaking ties by index throughout.
fn bfs_order(node_count: usize, edges: &[Edge]) -> Vec<NoteId> {
    let mut adjacency: Vec<Vec<NoteId>> = vec![Vec::new(); node_count];
    for edge in edges {
        if edge.from != edge.to {
            adjacency[usize::from(edge.from)].push(edge.to);
            adjacency[usize::from(edge.to)].push(edge.from);
        }
    }

    let mut visited = vec![false; node_count];
    let mut order = Vec::with_capacity(node_count);

    for start in (0..node_count).map(NoteId::from) {
        if visited[usize::from(start)] {
            continue;
        }

        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start);
        visited[usize::from(start)] = true;

        while let Some(node) = queue.pop_front() {
            order.push(node);

            for &neighbor in &adjacency[usize::from(node)] {
                if !visited[usize::from(neighbor)] {
                    visited[usize::from(neighbor)] = true;
                    queue.push_back(neighbor);
                }
            }
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
    fn crossing_minimized_order_finds_a_planar_cycle() {
        // A 5-cycle (0-2-4-1-3-0) described with edges out of index order, so the naive index
        // order would cross, but *some* circular order (the cycle order itself) never does.
        let edges = vec![edge(0, 2), edge(2, 4), edge(4, 1), edge(1, 3), edge(3, 0)];

        let order = crossing_minimized_order(5, &edges);

        assert_eq!(count_crossings(&order, &edges), 0);
    }
}
