use eframe::egui::{self, Widget};

/// iOS-style toggle switch:
///
/// ``` text
///      _____________
///     /       /.....\
///    |       |.......|
///     \_______\_____/
/// ```
///
/// ```
/// # let ui = &mut egui::Ui::__test();
/// # let mut my_bool = true;
/// ui.add(widget::Toggle::new(&mut on));
/// ```
pub struct Toggle<'a> {
    on: &'a mut bool,
    enabled: bool,
}

impl<'a> Toggle<'a> {
    pub fn new(on: &'a mut bool) -> Self {
        Self { on, enabled: true }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<'a> Widget for Toggle<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let Self { on, enabled } = self;

        ui.set_enabled(enabled);

        let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

        let how_on = ui.ctx().animate_bool(response.id, *on);
        let visuals = ui.style().interact_selectable(&response, *on);
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter()
            .rect(rect, radius, visuals.bg_fill, visuals.bg_stroke);
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter()
            .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);

        response
    }
}
