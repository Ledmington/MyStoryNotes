use egui::{Pos2, Rect, Ui, Vec2};

use crate::fonts::{self, icon};

use super::camera::{View, zoom_by};

/// Multiplicative zoom change applied by a single click of the +/- zoom buttons.
const ZOOM_BUTTON_STEP: f32 = 1.25;

/// Screen pixels the pan buttons move the camera by per click, before dividing by zoom.
const PAN_BUTTON_STEP: f32 = 80.0;

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
pub(super) fn draw_view_controls(ui: &mut Ui, canvas_rect: Rect, view: &mut View, centroid: Pos2) {
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
