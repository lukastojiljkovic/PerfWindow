//! The pieces the dashboard's cards are drawn from, after the window's own:
//! the card frame, stat rows and grids, bar meters, braille charts and
//! sparklines.

use super::Palette;
use crate::cli::report::clean;
use crate::history::{samples_or_empty, RingBuffer};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Chart, Dataset, GraphType, Paragraph, RenderDirection, Sparkline,
};
use ratatui::Frame;

/// The narrowest a stat (label and value) is drawn in a stat grid.
pub(super) const STAT_WIDTH: u16 = 18;
/// Blank cells between the columns of a stat grid.
const STAT_GAP: u16 = 2;

/// Draw a card's frame as the window draws it: the panel fill, a border with
/// the accent along its top edge, the upper-case title in accent on the left
/// and `sub` faint on the right. Returns the area inside.
pub(super) fn card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    sub: Option<&str>,
    p: &Palette,
) -> Rect {
    let block = Block::bordered()
        .border_style(p.fg(p.theme.border))
        .style(Style::new().bg(p.color(p.theme.panel)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let top = Rect { height: 1, ..area };
    frame.buffer_mut().set_style(top, p.fg(p.theme.accent));

    let title = format!(" {} ", title.to_uppercase());
    let room = usize::from(area.width).saturating_sub(title.chars().count() + 6);
    let mut titles = Block::new().title_top(Line::styled(
        title,
        p.fg(p.theme.accent).add_modifier(Modifier::BOLD),
    ));
    let sub = sub.map(|s| fit(&clean(s).to_uppercase(), room));
    if let Some(sub) = sub.filter(|s| !s.is_empty()) {
        titles =
            titles.title_top(Line::styled(format!(" {sub} "), p.fg(p.theme.faint)).right_aligned());
    }
    let title_row = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..top
    };
    frame.render_widget(titles, title_row);
    inner
}

/// A reading as the window's stat rows show it.
pub(super) struct Stat {
    label: String,
    value: String,
    style: Style,
}

impl Stat {
    /// A stat with its label and value cleared of control characters.
    pub(super) fn new(label: &str, value: &str, style: Style) -> Stat {
        Stat {
            label: clean(label),
            value: clean(value),
            style,
        }
    }
}

/// One stat, `width` cells wide: the label dim on the left and the value on
/// the right, cut with `…` where the two do not fit.
pub(super) fn stat_line(stat: &Stat, width: u16, p: &Palette) -> Vec<Span<'static>> {
    let width = usize::from(width);
    let label_width = stat.label.chars().count();
    let value = fit(&stat.value, width.saturating_sub(label_width + 1));
    let gap = width.saturating_sub(label_width + value.chars().count());
    vec![
        Span::styled(stat.label.clone(), p.label()),
        Span::raw(" ".repeat(gap)),
        Span::styled(value, stat.style),
    ]
}

/// `stats` in as many columns of at least [`STAT_WIDTH`] cells as `area`
/// holds, filled row by row; rows past its height are left out.
pub(super) fn stat_grid(frame: &mut Frame, area: Rect, stats: &[Stat], p: &Palette) {
    let columns = grid_columns(area.width);
    let width = (area.width - STAT_GAP * (columns as u16 - 1)) / columns as u16;
    let gap = " ".repeat(usize::from(STAT_GAP));
    let lines: Vec<Line> = stats
        .chunks(columns)
        .take(usize::from(area.height))
        .map(|row| {
            let mut spans = Vec::new();
            for (i, stat) in row.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(gap.clone()));
                }
                spans.extend(stat_line(stat, width, p));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// The columns a stat grid has at `width`.
fn grid_columns(width: u16) -> usize {
    usize::from(((width + STAT_GAP) / (STAT_WIDTH + STAT_GAP)).max(1))
}

/// The rows a stat grid of `count` stats takes at `width`.
pub(super) fn grid_rows(count: usize, width: u16) -> u16 {
    count.div_ceil(grid_columns(width)) as u16
}

/// `text` cut to `width` characters, ending in `…` when cut.
pub(super) fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else if width == 0 {
        String::new()
    } else {
        text.chars().take(width - 1).chain(['…']).collect()
    }
}

