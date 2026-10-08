//! The mini strip: a one-row, 28 logical px overlay bar.
//!
//! [`strip_row`] paints the whole strip — the PerfWindow mark, the configured
//! readings and the trailing opacity slider plus the expand/close controls —
//! and reports what the user did. Everything that can be decided without
//! measuring glyphs is a pure function ([`items`], [`fit_plan`]) so the
//! readings, their colours and their fit are unit-tested and snapshotted
//! headlessly.
//!
//! Readings come from the same [`crate::format`] helpers the cards and the
//! footer use, and temperature colours come from the dashboard's own warn/hot
//! ramp: a reading the dashboard would highlight is highlighted here too, and
//! a calm reading stays `ink`.
//!
//! The strip never scrolls, wraps or overlaps: a reading that does not fit the
//! monitor's width is dropped from the right, and the trailing slider group and
//! controls are reserved before the readings are measured.

use crate::config::{Config, MiniStripValues};
use crate::format::{
    finite, format_bytes_per_sec, format_gb_pair, format_percent, format_temp, letter_spaced,
    TempUnit,
};
use crate::ipc::Snapshot;
use crate::theme::Theme;
use crate::widgets::{temp_alert_color, TempKind};
use egui::{Color32, FontId, Galley, Margin, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use std::sync::Arc;

/// Height of the strip in logical pixels. Matches
/// [`crate::strip_window::STRIP_HEIGHT_LOGICAL`], which is what the overlay
/// window is sized to.
pub const STRIP_HEIGHT: f32 = crate::strip_window::STRIP_HEIGHT_LOGICAL;

/// `"—"`, the placeholder every reading without data renders as.
pub const MISSING: &str = "\u{2014}";

/// Horizontal padding inside the strip.
const PAD_X: f32 = 10.0;
/// Font size of the mark and of every reading.
const FONT_SIZE: f32 = 13.0;
/// Gap either side of a separator rule.
const SEP_GAP: f32 = 9.0;
/// Width of a separator rule.
const SEP_WIDTH: f32 = 1.0;
/// How far the separator rule is inset from the strip's top and bottom.
const SEP_INSET: f32 = 7.0;
/// Gap between a reading's label and its first value.
const LABEL_GAP: f32 = 6.0;
/// Gap between two values of the same reading (`23%  54°C`).
const VALUE_GAP: f32 = 7.0;
/// Trailing control size.
const BUTTON_W: f32 = 17.0;
const BUTTON_H: f32 = 17.0;
/// Gap between the two trailing controls.
const BUTTON_GAP: f32 = 4.0;
/// Gap between the last reading and the trailing controls.
const CONTROL_GAP: f32 = 6.0;
/// How far the icon inside a control is inset from the control's edge.
const ICON_INSET: f32 = 4.5;
/// Length of the opacity slider's track.
const OPACITY_TRACK_W: f32 = 56.0;
/// Height of the opacity slider's track.
const OPACITY_TRACK_H: f32 = 3.0;
/// Side of the opacity slider's square knob.
const OPACITY_KNOB: f32 = 9.0;

/// One reading on the strip: a `dim` label and one or more coloured value
/// fragments, e.g. `CPU` + `23%` + `54°C`.
pub struct StripItem {
    /// The dim left-hand label.
    pub label: &'static str,
    /// The fragments after the label, in painting order.
    pub parts: Vec<StripPart>,
}

/// One value fragment: its text plus the colour it is painted in.
pub struct StripPart {
    pub text: String,
    pub color: Color32,
}

/// Which control the user pressed on the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StripAction {
    #[default]
    None,
    /// The background-opacity setting was changed to this percentage.
    Opacity(u8),
    /// Back to the full window.
    Expand,
    /// Close PerfWindow entirely.
    Close,
}

