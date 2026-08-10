use std::collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use eframe::egui;
use egui::{Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2};

use crate::fonts::{self, icon};
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

/// The graph view's camera: `center` is the world-space point shown at the middle of the canvas,
/// `zoom` is the world-to-screen scale factor (screen pixels per world unit). Persists across
/// frames like [`Simulation`] does, so panning and zooming don't reset on every draw; own one for
/// as long as the graph view should remember its camera and pass the same instance every call to
/// [`draw`].
pub struct View {
    center: Pos2,
    zoom: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            center: Pos2::ZERO,
            zoom: 1.0,
        }
    }
}

impl View {
    /// A fresh camera centered on the origin at 1:1 zoom, matching where [`initial_layout`]
    /// places new graphs.
    pub fn new() -> Self {
        Self::default()
    }
}

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 4.0;

/// Multiplicative zoom change per "notch" of scroll-wheel input.
const ZOOM_WHEEL_SENSITIVITY: f32 = 0.0015;

/// Multiplicative zoom change applied by a single click of the +/- zoom buttons.
const ZOOM_BUTTON_STEP: f32 = 1.25;

/// Screen pixels the pan buttons move the camera by per click, before dividing by zoom.
const PAN_BUTTON_STEP: f32 = 80.0;

const MIN_DIST: f32 = 1.0;
const MAX_FORCE: f32 = 20_000.0;
const MAX_DT: f32 = 1.0 / 20.0;

/// Equilibrium distance and well depth for two notes with no link between them: a long distance
/// and a shallow well, just enough to stop disconnected nodes drifting apart forever.
const R_WEAK: f32 = 200.0;
const EPS_WEAK: f32 = 600.0;

/// Equilibrium distance and well depth for two linked notes: noticeably closer and a much deeper
/// well, so connected notes visibly cluster together.
const R_STRONG: f32 = 100.0;
const EPS_STRONG: f32 = 6_000.0;

/// How strongly a node's outgoing edges push each other apart angularly.
const ANGULAR_REPULSION: f32 = 4_000.0;
const MIN_ANGLE: f32 = 0.05;

/// Fraction of velocity lost per second, applied as `exp(-DAMPING_RATE * dt)`.
const DAMPING_RATE: f32 = 3.0;

/// Pull toward the origin, both to keep the whole graph from slowly drifting off-canvas (the
/// Lennard-Jones forces above are all relative/pairwise and have no absolute anchor) and to bound
/// the graph's overall footprint — declutter's rectangle spacing, not the LJ equilibrium
/// distances, is usually what actually keeps nearby notes apart, so this is the more reliable
/// lever for the graph's overall size.
const CENTERING: f32 = 0.8;

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

/// Runs the force-directed layout for every note in `project`, from a fresh [`Simulation`], until
/// it settles (see [`Simulation::step`]) or a generous step budget elapses, and returns each
/// note's final world-space position in `project.notes` order. Exposed so the physics [`draw`]
/// relies on can be exercised in tests without a live `egui::Ui`.
pub fn settle(project: &Project) -> Vec<Pos2> {
    let edges = resolve_edges(project);
    let mut sim = Simulation::new();
    sim.sync(&project.notes, &edges);

    for _ in 0..1800 {
        if !sim.step(&project.notes, &edges, 1.0 / 60.0) {
            break;
        }
    }

    sim.positions(&project.notes)
}

/// Draws the whole project as a graph: one rectangle per note, one line per markdown link
/// between notes. `open_note`'s outgoing links (and the node itself) are highlighted in the
/// palette's accent color. `sim` carries the real-time physics state across frames and `view`
/// carries the camera (pan/zoom) state — pass the same instances every call. The camera can be
/// dragged with the left mouse button and zoomed with the scroll wheel, or driven with the corner
/// buttons. Returns the index of a note the user clicked this frame, if any.
pub fn draw(
    ui: &mut Ui,
    project: &Project,
    open_note: Option<usize>,
    palette: &UiPalette,
    sim: &mut Simulation,
    view: &mut View,
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
    let centroid = average(&positions);

    let colors = Colors::from_palette(palette);
    let font_id = TextStyle::Body.resolve(ui.style());
    let mut rects = note_rects(ui, project, &positions, &font_id, colors.text);
    declutter(&mut rects);

    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let canvas_rect = response.rect;

    handle_camera_input(ui, &response, view);

    // Zoom is read only after `handle_camera_input` above so a scroll this frame is reflected in
    // this same frame's stroke widths and font size, not delayed by one frame.
    let style = Style {
        colors,
        font_id,
        zoom: view.zoom,
    };

    let segments = edge_segments(&edges, &rects, canvas_rect, view);
    let hovered_edge = find_hovered_edge(&response, &segments);
    let highlight = Highlight {
        open_note,
        hovered_edge,
        edges: &edges,
    };

    draw_edges(&painter, &segments, &highlight, &style);

    let screen_rects: Vec<Rect> = rects
        .iter()
        .map(|rect| {
            Rect::from_min_max(
                to_screen(canvas_rect, view, rect.min),
                to_screen(canvas_rect, view, rect.max),
            )
        })
        .collect();

    let clicked = draw_nodes(ui, &painter, project, &screen_rects, &highlight, &style);

    draw_view_controls(ui, canvas_rect, view, centroid);

    clicked
}

