use crate::theme::Theme;
use egui::{Rect, Response, Sense, Vec2};

/// A thin 6 px-tall horizontal progress bar spanning the available width.
///
/// `fraction` is 0.0–1.0 (clamped). The background uses `theme.track`; the
/// fill uses `theme.accent`, or `theme.warn` when `warn` is `true`.
pub fn bar_meter(ui: &mut egui::Ui, theme: &Theme, fraction: f32, warn: bool) -> Response {
    let available_w = ui.available_width();
    // Reject NaN / negative width before it reaches the tessellator (see the
    // sparkline comment for the failure mode — abort, not a panic).
    if !available_w.is_finite() || available_w <= 0.0 {
        let (_, response) = ui.allocate_exact_size(Vec2::new(0.0, 6.0), Sense::hover());
        return response;
    }
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
///
/// `p_core_count = Some(n)` switches to hybrid-CPU coloring: the first `n`
/// bars (P-Cores) paint at full `theme.accent` opacity and a wider visual gap
/// separates them from the trailing E-Core bars, which paint dimmer.
/// `None` paints uniformly at the standard 0.72 opacity.
pub fn core_strip(ui: &mut egui::Ui, theme: &Theme, loads: &[f32], p_core_count: Option<usize>) {
    if loads.is_empty() {
        return;
    }

    let strip_h = 24.0;
    let gap = 3.0;
    // Extra horizontal pixels inserted once, between the P-Core cluster and
    // the E-Core cluster, so the eye can find the boundary even when every
    // bar has the same load.
    let cluster_gap_extra = 8.0;
    let available_w = ui.available_width();
    if !available_w.is_finite() || available_w <= 0.0 {
        return;
    }
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, strip_h), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let n = loads.len() as f32;
    let total_gap = gap * (n - 1.0);
    let extra = if matches!(p_core_count, Some(c) if c > 0 && c < loads.len()) {
        cluster_gap_extra
    } else {
        0.0
    };
    // When the available width is too small for `n` bars + gaps, the `.max(1.0)`
    // floor trades exact tiling for a 1 px minimum: trailing bars then overflow
    // `rect`, which `painter_at(rect)` clipping absorbs harmlessly.
    let bar_w = ((available_w - total_gap - extra) / n).max(1.0);

    let p_color = theme.accent.gamma_multiply(0.85);
    let e_color = theme.accent.gamma_multiply(0.45);
    let uniform_color = theme.accent.gamma_multiply(0.72);

    let mut x = rect.min.x;
    for (i, &load) in loads.iter().enumerate() {
        let fraction = (load / 100.0).clamp(0.0, 1.0);
        let bar_h = (fraction * strip_h).max(2.0);

        let color = match p_core_count {
            Some(c) if i < c => p_color,
            Some(_) => e_color,
            None => uniform_color,
        };

        let bar_rect = Rect::from_min_size(
            egui::Pos2::new(x, rect.max.y - bar_h),
            Vec2::new(bar_w, bar_h),
        );
        painter.rect_filled(bar_rect, 0.0, color);

        x += bar_w + gap;
        // Insert the cluster gap after the last P-Core bar.
        if matches!(p_core_count, Some(c) if i + 1 == c) {
            x += cluster_gap_extra;
        }
    }
}

/// Default heat-map cell size, comfortable for a temperature label + corner tag.
const CELL_W: f32 = 44.0;
const CELL_H: f32 = 28.0;
/// Compact fallback cell size for high-core-count machines; the corner tag is
/// dropped at this size.
const COMPACT_CELL_W: f32 = 32.0;
const COMPACT_CELL_H: f32 = 18.0;
/// Gap between heat-map cells.
const CELL_GAP: f32 = 4.0;
const CORNER_PAD: f32 = 3.0;

/// How the heat-map lays out under a width/height budget. Pure data so the
/// clamp logic is unit-testable without egui.
#[derive(Debug, Clone, Copy, PartialEq)]
struct GridPlan {
    cols: usize,
    rows: usize,
    cell_w: f32,
    cell_h: f32,
    compact: bool,
    /// Cores rendered as cells; `hidden` overflow into a trailing "+N" cell.
    shown: usize,
    hidden: usize,
}

impl GridPlan {
    fn total_height(&self) -> f32 {
        self.rows as f32 * self.cell_h + (self.rows as f32 - 1.0).max(0.0) * CELL_GAP
    }
}

