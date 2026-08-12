use eframe::egui;
use egui::{Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2};

use super::simulation::Simulation;
use super::{ConnectionId, Edge, resolve_edges};
use crate::fonts::{self, icon};
use crate::project::{NoteId, Project};
use crate::settings::{self, SimulationSettings, UiPalette};

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
    /// A fresh camera centered on the origin at 1:1 zoom, matching where `initial_layout`
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

/// Which notes to highlight in the graph view: `open_note` from outside it, e.g. the sidebar's
/// currently-open note. `hovered_note` is a fallback used only if the mouse isn't directly over a
/// node in the graph view itself this frame — `draw` also detects that on its own, e.g. for the
/// sidebar note list to highlight a note by hovering its row. `open_note`'s node and outgoing
/// edges are highlighted in the palette's accent color, as is `hovered_note` (and, unlike
/// `open_note`, all of *its* directly connected notes and the edges to them).
pub struct NoteHighlight {
    pub open_note: Option<NoteId>,
    pub hovered_note: Option<NoteId>,
}

/// Draws the whole project as a graph: one rectangle per note, one line per markdown link
/// between notes, highlighted per `note_highlight` (see [`NoteHighlight`]). `simulation_settings`
/// tunes the physics `sim` runs. `sim` carries the real-time physics state across frames and
/// `view` carries the camera (pan/zoom) state — pass the same instances every call. The camera
/// can be dragged with the left mouse button and zoomed with the scroll wheel, or driven with the
/// corner buttons. Returns the index of a note the user clicked this frame, if any.
pub fn draw(
    ui: &mut Ui,
    project: &Project,
    note_highlight: NoteHighlight,
    palette: &UiPalette,
    simulation_settings: &SimulationSettings,
    sim: &mut Simulation,
    view: &mut View,
) -> Option<NoteId> {
    let edges = resolve_edges(project);
    sim.sync(&project.notes, &edges);

    if project.notes.is_empty() {
        ui.centered_and_justified(|ui| ui.label("No notes yet."));
        return None;
    }

    let dt = ui.ctx().input(|input| input.stable_dt);
    if sim.step(&project.notes, &edges, dt, simulation_settings) {
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

    painter.rect_filled(canvas_rect, 0.0, style.colors.canvas);

    let screen_rects: Vec<Rect> = rects
        .iter()
        .map(|rect| {
            Rect::from_min_max(
                to_screen(canvas_rect, view, rect.min),
                to_screen(canvas_rect, view, rect.max),
            )
        })
        .collect();

    // Registered before computing `highlight` (so hover state is known in time to highlight
    // edges too, which are drawn before nodes) and reused as-is when nodes are drawn below,
    // rather than interacting with the same rects twice.
    let node_responses = interact_nodes(ui, project, &screen_rects);
    let hovered_node_directly = node_responses
        .iter()
        .position(|response| response.as_ref().is_some_and(egui::Response::hovered))
        .map(NoteId::from);

    let segments = edge_segments(&edges, &rects, canvas_rect, view);
    let hovered_edge = find_hovered_edge(&response, &segments);
    let hovered_note = hovered_node_directly.or(note_highlight.hovered_note);
    let highlight = Highlight {
        open_note: note_highlight.open_note,
        hovered_note,
        hovered_edge,
        edges: &edges,
    };

    draw_edges(&painter, &segments, &highlight, &style);

    let node_positions: Vec<(Rect, Pos2)> = screen_rects
        .iter()
        .copied()
        .zip(positions.iter().copied())
        .collect();
    let clicked = draw_nodes(
        &painter,
        project,
        &node_positions,
        &node_responses,
        &highlight,
        &style,
        sim,
    );

    draw_view_controls(ui, canvas_rect, view, centroid);

    clicked
}

/// Palette-derived colors, reused across every edge and node drawn this frame.
struct Colors {
    text: Color32,
    canvas: Color32,
    node_fill: Color32,
    edge: Color32,
    accent: Color32,
}

impl Colors {
    fn from_palette(palette: &UiPalette) -> Self {
        let text = settings::rgb(palette.text);
        let panel_background = settings::rgb(palette.panel_background);
        Self {
            // Slightly darker than the panel behind it (rather than reusing that fill directly)
            // so nodes and edges, which are both only subtly lighter than the panel background,
            // have something to stand out against.
            canvas: mix(panel_background, Color32::BLACK, 0.12),
            node_fill: mix(panel_background, text, 0.12),
            edge: mix(panel_background, text, 0.4),
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
/// possible at this point — [`declutter`] resolves them afterwards. The manuscript note (which
/// isn't drawn — see `draw_nodes`) gets a zero-sized rect instead of a real one, so `declutter`
/// never pushes a real node aside to make room for a node nothing ever draws.
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
            if note.is_manuscript {
                return Rect::from_center_size(center, Vec2::ZERO);
            }

            let galley =
                ui.painter()
                    .layout_no_wrap(note.name.clone(), font_id.clone(), text_color);
            Rect::from_center_size(center, galley.size() + Vec2::new(24.0, 16.0))
        })
        .collect()
}

/// What should currently be drawn as highlighted: `open_note`'s node and outgoing edges;
/// `hovered_note`'s node, every note directly connected to it, and the edges between them; and
/// whichever edge (if any) the mouse is hovering directly, plus that edge's two endpoint nodes.
struct Highlight<'a> {
    open_note: Option<NoteId>,
    hovered_note: Option<NoteId>,
    hovered_edge: Option<ConnectionId>,
    edges: &'a [Edge],
}

