use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use egui::{Pos2, Vec2};

use super::layout::initial_layout;
use super::{Edge, resolve_edges};
use crate::project::{Note, NoteId, Project};
use crate::settings::SimulationSettings;

const MIN_DIST: f32 = 1.0;
const MAX_FORCE: f32 = 20_000.0;
const MAX_DT: f32 = 1.0 / 20.0;

/// Below this angle (radians) between two outgoing neighbors, [`add_angular_balance_forces`]
/// treats them as already maximally spread apart — a numerical-stability floor, not a
/// user-tunable "feel" parameter, so it isn't exposed in [`SimulationSettings`].
const MIN_ANGLE: f32 = 0.05;

/// Below this squared speed a node is considered settled.
const REST_VELOCITY_SQ: f32 = 4.0;

/// A node's physics state: where it is and how fast it's moving.
struct NodeState {
    pos: Pos2,
    vel: Vec2,
}

/// Persistent per-note physics state for the real-time graph view. Positions and velocities
/// survive across frames and are keyed by note name (stable across [`Project::create_note`]'s
/// re-sorting, unlike a note's index) rather than reset on every draw. Own one of these for as
/// long as the graph view should keep animating; call `Simulation::step` once per frame.
#[derive(Default)]
pub struct Simulation {
    nodes: HashMap<String, NodeState>,
}

impl Simulation {
    /// An empty simulation with no nodes yet; the first `Simulation::step` call seeds it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops entries for notes that no longer exist and adds entries for new ones. If every note
    /// turns out to be new (the very first call, or after switching to an entirely different
    /// project) all of them are seeded at once from [`initial_layout`]; otherwise each new note
    /// is placed near its already-positioned linked neighbors (or the graph's centroid, if it has
    /// none) so the rest of the graph doesn't jump.
    pub(super) fn sync(&mut self, notes: &[Note], edges: &[Edge]) {
        self.nodes
            .retain(|name, _| notes.iter().any(|note| &note.name == name));

        let new_ids: Vec<NoteId> = (0..notes.len())
            .map(NoteId::from)
            .filter(|&id| !self.nodes.contains_key(&notes[usize::from(id)].name))
            .collect();

        if new_ids.is_empty() {
            return;
        }

        if new_ids.len() == notes.len() {
            for (note, pos) in notes.iter().zip(initial_layout(notes.len(), edges)) {
                self.nodes.insert(
                    note.name.clone(),
                    NodeState {
                        pos,
                        vel: Vec2::ZERO,
                    },
                );
            }
            return;
        }

        let centroid = self.centroid(notes);

        for &id in &new_ids {
            let linked_neighbors: Vec<Pos2> = edges
                .iter()
                .filter(|edge| edge.from == id || edge.to == id)
                .filter_map(|edge| {
                    let other = if edge.from == id { edge.to } else { edge.from };
                    self.nodes
                        .get(&notes[usize::from(other)].name)
                        .map(|state| state.pos)
                })
                .collect();

            let base = if linked_neighbors.is_empty() {
                centroid
            } else {
                let sum = linked_neighbors
                    .iter()
                    .fold(Vec2::ZERO, |acc, pos| acc + pos.to_vec2());
                (sum / linked_neighbors.len() as f32).to_pos2()
            };

            let pos = base + deterministic_offset(&notes[usize::from(id)].name);
            self.nodes.insert(
                notes[usize::from(id)].name.clone(),
                NodeState {
                    pos,
                    vel: Vec2::ZERO,
                },
            );
        }
    }

    /// The average position of every currently-positioned note; the origin if there are none.
    fn centroid(&self, notes: &[Note]) -> Pos2 {
        let positioned: Vec<Pos2> = notes
            .iter()
            .filter_map(|note| self.nodes.get(&note.name))
            .map(|state| state.pos)
            .collect();

        if positioned.is_empty() {
            return Pos2::ZERO;
        }

        let sum = positioned
            .iter()
            .fold(Vec2::ZERO, |acc, pos| acc + pos.to_vec2());
        (sum / positioned.len() as f32).to_pos2()
    }

