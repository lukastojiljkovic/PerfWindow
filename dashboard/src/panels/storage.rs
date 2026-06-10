//! The storage component panel: a per-disk table.

use super::{card, panel_title};
use crate::format::{finite, format_bytes_per_sec, format_temp, TempUnit};
use crate::ipc::StorageInfo;
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::widgets::bars::bar_meter;
use crate::widgets::{health_color, temp_color, TempKind};
use egui::{Color32, FontId, Pos2, Sense, Stroke, Vec2};

/// Fixed width of the TEMP column.
const TEMP_COL_W: f32 = 42.0;
/// Fixed width of the HEALTH column (sized to fit "100%" with breathing room).
const HEALTH_COL_W: f32 = 50.0;
/// Fixed width of the CAPACITY column.
const CAP_COL_W: f32 = 80.0;
/// Horizontal gap between columns.
const COL_GAP: f32 = 11.0;
/// Flexible-column ratio: DISK gets `1.7fr`, ACTIVITY gets `1fr`.
const DISK_FR: f32 = 1.7;
/// Height of one disk row (taller to fit a secondary `R\u{2193} … W\u{2191} …`
/// throughput line under the disk name).
const DISK_ROW_H: f32 = 38.0;
/// Activity percentage at/above which the activity bar switches to its warn colour.
const ACTIVITY_WARN: f64 = 80.0;

/// Render the STORAGE card: a title, a faint column header and one row per disk.
///
/// Each disk row shows its kind-prefixed name, a temperature-coloured reading,
/// an activity bar and a right-aligned capacity figure, separated from the
/// previous row by a 1 px rule. With no disks the card still renders its title
/// (`"0 DISKS"`) and header — that is acceptable and cannot panic. Every absent
/// (`None`) reading degrades gracefully.
pub fn storage_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    disks: &[StorageInfo],
    unit: TempUnit,
    _capacity: Capacity,
    min_h: f32,
) {
    // Storage card is laid out with a minimum span of 2 columns (see
    // `layout_spans` in ui/mod.rs), so its allocated width is always wide
    // enough to fit the TEMP column at every viable window size.
    let show_temp = true;

    card(ui, theme, min_h, |ui| {
        panel_title(
            ui,
            theme,
            "STORAGE",
            Some(&format!("{} DISKS", disks.len())),
        );

        header_row(ui, theme, show_temp);

        for disk in disks {
            disk_row(ui, theme, disk, unit, show_temp);
        }
    });
}

/// Split the available row width into the column widths honouring the
/// `1.7fr / 42 / 1fr / 50 / 80` grid (DISK, TEMP, ACTIVITY, HEALTH, CAPACITY)
/// and the 11 px inter-column gaps. When `show_temp` is `false` the TEMP
/// column is omitted entirely and its budget folded back into the flexible
/// pool — DISK and ACTIVITY grow to fill the space.
///
/// The flexible pool is floored at zero, so every returned width is
/// non-negative. If `total_w` is narrower than the fixed columns the cells
/// simply sum to more than `total_w`; the per-row `painter_at` clip in
/// `disk_row` trims that overflow harmlessly.
fn column_widths(total_w: f32, show_temp: bool) -> Vec<f32> {
    let temp_budget = if show_temp { TEMP_COL_W } else { 0.0 };
    let gap_count = if show_temp { 4.0 } else { 3.0 };
    let flexible =
        (total_w - temp_budget - HEALTH_COL_W - CAP_COL_W - gap_count * COL_GAP).max(0.0);
    let disk_w = flexible * (DISK_FR / (DISK_FR + 1.0));
    let activity_w = flexible - disk_w;
    if show_temp {
        vec![disk_w, TEMP_COL_W, activity_w, HEALTH_COL_W, CAP_COL_W]
    } else {
        vec![disk_w, activity_w, HEALTH_COL_W, CAP_COL_W]
    }
}

