//! The storage component panel: a per-disk table.

use super::{card, panel_title};
use crate::format::{format_temp, TempUnit};
use crate::ipc::StorageInfo;
use crate::theme::Theme;
use crate::widgets::bars::bar_meter;
use crate::widgets::{temp_color, TempKind};
use egui::{FontId, Pos2, Sense, Stroke, Vec2};

/// Fixed width of the TEMP column (mockup `.pw-disk` grid `42px`).
const TEMP_COL_W: f32 = 42.0;
/// Fixed width of the CAPACITY column (mockup `.pw-disk` grid `80px`).
const CAP_COL_W: f32 = 80.0;
/// Horizontal gap between columns (mockup `.pw-disk { gap: 11px }`).
const COL_GAP: f32 = 11.0;
/// Flexible-column ratio: DISK gets `1.7fr`, ACTIVITY gets `1fr`.
const DISK_FR: f32 = 1.7;
/// Height of one disk row (mockup `.pw-disk { padding: 7px 0 }` over an 11 px line).
const DISK_ROW_H: f32 = 25.0;
/// Activity percentage at/above which the activity bar switches to its warn colour.
const ACTIVITY_WARN: f64 = 80.0;

/// Render the STORAGE card: a title, a faint column header and one row per disk.
///
/// Each disk row shows its kind-prefixed name, a temperature-coloured reading,
/// an activity bar and a right-aligned capacity figure, separated from the
/// previous row by a 1 px rule. With no disks the card still renders its title
/// (`"0 DISKS"`) and header — that is acceptable and cannot panic. Every absent
/// (`None`) reading degrades gracefully.
pub fn storage_panel(ui: &mut egui::Ui, theme: &Theme, disks: &[StorageInfo], unit: TempUnit) {
    card(ui, theme, |ui| {
        panel_title(
            ui,
            theme,
            "STORAGE",
            Some(&format!("{} DISKS", disks.len())),
        );

        header_row(ui, theme);

        for disk in disks {
            disk_row(ui, theme, disk, unit);
        }
    });
}

/// Split the available row width into the four column widths
/// (`DISK`, `TEMP`, `ACTIVITY`, `CAPACITY`) honouring the `1.7fr / 42 / 1fr / 80`
/// grid and the 11 px inter-column gaps.
fn column_widths(total_w: f32) -> [f32; 4] {
    let flexible = (total_w - TEMP_COL_W - CAP_COL_W - 3.0 * COL_GAP).max(0.0);
    let disk_w = flexible * (DISK_FR / (DISK_FR + 1.0));
    let activity_w = flexible - disk_w;
    [disk_w, TEMP_COL_W, activity_w, CAP_COL_W]
}

/// Draw the faint, upper-cased column-header row (mockup `.pw-disk-h`).
fn header_row(ui: &mut egui::Ui, theme: &Theme) {
    let total_w = ui.available_width();
    let widths = column_widths(total_w);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, 12.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let font = FontId::new(8.0, theme.font_data.egui());
    let labels = ["DISK", "TEMP", "ACTIVITY", "CAPACITY"];
    // TEMP is centred and CAPACITY right-aligned, mirroring their data cells.
    let aligns = [Align::Left, Align::Center, Align::Left, Align::Right];

    let mut x = rect.min.x;
    for ((label, &w), align) in labels.iter().zip(widths.iter()).zip(aligns.iter()) {
        let galley = painter.layout_no_wrap((*label).to_owned(), font.clone(), theme.faint);
        let tx = align.place(x, w, galley.size().x);
        let ty = rect.min.y + (rect.height() - galley.size().y) / 2.0;
        painter.galley(Pos2::new(tx, ty), galley, theme.faint);
        x += w + COL_GAP;
    }
}