/// Palette-derived colors, reused across every edge and node drawn this frame.
struct Colors {
    text: Color32,
    node_fill: Color32,
    edge: Color32,
    accent: Color32,
}

impl Colors {
    fn from_palette(palette: &UiPalette) -> Self {
        let text = settings::rgb(palette.text);
        Self {
            node_fill: mix(settings::rgb(palette.panel_background), text, 0.12),
            edge: mix(settings::rgb(palette.panel_background), text, 0.4),
            accent: settings::rgb(palette.accent),
            text,
        }
    }
}

/// Everything about *how* the graph is drawn this frame — palette colors, the label font, and the
/// camera's current zoom (for scaling stroke widths, rounding, and font size) — as opposed to
/// *what* is drawn.
struct Style {
    colors: Colors,
    font_id: FontId,
    zoom: f32,
}

/// World-space bounding rectangles for each note's label, centered on `positions`. Overlaps are
/// possible at this point — [`declutter`] resolves them afterwards.
fn note_rects(
    ui: &Ui,
    project: &Project,
    positions: &[Pos2],
    font_id: &FontId,
    text_color: Color32,
) -> Vec<Rect> {
    project
        .notes
        .iter()
        .zip(positions)
        .map(|(note, &center)| {
            let galley =
                ui.painter()
                    .layout_no_wrap(note.name.clone(), font_id.clone(), text_color);
            Rect::from_center_size(center, galley.size() + Vec2::new(24.0, 16.0))
        })
        .collect()
}

/// What should currently be drawn as highlighted: `open_note`'s node and outgoing edges, plus
/// whichever edge (if any) the mouse is hovering and that edge's two endpoint nodes.
struct Highlight<'a> {
    open_note: Option<usize>,
    hovered_edge: Option<usize>,
    edges: &'a [Edge],
}

impl Highlight<'_> {
    /// Whether the edge at `index` should be drawn highlighted.
    fn is_edge(&self, index: usize) -> bool {
        self.open_note == Some(self.edges[index].from) || self.hovered_edge == Some(index)
    }

    /// Whether the note at `index` should be drawn highlighted.
    fn is_node(&self, index: usize) -> bool {
        let is_hovered_endpoint = self.hovered_edge.is_some_and(|hovered| {
            self.edges[hovered].from == index || self.edges[hovered].to == index
        });
        self.open_note == Some(index) || is_hovered_endpoint
    }
}

/// Applies this frame's drag (pan) and scroll-wheel (zoom, anchored to the cursor) input to
/// `view`.
fn handle_camera_input(ui: &Ui, response: &egui::Response, view: &mut View) {
    if response.dragged() {
        view.center -= response.drag_delta() / view.zoom;
    }

    if let Some(hover_pos) = response.hover_pos() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = (scroll * ZOOM_WHEEL_SENSITIVITY).exp();
            zoom_at(view, hover_pos, response.rect.center(), factor);
        }
    }
}

/// Projects a world-space point to screen space, for the given camera and canvas rect.
fn to_screen(canvas_rect: Rect, view: &View, world: Pos2) -> Pos2 {
    canvas_rect.center() + (world - view.center) * view.zoom
}

/// Each edge's two endpoints (note centers), projected to screen space.
fn edge_segments(edges: &[Edge], rects: &[Rect], canvas_rect: Rect, view: &View) -> Vec<[Pos2; 2]> {
    edges
        .iter()
        .map(|edge| {
            [
                to_screen(canvas_rect, view, rects[edge.from].center()),
                to_screen(canvas_rect, view, rects[edge.to].center()),
            ]
        })
        .collect()
}