/// Draw the faint, upper-cased column-header row.
fn header_row(ui: &mut egui::Ui, theme: &Theme, show_temp: bool) {
    let total_w = ui.available_width();
    // Reject NaN / non-positive width before it reaches the tessellator (see
    // widgets/sparkline.rs for the failure mode — abort, not a panic).
    if !total_w.is_finite() || total_w <= 0.0 {
        return;
    }
    let widths = column_widths(total_w, show_temp);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, 12.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let font = FontId::new(8.0, theme.font_data.egui());
    // TEMP and HEALTH are centred; CAPACITY is right-aligned, mirroring their data cells.
    let (labels, aligns): (&[&str], &[Align]) = if show_temp {
        (
            &["DISK", "TEMP", "ACTIVITY", "HEALTH", "CAPACITY"],
            &[
                Align::Left,
                Align::Center,
                Align::Left,
                Align::Center,
                Align::Right,
            ],
        )
    } else {
        (
            &["DISK", "ACTIVITY", "HEALTH", "CAPACITY"],
            &[Align::Left, Align::Left, Align::Center, Align::Right],
        )
    };

    let mut x = rect.min.x;
    for ((label, &w), align) in labels.iter().zip(widths.iter()).zip(aligns.iter()) {
        let galley = painter.layout_no_wrap((*label).to_owned(), font.clone(), theme.faint);
        let tx = align.place(x, w, galley.size().x);
        let ty = rect.min.y + (rect.height() - galley.size().y) / 2.0;
        painter.galley(Pos2::new(tx, ty), galley, theme.faint);
        x += w + COL_GAP;
    }
}

