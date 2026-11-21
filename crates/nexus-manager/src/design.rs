use eframe::egui::{self, Color32, Frame, Margin, RichText, Stroke, Ui};

pub const BG: Color32 = Color32::from_rgb(13, 18, 28);
pub const SIDEBAR: Color32 = Color32::from_rgb(16, 23, 36);
pub const CARD: Color32 = Color32::from_rgb(23, 32, 48);
pub const LINE: Color32 = Color32::from_rgb(47, 62, 81);
pub const TEXT: Color32 = Color32::from_rgb(238, 245, 253);
pub const MUTED: Color32 = Color32::from_rgb(158, 177, 199);
pub const ACCENT: Color32 = Color32::from_rgb(91, 231, 207);
pub const VIOLET: Color32 = Color32::from_rgb(172, 153, 255);

pub fn setup(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = CARD;
    style.visuals.override_text_color = None;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    style.visuals.selection.bg_fill = Color32::from_rgb(29, 72, 76);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 48, 69);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 68, 88);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 48, 69);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(45, 68, 88);
    style.visuals.extreme_bg_color = Color32::from_rgb(11, 17, 26);
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(13.0, 8.0);
    ctx.set_style(style);
}

pub fn frame(fill: Color32, padding: i8) -> Frame {
    Frame::new().fill(fill).inner_margin(Margin::same(padding))
}

pub fn card<R>(ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> R {
    frame(CARD, 18).corner_radius(12).stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui)
        }).inner
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(19.0).strong().color(TEXT));
}

pub fn hint(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(13.0).color(MUTED));
}

pub fn bytes(value: u64) -> String {
    if value >= 1 << 30 {
        format!("{:.1} GiB", value as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} MiB", value as f64 / (1u64 << 20) as f64)
    }
}

pub fn navigation_row(ui: &mut Ui, label: &str, icon: usize, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, active, label));
    if ui.is_rect_visible(rect) {
        let color = if active { ACCENT } else { MUTED };
        if active || response.hovered() || response.has_focus() {
            let fill = if active { Color32::from_rgb(25, 62, 68) } else { CARD };
            ui.painter().rect_filled(rect, 7, fill);
        }
        if active {
            let marker = egui::Rect::from_min_size(rect.left_center() - egui::vec2(0.0, 8.0), egui::vec2(2.0, 16.0));
            ui.painter().rect_filled(marker, 1, ACCENT);
        }
        draw_icon(ui, rect.left_center() + egui::vec2(17.0, 0.0), icon, color);
        ui.painter().text(rect.left_center() + egui::vec2(32.0, 0.0), egui::Align2::LEFT_CENTER,
            label, egui::FontId::proportional(13.0), color);
    }
    response
}

pub fn draw_icon(ui: &Ui, center: egui::Pos2, icon: usize, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.25_f32, color);
    let pos = |x, y| center + egui::vec2(x, y);
    let line = |a, b| { painter.line_segment([a, b], stroke); };
    let outline = |x, y, w, h| {
        let rect = egui::Rect::from_min_size(pos(x, y), egui::vec2(w, h));
        painter.rect_stroke(rect, 1, stroke, egui::StrokeKind::Inside);
    };
    match icon {
        0 => for (x, y) in [(-6.0, -6.0), (1.0, -6.0), (-6.0, 1.0), (1.0, 1.0)] { outline(x, y, 5.0, 5.0); },
        1 => { outline(-7.0, -5.0, 14.0, 9.0); line(pos(0.0, 4.0), pos(0.0, 7.0)); line(pos(-4.0, 7.0), pos(4.0, 7.0)); }
        2 => { outline(-7.0, -6.0, 14.0, 5.0); outline(-7.0, 1.0, 14.0, 5.0); }
        3 => { outline(-7.0, -5.0, 14.0, 10.0); line(pos(-2.0, -5.0), pos(-2.0, 5.0)); line(pos(3.0, -5.0), pos(3.0, 5.0)); }
        4 => {
            painter.add(egui::Shape::closed_line(vec![pos(0.0, -7.0), pos(7.0, 0.0), pos(0.0, 7.0), pos(-7.0, 0.0)], stroke));
            painter.circle_filled(center, 1.8, color);
        }
        5 => { painter.circle_stroke(center, 6.0, stroke); painter.circle_filled(pos(2.0, -2.0), 2.0, color); }
        6 => for (y, slider) in [(-5.0, -3.0), (0.0, 3.0), (5.0, 0.0)] {
            line(pos(-7.0, y), pos(7.0, y)); painter.circle_filled(pos(slider, y), 2.0, color);
        },
        _ => {
            painter.add(egui::Shape::line(vec![pos(-6.0, 0.0), pos(-4.0, -5.0), pos(2.0, -6.0), pos(6.0, -2.0), pos(6.0, 3.0), pos(2.0, 6.0), pos(-3.0, 6.0)], stroke));
            line(pos(-6.0, 0.0), pos(-7.0, -5.0)); line(pos(-6.0, 0.0), pos(-1.0, 0.0));
        }
    }
}