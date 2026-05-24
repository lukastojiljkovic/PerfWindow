use crate::theme::Theme;
use egui::{Rect, Response, Sense, Vec2};

/// A thin 6 px-tall horizontal progress bar spanning the available width.
///
/// `fraction` is 0.0–1.0 (clamped). The background uses `theme.track`; the
/// fill uses `theme.accent`, or `theme.warn` when `warn` is `true`.
/// Matches the `.pw-bar` element in the mockup.
pub fn bar_meter(ui: &mut egui::Ui, theme: &Theme, fraction: f32, warn: bool) -> Response {
    let available_w = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available_w, 6.0), Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // Background track
        painter.rect_filled(rect, 0.0, theme.track);

        // Filled portion
        let fill_w = (fraction.clamp(0.0, 1.0) * rect.width()).max(0.0);
        if fill_w > 0.0 {
            let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
            let fill_color = if warn { theme.warn } else { theme.accent };
            painter.rect_filled(fill_rect, 0.0, fill_color);
        }
    }

    response
}

/// A 24 px-tall row of equal-width vertical bars, one per element of `loads`.
///
/// Each bar's height is proportional to its load (0–100), minimum 2 px.
/// Bars are separated by 3 px gaps and sit on the baseline (grow upward).
/// Fill is `theme.accent` at ~0.72 opacity.
/// Matches the `.pw-cores` element in the mockup.
pub fn core_strip(ui: &mut egui::Ui, theme: &Theme, loads: &[f32]) {
    if loads.is_empty() {
        return;
    }

    let strip_h = 24.0;
    let gap = 3.0;
    let available_w = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, strip_h), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let n = loads.len() as f32;
    let total_gap = gap * (n - 1.0);
    // When the available width is too small for `n` bars + gaps, the `.max(1.0)`
    // floor trades exact tiling for a 1 px minimum: trailing bars then overflow
    // `rect`, which `painter_at(rect)` clipping absorbs harmlessly.
    let bar_w = ((available_w - total_gap) / n).max(1.0);

    // accent at 0.72 opacity
    let bar_color = theme.accent.gamma_multiply(0.72);

    for (i, &load) in loads.iter().enumerate() {
        let fraction = (load / 100.0).clamp(0.0, 1.0);
        let bar_h = (fraction * strip_h).max(2.0);

        let x = rect.min.x + i as f32 * (bar_w + gap);
        let bar_rect = Rect::from_min_size(
            egui::Pos2::new(x, rect.max.y - bar_h),
            Vec2::new(bar_w, bar_h),
        );
        painter.rect_filled(bar_rect, 0.0, bar_color);
    }
}

/// A 2-D heat-map of CPU cores: each cell is coloured by load (`theme.track`
/// → `theme.accent` → `theme.hot`) and labelled with the per-core temperature
/// when available, falling back to the load percentage.
///
/// `loads` and `temps` are parallel arrays; `temps[i]` may be `None` even
/// when `loads[i]` is present. If `loads` is shorter than `temps`, the extra
/// temperatures are ignored. Cells wrap into rows that fit the available
/// width.
pub fn core_grid(
    ui: &mut egui::Ui,
    theme: &Theme,
    loads: &[f32],
    temps: &[Option<f32>],
    unit: crate::format::TempUnit,
) {
    if loads.is_empty() {
        return;
    }

    const CELL_W: f32 = 44.0;
    const CELL_H: f32 = 28.0;
    const GAP: f32 = 4.0;
    const CORNER_PAD: f32 = 3.0;

    let available_w = ui.available_width();
    let cols = ((available_w + GAP) / (CELL_W + GAP)).floor().max(1.0) as usize;
    let rows = loads.len().div_ceil(cols);
    let total_h = rows as f32 * CELL_H + (rows as f32 - 1.0).max(0.0) * GAP;
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, total_h), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let label_font = egui::FontId::new(11.0, theme.font_data.egui());
    let corner_font = egui::FontId::new(8.0, theme.font_data.egui());

    for (i, &load) in loads.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let cell_min = egui::Pos2::new(
            rect.min.x + col as f32 * (CELL_W + GAP),
            rect.min.y + row as f32 * (CELL_H + GAP),
        );
        let cell_rect = Rect::from_min_size(cell_min, Vec2::new(CELL_W, CELL_H));

        // Background colour: ramp track → accent → hot as load grows.
        let load_frac = (load / 100.0).clamp(0.0, 1.0);
        let bg = if load_frac < 0.5 {
            lerp_color(theme.track, theme.accent, load_frac * 2.0)
        } else {
            lerp_color(theme.accent, theme.hot, (load_frac - 0.5) * 2.0)
        };
        painter.rect_filled(cell_rect, 0.0, bg);
        painter.rect_stroke(
            cell_rect,
            0.0,
            egui::Stroke::new(1.0, theme.border),
            egui::StrokeKind::Inside,
        );

        // Centred label: temperature if available, otherwise load percentage.
        let label = match temps.get(i).copied().flatten() {
            Some(t) => crate::format::format_temp_compact(Some(t as f64), unit),
            None => format!("{}%", load.round() as i64),
        };
        // Foreground colour: bg fill is in the accent family — read against
        // the theme's ink/bg pair to keep contrast as the cell heats.
        let fg = if load_frac >= 0.5 {
            theme.bg
        } else {
            theme.ink
        };
        let galley = painter.layout_no_wrap(label, label_font.clone(), fg);
        let text_pos = egui::Pos2::new(
            cell_rect.center().x - galley.size().x / 2.0,
            cell_rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(text_pos, galley, fg);

        // Corner tag: "C0" / "C1" / ... in dim, anchored top-left.
        let corner = format!("C{i}");
        let corner_galley = painter.layout_no_wrap(corner, corner_font.clone(), theme.dim);
        painter.galley(
            egui::Pos2::new(cell_min.x + CORNER_PAD, cell_min.y + 1.0),
            corner_galley,
            theme.dim,
        );
    }
}

/// Linearly interpolate two `Color32` values channel-wise. `t` in `[0,1]`.
fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t).round() as u8 };
    egui::Color32::from_rgba_unmultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        lerp(a.a(), b.a()),
    )
}