/// Build the strip's readings, left to right, from the configured selection
/// and the latest snapshot.
///
/// A selected reading whose data is absent still appears, with [`MISSING`] in
/// place of the value, so the strip does not silently change shape when a
/// sensor drops out. Battery is the one exception: it is only built when a
/// battery exists, because "no battery" is a property of the machine rather
/// than a momentary gap.
pub fn items(
    snapshot: Option<&Snapshot>,
    values: &MiniStripValues,
    unit: TempUnit,
    theme: &Theme,
) -> Vec<StripItem> {
    let cpu = snapshot.and_then(|s| s.cpu.as_ref());
    let gpu = snapshot
        .and_then(|s| s.gpu.as_deref())
        .and_then(|gpus| gpus.first());
    let ram = snapshot.and_then(|s| s.ram.as_ref());
    let net = snapshot.and_then(|s| s.net.as_ref());
    let battery = snapshot.and_then(|s| s.battery.as_ref());

    let mut readings = Vec::new();

    if values.cpu_load || values.cpu_temp {
        let mut parts = Vec::new();
        if values.cpu_load {
            parts.push(percent_part(cpu.and_then(|c| c.load), theme));
        }
        if values.cpu_temp {
            parts.push(temp_part(cpu.and_then(|c| c.temp), unit, theme));
        }
        readings.push(StripItem {
            label: "CPU",
            parts,
        });
    }

    if values.gpu_load || values.gpu_temp {
        let mut parts = Vec::new();
        if values.gpu_load {
            parts.push(percent_part(gpu.and_then(|g| g.load), theme));
        }
        if values.gpu_temp {
            parts.push(temp_part(gpu.and_then(|g| g.temp), unit, theme));
        }
        readings.push(StripItem {
            label: "GPU",
            parts,
        });
    }

    if values.vram {
        readings.push(StripItem {
            label: "VRAM",
            parts: vec![StripPart {
                text: format_gb_pair(
                    gpu.and_then(|g| g.vram_used_mb),
                    gpu.and_then(|g| g.vram_total_mb),
                ),
                color: theme.ink,
            }],
        });
    }

    if values.ram {
        readings.push(StripItem {
            label: "RAM",
            parts: vec![StripPart {
                text: format_gb_pair(ram.and_then(|r| r.used_mb), ram.and_then(|r| r.total_mb)),
                color: theme.ink,
            }],
        });
    }

    if values.network {
        readings.push(StripItem {
            label: "NET",
            parts: vec![StripPart {
                text: net_value(net),
                color: theme.ink,
            }],
        });
    }

    if values.battery {
        if let Some(battery) = battery {
            readings.push(StripItem {
                label: "BAT",
                parts: vec![StripPart {
                    text: percent_value(battery.charge_pct),
                    color: theme.ink,
                }],
            });
        }
    }

    readings
}

/// A percentage reading, or [`MISSING`] when the reading is absent or
/// non-finite — never a confident `"—%"`.
fn percent_part(load: Option<f64>, theme: &Theme) -> StripPart {
    StripPart {
        text: percent_value(load),
        color: theme.ink,
    }
}

fn percent_value(load: Option<f64>) -> String {
    match finite(load) {
        Some(v) => format!("{}%", format_percent(Some(v))),
        None => MISSING.to_string(),
    }
}

/// `"54°C"` / `"136°F"`: the dashboard's `format_temp` plus the unit letter,
/// because the strip has no separate unit chip. A missing reading stays the
/// bare [`MISSING`]. A reading the dashboard would call warn or hot carries the
/// same colour; a calm one stays `ink`.
fn temp_part(celsius: Option<f64>, unit: TempUnit, theme: &Theme) -> StripPart {
    match finite(celsius) {
        Some(c) => StripPart {
            text: format!("{}{}", format_temp(Some(c), unit), unit_letter(unit)),
            color: temp_alert_color(c, TempKind::Processor, theme).unwrap_or(theme.ink),
        },
        None => StripPart {
            text: MISSING.to_string(),
            color: theme.ink,
        },
    }
}

fn unit_letter(unit: TempUnit) -> char {
    match unit {
        TempUnit::Celsius => 'C',
        TempUnit::Fahrenheit => 'F',
    }
}

