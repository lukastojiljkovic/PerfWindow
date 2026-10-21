//! One frame of the dashboard: the window's cards laid out to fit the
//! terminal, and a footer.

use super::cards::{self, battery_stats, ram_stats, sensor_stats};
use super::widgets::grid_rows;
use super::Model;
use crate::cli::report::local_time;
use crate::format::format_uptime;
use crate::ipc::Snapshot;
use crate::panels::sensors::has_content;
use crate::widgets::Severity;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

/// The smallest terminal the dashboard draws in; below it, it asks for room.
const MIN_WIDTH: u16 = 50;
const MIN_HEIGHT: u16 = 10;
/// Most rows the Sensors card asks for; more sensors go to more columns.
const MAX_SENSOR_ROWS: u16 = 6;

/// Draw one frame of the dashboard.
pub fn draw(frame: &mut Frame, model: &Model) {
    let p = &model.palette;
    let area = frame.area();
    frame.render_widget(
        Block::new().style(Style::new().bg(p.color(p.theme.bg))),
        area,
    );
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let message = format!("Enlarge the terminal to at least {MIN_WIDTH} x {MIN_HEIGHT}.");
        let middle = Rect {
            y: area.y + area.height / 2,
            height: 1,
            ..area
        };
        frame.render_widget(Paragraph::new(message).style(p.label()).centered(), middle);
        return;
    }
    let [body, footer] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    for (panel, rect) in layout(model, body) {
        draw_panel(frame, model, panel, rect);
    }
    draw_footer(frame, model, footer);
}

/// A panel of the dashboard: one per card the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Cpu,
    Gpu(usize),
    Igpu,
    Ram,
    Network,
    Battery,
    Storage,
    Sensors,
}

impl Panel {
    /// Which row the panel shares, as the window groups its cards.
    fn group(self) -> u8 {
        match self {
            Panel::Cpu => 0,
            Panel::Gpu(_) | Panel::Igpu => 1,
            Panel::Ram | Panel::Network | Panel::Battery => 2,
            Panel::Storage | Panel::Sensors => 3,
        }
    }

    /// The narrowest the panel draws well at.
    fn min_width(self) -> u16 {
        match self {
            Panel::Cpu => MIN_WIDTH,
            Panel::Gpu(_) | Panel::Igpu => 34,
            Panel::Ram | Panel::Network => 30,
            Panel::Battery | Panel::Sensors => 24,
            Panel::Storage => 40,
        }
    }

    /// Its share of the width of a row.
    fn weight(self) -> u16 {
        match self {
            Panel::Storage => 5,
            Panel::Sensors => 3,
            _ => 1,
        }
    }

    /// How soon the panel is left out of a terminal too small for every
    /// one: the higher, the sooner.
    fn drop_rank(self) -> u8 {
        match self {
            Panel::Cpu => 0,
            Panel::Gpu(0) | Panel::Ram => 1,
            Panel::Network => 2,
            Panel::Storage => 3,
            Panel::Gpu(_) => 4,
            Panel::Igpu => 5,
            Panel::Battery => 6,
            Panel::Sensors => 7,
        }
    }

    /// Whether the panel has a chart that takes spare height.
    fn grows(self) -> bool {
        !matches!(self, Panel::Battery | Panel::Storage | Panel::Sensors)
    }

    /// The rows the panel needs at `width`, its border included.
    fn height(self, model: &Model, width: u16) -> u16 {
        let inner = width.saturating_sub(2);
        let snapshot = &model.snapshot;
        2 + match self {
            Panel::Cpu | Panel::Gpu(_) | Panel::Igpu | Panel::Network => 5,
            // The used and swap bars, then the stats.
            Panel::Ram => snapshot
                .ram
                .as_ref()
                .map_or(0, |ram| 2 + grid_rows(ram_stats(model, ram).len(), inner)),
            Panel::Battery => snapshot.battery.as_ref().map_or(0, |battery| {
                1 + grid_rows(battery_stats(model, battery).len(), inner)
            }),
            Panel::Storage => 1 + snapshot.storage.as_ref().map_or(0, |d| d.len() as u16),
            Panel::Sensors => grid_rows(sensor_stats(model).len(), inner).min(MAX_SENSOR_ROWS),
        }
    }
}

