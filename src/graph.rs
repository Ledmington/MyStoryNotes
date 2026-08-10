use std::collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use eframe::egui;
use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2};

use crate::markdown;
use crate::project::{Note, Project};
use crate::settings::{self, UiPalette};

/// A markdown link from one note to another, resolved to indices into [`Project::notes`].
struct Edge {
    from: usize,
    to: usize,
}

/// Resolves every note's markdown links into [`Edge`]s indexing into `project.notes`.
fn resolve_edges(project: &Project) -> Vec<Edge> {
    project
        .notes
        .iter()
        .enumerate()
        .flat_map(|(from, note)| {
            markdown::extract_links(&note.source)
                .into_iter()
                .filter_map(move |target| project.notes.iter().position(|note| note.name == target))
                .map(move |to| Edge { from, to })
        })
        .collect()
}

/// A node's physics state: where it is and how fast it's moving.
struct NodeState {
    pos: Pos2,
    vel: Vec2,
}

/// Persistent per-note physics state for the real-time graph view. Positions and velocities
/// survive across frames and are keyed by note name (stable across [`Project::create_note`]'s
/// re-sorting, unlike a note's index) rather than reset on every draw. Own one of these for as
/// long as the graph view should keep animating; call [`Simulation::step`] once per frame.
#[derive(Default)]
pub struct Simulation {
    nodes: HashMap<String, NodeState>,
}