/// Draw one disk row: a 1 px top rule then the four data cells.
fn disk_row(ui: &mut egui::Ui, theme: &Theme, disk: &StorageInfo, unit: TempUnit) {
    let total_w = ui.available_width();
    let widths = column_widths(total_w);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, DISK_ROW_H), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);

    // 1 px top rule separating this row from the previous one.
    painter.add(egui::Shape::line_segment(
        [
            Pos2::new(rect.min.x, rect.min.y),
            Pos2::new(rect.max.x, rect.min.y),
        ],
        Stroke::new(1.0, theme.border),
    ));

    let center_y = rect.center().y;

    // --- DISK: "<KIND> · <name>", ink, ellipsis-truncated. ---
    let name = format!("{} · {}", disk.kind.to_uppercase(), disk.name);
    let name_font = FontId::new(11.0, theme.font_data.egui());
    let mut name_galley = painter.layout_no_wrap(name.clone(), name_font.clone(), theme.ink);
    if name_galley.size().x > widths[0] {
        // Re-layout with a hard wrap width and an ellipsis so a long disk name
        // never spills into the TEMP column.
        let job = ellipsised_job(&name, name_font, theme.ink, widths[0]);
        name_galley = painter.layout_job(job);
    }
    let name_y = center_y - name_galley.size().y / 2.0;
    painter.galley(Pos2::new(rect.min.x, name_y), name_galley, theme.ink);

    // --- TEMP: centred, temperature-coloured when present. ---
    let temp_x0 = rect.min.x + widths[0] + COL_GAP;
    let temp_str = format_temp(disk.temp, unit);
    let temp_color = disk
        .temp
        .map(|t| temp_color(t, TempKind::Disk, theme))
        .unwrap_or(theme.dim);
    let temp_font = FontId::new(11.0, theme.font_data.egui());
    let temp_galley = painter.layout_no_wrap(temp_str, temp_font, temp_color);
    let temp_x = temp_x0 + (widths[1] - temp_galley.size().x) / 2.0;
    painter.galley(
        Pos2::new(temp_x, center_y - temp_galley.size().y / 2.0),
        temp_galley,
        temp_color,
    );

    // --- ACTIVITY: a bar_meter filled to activity/100, warn when high. ---
    let act_x0 = temp_x0 + widths[1] + COL_GAP;
    let activity = disk.activity.unwrap_or(0.0);
    let fraction = (activity / 100.0).clamp(0.0, 1.0) as f32;
    let warn = activity >= ACTIVITY_WARN;
    {
        // `bar_meter` spans the Ui's available width, so confine it to a child
        // Ui sized to exactly the ACTIVITY column and vertically centred.
        let bar_rect =
            egui::Rect::from_min_size(Pos2::new(act_x0, center_y - 3.0), Vec2::new(widths[2], 6.0));
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar_meter(&mut bar_ui, theme, fraction, warn);
    }

    // --- CAPACITY: right-aligned, dim. ---
    let cap_x0 = act_x0 + widths[2] + COL_GAP;
    let cap_str = format_capacity(disk.used_gb, disk.total_gb);
    let cap_font = FontId::new(11.0, theme.font_data.egui());
    let cap_galley = painter.layout_no_wrap(cap_str, cap_font, theme.dim);
    let cap_x = cap_x0 + widths[3] - cap_galley.size().x;
    painter.galley(
        Pos2::new(cap_x, center_y - cap_galley.size().y / 2.0),
        cap_galley,
        theme.dim,
    );
}

/// Format a `used_gb` / `total_gb` pair (both already in gigabytes).
///
/// Drives above 1000 GB are shown in terabytes (`"1.42 / 1.81 TB"`), smaller
/// ones in gigabytes (`"412 / 931 GB"`). A missing value yields `"—"`.
fn format_capacity(used_gb: Option<f64>, total_gb: Option<f64>) -> String {
    match (used_gb, total_gb) {
        (Some(used), Some(total)) => {
            if total >= 1000.0 {
                format!("{:.2} / {:.2} TB", used / 1000.0, total / 1000.0)
            } else {
                format!("{:.0} / {:.0} GB", used, total)
            }
        }
        _ => "—".to_string(),
    }
}

/// Build a single-line text layout that hard-wraps (and so ellipsises) at
/// `max_width`, used to truncate over-long disk names.
fn ellipsised_job(
    text: &str,
    font: FontId,
    color: egui::Color32,
    max_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::text::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    job
}

/// Horizontal alignment of a column's text within its cell.
#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    /// X coordinate at which to start drawing `text_w`-wide text inside a cell
    /// of width `cell_w` whose left edge is at `x0`.
    fn place(self, x0: f32, cell_w: f32, text_w: f32) -> f32 {
        match self {
            Align::Left => x0,
            Align::Center => x0 + (cell_w - text_w) / 2.0,
            Align::Right => x0 + cell_w - text_w,
        }
    }
}