    /// Advances the simulation by `dt` seconds under the force model set out in [`lj_force`] and
    /// [`add_angular_balance_forces`] below, tuned by `settings`, and returns whether any node is
    /// still moving fast enough to be worth another repaint.
    pub(super) fn step(
        &mut self,
        notes: &[Note],
        edges: &[Edge],
        dt: f32,
        settings: &SimulationSettings,
    ) -> bool {
        let n = notes.len();

        if n == 0 {
            return false;
        }

        let dt = dt.min(MAX_DT);
        let positions: Vec<Pos2> = notes
            .iter()
            .map(|note| self.nodes[&note.name].pos)
            .collect();

        let connected: HashSet<(NoteId, NoteId)> = edges
            .iter()
            .filter(|edge| edge.from != edge.to)
            .map(|edge| (edge.from.min(edge.to), edge.from.max(edge.to)))
            .collect();

        let mut forces = vec![Vec2::ZERO; n];

        for i in (0..n).map(NoteId::from) {
            for j in ((usize::from(i) + 1)..n).map(NoteId::from) {
                let (r_eq, epsilon) = if connected.contains(&(i, j)) {
                    (settings.strong_distance, settings.strong_strength)
                } else {
                    (settings.weak_distance, settings.weak_strength)
                };

                let force = lj_force(
                    positions[usize::from(i)] - positions[usize::from(j)],
                    r_eq,
                    epsilon,
                );
                forces[usize::from(i)] += force;
                forces[usize::from(j)] -= force;
            }
        }

        add_angular_balance_forces(
            &positions,
            edges,
            &mut forces,
            settings.angular_repulsion,
            settings.strong_distance,
        );

        for (force, position) in forces.iter_mut().zip(&positions) {
            *force -= position.to_vec2() * settings.centering;
        }

        // Each individual force above is already capped, but a node with many close neighbors
        // (e.g. a hub with several outgoing edges) can still accumulate a huge *sum* of otherwise
        // reasonable contributions. Cap the total too, so one node's degree can never translate
        // into an unbounded single-frame kick.
        for force in &mut forces {
            let len = force.length();
            if len > MAX_FORCE {
                *force *= MAX_FORCE / len;
            }
        }

        let damping = (-settings.damping * dt).exp();
        let mut moving = false;

        for (i, note) in notes.iter().enumerate() {
            let state = self.nodes.get_mut(&note.name).expect("synced above");
            state.vel = (state.vel + forces[i] * dt) * damping;
            state.pos += state.vel * dt;

            if state.vel.length_sq() > REST_VELOCITY_SQ {
                moving = true;
            }
        }

        moving
    }

    /// Current positions in `notes` order. Every name in `notes` must already be present, i.e.
    /// this must be called after `Simulation::sync`.
    pub(super) fn positions(&self, notes: &[Note]) -> Vec<Pos2> {
        notes
            .iter()
            .map(|note| self.nodes[&note.name].pos)
            .collect()
    }
}

/// A Lennard-Jones-style force: repulsive when the two nodes (separated by `delta`, "self" minus
/// "other") are closer than `r_eq`, attractive when farther, and zero at exactly `r_eq` and at
/// long range. `epsilon` sets how deep that well is, i.e. how strongly the pair resists moving
/// away from `r_eq`. Returned as the force to apply to the "self" node.
fn lj_force(delta: Vec2, r_eq: f32, epsilon: f32) -> Vec2 {
    let len = delta.length();
    let r = len.max(MIN_DIST);
    // Coincident nodes have no well-defined direction; push along a fixed axis so they don't
    // stay stuck on top of each other. Real overlaps are transient since new nodes are placed
    // with a deterministic offset and forces run every frame.
    let direction = if len > 1e-4 {
        delta / len
    } else {
        Vec2::new(1.0, 0.0)
    };

    let sigma = r_eq / 2f32.powf(1.0 / 6.0);
    let sr6 = (sigma / r).powi(6);
    let sr12 = sr6 * sr6;
    let magnitude = ((24.0 * epsilon / r) * (2.0 * sr12 - sr6)).clamp(-MAX_FORCE, MAX_FORCE);

    direction * magnitude
}