/// `"↓1.2 MB/s ↑120 KB/s"`, matching the footer's `NET` figures. An absent
/// reading falls back to the footer's `0 KB/s`; a machine with no network
/// section at all renders [`MISSING`].
fn net_value(net: Option<&crate::ipc::NetInfo>) -> String {
    match net {
        Some(net) => {
            let down = format_bytes_per_sec(finite(net.down_bps).unwrap_or(0.0));
            let up = format_bytes_per_sec(finite(net.up_bps).unwrap_or(0.0));
            format!("\u{2193}{down} \u{2191}{up}")
        }
        None => MISSING.to_string(),
    }
}

/// Lay `widths` out left to right after `cursor`, each reading introduced by a
/// thin separator rule `sep_gap` away on either side, and stop at the first
/// reading that would cross `avail_right`.
///
/// Returns `(rule_x, text_x)` per surviving reading. The survivors are always
/// a prefix, in order; whatever does not fit is dropped from the right rather
/// than wrapped or overlapped. Pure, and the renderer paints at exactly these
/// offsets.
pub fn fit_plan(
    widths: &[f32],
    cursor: f32,
    sep_gap: f32,
    sep_width: f32,
    avail_right: f32,
) -> Vec<(f32, f32)> {
    let mut x = cursor;
    let mut plan = Vec::new();
    for width in widths {
        if !width.is_finite() || *width < 0.0 {
            break;
        }
        let rule_x = x + sep_gap;
        let text_x = rule_x + sep_width + sep_gap;
        if text_x + width > avail_right {
            break;
        }
        plan.push((rule_x, text_x));
        x = text_x + width;
    }
    plan
}

/// Draw the strip row into `ui` and report what the user did.
///
/// Left to right: the PerfWindow mark, the selected readings (each introduced
/// by a thin `border` rule, label `dim`, values in `ink` or the dashboard's own
/// warn/hot colours), then a compact Background-opacity slider and the expand
/// and close controls. A double-click anywhere on the readings expands. There
/// is no right-click menu: the window is only 28 px tall, so a popup would be
/// clipped to the strip, and the controls already cover expand and close.
///
/// The whole strip background is painted here, once, dimmed by the shared
/// `background_opacity` setting; the mark, the readings, the rules, the slider
/// and the controls stay solid.
pub fn strip_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    snapshot: Option<&Snapshot>,
    config: &Config,
) -> StripAction {
    let height = STRIP_HEIGHT.min(ui.available_height().max(0.0)).max(0.0);
    let mut action = StripAction::None;

    let frame = egui::Frame::NONE
        .fill(theme.surface(theme.chrome, config.background_opacity))
        .inner_margin(Margin::symmetric(PAD_X as i8, 0));

    let inner = frame.show(ui, |ui| {
        ui.set_min_height(height);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            // Reserve the trailing slider group and controls first: the
            // readings are measured against the room that is genuinely left
            // for them, and are the ones dropped once it runs out.
            let reserve = opacity_reserve(ui, theme);
            let opacity_w = opacity_group_width(&reserve);
            let body_w = readings_width(ui.available_width(), opacity_w);
            let (body_rect, body_response) =
                ui.allocate_exact_size(Vec2::new(body_w, height), Sense::click());
            paint_readings(ui, theme, body_rect, snapshot, config);

            if body_response.double_clicked() {
                action = StripAction::Expand;
            }

            ui.add_space(CONTROL_GAP);
            if let Some(value) =
                opacity_group(ui, theme, height, reserve, config.background_opacity)
            {
                action = StripAction::Opacity(value);
            }

            ui.add_space(CONTROL_GAP);
            let (expand_rect, expand_response) =
                ui.allocate_exact_size(Vec2::new(BUTTON_W, height.min(BUTTON_H)), Sense::click());
            if paint_control(ui, theme, expand_rect, expand_response.hovered()) {
                paint_expand_icon(ui, expand_rect, theme, expand_response.hovered());
            }
            if expand_response.clicked() {
                action = StripAction::Expand;
            }

            ui.add_space(BUTTON_GAP);
            let (close_rect, close_response) =
                ui.allocate_exact_size(Vec2::new(BUTTON_W, height.min(BUTTON_H)), Sense::click());
            if paint_control(ui, theme, close_rect, close_response.hovered()) {
                paint_close_icon(ui, close_rect, theme, close_response.hovered());
            }
            if close_response.clicked() {
                action = StripAction::Close;
            }
        });
    });

    // 1 px `border` rule along the strip's bottom edge, drawn after the frame
    // so it sits on top of the fill.
    let rect = inner.response.rect;
    let y = rect.bottom() - 0.5;
    ui.painter().add(Shape::line_segment(
        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
        Stroke::new(1.0_f32, theme.border),
    ));

    action
}