/// A labelled bar meter on one line, like the window's: `label`, the bar
/// with `fraction` of it in `fill` and the rest in the track colour, then
/// `value`.
pub(super) fn bar_line(
    label: &str,
    fraction: f64,
    value: &str,
    fill: Style,
    width: u16,
    p: &Palette,
) -> Line<'static> {
    let label = format!("{label:<7}");
    let value = format!(" {value}");
    let room = usize::from(width).saturating_sub(label.chars().count() + value.chars().count());
    let filled = ((fraction.clamp(0.0, 1.0) * room as f64).round() as usize).min(room);
    Line::from(vec![
        Span::styled(label, p.label()),
        Span::styled("━".repeat(filled), fill),
        Span::styled("─".repeat(room - filled), p.fg(p.theme.track)),
        Span::styled(value, p.value()),
    ])
}

/// The samples of a history buffer, oldest first.
pub(super) fn samples(buffer: Option<&RingBuffer>) -> Vec<f32> {
    samples_or_empty(buffer).collect()
}

/// A braille chart of `series` on a 0 to 100 scale, with the newest sample
/// at the right edge and one sample per column of braille dots. The first
/// series is filled, as the window fills its sparklines; the rest are lines
/// over it. Absent samples are gaps.
pub(super) fn chart(frame: &mut Frame, area: Rect, series: &[(Vec<f32>, Style)], p: &Palette) {
    let columns = usize::from(area.width) * 2;
    let points: Vec<Vec<(f64, f64)>> = series
        .iter()
        .map(|(samples, _)| {
            let recent = &samples[samples.len().saturating_sub(columns)..];
            let offset = columns - recent.len();
            recent
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_finite())
                .map(|(i, &v)| ((offset + i) as f64, f64::from(v).clamp(0.0, 100.0)))
                .collect()
        })
        .collect();
    let datasets = points
        .iter()
        .zip(series)
        .enumerate()
        .map(|(i, (data, (_, style)))| {
            Dataset::default()
                .data(data)
                .marker(Marker::Braille)
                .graph_type(if i == 0 {
                    GraphType::Area
                } else {
                    GraphType::Line
                })
                .style(*style)
        })
        .collect();
    let chart = Chart::new(datasets)
        .x_axis(Axis::default().bounds([0.0, columns.saturating_sub(1) as f64]))
        .y_axis(Axis::default().bounds([0.0, 100.0]))
        .style(Style::new().bg(p.color(p.theme.panel)));
    frame.render_widget(chart, area);
}

/// A block sparkline of byte rates, newest at the right edge, scaled to its
/// own peak.
pub(super) fn sparkline(frame: &mut Frame, area: Rect, buffer: Option<&RingBuffer>, style: Style) {
    let rates: Vec<u64> = samples(buffer)
        .iter()
        .rev()
        .take(usize::from(area.width))
        .map(|&v| if v.is_finite() { v.max(0.0) as u64 } else { 0 })
        .collect();
    let sparkline = Sparkline::default()
        .data(&rates)
        .direction(RenderDirection::RightToLeft)
        .style(style);
    frame.render_widget(sparkline, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_value_is_cut_with_an_ellipsis() {
        assert_eq!(fit("HomeNet", 10), "HomeNet");
        assert_eq!(fit("HomeNet", 5), "Home…");
        assert_eq!(fit("HomeNet", 0), "");
    }

    #[test]
    fn a_stat_grid_takes_as_many_columns_as_fit() {
        assert_eq!(grid_columns(10), 1);
        assert_eq!(grid_columns(STAT_WIDTH * 2 + STAT_GAP), 2);
        assert_eq!(grid_rows(5, STAT_WIDTH * 2 + STAT_GAP), 3);
        assert_eq!(grid_rows(0, 80), 0);
    }
}
