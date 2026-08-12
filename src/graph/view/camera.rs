use egui::{Pos2, Ui};

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 4.0;

/// Multiplicative zoom change per "notch" of scroll-wheel input.
const ZOOM_WHEEL_SENSITIVITY: f32 = 0.0015;

/// The graph view's camera: `center` is the world-space point shown at the middle of the canvas,
/// `zoom` is the world-to-screen scale factor (screen pixels per world unit). Persists across
/// frames like [`crate::graph::Simulation`] does, so panning and zooming don't reset on every
/// draw; own one for as long as the graph view should remember its camera and pass the same
/// instance every call to [`super::draw`].
pub struct View {
    pub(super) center: Pos2,
    pub(super) zoom: f32,
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

/// Applies this frame's drag (pan) and scroll-wheel (zoom, anchored to the cursor) input to
/// `view`.
pub(super) fn handle_camera_input(ui: &Ui, response: &egui::Response, view: &mut View) {
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
pub(super) fn to_screen(canvas_rect: egui::Rect, view: &View, world: Pos2) -> Pos2 {
    canvas_rect.center() + (world - view.center) * view.zoom
}

/// The inverse of [`to_screen`]: projects a screen-space point back to world space.
pub(super) fn to_world(canvas_rect: egui::Rect, view: &View, screen: Pos2) -> Pos2 {
    view.center + (screen - canvas_rect.center()) / view.zoom
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
pub(super) fn zoom_by(view: &mut View, factor: f32) {
    view.zoom = (view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
}