/// The panels the window shows for `snapshot`, in its order.
fn panels(snapshot: &Snapshot) -> Vec<Panel> {
    let gpus = snapshot.gpu.as_ref().map_or(0, Vec::len);
    let disks = snapshot.storage.as_ref().is_some_and(|d| !d.is_empty());
    let fans = snapshot.fans.as_deref().unwrap_or(&[]);
    let voltages = snapshot.voltages.as_deref().unwrap_or(&[]);
    let sensors = has_content(snapshot.board.as_ref(), fans, voltages);
    [Panel::Cpu]
        .into_iter()
        .filter(|_| snapshot.cpu.is_some())
        .chain((0..gpus).map(Panel::Gpu))
        .chain(snapshot.igpu.as_ref().map(|_| Panel::Igpu))
        .chain(snapshot.ram.as_ref().map(|_| Panel::Ram))
        .chain(snapshot.net.as_ref().map(|_| Panel::Network))
        .chain(snapshot.battery.as_ref().map(|_| Panel::Battery))
        .chain(disks.then_some(Panel::Storage))
        .chain(sensors.then_some(Panel::Sensors))
        .collect()
}

/// Where each panel goes in `area`. The least important panels are left
/// out until the rest fit; then each one left out is given back, most
/// important first, if it still fits.
fn layout(model: &Model, area: Rect) -> Vec<(Panel, Rect)> {
    let all = panels(&model.snapshot);
    let mut shown = all.clone();
    let mut dropped = Vec::new();
    while place(&shown, model, area).is_none() {
        let Some(last) = shown.iter().max_by_key(|panel| panel.drop_rank()).copied() else {
            break;
        };
        shown.retain(|panel| *panel != last);
        dropped.push(last);
    }
    for panel in dropped.into_iter().rev() {
        let trial: Vec<Panel> = all
            .iter()
            .copied()
            .filter(|p| *p == panel || shown.contains(p))
            .collect();
        if place(&trial, model, area).is_some() {
            shown = trial;
        }
    }
    place(&shown, model, area).unwrap_or_default()
}

/// `panels` placed in `area`: one row per group, wrapped where the row is
/// too narrow, each row as tall as its tallest panel and the spare height
/// shared by the rows with charts. `None` when they do not fit.
fn place(panels: &[Panel], model: &Model, area: Rect) -> Option<Vec<(Panel, Rect)>> {
    let mut rows: Vec<Vec<Panel>> = Vec::new();
    for &panel in panels {
        match rows.last_mut() {
            Some(row)
                if row[0].group() == panel.group()
                    && row.iter().map(|p| p.min_width()).sum::<u16>() + panel.min_width()
                        <= area.width =>
            {
                row.push(panel)
            }
            _ => rows.push(vec![panel]),
        }
    }
    let cells: Vec<Vec<Rect>> = rows
        .iter()
        .map(|row| {
            Layout::horizontal(row.iter().map(|p| Constraint::Fill(p.weight())))
                .split(Rect { height: 1, ..area })
                .to_vec()
        })
        .collect();
    let mut heights: Vec<u16> = rows
        .iter()
        .zip(&cells)
        .map(|(row, cells)| {
            row.iter()
                .zip(cells)
                .map(|(panel, cell)| panel.height(model, cell.width))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let spare = area.height.checked_sub(heights.iter().sum())?;
    let growing: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].iter().any(|p| p.grows()))
        .collect();
    if let Some(&first) = growing.first() {
        let share = spare / growing.len() as u16;
        for &i in &growing {
            heights[i] += share;
        }
        heights[first] += spare - share * growing.len() as u16;
    }
    let mut y = area.y;
    let mut placed = Vec::new();
    for ((row, cells), height) in rows.iter().zip(&cells).zip(heights) {
        for (panel, cell) in row.iter().zip(cells) {
            placed.push((*panel, Rect { y, height, ..*cell }));
        }
        y += height;
    }
    Some(placed)
}