/// The `100 %` readout the opacity group's width is measured from. The widest
/// value reserves the slot, so the strip's layout never shifts as the setting
/// changes.
fn opacity_reserve(ui: &egui::Ui, theme: &Theme) -> Arc<Galley> {
    ui.painter().layout_no_wrap(
        "100%".to_owned(),
        FontId::new(FONT_SIZE, theme.font_data.egui()),
        theme.dim,
    )
}

/// The total width the opacity group occupies: its leading `border` rule, the
/// track with the knob's overhang at either end, and the value slot.
fn opacity_group_width(reserve: &Galley) -> f32 {
    SEP_GAP
        + SEP_WIDTH
        + SEP_GAP
        + OPACITY_KNOB / 2.0
        + OPACITY_TRACK_W
        + OPACITY_KNOB / 2.0
        + VALUE_GAP
        + reserve.size().x
}

/// The room left for the mark and the readings once the trailing opacity group
/// and controls are reserved.
fn readings_width(available: f32, opacity_w: f32) -> f32 {
    (available - opacity_w - BUTTON_W * 2.0 - BUTTON_GAP - CONTROL_GAP * 2.0).max(0.0)
}

/// Draw the compact opacity slider and report a new value when the user picked
/// one. Clicking or dragging anywhere on the track snaps to the nearest 5 %
/// stop, exactly like Settings; the wheel over the group steps 5 % per notch.
fn opacity_group(
    ui: &mut egui::Ui,
    theme: &Theme,
    height: f32,
    reserve: Arc<Galley>,
    value: u8,
) -> Option<u8> {
    let knob_r = OPACITY_KNOB / 2.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(opacity_group_width(&reserve), height),
        Sense::click_and_drag(),
    );
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let painter = ui.painter_at(rect);

    // The group is introduced by the same thin `border` rule the readings use.
    let rule_x = rect.left() + SEP_GAP;
    painter.add(Shape::line_segment(
        [
            Pos2::new(rule_x, rect.top() + SEP_INSET),
            Pos2::new(rule_x, rect.bottom() - SEP_INSET),
        ],
        Stroke::new(SEP_WIDTH, theme.border),
    ));

    let content_left = rule_x + SEP_WIDTH + SEP_GAP;
    let track = Rect::from_min_max(
        Pos2::new(
            content_left + knob_r,
            rect.center().y - OPACITY_TRACK_H / 2.0,
        ),
        Pos2::new(
            content_left + knob_r + OPACITY_TRACK_W,
            rect.center().y + OPACITY_TRACK_H / 2.0,
        ),
    );
    crate::ui::slider::paint(&painter, theme, track, value, OPACITY_KNOB);

    // The live value, left-aligned in the `100 %` slot reserved above.
    let value_galley = painter.layout_no_wrap(
        format!("{value}%"),
        FontId::new(FONT_SIZE, theme.font_data.egui()),
        theme.dim,
    );
    paint_galley(
        &painter,
        &value_galley,
        track.right() + knob_r + VALUE_GAP,
        rect.center().y,
        theme.dim,
    );

    let dragged = response.dragged_by(egui::PointerButton::Primary);
    if response.hovered() || dragged {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    // Primary button only: a right-click or right-drag on the slider does
    // nothing.
    let mut picked = None;
    if let Some(pos) = response.interact_pointer_pos() {
        if response.clicked() || dragged {
            picked = Some(crate::ui::slider::opacity_at(track, pos.x));
        }
    }
    if picked.is_none() && response.hovered() {
        let notches = wheel_notches(ui);
        if notches != 0 {
            picked = Some(crate::ui::slider::stepped(value, notches));
        }
    }
    picked.filter(|&next| next != value)
}