impl Highlight<'_> {
    /// Whether `a` and `b` are the two endpoints, in either direction, of some edge.
    fn are_connected(&self, a: NoteId, b: NoteId) -> bool {
        self.edges
            .iter()
            .any(|edge| (edge.from == a && edge.to == b) || (edge.from == b && edge.to == a))
    }

    /// Whether the edge at `index` should be drawn highlighted.
    fn is_edge(&self, index: ConnectionId) -> bool {
        let edge = &self.edges[usize::from(index)];

        self.open_note == Some(edge.from)
            || self.hovered_edge == Some(index)
            || self.hovered_note == Some(edge.from)
            || self.hovered_note == Some(edge.to)
    }

    /// Whether the note at `index` should be drawn highlighted.
    fn is_node(&self, index: NoteId) -> bool {
        let is_hovered_endpoint = self.hovered_edge.is_some_and(|hovered| {
            let edge = &self.edges[usize::from(hovered)];
            edge.from == index || edge.to == index
        });
        let is_hovered_neighbor = self
            .hovered_note
            .is_some_and(|hovered| self.are_connected(index, hovered));

        self.open_note == Some(index)
            || self.hovered_note == Some(index)
            || is_hovered_endpoint
            || is_hovered_neighbor
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
                to_screen(canvas_rect, view, rects[usize::from(edge.from)].center()),
                to_screen(canvas_rect, view, rects[usize::from(edge.to)].center()),
            ]
        })
        .collect()
}

/// Registers each non-manuscript note's on-screen rect as an interactive (click-and-drag) widget
/// and returns its `Response`, in `project.notes` order (`None` for the manuscript note's slot,
/// kept so indices still line up — it isn't drawn, so it should never be interactive either).
///
/// Called before drawing anything so a node's own hover state is known in time to highlight
/// edges too (drawn before nodes) — deliberately *not* derived from the canvas-wide response's
/// `hover_pos`, the way it used to be: once a node senses drag as well as click, egui's hit-test
/// gives it exclusive hover ownership over whatever's behind it (the canvas) while the pointer is
/// over it, the same way a button on top of a `ScrollArea` does, so the canvas response's own
/// `hovered()` (and so `hover_pos()`) is reliably false there — see the "The only interactive
/// widgets we mark as hovered are the ones in `hits.click` and `hits.drag`!" comment in egui's
/// `interaction.rs`.
fn interact_nodes(
    ui: &mut Ui,
    project: &Project,
    screen_rects: &[Rect],
) -> Vec<Option<egui::Response>> {
    screen_rects
        .iter()
        .enumerate()
        .map(|(index, &screen_rect)| {
            if project.notes[index].is_manuscript {
                return None;
            }

            Some(
                ui.interact(
                    screen_rect,
                    Id::new(("graph-node", index)),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand),
            )
        })
        .collect()
}