/// Draw `panel` in `area` with the card it stands for.
fn draw_panel(frame: &mut Frame, model: &Model, panel: Panel, area: Rect) {
    let snapshot = &model.snapshot;
    match panel {
        Panel::Cpu => {
            if let Some(cpu) = &snapshot.cpu {
                cards::cpu(frame, area, model, cpu);
            }
        }
        Panel::Gpu(i) => {
            if let Some(gpu) = snapshot.gpu.as_ref().and_then(|gpus| gpus.get(i)) {
                let load = model.history.gpu(i).map(|h| &h.load);
                cards::gpu(frame, area, model, gpu, load, model.gpu_temps.get(i));
            }
        }
        Panel::Igpu => {
            if let Some(gpu) = &snapshot.igpu {
                let (load, temp) = (Some(&model.igpu_load), Some(&model.igpu_temp));
                cards::gpu(frame, area, model, gpu, load, temp);
            }
        }
        Panel::Ram => {
            if let Some(ram) = &snapshot.ram {
                cards::ram(frame, area, model, ram);
            }
        }
        Panel::Network => {
            if let Some(net) = &snapshot.net {
                cards::network(frame, area, model, net);
            }
        }
        Panel::Battery => {
            if let Some(battery) = &snapshot.battery {
                cards::battery(frame, area, model, battery);
            }
        }
        Panel::Storage => {
            cards::storage(
                frame,
                area,
                model,
                snapshot.storage.as_deref().unwrap_or(&[]),
            );
        }
        Panel::Sensors => cards::sensors(frame, area, model),
    }
}

/// The footer: the wordmark and version, uptime, when the sample was taken,
/// a warning when the sensors run degraded, and the key that leaves.
fn draw_footer(frame: &mut Frame, model: &Model, area: Rect) {
    let p = &model.palette;
    let snapshot = &model.snapshot;
    frame.render_widget(
        Block::new().style(Style::new().bg(p.color(p.theme.chrome))),
        area,
    );
    let wordmark = p.fg(p.theme.accent).add_modifier(Modifier::BOLD);
    let mut left = vec![
        Span::styled(format!(" {} PERFWINDOW ", p.glyphs.mark), wordmark),
        Span::styled(env!("CARGO_PKG_VERSION"), p.label()),
    ];
    if let Some(uptime) = snapshot.uptime_sec {
        left.extend([
            Span::styled("   UP ", p.label()),
            Span::styled(format_uptime(uptime), p.fg(p.theme.ink)),
        ]);
    }
    if let Some(([.., hour, minute, second], _)) = local_time(snapshot) {
        left.extend([
            Span::styled("   SAMPLED ", p.label()),
            Span::styled(
                format!("{hour:02}:{minute:02}:{second:02}"),
                p.fg(p.theme.ink),
            ),
        ]);
    }
    if snapshot.health.as_ref().is_some_and(|h| h.degraded) {
        let warn = p.rated(Some(Severity::Warn), p.theme.ok);
        left.push(Span::styled("   SENSORS DEGRADED", warn));
    }
    let keys = Line::from(vec![
        Span::styled("q", p.value()),
        Span::styled(" quit ", p.label()),
    ]);
    frame.render_widget(Paragraph::new(Line::from(left)), area);
    frame.render_widget(Paragraph::new(keys.right_aligned()), area);
}

#[cfg(test)]
mod tests {
    use super::super::{Depth, Palette};
    use super::*;
    use crate::cli::fixtures::{snapshot, FULL};
    use crate::cli::term::Glyphs;
    use crate::config::ThemeId;
    use crate::format::TempUnit;
    use crate::theme::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// The FULL sample after eighty whose loads, GPU temperatures and
    /// network rates rise and fall, so every chart has a history to draw.
    fn model() -> Model {
        let palette = Palette {
            theme: Theme::for_id(ThemeId::Amber),
            depth: Depth::TrueColor,
            glyphs: Glyphs::FULL,
        };
        let mut model = Model::new(snapshot(FULL), palette, TempUnit::Celsius);
        for i in 0..80 {
            let wave = |base: f64, swing: f64| base + swing * (f64::from(i) / 6.0).sin();
            let mut sample = snapshot(FULL);
            sample
                .cpu
                .iter_mut()
                .for_each(|cpu| cpu.load = Some(wave(40.0, 30.0)));
            for gpu in sample.gpu.iter_mut().flatten() {
                gpu.load = Some(wave(55.0, 40.0));
                gpu.temp = Some(wave(60.0, 8.0));
            }
            for net in sample.net.iter_mut() {
                net.down_bps = Some(wave(2.0e6, 1.9e6));
                net.up_bps = Some(wave(3.0e5, 2.5e5));
            }
            model.record(sample);
        }
        model.record(snapshot(FULL));
        model
    }