/// This frame's wheel notches, positive when the user scrolled up: one notch
/// steps the setting one 5 % stop.
fn wheel_notches(ui: &egui::Ui) -> i32 {
    let delta: f32 = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / 50.0,
                    egui::MouseWheelUnit::Page => delta.y,
                }),
                _ => None,
            })
            .sum()
    });
    delta.round() as i32
}

/// Paint the mark and the readings that fit `rect`.
fn paint_readings(
    ui: &egui::Ui,
    theme: &Theme,
    rect: Rect,
    snapshot: Option<&Snapshot>,
    config: &Config,
) {
    if !ui.is_rect_visible(rect) || rect.width() <= 0.0 {
        return;
    }
    let painter = ui.painter_at(rect);
    let data_font = FontId::new(FONT_SIZE, theme.font_data.egui());
    let display_font = FontId::new(FONT_SIZE, theme.font_display.egui());

    // The PerfWindow mark, in the display font like the title-bar wordmark.
    let mark = painter.layout_no_wrap(
        letter_spaced("\u{259a} PERFWINDOW"),
        display_font,
        theme.accent,
    );
    let cursor = rect.min.x + mark.size().x;
    paint_galley(&painter, &mark, rect.min.x, rect.center().y, theme.accent);

    // Measure every reading, then let `fit_plan` decide how many survive.
    let readings = items(snapshot, &config.mini_strip_values, config.unit, theme);
    let measured: Vec<MeasuredItem> = readings
        .iter()
        .map(|item| measure(&painter, theme, &data_font, item))
        .collect();
    let widths: Vec<f32> = measured.iter().map(|m| m.width).collect();
    let plan = fit_plan(&widths, cursor, SEP_GAP, SEP_WIDTH, rect.right());

    for (item, (rule_x, text_x)) in measured.iter().zip(plan) {
        painter.add(Shape::line_segment(
            [
                Pos2::new(rule_x, rect.top() + SEP_INSET),
                Pos2::new(rule_x, rect.bottom() - SEP_INSET),
            ],
            Stroke::new(SEP_WIDTH, theme.border),
        ));

        let mut x = text_x;
        paint_galley(&painter, &item.label, x, rect.center().y, theme.dim);
        x += item.label.size().x;
        for (i, (galley, color)) in item.parts.iter().enumerate() {
            x += if i == 0 { LABEL_GAP } else { VALUE_GAP };
            paint_galley(&painter, galley, x, rect.center().y, *color);
            x += galley.size().x;
        }
    }
}

/// One reading's laid-out galleys plus the width the fit maths works with.
struct MeasuredItem {
    label: Arc<Galley>,
    parts: Vec<(Arc<Galley>, Color32)>,
    width: f32,
}

fn measure(
    painter: &egui::Painter,
    theme: &Theme,
    font: &FontId,
    item: &StripItem,
) -> MeasuredItem {
    let label = painter.layout_no_wrap(item.label.to_owned(), font.clone(), theme.dim);
    let mut parts = Vec::with_capacity(item.parts.len());
    let mut width = label.size().x;
    for (i, part) in item.parts.iter().enumerate() {
        let galley = painter.layout_no_wrap(part.text.clone(), font.clone(), part.color);
        width += if i == 0 { LABEL_GAP } else { VALUE_GAP };
        width += galley.size().x;
        parts.push((galley, part.color));
    }
    MeasuredItem {
        label,
        parts,
        width,
    }
}

/// Paint a left-aligned galley vertically centred on `centre_y`. The galleys
/// are laid out with their final colour, so `color` only has to agree with it.
fn paint_galley(
    painter: &egui::Painter,
    galley: &Arc<Galley>,
    x: f32,
    centre_y: f32,
    color: Color32,
) {
    let pos = Pos2::new(x, centre_y - galley.size().y / 2.0);
    painter.galley(pos, Arc::clone(galley), color);
}