/// Screen pixels within which the mouse counts as hovering over an edge.
const EDGE_HOVER_RADIUS: f32 = 6.0;

/// The index of the edge closest to the mouse, if any is within [`EDGE_HOVER_RADIUS`] of it.
fn find_hovered_edge(response: &egui::Response, segments: &[[Pos2; 2]]) -> Option<ConnectionId> {
    let pos = response.hover_pos()?;

    segments
        .iter()
        .enumerate()
        .map(|(index, &[a, b])| (index, distance_to_segment(pos, a, b)))
        .filter(|&(_, distance)| distance <= EDGE_HOVER_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| ConnectionId::from(index))
}

/// Draws every edge as a line, highlighted (in the accent color, thicker) per `highlight`.
fn draw_edges(
    painter: &egui::Painter,
    segments: &[[Pos2; 2]],
    highlight: &Highlight,
    style: &Style,
) {
    for (index, &segment) in segments.iter().enumerate() {
        let highlighted = highlight.is_edge(ConnectionId::from(index));
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
/// `highlight`. Each node can also be dragged, which overrides its position directly in `sim`
/// (see [`Simulation::drag_to`]) rather than waiting for physics to catch up. `node_positions`
/// pairs each note's on-screen rect this frame with its raw (pre-declutter) world-space
/// position — drawing uses the former, dragging updates relative to the latter. `node_responses`
/// (`project.notes` order, from [`interact_nodes`]) is reused as-is rather than interacting with
/// the same rects a second time. Returns the index of a note clicked this frame, if any.
fn draw_nodes(
    painter: &egui::Painter,
    project: &Project,
    node_positions: &[(Rect, Pos2)],
    node_responses: &[Option<egui::Response>],
    highlight: &Highlight,
    style: &Style,
    sim: &mut Simulation,
) -> Option<NoteId> {
    let mut clicked = None;

    for (index, &(screen_rect, position)) in node_positions.iter().enumerate() {
        let index = NoteId::from(index);

        // The manuscript note isn't part of the graph at all (see `resolve_edges`) — it would
        // obviously end up linked from just about every other note. Its slot in `screen_rects`
        // (and `node_responses`) is kept (zero-sized, `None`) purely so indices still line up
        // with `project.notes`; skip drawing and hit-testing it here.
        let Some(response) = &node_responses[usize::from(index)] else {
            continue;
        };

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
            project.notes[usize::from(index)].name.clone(),
            scaled_font,
            style.colors.text,
        );
        painter.galley(
            screen_rect.center() - screen_galley.size() / 2.0,
            screen_galley,
            style.colors.text,
        );

        if response.clicked() {
            clicked = Some(index);
        }

        if response.dragged() {
            let world_delta = response.drag_delta() / style.zoom;
            sim.drag_to(
                &project.notes[usize::from(index)].name,
                position + world_delta,
            );
        }
    }

    clicked
}

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

/// Pushes apart any node rectangles the simulation left overlapping. `Simulation::step` only
/// reasons about points, not each note's actual label size, so long note names in a dense cluster
/// can still end up overlapping; this guarantees every label stays readable regardless. Purely
/// cosmetic and recomputed fresh every frame — it doesn't feed back into the simulation. Skips any
/// zero-area rect (the manuscript note's, from `note_rects` — nothing is ever drawn there), so a
/// node nothing draws never pushes a real one aside.
fn declutter(rects: &mut [Rect]) {
    const GAP: f32 = 12.0;
    const ITERATIONS: usize = 300;

    for _ in 0..ITERATIONS {
        let mut moved = false;

        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                if rects[i].area() <= 0.0 || rects[j].area() <= 0.0 {
                    continue;
                }

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

/// Blends two colors channel-wise; `t` of 0.0 is `a`, 1.0 is `b`.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}