    /// The frame `model` draws in a `width` x `height` terminal, as text.
    fn render(model: &Model, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| draw(frame, model)).expect("draws");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_shows(frame: &str, needles: &[&str]) {
        for needle in needles {
            assert!(frame.contains(needle), "missing {needle:?} in:\n{frame}");
        }
    }

    #[test]
    fn a_wide_terminal_shows_every_card_the_window_shows() {
        let frame = render(&model(), 120, 40);
        assert_shows(
            &frame,
            &[
                "┌ CPU ",
                "TEST CPU",
                "┌ GPU ",
                "RTX 4070",
                "RTX 3060",
                "┌ IGPU ",
                "┌ RAM ",
                "┌ NETWORK ",
                "┌ BATTERY ",
                "┌ STORAGE ",
                "┌ SENSORS ",
                "CORES",
                "58 °C",
                "DISCHARGING",
                "Samsung 980 Pro",
                "HomeNet",
                "CPU FAN",
                "▚ PERFWINDOW",
                "UP 3d 4h",
                "q quit",
            ],
        );
        // The load history is drawn in braille.
        assert!(frame.contains('⣿'), "{frame}");
    }

    #[test]
    fn classic_conhost_gets_no_block_element_its_font_lacks() {
        let mut model = model();
        model.palette.glyphs = Glyphs::CP437;
        let frame = render(&model, 120, 40);
        let missing = |c: char| ('\u{2580}'..='\u{259f}').contains(&c) && !"▀▄█▌▐░▒▓".contains(c);
        assert!(!frame.chars().any(missing), "{frame}");
        assert!(frame.contains("▌ PERFWINDOW"), "{frame}");
    }

    #[test]
    fn an_80_by_24_terminal_keeps_the_most_important_cards() {
        let frame = render(&model(), 80, 24);
        assert_shows(
            &frame,
            &["┌ CPU ", "┌ GPU ", "┌ RAM ", "┌ NETWORK ", "34 %", "q quit"],
        );
        assert!(!frame.contains("┌ SENSORS "), "{frame}");
    }

    #[test]
    fn a_tiny_terminal_asks_for_room() {
        let frame = render(&model(), 40, 8);
        assert!(frame.contains("Enlarge the terminal"), "{frame}");
        assert!(!frame.contains("┌ CPU "), "{frame}");
    }

    #[test]
    fn panels_follow_the_window_order_and_drop_the_least_important_first() {
        let model = model();
        let all = panels(&model.snapshot);
        assert_eq!(
            all,
            [
                Panel::Cpu,
                Panel::Gpu(0),
                Panel::Gpu(1),
                Panel::Igpu,
                Panel::Ram,
                Panel::Network,
                Panel::Battery,
                Panel::Storage,
                Panel::Sensors,
            ]
        );
        let shown: Vec<Panel> = layout(&model, Rect::new(0, 0, 80, 23))
            .into_iter()
            .map(|(panel, _)| panel)
            .collect();
        // The second GPU comes back: it shares the first one's row.
        assert_eq!(
            shown,
            [
                Panel::Cpu,
                Panel::Gpu(0),
                Panel::Gpu(1),
                Panel::Ram,
                Panel::Network
            ]
        );
    }

    #[test]
    fn placed_panels_stay_inside_the_body_without_overlapping() {
        let model = model();
        for (width, height) in [(120, 39), (80, 23), (200, 60), (50, 9)] {
            let body = Rect::new(0, 0, width, height);
            let placed = layout(&model, body);
            assert!(!placed.is_empty(), "{width} x {height}");
            for (i, (_, a)) in placed.iter().enumerate() {
                assert_eq!(a.intersection(body), *a, "{width} x {height}");
                for (_, b) in &placed[i + 1..] {
                    assert!(!a.intersects(*b), "{a:?} and {b:?} at {width} x {height}");
                }
            }
        }
    }
}