/// Adds a tangential force to every pair of a node's *outgoing* neighbors that pushes them apart
/// around that node, so a note's outgoing links tend to fan out rather than bunch up in one
/// direction. The force grows as the angle between two neighbors shrinks and vanishes as it
/// approaches a straight line (they're already as spread out as a pair can be). `strength` scales
/// the force overall; `falloff_distance` is the distance beyond which it starts fading out (see
/// below) — callers pass [`SimulationSettings::angular_repulsion`] and
/// [`SimulationSettings::strong_distance`].
fn add_angular_balance_forces(
    positions: &[Pos2],
    edges: &[Edge],
    forces: &mut [Vec2],
    strength: f32,
    falloff_distance: f32,
) {
    let n = positions.len();
    let mut outgoing: Vec<Vec<NoteId>> = vec![Vec::new(); n];

    for edge in edges {
        if edge.from != edge.to {
            outgoing[usize::from(edge.from)].push(edge.to);
        }
    }

    for (center, neighbors) in outgoing.iter().enumerate() {
        let center = NoteId::from(center);

        // A node with `k` outgoing edges produces C(k, 2) pairs, and each neighbor sits in `k -
        // 1` of them; without normalizing by that, a highly-linked hub would push its whole
        // neighborhood out much harder than a hub with only two or three links, purely because it
        // has more pairs to sum, not because any single pair is more crowded.
        let pairs_per_neighbor = (neighbors.len() as f32 - 1.0).max(1.0);

        for a in 0..neighbors.len() {
            for b in (a + 1)..neighbors.len() {
                let (i, j) = (neighbors[a], neighbors[b]);

                let offset_i = positions[usize::from(i)] - positions[usize::from(center)];
                let offset_j = positions[usize::from(j)] - positions[usize::from(center)];
                let (len_i, len_j) = (offset_i.length(), offset_j.length());

                if len_i < MIN_DIST || len_j < MIN_DIST {
                    continue;
                }

                let dir_i = offset_i / len_i;
                let dir_j = offset_j / len_j;

                let theta = dir_i.dot(dir_j).clamp(-1.0, 1.0).acos();
                if theta >= std::f32::consts::PI - 1e-3 {
                    continue;
                }

                // Fades out as the neighbors get farther from `center`, same as the LJ forces
                // above: without this, the force stays at full strength no matter how large the
                // graph has already grown, which is enough on its own to pump in more energy than
                // damping can remove and make the whole graph expand without bound.
                let avg_len = (len_i + len_j) * 0.5;
                let falloff = (falloff_distance / avg_len).min(1.0);

                let raw_magnitude =
                    strength * (1.0 / theta.max(MIN_ANGLE) - 1.0 / std::f32::consts::PI);
                let magnitude =
                    (raw_magnitude * falloff / pairs_per_neighbor).clamp(0.0, MAX_FORCE);
                if magnitude <= 0.0 {
                    continue;
                }

                let cross = dir_i.x * dir_j.y - dir_i.y * dir_j.x;
                let sign = if cross >= 0.0 { 1.0 } else { -1.0 };

                let perp_i = Vec2::new(-dir_i.y, dir_i.x);
                let perp_j = Vec2::new(-dir_j.y, dir_j.x);

                forces[usize::from(i)] -= perp_i * sign * magnitude;
                forces[usize::from(j)] += perp_j * sign * magnitude;
            }
        }
    }
}

/// A small, deterministic-per-name offset (no RNG) for placing a newly added note: nudges it
/// away from its base position so notes added in the same sync land at slightly different spots
/// instead of exactly on top of each other.
fn deterministic_offset(name: &str) -> Vec2 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();

    let angle = (hash as f32 / u64::MAX as f32) * std::f32::consts::TAU;
    Vec2::angled(angle) * 40.0
}