/// Draw one disk row: a 1 px top rule then the four data cells. The full row
/// is a single hover target — hovering reveals SMART lifetime details
/// (power-on hours, cycle count, NVMe Available Spare) that do not fit on
/// the row itself.
fn disk_row(ui: &mut egui::Ui, theme: &Theme, disk: &StorageInfo, unit: TempUnit, show_temp: bool) {
    let total_w = ui.available_width();
    // Same degenerate-width guard as `header_row`.
    if !total_w.is_finite() || total_w <= 0.0 {
        return;
    }
    let widths = column_widths(total_w, show_temp);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(total_w, DISK_ROW_H), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    if let Some(tip_text) = lifetime_tooltip(disk) {
        response.on_hover_text(tip_text);
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

    // --- DISK: "<Kind> · <name>", ink, on the upper half. A secondary line
    // beneath shows current read/write throughput when both readings exist. ---
    let name = format!("{} · {}", kind_label(&disk.kind), disk.name);
    let name_font = FontId::new(11.0, theme.font_data.egui());
    let name_galley = painter.layout_job(ellipsised_job(&name, name_font, theme.ink, widths[0]));
    // Place the name near the top of the row so the secondary R/W line fits
    // underneath without overlapping the centred TEMP/ACTIVITY/HEALTH/CAPACITY cells.
    let name_y = rect.min.y + 4.0;
    let name_h = name_galley.size().y;
    painter.galley(Pos2::new(rect.min.x, name_y), name_galley, theme.ink);

    if let (Some(r), Some(w)) = (finite(disk.read_bps), finite(disk.write_bps)) {
        let rw_str = format!(
            "R\u{2193} {}   W\u{2191} {}",
            format_bytes_per_sec(r),
            format_bytes_per_sec(w),
        );
        let rw_font = FontId::new(9.0, theme.font_data.egui());
        let rw_galley = painter.layout_no_wrap(rw_str, rw_font, theme.dim);
        painter.galley(
            Pos2::new(rect.min.x, name_y + name_h + 2.0),
            rw_galley,
            theme.dim,
        );
    }

    // --- TEMP: centred, temperature-coloured when present. Skipped entirely
    // in single-column capacity (`show_temp == false`); the hover tooltip
    // still surfaces the figure when one is wanted. ---
    let mut col_x = rect.min.x + widths[0] + COL_GAP;
    let mut idx = 1;
    if show_temp {
        let temp = finite(disk.temp);
        let temp_str = format_temp(temp, unit);
        let temp_color = temp
            .map(|t| disk_temp_color(t, disk.temp_warn_c, disk.temp_crit_c, theme))
            .unwrap_or(theme.dim);
        let temp_font = FontId::new(11.0, theme.font_data.egui());
        let temp_galley = painter.layout_no_wrap(temp_str, temp_font, temp_color);
        let temp_x = col_x + (widths[idx] - temp_galley.size().x) / 2.0;
        painter.galley(
            Pos2::new(temp_x, center_y - temp_galley.size().y / 2.0),
            temp_galley,
            temp_color,
        );
        col_x += widths[idx] + COL_GAP;
        idx += 1;
    }

    // --- ACTIVITY: a bar_meter filled to activity/100, warn when high. ---
    let activity = finite(disk.activity).unwrap_or(0.0);
    let fraction = (activity / 100.0).clamp(0.0, 1.0) as f32;
    let warn = activity >= ACTIVITY_WARN;
    {
        // `bar_meter` spans the Ui's available width, so confine it to a child
        // Ui sized to exactly the ACTIVITY column and vertically centred.
        let bar_rect = egui::Rect::from_min_size(
            Pos2::new(col_x, center_y - 3.0),
            Vec2::new(widths[idx], 6.0),
        );
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar_meter(&mut bar_ui, theme, fraction, warn);
    }
    col_x += widths[idx] + COL_GAP;
    idx += 1;

    // --- HEALTH: centred, coloured by remaining-life severity. ---
    let health = finite(disk.health);
    let (health_str, health_col) = match health {
        Some(pct) => (format!("{:.0}%", pct), health_color(pct, theme)),
        None => ("—".to_string(), theme.dim),
    };
    let health_font = FontId::new(11.0, theme.font_data.egui());
    let health_galley = painter.layout_no_wrap(health_str, health_font, health_col);
    let health_x = col_x + (widths[idx] - health_galley.size().x) / 2.0;
    painter.galley(
        Pos2::new(health_x, center_y - health_galley.size().y / 2.0),
        health_galley,
        health_col,
    );
    col_x += widths[idx] + COL_GAP;
    idx += 1;

    // --- CAPACITY: right-aligned, dim. ---
    let cap_str = format_capacity(finite(disk.used_gb), finite(disk.total_gb));
    let cap_font = FontId::new(11.0, theme.font_data.egui());
    let cap_galley = painter.layout_no_wrap(cap_str, cap_font, theme.dim);
    let cap_x = col_x + widths[idx] - cap_galley.size().x;
    painter.galley(
        Pos2::new(cap_x, center_y - cap_galley.size().y / 2.0),
        cap_galley,
        theme.dim,
    );
}

/// ok / warn / hot colour for a disk temperature. When the drive publishes
/// its own thermal thresholds (and they are coherent: warn < crit), classify
/// against those — ok below warn, warn band up to crit, hot at/after crit.
/// Otherwise fall back to the fixed [`TempKind::Disk`] bands.
fn disk_temp_color(
    celsius: f64,
    warn_c: Option<f64>,
    crit_c: Option<f64>,
    theme: &Theme,
) -> Color32 {
    match (finite(warn_c), finite(crit_c)) {
        (Some(warn), Some(crit)) if warn < crit => {
            if celsius >= crit {
                theme.hot
            } else if celsius >= warn {
                theme.warn
            } else {
                theme.ok
            }
        }
        _ => temp_color(celsius, TempKind::Disk, theme),
    }
}

/// Compose a multi-line hover tooltip for a disk row. Returns `None` when no
/// lifetime metric is present (older `sensord` builds, drives without SMART).
fn lifetime_tooltip(disk: &StorageInfo) -> Option<String> {
    if disk.power_on_hours.is_none()
        && disk.power_on_count.is_none()
        && disk.available_spare_pct.is_none()
        && disk.read_bps.is_none()
        && disk.write_bps.is_none()
        && disk.data_written_gb.is_none()
        && disk.percentage_used_pct.is_none()
    {
        return None;
    }
    let mut out = format!("{} · {}\n", kind_label(&disk.kind), disk.name);
    if let (Some(r), Some(w)) = (finite(disk.read_bps), finite(disk.write_bps)) {
        out.push_str(&format!(
            "  Read   {}\n  Write  {}\n",
            format_bytes_per_sec(r),
            format_bytes_per_sec(w),
        ));
    }
    if let Some(h) = disk.power_on_hours {
        out.push_str(&format!(
            "  Power-on hours    {}\n",
            format_power_on_hours(h)
        ));
    }
    if let Some(c) = disk.power_on_count {
        out.push_str(&format!("  Power-on cycles   {}\n", c));
    }
    if let Some(gb) = finite(disk.data_written_gb) {
        out.push_str(&format!("  Data written      {}\n", format_data_volume(gb)));
    }
    if let Some(p) = finite(disk.percentage_used_pct) {
        out.push_str(&format!("  NVMe wear         {:.0} %\n", p));
    }
    if let Some(s) = finite(disk.available_spare_pct) {
        out.push_str(&format!("  NVMe spare        {:.0} %\n", s));
    }
    Some(out.trim_end().to_string())
}

/// Format a lifetime data volume given in GB: terabytes with one decimal once
/// the figure crosses 1000 GB, whole gigabytes below.
fn format_data_volume(gb: f64) -> String {
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else {
        format!("{:.0} GB", gb)
    }
}

/// Format cumulative power-on hours with a parenthetical year/day breakdown
/// once the figure exceeds a year. `5057 h (≈ 0 y 211 d)`.
fn format_power_on_hours(hours: i64) -> String {
    if hours <= 0 {
        return format!("{} h", hours);
    }
    let days_total = hours / 24;
    let years = days_total / 365;
    let days = days_total % 365;
    format!("{} h  (\u{2248} {} y {} d)", hours, years, days)
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

/// Display form of a disk `kind` tag. `sensord` emits lower-case tags
/// (`"nvme"`, `"ssd"`, `"hdd"`); this restores their conventional casing.
/// An unrecognised tag is upper-cased as a fallback.
fn kind_label(kind: &str) -> String {
    match kind.to_ascii_lowercase().as_str() {
        "nvme" => "NVMe".to_string(),
        "ssd" => "SSD".to_string(),
        "hdd" => "HDD".to_string(),
        _ => kind.to_uppercase(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeId;

    fn theme() -> Theme {
        Theme::for_id(ThemeId::Slate)
    }

    #[test]
    fn disk_temp_color_uses_the_drive_thresholds_when_present() {
        let t = theme();
        // NVMe-style thresholds far above the fixed Disk bands (50/60).
        let warn = Some(82.0);
        let crit = Some(85.0);
        assert_eq!(disk_temp_color(70.0, warn, crit, &t), t.ok);
        assert_eq!(disk_temp_color(82.0, warn, crit, &t), t.warn);
        assert_eq!(disk_temp_color(84.9, warn, crit, &t), t.warn);
        assert_eq!(disk_temp_color(85.0, warn, crit, &t), t.hot);
    }

    #[test]
    fn disk_temp_color_falls_back_to_fixed_bands() {
        let t = theme();
        // Absent, partial, non-finite or incoherent thresholds all fall back.
        for (warn, crit) in [
            (None, None),
            (Some(82.0), None),
            (None, Some(85.0)),
            (Some(f64::NAN), Some(85.0)),
            (Some(85.0), Some(82.0)),
        ] {
            assert_eq!(disk_temp_color(45.0, warn, crit, &t), t.ok);
            assert_eq!(disk_temp_color(55.0, warn, crit, &t), t.warn);
            assert_eq!(disk_temp_color(65.0, warn, crit, &t), t.hot);
        }
    }

    #[test]
    fn data_volume_switches_to_terabytes_at_1000_gb() {
        assert_eq!(format_data_volume(20480.0), "20.5 TB");
        assert_eq!(format_data_volume(1000.0), "1.0 TB");
        assert_eq!(format_data_volume(999.4), "999 GB");
        assert_eq!(format_data_volume(0.0), "0 GB");
    }

    fn bare_disk() -> StorageInfo {
        StorageInfo {
            name: "Test SSD".into(),
            kind: "nvme".into(),
            temp: None,
            activity: None,
            used_gb: None,
            total_gb: None,
            health: None,
            read_bps: None,
            write_bps: None,
            power_on_hours: None,
            power_on_count: None,
            available_spare_pct: None,
            percentage_used_pct: None,
            temp_warn_c: None,
            temp_crit_c: None,
            data_read_gb: None,
            data_written_gb: None,
        }
    }

    #[test]
    fn lifetime_tooltip_includes_written_volume_and_wear() {
        let disk = StorageInfo {
            data_written_gb: Some(20480.0),
            percentage_used_pct: Some(3.4),
            ..bare_disk()
        };
        let tip = lifetime_tooltip(&disk).expect("new lifetime fields summon the tooltip");
        assert!(tip.contains("Data written      20.5 TB"));
        assert!(tip.contains("NVMe wear         3 %"));
    }

    #[test]
    fn lifetime_tooltip_stays_absent_without_any_metric() {
        assert!(lifetime_tooltip(&bare_disk()).is_none());
        // Non-finite new fields count as present for the gate but render no
        // line — the header alone is still a valid tooltip, never a panic.
        let nan_disk = StorageInfo {
            data_written_gb: Some(f64::NAN),
            ..bare_disk()
        };
        let tip = lifetime_tooltip(&nan_disk).expect("field present");
        assert!(!tip.contains("Data written"));
    }
}