/// The index of the edge closest to the mouse, if any is within [`EDGE_HOVER_RADIUS`] of it.
fn find_hovered_edge(response: &egui::Response, segments: &[[Pos2; 2]]) -> Option<usize> {
    let pos = response.hover_pos()?;

    segments
        .iter()
        .enumerate()
        .map(|(index, &[a, b])| (index, distance_to_segment(pos, a, b)))
        .filter(|&(_, distance)| distance <= EDGE_HOVER_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// Draws every edge as a line, highlighted (in the accent color, thicker) per `highlight`.
fn draw_edges(
    painter: &egui::Painter,
    segments: &[[Pos2; 2]],
    highlight: &Highlight,
    style: &Style,
) {
    for (index, &segment) in segments.iter().enumerate() {
        let highlighted = highlight.is_edge(index);
        let color = if highlighted {
            style.colors.accent
        } else {
            style.colors.edge
        };
        let width = if highlighted { 2.5 } else { 1.0 };

        painter.line_segment(segment, Stroke::new(width * style.zoom, color));
    }
}

/// Draws every note as a rounded rectangle with its name centered inside, highlighted per
/// `highlight`. Returns the index of a note clicked this frame, if any.
fn draw_nodes(
    ui: &mut Ui,
    painter: &egui::Painter,
    project: &Project,
    screen_rects: &[Rect],
    highlight: &Highlight,
    style: &Style,
) -> Option<usize> {
    let mut clicked = None;

    for (index, &screen_rect) in screen_rects.iter().enumerate() {
        let highlighted = highlight.is_node(index);
        let border = if highlighted {
            style.colors.accent
        } else {
            style.colors.edge
        };

        painter.rect(
            screen_rect,
            4.0 * style.zoom,
            style.colors.node_fill,
            Stroke::new(if highlighted { 2.0 } else { 1.0 } * style.zoom, border),
            StrokeKind::Outside,
        );

        let mut scaled_font = style.font_id.clone();
        scaled_font.size *= style.zoom;
        let screen_galley = painter.layout_no_wrap(
            project.notes[index].name.clone(),
            scaled_font,
            style.colors.text,
        );
        painter.galley(
            screen_rect.center() - screen_galley.size() / 2.0,
            screen_galley,
            style.colors.text,
        );

        let response = ui
            .interact(screen_rect, Id::new(("graph-node", index)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.clicked() {
            clicked = Some(index);
        }
    }

    clicked
}

/// Screen pixels within which the mouse counts as hovering over an edge.
const EDGE_HOVER_RADIUS: f32 = 6.0;

/// The shortest distance from `point` to the line segment from `a` to `b`.
fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();

    let t = if len_sq > 1e-6 {
        ((point - a).dot(ab) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };

    point.distance(a + ab * t)
}

/// The average of a non-empty slice of points; the origin if it's empty.
fn average(points: &[Pos2]) -> Pos2 {
    if points.is_empty() {
        return Pos2::ZERO;
    }

    let sum = points.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2());
    (sum / points.len() as f32).to_pos2()
}

/// Multiplies `view.zoom` by `factor` (clamped to `[MIN_ZOOM, MAX_ZOOM]`), moving `view.center` so
/// the world point under `anchor_screen` stays fixed on screen — i.e. zooming towards/away from
/// that point rather than the camera center. `screen_center` is where `view.center` itself
/// projects to, i.e. the canvas's screen-space center.
fn zoom_at(view: &mut View, anchor_screen: Pos2, screen_center: Pos2, factor: f32) {
    let new_zoom = (view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let world_under_anchor = view.center + (anchor_screen - screen_center) / view.zoom;
    view.zoom = new_zoom;
    view.center = world_under_anchor - (anchor_screen - screen_center) / view.zoom;
}

/// Multiplies `view.zoom` by `factor` (clamped to `[MIN_ZOOM, MAX_ZOOM]`) without moving
/// `view.center` — i.e. zooming around the center of the viewport, for the zoom buttons (as
/// opposed to [`zoom_at`], which keeps a specific screen point fixed, for the scroll wheel).
fn zoom_by(view: &mut View, factor: f32) {
    view.zoom = (view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
}

/// A small square icon button placed at an exact screen position, for the corner overlay
/// controls. Returns whether it was clicked this frame.
fn place_button(ui: &mut Ui, min: Pos2, icon_char: char) -> bool {
    const SIZE: f32 = 24.0;
    let rect = Rect::from_min_size(min, Vec2::splat(SIZE));
    let label = fonts::icon_only(ui, icon_char);
    ui.put(rect, egui::Button::new(label)).clicked()
}

/// Overlay controls in the canvas's corners, for driving the camera without a mouse: zoom
/// in/reset/out stacked in the bottom-right, and an arrow cross for panning (with a recenter
/// button in its middle) in the bottom-left. `centroid` is the graph's current center of mass, in
/// world space, for the recenter button to jump to.
fn draw_view_controls(ui: &mut Ui, canvas_rect: Rect, view: &mut View, centroid: Pos2) {
    const BUTTON: f32 = 24.0;
    const GAP: f32 = 2.0;
    const MARGIN: f32 = 10.0;

    let zoom_x = canvas_rect.right() - MARGIN - BUTTON;
    let mut y = canvas_rect.bottom() - MARGIN - BUTTON;

    if place_button(ui, Pos2::new(zoom_x, y), icon::SEARCH_MINUS) {
        zoom_by(view, 1.0 / ZOOM_BUTTON_STEP);
    }
    y -= BUTTON + GAP;
    if place_button(ui, Pos2::new(zoom_x, y), icon::CROSSHAIRS) {
        *view = View::default();
    }
    y -= BUTTON + GAP;
    if place_button(ui, Pos2::new(zoom_x, y), icon::SEARCH_PLUS) {
        zoom_by(view, ZOOM_BUTTON_STEP);
    }

    let pad_x = canvas_rect.left() + MARGIN;
    let pad_y = canvas_rect.bottom() - MARGIN - BUTTON * 3.0 - GAP * 2.0;
    let step = BUTTON + GAP;

    if place_button(ui, Pos2::new(pad_x + step, pad_y), icon::ARROW_UP) {
        view.center.y -= PAN_BUTTON_STEP / view.zoom;
    }
    if place_button(ui, Pos2::new(pad_x, pad_y + step), icon::ARROW_LEFT) {
        view.center.x -= PAN_BUTTON_STEP / view.zoom;
    }
    if place_button(
        ui,
        Pos2::new(pad_x + step * 2.0, pad_y + step),
        icon::ARROW_RIGHT,
    ) {
        view.center.x += PAN_BUTTON_STEP / view.zoom;
    }
    if place_button(
        ui,
        Pos2::new(pad_x + step, pad_y + step * 2.0),
        icon::ARROW_DOWN,
    ) {
        view.center.y += PAN_BUTTON_STEP / view.zoom;
    }
    if place_button(ui, Pos2::new(pad_x + step, pad_y + step), icon::BULLSEYE) {
        view.center = centroid;
    }
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

    /// Steps `sim` until it settles, then confirms it stays put for a further window — the core
    /// "does it converge" property every scenario below relies on. Returns the settled positions.
    fn assert_converges(sim: &mut Simulation, notes: &[Note], edges: &[Edge]) -> Vec<Pos2> {
        for _ in 0..1800 {
            sim.step(notes, edges, 1.0 / 60.0);
        }

        let before = sim.positions(notes);

        for _ in 0..600 {
            sim.step(notes, edges, 1.0 / 60.0);
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
                        positions[e1.from],
                        positions[e1.to],
                        positions[e2.from],
                        positions[e2.to],
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
        let edges = vec![Edge { from: 0, to: 1 }];

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
        let connected_edges = vec![Edge { from: 0, to: 1 }];
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
        let edges: Vec<Edge> = (0..6)
            .map(|i| Edge {
                from: i,
                to: (i + 1) % 6,
            })
            .collect();

        let mut sim = Simulation::new();
        sim.sync(&notes, &edges);
        let positions = assert_converges(&mut sim, &notes, &edges);

        assert_eq!(
            count_segment_crossings(&positions, &edges),
            0,
            "a ring should always be drawable with no crossing edges"
        );

        let connected: HashSet<(usize, usize)> = edges
            .iter()
            .map(|edge| (edge.from.min(edge.to), edge.from.max(edge.to)))
            .collect();

        let mut connected_total = 0.0;
        let mut unconnected_total = 0.0;
        let mut unconnected_count = 0;

        for i in 0..6 {
            for j in (i + 1)..6 {
                let distance = (positions[i] - positions[j]).length();
                if connected.contains(&(i, j)) {
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
        let edges: Vec<Edge> = (1..6).map(|i| Edge { from: 0, to: i }).collect();

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
            "tests/fixtures/example-project.mystorynotes",
        ))
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