/// Choose a layout for `n` cores inside `avail_w` x `max_h`: default cells if
/// every row fits, compact cells otherwise, and — when even compact cells
/// cannot seat every core — as many as fit with the last slot reserved for a
/// "+N" overflow marker. Always plans at least one row so a tiny budget still
/// shows something rather than nothing.
fn plan_grid(n: usize, avail_w: f32, max_h: f32) -> GridPlan {
    let cols_for =
        |cell_w: f32| (((avail_w + CELL_GAP) / (cell_w + CELL_GAP)).floor() as usize).max(1);
    let rows_budget = |cell_h: f32| ((max_h + CELL_GAP) / (cell_h + CELL_GAP)).floor() as usize;

    // Default cells only when at least one default row genuinely fits — a
    // budget below CELL_H must fall through to compact cells.
    let cols = cols_for(CELL_W);
    let rows_needed = n.div_ceil(cols);
    if rows_needed <= rows_budget(CELL_H) {
        return GridPlan {
            cols,
            rows: rows_needed,
            cell_w: CELL_W,
            cell_h: CELL_H,
            compact: false,
            shown: n,
            hidden: 0,
        };
    }

    let cols = cols_for(COMPACT_CELL_W);
    let max_rows = rows_budget(COMPACT_CELL_H).max(1);
    let rows_needed = n.div_ceil(cols);
    if rows_needed <= max_rows {
        return GridPlan {
            cols,
            rows: rows_needed,
            cell_w: COMPACT_CELL_W,
            cell_h: COMPACT_CELL_H,
            compact: true,
            shown: n,
            hidden: 0,
        };
    }

    let capacity = cols * max_rows;
    let shown = capacity.saturating_sub(1);
    GridPlan {
        cols,
        rows: max_rows,
        cell_w: COMPACT_CELL_W,
        cell_h: COMPACT_CELL_H,
        compact: true,
        shown,
        hidden: n - shown,
    }
}