impl Simulation {
    /// An empty simulation with no nodes yet; the first [`Simulation::step`] call seeds it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops entries for notes that no longer exist and adds entries for new ones. If every note
    /// turns out to be new (the very first call, or after switching to an entirely different
    /// project) all of them are seeded at once from [`initial_layout`]; otherwise each new note
    /// is placed near its already-positioned linked neighbors (or the graph's centroid, if it has
    /// none) so the rest of the graph doesn't jump.
    fn sync(&mut self, notes: &[Note], edges: &[Edge]) {
        self.nodes
            .retain(|name, _| notes.iter().any(|note| &note.name == name));

        let new_indices: Vec<usize> = (0..notes.len())
            .filter(|&i| !self.nodes.contains_key(&notes[i].name))
            .collect();

        if new_indices.is_empty() {
            return;
        }

        if new_indices.len() == notes.len() {
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

        for &i in &new_indices {
            let linked_neighbors: Vec<Pos2> = edges
                .iter()
                .filter(|edge| edge.from == i || edge.to == i)
                .filter_map(|edge| {
                    let other = if edge.from == i { edge.to } else { edge.from };
                    self.nodes.get(&notes[other].name).map(|state| state.pos)
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

            let pos = base + deterministic_offset(&notes[i].name);
            self.nodes.insert(
                notes[i].name.clone(),
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

    /// Advances the simulation by `dt` seconds under the force model (see module docs) and
    /// returns whether any node is still moving fast enough to be worth another repaint.
    fn step(&mut self, notes: &[Note], edges: &[Edge], dt: f32) -> bool {
        let n = notes.len();

        if n == 0 {
            return false;
        }

        let dt = dt.min(MAX_DT);
        let positions: Vec<Pos2> = notes
            .iter()
            .map(|note| self.nodes[&note.name].pos)
            .collect();

        let connected: HashSet<(usize, usize)> = edges
            .iter()
            .filter(|edge| edge.from != edge.to)
            .map(|edge| (edge.from.min(edge.to), edge.from.max(edge.to)))
            .collect();

        let mut forces = vec![Vec2::ZERO; n];

        for i in 0..n {
            for j in (i + 1)..n {
                let (r_eq, epsilon) = if connected.contains(&(i, j)) {
                    (R_STRONG, EPS_STRONG)
                } else {
                    (R_WEAK, EPS_WEAK)
                };

                let force = lj_force(positions[i] - positions[j], r_eq, epsilon);
                forces[i] += force;
                forces[j] -= force;
            }
        }

        add_angular_balance_forces(&positions, edges, &mut forces);

        for (force, position) in forces.iter_mut().zip(&positions) {
            *force -= position.to_vec2() * CENTERING;
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

        let damping = (-DAMPING_RATE * dt).exp();
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
    /// this must be called after [`Simulation::sync`].
    fn positions(&self, notes: &[Note]) -> Vec<Pos2> {
        notes
            .iter()
            .map(|note| self.nodes[&note.name].pos)
            .collect()
    }
}

const MIN_DIST: f32 = 1.0;
const MAX_FORCE: f32 = 20_000.0;
const MAX_DT: f32 = 1.0 / 20.0;

/// Equilibrium distance and well depth for two notes with no link between them: a long distance
/// and a shallow well, just enough to stop disconnected nodes drifting apart forever.
const R_WEAK: f32 = 300.0;
const EPS_WEAK: f32 = 600.0;

/// Equilibrium distance and well depth for two linked notes: noticeably closer and a much deeper
/// well, so connected notes visibly cluster together.
const R_STRONG: f32 = 140.0;
const EPS_STRONG: f32 = 6_000.0;

/// How strongly a node's outgoing edges push each other apart angularly.
const ANGULAR_REPULSION: f32 = 4_000.0;
const MIN_ANGLE: f32 = 0.05;

/// Fraction of velocity lost per second, applied as `exp(-DAMPING_RATE * dt)`.
const DAMPING_RATE: f32 = 3.0;

/// Weak pull toward the origin so the whole graph doesn't slowly drift off-canvas; the
/// Lennard-Jones forces above are all relative/pairwise and have no absolute anchor.
const CENTERING: f32 = 0.4;

/// Below this squared speed a node is considered settled.
const REST_VELOCITY_SQ: f32 = 4.0;

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
/// approaches a straight line (they're already as spread out as a pair can be).
fn add_angular_balance_forces(positions: &[Pos2], edges: &[Edge], forces: &mut [Vec2]) {
    let n = positions.len();
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];

    for edge in edges {
        if edge.from != edge.to {
            outgoing[edge.from].push(edge.to);
        }
    }

    for (center, neighbors) in outgoing.iter().enumerate() {
        // A node with `k` outgoing edges produces C(k, 2) pairs, and each neighbor sits in `k -
        // 1` of them; without normalizing by that, a highly-linked hub would push its whole
        // neighborhood out much harder than a hub with only two or three links, purely because it
        // has more pairs to sum, not because any single pair is more crowded.
        let pairs_per_neighbor = (neighbors.len() as f32 - 1.0).max(1.0);

        for a in 0..neighbors.len() {
            for b in (a + 1)..neighbors.len() {
                let (i, j) = (neighbors[a], neighbors[b]);

                let offset_i = positions[i] - positions[center];
                let offset_j = positions[j] - positions[center];
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
                let falloff = (R_STRONG / avg_len).min(1.0);

                let raw_magnitude =
                    ANGULAR_REPULSION * (1.0 / theta.max(MIN_ANGLE) - 1.0 / std::f32::consts::PI);
                let magnitude =
                    (raw_magnitude * falloff / pairs_per_neighbor).clamp(0.0, MAX_FORCE);
                if magnitude <= 0.0 {
                    continue;
                }

                let cross = dir_i.x * dir_j.y - dir_i.y * dir_j.x;
                let sign = if cross >= 0.0 { 1.0 } else { -1.0 };

                let perp_i = Vec2::new(-dir_i.y, dir_i.x);
                let perp_j = Vec2::new(-dir_j.y, dir_j.x);

                forces[i] -= perp_i * sign * magnitude;
                forces[j] += perp_j * sign * magnitude;
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

/// Places `node_count` nodes on a circle, in a circular order chosen by a bounded deterministic
/// local search to reduce how many edges (as chords of the circle) cross each other. Not
/// guaranteed to find a crossing-free order — one doesn't always exist — just a reasonable
/// starting point for the real-time simulation above to refine.
fn initial_layout(node_count: usize, edges: &[Edge]) -> Vec<Pos2> {
    if node_count == 0 {
        return Vec::new();
    }

    let order = crossing_minimized_order(node_count, edges);
    let radius = 120.0 + 20.0 * node_count as f32;

    let mut positions = vec![Pos2::ZERO; node_count];
    for (slot, &node) in order.iter().enumerate() {
        let angle = slot as f32 / node_count as f32 * std::f32::consts::TAU;
        positions[node] = Pos2::new(radius * angle.cos(), radius * angle.sin());
    }

    positions
}

/// A circular order of `0..node_count`, seeded with a BFS traversal (so linked notes start out
/// close together) and then refined by a capped number of pairwise-swap attempts, each kept only
/// if it strictly reduces the total crossing count.
fn crossing_minimized_order(node_count: usize, edges: &[Edge]) -> Vec<usize> {
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
fn bfs_order(node_count: usize, edges: &[Edge]) -> Vec<usize> {
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for edge in edges {
        if edge.from != edge.to {
            adjacency[edge.from].push(edge.to);
            adjacency[edge.to].push(edge.from);
        }
    }

    let mut visited = vec![false; node_count];
    let mut order = Vec::with_capacity(node_count);

    for start in 0..node_count {
        if visited[start] {
            continue;
        }

        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited[start] = true;

        while let Some(node) = queue.pop_front() {
            order.push(node);

            for &neighbor in &adjacency[node] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
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
fn count_crossings(order: &[usize], edges: &[Edge]) -> usize {
    let mut slot_of = vec![0usize; order.len()];
    for (slot, &node) in order.iter().enumerate() {
        slot_of[node] = slot;
    }

    let mut crossings = 0;

    for i in 0..edges.len() {
        let (a, b) = (slot_of[edges[i].from], slot_of[edges[i].to]);
        if a == b {
            continue;
        }

        for edge in &edges[i + 1..] {
            let (c, d) = (slot_of[edge.from], slot_of[edge.to]);

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

/// Pushes apart any node rectangles the simulation left overlapping. [`Simulation::step`] only
/// reasons about points, not each note's actual label size, so long note names in a dense cluster
/// can still end up overlapping; this guarantees every label stays readable regardless. Purely
/// cosmetic and recomputed fresh every frame — it doesn't feed back into the simulation.
fn declutter(rects: &mut [Rect]) {
    const GAP: f32 = 12.0;
    const ITERATIONS: usize = 300;

    for _ in 0..ITERATIONS {
        let mut moved = false;

        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let overlap = rects[i]
                    .expand(GAP / 2.0)
                    .intersect(rects[j].expand(GAP / 2.0));

                if overlap.width() <= 0.0 || overlap.height() <= 0.0 {
                    continue;
                }

                moved = true;

                let delta = rects[i].center() - rects[j].center();
                let push = if overlap.width() < overlap.height() {
                    Vec2::new(overlap.width() / 2.0 * delta.x.signum(), 0.0)
                } else {
                    Vec2::new(0.0, overlap.height() / 2.0 * delta.y.signum())
                };
                // Centers can coincide exactly (signum gives 0); nudge deterministically instead
                // of leaving the pair stuck on top of each other.
                let push = if push == Vec2::ZERO {
                    Vec2::new(overlap.width().min(overlap.height()) / 2.0, 0.0)
                } else {
                    push
                };

                rects[i] = rects[i].translate(push);
                rects[j] = rects[j].translate(-push);
            }
        }

        if !moved {
            break;
        }
    }
}

/// Draws the whole project as a graph: one rectangle per note, one line per markdown link
/// between notes. `open_note`'s outgoing links (and the node itself) are highlighted in the
/// palette's accent color. `sim` carries the real-time physics state across frames — pass the
/// same instance every call. Returns the index of a note the user clicked this frame, if any.
pub fn draw(
    ui: &mut Ui,
    project: &Project,
    open_note: Option<usize>,
    palette: &UiPalette,
    sim: &mut Simulation,
) -> Option<usize> {
    let edges = resolve_edges(project);
    sim.sync(&project.notes, &edges);

    if project.notes.is_empty() {
        ui.centered_and_justified(|ui| ui.label("No notes yet."));
        return None;
    }

    let dt = ui.ctx().input(|input| input.stable_dt);
    if sim.step(&project.notes, &edges, dt) {
        ui.ctx().request_repaint();
    }
    let positions = sim.positions(&project.notes);

    let text_color = settings::rgb(palette.text);
    let node_fill = mix(settings::rgb(palette.panel_background), text_color, 0.12);
    let edge_color = mix(settings::rgb(palette.panel_background), text_color, 0.4);
    let accent = settings::rgb(palette.accent);
    let font_id = TextStyle::Body.resolve(ui.style());

    let mut clicked = None;

    egui::ScrollArea::both().show(ui, |ui| {
        let galleys: Vec<_> = project
            .notes
            .iter()
            .map(|note| {
                ui.painter()
                    .layout_no_wrap(note.name.clone(), font_id.clone(), text_color)
            })
            .collect();

        let mut rects: Vec<Rect> = galleys
            .iter()
            .zip(&positions)
            .map(|(galley, &center)| {
                Rect::from_center_size(center, galley.size() + Vec2::new(24.0, 16.0))
            })
            .collect();

        declutter(&mut rects);

        let bounds = rects
            .iter()
            .fold(Rect::NOTHING, |acc, rect| acc.union(*rect));
        let margin = Vec2::splat(24.0);
        let canvas_size = bounds.size() + margin * 2.0;

        let (response, painter) = ui.allocate_painter(canvas_size, Sense::hover());
        let offset = response.rect.min - bounds.min + margin;

        for edge in &edges {
            let highlighted = open_note == Some(edge.from);
            let color = if highlighted { accent } else { edge_color };
            let width = if highlighted { 2.5 } else { 1.0 };

            painter.line_segment(
                [
                    rects[edge.from].center() + offset,
                    rects[edge.to].center() + offset,
                ],
                Stroke::new(width, color),
            );
        }

        for (index, (rect, galley)) in rects.iter().zip(&galleys).enumerate() {
            let screen_rect = rect.translate(offset);
            let is_open = open_note == Some(index);
            let border = if is_open { accent } else { edge_color };

            painter.rect(
                screen_rect,
                4.0,
                node_fill,
                Stroke::new(if is_open { 2.0 } else { 1.0 }, border),
                StrokeKind::Outside,
            );
            painter.galley(
                screen_rect.center() - galley.size() / 2.0,
                galley.clone(),
                text_color,
            );

            let response = ui
                .interact(screen_rect, Id::new(("graph-node", index)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);

            if response.clicked() {
                clicked = Some(index);
            }
        }
    });

    clicked
}

/// Blends two colors channel-wise; `t` of 0.0 is `a`, 1.0 is `b`.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(name: &str) -> Note {
        Note {
            name: name.to_owned(),
            source: String::new(),
        }
    }

    #[test]
    fn crossing_minimized_order_finds_a_planar_cycle() {
        // A 5-cycle (0-2-4-1-3-0) described with edges out of index order, so the naive index
        // order would cross, but *some* circular order (the cycle order itself) never does.
        let edges = vec![
            Edge { from: 0, to: 2 },
            Edge { from: 2, to: 4 },
            Edge { from: 4, to: 1 },
            Edge { from: 1, to: 3 },
            Edge { from: 3, to: 0 },
        ];

        let order = crossing_minimized_order(5, &edges);

        assert_eq!(count_crossings(&order, &edges), 0);
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
        for (r_eq, epsilon) in [(R_WEAK, EPS_WEAK), (R_STRONG, EPS_STRONG)] {
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

    #[test]
    fn simulation_settles_for_the_example_project() {
        // Regression test for a real bug: the example project's "Overview" note links out to
        // eight other notes, and the angular-balance force used to have no distance falloff, no
        // magnitude clamp, and no normalization by how many neighbors it was spread across — so a
        // single well-linked hub note was enough to pump in more energy than damping could remove
        // and the whole graph expanded without ever settling.
        let project =
            crate::project::Project::open(std::path::PathBuf::from("example-project.mystorynotes"))
                .unwrap();
        let edges = resolve_edges(&project);

        let mut sim = Simulation::new();
        sim.sync(&project.notes, &edges);

        for _ in 0..1800 {
            sim.step(&project.notes, &edges, 1.0 / 60.0);
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
        let edges = vec![Edge { from: 0, to: 1 }, Edge { from: 0, to: 2 }];

        let mut forces = vec![Vec2::ZERO; 3];
        add_angular_balance_forces(&positions, &edges, &mut forces);

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