/// Runs the force-directed layout for every note in `project`, from a fresh [`Simulation`], until
/// it settles (see `Simulation::step`) or a generous step budget elapses, and returns each
/// note's final world-space position in `project.notes` order. Exposed so the physics [`super::draw`]
/// relies on can be exercised in tests without a live `egui::Ui`.
pub fn settle(project: &Project, settings: &SimulationSettings) -> Vec<Pos2> {
    let edges = resolve_edges(project);
    let mut sim = Simulation::new();
    sim.sync(&project.notes, &edges);

    for _ in 0..1800 {
        if !sim.step(&project.notes, &edges, 1.0 / 60.0, settings) {
            break;
        }
    }

    sim.positions(&project.notes)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::project::{Note, NoteId};

    fn note(name: &str) -> Note {
        Note {
            name: name.to_owned(),
            source: String::new(),
        }
    }

    fn edge(from: usize, to: usize) -> Edge {
        Edge {
            from: NoteId::from(from),
            to: NoteId::from(to),
        }
    }

    #[test]
    fn sync_preserves_existing_positions_and_drops_removed_notes() {
        let mut sim = Simulation::new();
        let notes_ab = vec![note("A"), note("B")];
        sim.sync(&notes_ab, &[]);

        let pos_a = sim.nodes["A"].pos;

        let notes_abc = vec![note("A"), note("B"), note("C")];
        sim.sync(&notes_abc, &[]);

        assert_eq!(sim.nodes["A"].pos, pos_a);
        assert!(sim.nodes.contains_key("C"));

        let notes_ac = vec![note("A"), note("C")];
        sim.sync(&notes_ac, &[]);

        assert_eq!(sim.nodes["A"].pos, pos_a);
        assert!(!sim.nodes.contains_key("B"));
        assert!(sim.nodes.contains_key("C"));
    }

    #[test]
    fn lj_force_is_repulsive_below_and_attractive_above_equilibrium() {
        let settings = SimulationSettings::default();

        for (r_eq, epsilon) in [
            (settings.weak_distance, settings.weak_strength),
            (settings.strong_distance, settings.strong_strength),
        ] {
            let repulsive = lj_force(Vec2::new(r_eq * 0.5, 0.0), r_eq, epsilon);
            assert!(repulsive.x > 0.0, "expected repulsion below equilibrium");

            let at_equilibrium = lj_force(Vec2::new(r_eq, 0.0), r_eq, epsilon);
            assert!(
                at_equilibrium.x.abs() < 1e-3,
                "expected ~zero force at equilibrium"
            );

            let attractive = lj_force(Vec2::new(r_eq * 3.0, 0.0), r_eq, epsilon);
            assert!(attractive.x < 0.0, "expected attraction above equilibrium");
        }
    }

    /// Steps `sim` until it settles, then confirms it stays put for a further window — the core
    /// "does it converge" property every scenario below relies on. Returns the settled positions.
    fn assert_converges(sim: &mut Simulation, notes: &[Note], edges: &[Edge]) -> Vec<Pos2> {
        let settings = SimulationSettings::default();

        for _ in 0..1800 {
            sim.step(notes, edges, 1.0 / 60.0, &settings);
        }

        let before = sim.positions(notes);

        for _ in 0..600 {
            sim.step(notes, edges, 1.0 / 60.0, &settings);
        }

        let after = sim.positions(notes);

        let max_move = before
            .iter()
            .zip(&after)
            .map(|(a, b)| (*a - *b).length())
            .fold(0.0f32, f32::max);
        assert!(
            max_move < 5.0,
            "did not converge: moved {max_move}px in a further 10s after 30s of settling"
        );

        after
    }

    /// Whether segments `(a, b)` and `(c, d)` properly cross (share no endpoint and their
    /// interiors intersect), via the standard orientation test.
    fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
        fn orientation(p: Pos2, q: Pos2, r: Pos2) -> f32 {
            (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
        }

        let (d1, d2) = (orientation(c, d, a), orientation(c, d, b));
        let (d3, d4) = (orientation(a, b, c), orientation(a, b, d));

        (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
    }

    /// Counts pairs of edges (with no shared endpoint) whose line segments cross, given a set of
    /// settled positions.
    fn count_segment_crossings(positions: &[Pos2], edges: &[Edge]) -> usize {
        let mut crossings = 0;

        for i in 0..edges.len() {
            for edge in &edges[i + 1..] {
                let (e1, e2) = (&edges[i], edge);
                let shares_endpoint =
                    e1.from == e2.from || e1.from == e2.to || e1.to == e2.from || e1.to == e2.to;

                if !shares_endpoint
                    && segments_cross(
                        positions[usize::from(e1.from)],
                        positions[usize::from(e1.to)],
                        positions[usize::from(e2.from)],
                        positions[usize::from(e2.to)],
                    )
                {
                    crossings += 1;
                }
            }
        }

        crossings
    }

    #[test]
    fn two_connected_nodes_converge_close_together() {
        let notes = vec![note("A"), note("B")];
        let edges = vec![edge(0, 1)];

        let mut sim = Simulation::new();
        sim.sync(&notes, &edges);
        let positions = assert_converges(&mut sim, &notes, &edges);

        let separation = (positions[0] - positions[1]).length();
        assert!(
            separation.is_finite() && separation > MIN_DIST,
            "nodes collapsed onto each other: {separation}px apart"
        );
    }

    #[test]
    fn two_unconnected_nodes_converge_farther_apart_than_two_connected_ones() {
        let notes = vec![note("A"), note("B")];

        let mut connected_sim = Simulation::new();
        let connected_edges = vec![edge(0, 1)];
        connected_sim.sync(&notes, &connected_edges);
        let connected_positions = assert_converges(&mut connected_sim, &notes, &connected_edges);
        let connected_separation = (connected_positions[0] - connected_positions[1]).length();

        let mut unconnected_sim = Simulation::new();
        let unconnected_edges: Vec<Edge> = vec![];
        unconnected_sim.sync(&notes, &unconnected_edges);
        let unconnected_positions =
            assert_converges(&mut unconnected_sim, &notes, &unconnected_edges);
        let unconnected_separation = (unconnected_positions[0] - unconnected_positions[1]).length();

        assert!(
            unconnected_separation > connected_separation,
            "unconnected nodes ({unconnected_separation}px apart) should settle farther apart than connected ones ({connected_separation}px apart)"
        );
    }

    #[test]
    fn a_cycle_converges_with_no_crossings_and_connected_pairs_closer_on_average() {
        // A 6-node ring: every node is connected to exactly its two neighbors in the cycle, and
        // unconnected to the other three. A ring can always be drawn with no crossing edges, so
        // the settled layout should have none, and the six ring edges should average a shorter
        // distance than the nine non-edges.
        let notes: Vec<Note> = (0..6).map(|i| note(&format!("N{i}"))).collect();
        let edges: Vec<Edge> = (0..6).map(|i| edge(i, (i + 1) % 6)).collect();

        let mut sim = Simulation::new();
        sim.sync(&notes, &edges);
        let positions = assert_converges(&mut sim, &notes, &edges);

        assert_eq!(
            count_segment_crossings(&positions, &edges),
            0,
            "a ring should always be drawable with no crossing edges"
        );

        let connected: HashSet<(NoteId, NoteId)> = edges
            .iter()
            .map(|edge| (edge.from.min(edge.to), edge.from.max(edge.to)))
            .collect();

        let mut connected_total = 0.0;
        let mut unconnected_total = 0.0;
        let mut unconnected_count = 0;

        for i in 0..6 {
            for j in (i + 1)..6 {
                let distance = (positions[i] - positions[j]).length();
                if connected.contains(&(NoteId::from(i), NoteId::from(j))) {
                    connected_total += distance;
                } else {
                    unconnected_total += distance;
                    unconnected_count += 1;
                }
            }
        }

        let connected_avg = connected_total / edges.len() as f32;
        let unconnected_avg = unconnected_total / unconnected_count as f32;

        assert!(
            connected_avg < unconnected_avg,
            "connected pairs should average closer together: connected_avg={connected_avg}, unconnected_avg={unconnected_avg}"
        );
    }

    #[test]
    fn star_configuration_converges_without_exploding() {
        // A minimal, isolated reproduction of the real bug this simulation used to have: a hub
        // linked out to several leaves. The angular-balance force used to have no distance
        // falloff, clamp, or degree normalization, so this shape alone was enough to make the
        // whole graph expand forever instead of settling.
        let notes: Vec<Note> = (0..6).map(|i| note(&format!("N{i}"))).collect();
        let edges: Vec<Edge> = (1..6).map(|i| edge(0, i)).collect();

        let mut sim = Simulation::new();
        sim.sync(&notes, &edges);
        let positions = assert_converges(&mut sim, &notes, &edges);

        let hub = positions[0];
        let leaves = &positions[1..];

        let hub_leaf_avg: f32 = leaves
            .iter()
            .map(|leaf| (*leaf - hub).length())
            .sum::<f32>()
            / leaves.len() as f32;

        let mut leaf_leaf_total = 0.0;
        let mut leaf_leaf_count = 0;
        for i in 0..leaves.len() {
            for j in (i + 1)..leaves.len() {
                leaf_leaf_total += (leaves[i] - leaves[j]).length();
                leaf_leaf_count += 1;
            }
        }
        let leaf_leaf_avg = leaf_leaf_total / leaf_leaf_count as f32;

        assert!(
            hub_leaf_avg.is_finite() && hub_leaf_avg < 5000.0,
            "hub and leaves settled implausibly far apart: {hub_leaf_avg}px"
        );
        assert!(
            hub_leaf_avg < leaf_leaf_avg,
            "connected hub-leaf pairs should average closer together than unconnected leaf-leaf pairs: hub_leaf_avg={hub_leaf_avg}, leaf_leaf_avg={leaf_leaf_avg}"
        );
    }

    #[test]
    fn simulation_settles_for_the_example_project() {
        // Regression test for a real bug: the example project's "Overview" note links out to
        // eight other notes, and the angular-balance force used to have no distance falloff, no
        // magnitude clamp, and no normalization by how many neighbors it was spread across — so a
        // single well-linked hub note was enough to pump in more energy than damping could remove
        // and the whole graph expanded without ever settling.
        let project = crate::project::Project::open(std::path::PathBuf::from(
            "tests/fixtures/example_project.mystorynotes",
        ))
        .unwrap();
        let edges = resolve_edges(&project);

        let mut sim = Simulation::new();
        sim.sync(&project.notes, &edges);
        let settings = SimulationSettings::default();

        for _ in 0..1800 {
            sim.step(&project.notes, &edges, 1.0 / 60.0, &settings);
        }

        // The tangential angular-balance force doesn't necessarily settle to exactly zero net
        // torque, so a small residual drift (e.g. the whole graph slowly turning) is expected and
        // fine; it just shouldn't still be gaining speed after 30 seconds.
        let max_speed = project
            .notes
            .iter()
            .map(|n| sim.nodes[&n.name].vel.length())
            .fold(0.0f32, f32::max);
        assert!(
            max_speed < 100.0,
            "still moving unexpectedly fast after 30s of simulated time: {max_speed}px/s"
        );

        let positions = sim.positions(&project.notes);
        let centroid = positions
            .iter()
            .fold(Vec2::ZERO, |acc, p| acc + p.to_vec2())
            / positions.len() as f32;

        for pos in &positions {
            let distance = (*pos - centroid.to_pos2()).length();
            assert!(
                distance < 2000.0,
                "a node settled far from the rest of the graph: {distance}px away"
            );
        }
    }

    #[test]
    fn angular_balance_pushes_close_outgoing_neighbors_apart() {
        let center = Pos2::ZERO;
        let i_pos = Pos2::new(100.0 * 0.1_f32.cos(), 100.0 * 0.1_f32.sin());
        let j_pos = Pos2::new(100.0 * (-0.1_f32).cos(), 100.0 * (-0.1_f32).sin());
        let positions = [center, i_pos, j_pos];
        let edges = vec![edge(0, 1), edge(0, 2)];

        let settings = SimulationSettings::default();
        let mut forces = vec![Vec2::ZERO; 3];
        add_angular_balance_forces(
            &positions,
            &edges,
            &mut forces,
            settings.angular_repulsion,
            settings.strong_distance,
        );

        assert!(
            forces[1].y > 0.0,
            "neighbor above the axis should be pushed further up"
        );
        assert!(
            forces[2].y < 0.0,
            "neighbor below the axis should be pushed further down"
        );
    }
}