/// Paint a control's frame: a 1 px `border` box that fills `accent` and flips
/// its icon to `bg` on hover. Returns `true` when the control is large enough
/// to hold an icon.
fn paint_control(ui: &egui::Ui, theme: &Theme, rect: Rect, hovered: bool) -> bool {
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let painter = ui.painter_at(rect);
    if hovered {
        painter.rect_filled(rect, 0.0, theme.accent);
    }
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, if hovered { theme.accent } else { theme.border }),
        StrokeKind::Inside,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    rect.width() > ICON_INSET * 2.0 && rect.height() > ICON_INSET * 2.0
}

fn icon_stroke(theme: &Theme, hovered: bool) -> Stroke {
    Stroke::new(1.5_f32, if hovered { theme.bg } else { theme.dim })
}

/// The expand mark: two arrows pointing into opposite corners, drawn from six
/// line segments so the strip carries no glyph-font dependency. Clicking it
/// restores the full window.
fn paint_expand_icon(ui: &egui::Ui, rect: Rect, theme: &Theme, hovered: bool) {
    let painter = ui.painter_at(rect);
    let icon = rect.shrink(ICON_INSET);
    let stroke = icon_stroke(theme, hovered);
    let arm = (icon.width().min(icon.height()) * 0.4).max(2.0);

    // Top-right arrow: a shaft into the corner plus a two-segment head.
    painter.add(Shape::line_segment(
        [icon.center(), icon.right_top()],
        stroke,
    ));
    painter.add(Shape::line_segment(
        [
            Pos2::new(icon.right() - arm, icon.top()),
            Pos2::new(icon.right(), icon.top()),
        ],
        stroke,
    ));
    painter.add(Shape::line_segment(
        [
            Pos2::new(icon.right(), icon.top()),
            Pos2::new(icon.right(), icon.top() + arm),
        ],
        stroke,
    ));

    // Bottom-left arrow, mirrored.
    painter.add(Shape::line_segment(
        [icon.center(), icon.left_bottom()],
        stroke,
    ));
    painter.add(Shape::line_segment(
        [
            Pos2::new(icon.left() + arm, icon.bottom()),
            Pos2::new(icon.left(), icon.bottom()),
        ],
        stroke,
    ));
    painter.add(Shape::line_segment(
        [
            Pos2::new(icon.left(), icon.bottom()),
            Pos2::new(icon.left(), icon.bottom() - arm),
        ],
        stroke,
    ));
}