/// A 2-D heat-map of CPU cores: each cell is coloured by load (`theme.track`
/// → `theme.accent` → `theme.hot`) and labelled with the per-core temperature
/// when available, falling back to the load percentage.
///
/// `loads`, `temps` and `clocks` are parallel arrays; `temps[i]` /
/// `clocks[i]` may be `None` even when `loads[i]` is present. If `loads` is
/// shorter, the extras are ignored. Cells wrap into rows that fit the
/// available width, and the grid never exceeds `max_h`: when default-size
/// cells would overflow, it drops to compact cells, and beyond that caps the
/// cell count with a trailing "+N" overflow marker.
///
/// Hovering a cell shows its tag, load, temperature and — when sensord
/// publishes per-core clocks — the core's current frequency.
///
/// `p_core_count = Some(n)` switches the corner tag from a flat `C0..Cn` to
/// the hybrid-CPU `P1..Pn` / `E1..Em` split, and tints P-Core borders with
/// `theme.accent` while E-Cores keep the standard `theme.border`. `None`
/// renders uniformly (older / non-hybrid CPUs).
#[allow(clippy::too_many_arguments)]
pub fn core_grid(
    ui: &mut egui::Ui,
    theme: &Theme,
    loads: &[f32],
    temps: &[Option<f32>],
    clocks: &[Option<f64>],
    unit: crate::format::TempUnit,
    p_core_count: Option<usize>,
    max_h: f32,
) {
    if loads.is_empty() {
        return;
    }

    let available_w = ui.available_width();
    if !available_w.is_finite() || available_w <= 0.0 {
        return;
    }
    let plan = plan_grid(loads.len(), available_w, max_h);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(available_w, plan.total_height()), Sense::hover());

    // Per-cell hover detail. Resolved before painting so the `response` move
    // into `on_hover_text_at_pointer` happens after the borrow-free hit test.
    if let Some(i) = response
        .hover_pos()
        .and_then(|pos| hovered_cell(&plan, rect.min, pos))
        .filter(|&i| i < plan.shown && i < loads.len())
    {
        let text = cell_hover_text(
            i,
            loads[i],
            temps.get(i).copied().flatten(),
            clocks.get(i).copied().flatten(),
            p_core_count,
            unit,
        );
        response.on_hover_text_at_pointer(text);
    }

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let label_size = if plan.compact { 9.0 } else { 11.0 };
    let label_font = egui::FontId::new(label_size, theme.font_data.egui());
    let corner_font = egui::FontId::new(8.0, theme.font_data.egui());

    let cell_rect_at = |slot: usize| -> Rect {
        let col = slot % plan.cols;
        let row = slot / plan.cols;
        let cell_min = egui::Pos2::new(
            rect.min.x + col as f32 * (plan.cell_w + CELL_GAP),
            rect.min.y + row as f32 * (plan.cell_h + CELL_GAP),
        );
        Rect::from_min_size(cell_min, Vec2::new(plan.cell_w, plan.cell_h))
    };

    for (i, &load) in loads.iter().take(plan.shown).enumerate() {
        let cell_rect = cell_rect_at(i);

        // Background colour: ramp track → accent → hot as load grows.
        let load_frac = (load / 100.0).clamp(0.0, 1.0);
        let bg = if load_frac < 0.5 {
            lerp_color(theme.track, theme.accent, load_frac * 2.0)
        } else {
            lerp_color(theme.accent, theme.hot, (load_frac - 0.5) * 2.0)
        };
        painter.rect_filled(cell_rect, 0.0, bg);

        // On hybrid CPUs, P-Core cells get an accent-tinted border so the
        // performance cluster reads at a glance; E-Cores keep the standard
        // border colour. Non-hybrid renders uniformly.
        let is_p_core = matches!(p_core_count, Some(n) if i < n);
        let border_color = if is_p_core {
            theme.accent
        } else {
            theme.border
        };
        painter.rect_stroke(
            cell_rect,
            0.0,
            egui::Stroke::new(1.0, border_color),
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

        // Corner tag — dropped at compact size where it would collide with
        // the centred label. Hybrid CPUs: "P1..Pn" then "E1..Em"; else "C0..".
        if !plan.compact {
            let corner_galley =
                painter.layout_no_wrap(core_tag(i, p_core_count), corner_font.clone(), theme.dim);
            painter.galley(
                egui::Pos2::new(cell_rect.min.x + CORNER_PAD, cell_rect.min.y + 1.0),
                corner_galley,
                theme.dim,
            );
        }
    }

    // Overflow marker: a dim "+N" cell in the slot after the last shown core.
    if plan.hidden > 0 {
        let cell_rect = cell_rect_at(plan.shown);
        painter.rect_filled(cell_rect, 0.0, theme.track);
        painter.rect_stroke(
            cell_rect,
            0.0,
            egui::Stroke::new(1.0, theme.border),
            egui::StrokeKind::Inside,
        );
        let galley =
            painter.layout_no_wrap(format!("+{}", plan.hidden), label_font.clone(), theme.dim);
        let text_pos = egui::Pos2::new(
            cell_rect.center().x - galley.size().x / 2.0,
            cell_rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(text_pos, galley, theme.dim);
    }
}

/// Display tag for core `i`: hybrid CPUs read `P1..Pn` / `E1..Em`, the rest
/// a flat `C0..Cn`. Shared by the cell corner and the hover text.
fn core_tag(i: usize, p_core_count: Option<usize>) -> String {
    match p_core_count {
        Some(n) if i < n => format!("P{}", i + 1),
        Some(n) => format!("E{}", i - n + 1),
        None => format!("C{i}"),
    }
}

/// Which cell slot `pos` falls into, or `None` when it lands outside the
/// grid or inside an inter-cell gap. Pure so the hit test is unit-testable
/// without egui.
fn hovered_cell(plan: &GridPlan, origin: egui::Pos2, pos: egui::Pos2) -> Option<usize> {
    let dx = pos.x - origin.x;
    let dy = pos.y - origin.y;
    if dx < 0.0 || dy < 0.0 {
        return None;
    }
    let col = (dx / (plan.cell_w + CELL_GAP)) as usize;
    let row = (dy / (plan.cell_h + CELL_GAP)) as usize;
    if col >= plan.cols || row >= plan.rows {
        return None;
    }
    let in_cell_x = dx - col as f32 * (plan.cell_w + CELL_GAP) <= plan.cell_w;
    let in_cell_y = dy - row as f32 * (plan.cell_h + CELL_GAP) <= plan.cell_h;
    (in_cell_x && in_cell_y).then_some(row * plan.cols + col)
}

/// Hover line for one core cell: tag, load, then temperature and clock when
/// present — `"P1 · 47 % · 60° · 4.40 GHz"`. Non-finite readings are skipped
/// rather than rendered.
fn cell_hover_text(
    i: usize,
    load: f32,
    temp: Option<f32>,
    clock_mhz: Option<f64>,
    p_core_count: Option<usize>,
    unit: crate::format::TempUnit,
) -> String {
    let mut out = format!(
        "{} \u{00b7} {} %",
        core_tag(i, p_core_count),
        load.round() as i64
    );
    if let Some(t) = temp.filter(|t| t.is_finite()) {
        out.push_str(&format!(
            " \u{00b7} {}",
            crate::format::format_temp(Some(t as f64), unit)
        ));
    }
    if let Some(mhz) = clock_mhz.filter(|m| m.is_finite()) {
        out.push_str(&format!(" \u{00b7} {:.2} GHz", mhz / 1000.0));
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_grids_keep_default_cells_and_show_everything() {
        let plan = plan_grid(24, 270.0, 400.0);
        assert!(!plan.compact);
        assert_eq!(plan.shown, 24);
        assert_eq!(plan.hidden, 0);
        assert!(plan.total_height() <= 400.0);
    }

    #[test]
    fn tight_height_drops_to_compact_cells() {
        // 24 cores in 110 px: default cells need 5 rows (156 px) — compact
        // cells (18 px) seat them within the budget.
        let plan = plan_grid(24, 270.0, 110.0);
        assert!(plan.compact);
        assert_eq!(plan.shown, 24);
        assert_eq!(plan.hidden, 0);
        assert!(plan.total_height() <= 110.0);
    }

    #[test]
    fn overflow_caps_cells_and_reports_hidden_count() {
        let plan = plan_grid(256, 270.0, 110.0);
        assert!(plan.compact);
        assert!(plan.hidden > 0);
        assert_eq!(plan.shown + plan.hidden, 256);
        // The shown cells plus the "+N" marker fit the planned slots.
        assert!(plan.shown < plan.cols * plan.rows);
        assert!(plan.total_height() <= 110.0);
    }

    #[test]
    fn grid_height_never_exceeds_the_budget_beyond_one_minimum_row() {
        for n in [1usize, 2, 8, 16, 64, 128, 512] {
            for w in [50.0f32, 120.0, 270.0, 800.0] {
                for h in [10.0f32, 40.0, 110.0, 600.0] {
                    let plan = plan_grid(n, w, h);
                    assert_eq!(plan.shown + plan.hidden, n, "n={n} w={w} h={h}");
                    // One compact row is the floor even when the budget is
                    // smaller than a single row.
                    let floor = COMPACT_CELL_H.max(h);
                    assert!(
                        plan.total_height() <= floor,
                        "n={n} w={w} h={h} height={}",
                        plan.total_height()
                    );
                }
            }
        }
    }

    #[test]
    fn degenerate_budget_still_plans_one_row() {
        let plan = plan_grid(8, 100.0, f32::NAN);
        assert!(plan.rows >= 1);
        let plan = plan_grid(8, 100.0, 0.0);
        assert_eq!(plan.rows, 1);
    }

    #[test]
    fn core_tags_follow_the_hybrid_split() {
        assert_eq!(core_tag(0, None), "C0");
        assert_eq!(core_tag(3, None), "C3");
        assert_eq!(core_tag(0, Some(2)), "P1");
        assert_eq!(core_tag(1, Some(2)), "P2");
        assert_eq!(core_tag(2, Some(2)), "E1");
        assert_eq!(core_tag(5, Some(2)), "E4");
    }

    #[test]
    fn hovered_cell_maps_positions_and_rejects_gaps() {
        let plan = plan_grid(8, 270.0, 400.0); // default cells: 44x28, gap 4
        assert_eq!(plan.cols, 5);
        let origin = egui::Pos2::new(100.0, 50.0);

        // Centre of the first cell.
        let first = egui::Pos2::new(100.0 + 22.0, 50.0 + 14.0);
        assert_eq!(hovered_cell(&plan, origin, first), Some(0));
        // Second row, second column: slot = cols + 1.
        let slot = egui::Pos2::new(100.0 + 48.0 + 22.0, 50.0 + 32.0 + 14.0);
        assert_eq!(hovered_cell(&plan, origin, slot), Some(plan.cols + 1));
        // Inside the horizontal gap between cells 0 and 1.
        let gap = egui::Pos2::new(100.0 + 46.0, 50.0 + 14.0);
        assert_eq!(hovered_cell(&plan, origin, gap), None);
        // Above / left of the grid entirely.
        assert_eq!(
            hovered_cell(&plan, origin, egui::Pos2::new(50.0, 10.0)),
            None
        );
    }

    #[test]
    fn cell_hover_text_appends_only_present_readings() {
        use crate::format::TempUnit;
        assert_eq!(
            cell_hover_text(
                0,
                47.4,
                Some(60.0),
                Some(4400.0),
                Some(8),
                TempUnit::Celsius
            ),
            "P1 \u{00b7} 47 % \u{00b7} 60° \u{00b7} 4.40 GHz"
        );
        assert_eq!(
            cell_hover_text(2, 12.0, None, None, None, TempUnit::Celsius),
            "C2 \u{00b7} 12 %"
        );
        // Non-finite readings are skipped, never rendered.
        assert_eq!(
            cell_hover_text(
                0,
                5.0,
                Some(f32::NAN),
                Some(f64::INFINITY),
                None,
                TempUnit::Celsius
            ),
            "C0 \u{00b7} 5 %"
        );
    }
}