/// The close mark: two crossing line segments, no glyph font needed.
fn paint_close_icon(ui: &egui::Ui, rect: Rect, theme: &Theme, hovered: bool) {
    let painter = ui.painter_at(rect);
    let icon = rect.shrink(ICON_INSET);
    let stroke = icon_stroke(theme, hovered);
    painter.add(Shape::line_segment(
        [icon.left_top(), icon.right_bottom()],
        stroke,
    ));
    painter.add(Shape::line_segment(
        [icon.right_top(), icon.left_bottom()],
        stroke,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeId;
    use crate::ipc::parse_snapshot;

    /// A snapshot carrying every reading the strip can show, with the same
    /// figures the dashboard's own test fixtures use.
    fn fixture() -> Snapshot {
        parse_snapshot(
            r#"{"v":1,"ts":1747645200,
               "cpu":{"name":"Test CPU","load":23.4,"temp":58.0},
               "gpu":[{"name":"RTX 4070","kind":"discrete","load":41.2,"temp":62.0,
                       "vram_used_mb":6246.4,"vram_total_mb":8192.0}],
               "ram":{"used_mb":14540.8,"total_mb":32768,"load":47.2},
               "net":{"adapter":"Ethernet","down_bps":1258291.0,"up_bps":122880.0,
                      "link_bps":1000000000},
               "battery":{"charge_pct":87.4,"rate_w":-12.0}}"#,
        )
        .expect("the strip's test snapshot parses")
    }

    fn theme() -> Theme {
        Theme::for_id(ThemeId::Slate)
    }

    fn texts(item: &StripItem) -> Vec<String> {
        item.parts.iter().map(|p| p.text.clone()).collect()
    }

    fn labels(readings: &[StripItem]) -> Vec<&str> {
        readings.iter().map(|i| i.label).collect()
    }

    #[test]
    fn default_selection_renders_every_reading_but_battery() {
        let readings = items(
            Some(&fixture()),
            &MiniStripValues::default(),
            TempUnit::Celsius,
            &theme(),
        );
        assert_eq!(labels(&readings), ["CPU", "GPU", "VRAM", "RAM", "NET"]);
        assert_eq!(texts(&readings[0]), ["23%", "58°C"]);
        assert_eq!(texts(&readings[1]), ["41%", "62°C"]);
        assert_eq!(texts(&readings[2]), ["6.1 / 8.0 GB"]);
        assert_eq!(texts(&readings[3]), ["14.2 / 32.0 GB"]);
        assert_eq!(texts(&readings[4]), ["↓1.2 MB/s ↑120 KB/s"]);
    }

    #[test]
    fn the_selected_values_are_honoured_in_order() {
        let values = MiniStripValues {
            cpu_load: false,
            gpu_temp: false,
            vram: false,
            battery: true,
            ..MiniStripValues::default()
        };
        let readings = items(Some(&fixture()), &values, TempUnit::Celsius, &theme());
        assert_eq!(labels(&readings), ["CPU", "GPU", "RAM", "NET", "BAT"]);
        // CPU temperature without CPU load: one value, under the same label.
        assert_eq!(texts(&readings[0]), ["58°C"]);
        assert_eq!(texts(&readings[1]), ["41%"]);
        assert_eq!(texts(&readings[4]), ["87%"]);
    }

    #[test]
    fn missing_data_renders_an_em_dash_not_a_confident_zero() {
        let readings = items(
            None,
            &MiniStripValues::default(),
            TempUnit::Celsius,
            &theme(),
        );
        assert_eq!(labels(&readings), ["CPU", "GPU", "VRAM", "RAM", "NET"]);
        for reading in &readings {
            for text in texts(reading) {
                assert_eq!(text, MISSING);
            }
        }
    }

    #[test]
    fn a_battery_reading_needs_a_battery_to_show() {
        // Selected but no battery hardware: the reading is not built at all.
        let values = MiniStripValues {
            battery: true,
            ..MiniStripValues::default()
        };
        let no_battery =
            parse_snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","load":1.0}}"#).expect("parses");
        let readings = items(Some(&no_battery), &values, TempUnit::Celsius, &theme());
        assert!(!labels(&readings).contains(&"BAT"));
    }

    #[test]
    fn the_temperature_unit_is_honoured() {
        let values = MiniStripValues {
            cpu_load: false,
            gpu_load: false,
            gpu_temp: false,
            ..MiniStripValues::default()
        };
        let celsius = items(Some(&fixture()), &values, TempUnit::Celsius, &theme());
        let fahrenheit = items(Some(&fixture()), &values, TempUnit::Fahrenheit, &theme());
        assert_eq!(texts(&celsius[0]), ["58°C"]);
        assert_eq!(texts(&fahrenheit[0]), ["136°F"]);
    }

    #[test]
    fn a_missing_temperature_is_not_a_bare_unit_letter() {
        let part = temp_part(None, TempUnit::Celsius, &theme());
        assert_eq!(part.text, MISSING);
        let part = temp_part(Some(f64::NAN), TempUnit::Fahrenheit, &theme());
        assert_eq!(part.text, MISSING);
    }

    #[test]
    fn a_hot_temperature_carries_the_dashboards_warning_colour() {
        let theme = theme();
        assert_eq!(
            temp_part(Some(95.0), TempUnit::Celsius, &theme).color,
            theme.hot
        );
        assert_eq!(
            temp_part(Some(80.0), TempUnit::Celsius, &theme).color,
            theme.warn
        );
        assert_eq!(
            temp_part(Some(40.0), TempUnit::Celsius, &theme).color,
            theme.ink
        );
    }

    #[test]
    fn readings_fit_left_to_right_and_drop_from_the_right() {
        let widths = [100.0, 80.0, 60.0, 40.0];
        // 9 px of gap, a 1 px rule: 19 px of overhead per reading.
        let roomy = fit_plan(&widths, 0.0, 9.0, 1.0, 400.0);
        assert_eq!(roomy.len(), 4, "all four fit");
        assert_eq!(roomy[0], (9.0, 19.0));
        assert_eq!(roomy[1], (128.0, 138.0));
        assert_eq!(roomy[2], (227.0, 237.0));
        assert_eq!(roomy[3], (306.0, 316.0));
        // The offsets strictly increase: nothing can overlap.
        for pair in roomy.windows(2) {
            assert!(pair[0].1 < pair[1].0);
        }

        // Tighter: the fourth reading no longer fits and is dropped, not wrapped.
        let tight = fit_plan(&widths, 0.0, 9.0, 1.0, 340.0);
        assert_eq!(tight.len(), 3);
        assert_eq!(tight[2], (227.0, 237.0));
    }

    #[test]
    fn a_reading_that_does_not_fit_does_not_let_a_later_one_jump_ahead() {
        // The second reading is wider than the room left; it is dropped along
        // with everything after it, so the surviving order can never shuffle.
        let plan = fit_plan(&[100.0, 400.0, 10.0], 0.0, 9.0, 1.0, 200.0);
        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn no_room_means_no_readings() {
        assert!(fit_plan(&[10.0], 0.0, 9.0, 1.0, 10.0).is_empty());
        assert!(fit_plan(&[10.0], 50.0, 9.0, 1.0, 10.0).is_empty());
        assert!(fit_plan(&[], 0.0, 9.0, 1.0, 100.0).is_empty());
    }

    #[test]
    fn a_reading_that_lands_exactly_on_the_edge_still_fits() {
        // Rule 9..10, text 19..29, right edge 29.
        assert_eq!(fit_plan(&[10.0], 0.0, 9.0, 1.0, 29.0).len(), 1);
        assert_eq!(fit_plan(&[10.0], 0.0, 9.0, 1.0, 28.9).len(), 0);
    }

    #[test]
    fn the_opacity_group_is_reserved_before_the_readings_are_measured() {
        // The group, the two 17 px controls, the 4 px control gap and the two
        // 6 px control gaps: 200 - 90 - 34 - 4 - 12 = 60 px for the readings.
        let (available, group) = (200.0, 90.0);
        assert_eq!(readings_width(available, group), 60.0);

        // After the 40 px mark, neither 30 px reading fits in the reserved
        // room, and the plan drops both from the right.
        let widths = [30.0, 30.0];
        let reserved = fit_plan(
            &widths,
            40.0,
            SEP_GAP,
            SEP_WIDTH,
            readings_width(available, group),
        );
        assert!(
            reserved.is_empty(),
            "the reserved-away room holds no reading"
        );

        // The same readings fit when the whole row is theirs.
        let unreserved = fit_plan(&widths, 40.0, SEP_GAP, SEP_WIDTH, available);
        assert_eq!(unreserved.len(), 2);
    }

    #[test]
    fn a_narrow_monitor_keeps_the_group_and_drops_readings() {
        // A 900 px strip: the group and controls are still reserved, so the
        // readings that used to fit at 1920 px are the ones that go.
        let widths = [90.0, 90.0, 80.0, 80.0, 120.0];
        let group = 120.0;
        let body = readings_width(900.0, group);
        let plan = fit_plan(&widths, 180.0, SEP_GAP, SEP_WIDTH, body);
        assert!(plan.len() < widths.len(), "something must be dropped");
        assert!(plan.len() >= 2, "the leftmost readings stay");
        // The mark and every surviving reading sit left of the group.
        for (_, text_x) in &plan {
            assert!(*text_x < body);
        }
    }
}
