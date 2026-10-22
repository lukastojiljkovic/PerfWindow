//! The live dashboard `perfwindow-cli` opens in an interactive terminal: the
//! window's cards as bordered panels in the user's theme, redrawn on every
//! new sample, with the history behind their charts.

mod cards;
mod draw;
mod widgets;

use super::term::{self, Glyphs};
use crate::config::Config;
use crate::format::TempUnit;
use crate::history::{History, RingBuffer};
use crate::ipc::{PipeSensord, Snapshot};
use crate::theme::system::{effective_theme_id, windows_is_light};
use crate::theme::Theme;
use crate::widgets::Severity;
use egui::Color32;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use ratatui::DefaultTerminal;

/// Open the dashboard on `first`, redraw it on every new sample, and close
/// it when the user presses q, Esc or Ctrl+C. The terminal is restored
/// however the dashboard ends, a panic included (ratatui's hook). `Err`
/// carries the message for stderr when the service stops or the terminal
/// fails.
pub fn run(
    pipe: &PipeSensord,
    first: Snapshot,
    config: &Config,
    color: bool,
) -> Result<(), String> {
    let depth = match (color, term::truecolor()) {
        (false, _) => Depth::Mono,
        (true, true) => Depth::TrueColor,
        (true, false) => Depth::Indexed,
    };
    let palette = Palette {
        theme: Theme::for_id(effective_theme_id(config, windows_is_light())),
        depth,
        glyphs: term::glyphs(),
    };
    let mut model = Model::new(first, palette, config.unit);
    let result = match ratatui::try_init() {
        Ok(mut terminal) => watch(&mut terminal, pipe, &mut model),
        Err(e) => Err(format!("could not open the dashboard: {e}")),
    };
    ratatui::restore();
    result
}

/// Draw, then wait for a key or a new sample, until the user leaves or the
/// service stops.
fn watch(
    terminal: &mut DefaultTerminal,
    pipe: &PipeSensord,
    model: &mut Model,
) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("could not draw the dashboard: {e}");
    let mut stale = true;
    loop {
        if stale {
            terminal
                .draw(|frame| draw::draw(frame, model))
                .map_err(failed)?;
            stale = false;
        }
        if event::poll(super::POLL).map_err(failed)? {
            match event::read().map_err(failed)? {
                Event::Key(key) if quits(key) => return Ok(()),
                Event::Resize(..) => stale = true,
                _ => {}
            }
        }
        let poll = super::poll(pipe, Some(&model.snapshot));
        if !poll.alive {
            return Err(super::SERVICE_STOPPED.to_string());
        }
        if let Some(snapshot) = poll.fresh {
            model.record(snapshot);
            stale = true;
        }
    }
}

/// Whether `key` leaves the dashboard: q, Esc or Ctrl+C. Only presses count,
/// as Windows reports every release too.
fn quits(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => true,
            KeyCode::Char('c') => key.modifiers.contains(KeyModifiers::CONTROL),
            _ => false,
        }
}

/// What the dashboard draws: the newest sample, the history behind its
/// charts, and the colours and unit to draw them in.
pub struct Model {
    snapshot: Snapshot,
    /// The window's own sparkline history: CPU, RAM, GPU load and network.
    history: History,
    /// GPU temperatures by index, and the iGPU's load and temperature, which
    /// the window's history does not keep.
    gpu_temps: Vec<RingBuffer>,
    igpu_load: RingBuffer,
    igpu_temp: RingBuffer,
    palette: Palette,
    unit: TempUnit,
}

impl Model {
    /// A model drawing `first`, with it as the first sample of every chart.
    pub fn new(first: Snapshot, palette: Palette, unit: TempUnit) -> Model {
        let mut model = Model {
            snapshot: first.clone(),
            history: History::default(),
            gpu_temps: Vec::new(),
            igpu_load: RingBuffer::default(),
            igpu_temp: RingBuffer::default(),
            palette,
            unit,
        };
        model.record(first);
        model
    }

    /// Take in a new sample: it becomes the one drawn and extends every chart.
    pub fn record(&mut self, snapshot: Snapshot) {
        self.history.record(&snapshot);
        let gpus = snapshot.gpu.as_deref().unwrap_or(&[]);
        self.gpu_temps.resize_with(gpus.len(), RingBuffer::default);
        for (temps, gpu) in self.gpu_temps.iter_mut().zip(gpus) {
            temps.push(chart_value(gpu.temp));
        }
        if let Some(igpu) = &snapshot.igpu {
            self.igpu_load.push(chart_value(igpu.load));
            self.igpu_temp.push(chart_value(igpu.temp));
        }
        self.snapshot = snapshot;
    }
}

/// A reading as a chart sample: `NaN` when absent, which the chart skips.
fn chart_value(reading: Option<f64>) -> f32 {
    reading.map_or(f32::NAN, |v| v as f32)
}

/// How many colours the terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// None: `--no-color`, `NO_COLOR` or `TERM=dumb`.
    Mono,
    /// The xterm 256-colour palette.
    Indexed,
    /// 24-bit colour.
    TrueColor,
}

/// The user's PerfWindow theme as terminal styles, in the glyphs the
/// terminal's font can draw.
pub struct Palette {
    pub theme: Theme,
    pub depth: Depth,
    pub glyphs: Glyphs,
}

impl Palette {
    /// A theme colour as the terminal shows it: exact, the nearest of the
    /// 256-colour palette, or the terminal's own with colour off.
    fn color(&self, color: Color32) -> Color {
        match self.depth {
            Depth::Mono => Color::Reset,
            Depth::Indexed => Color::Indexed(nearest_indexed(color)),
            Depth::TrueColor => Color::Rgb(color.r(), color.g(), color.b()),
        }
    }

    /// Text in `color`.
    fn fg(&self, color: Color32) -> Style {
        Style::new().fg(self.color(color))
    }

    /// A label, dim as the window's stat labels.
    fn label(&self) -> Style {
        self.fg(self.theme.dim)
    }

    /// A value, bold ink as the window's stat values.
    fn value(&self) -> Style {
        self.fg(self.theme.ink).add_modifier(Modifier::BOLD)
    }

    /// A value the window colours: `calm` in its ok band, warn or hot past
    /// it. Without colour, warn is underlined and hot reversed.
    fn rated(&self, severity: Option<Severity>, calm: Color32) -> Style {
        let style = self.value();
        match (severity, self.depth) {
            (None, _) => style.fg(self.color(calm)),
            (Some(Severity::Warn), Depth::Mono) => style.add_modifier(Modifier::UNDERLINED),
            (Some(Severity::Hot), Depth::Mono) => style.add_modifier(Modifier::REVERSED),
            (Some(severity), _) => style.fg(self.color(severity.color(&self.theme))),
        }
    }
}

/// The xterm 256-colour palette entry nearest to `color`: a step of the
/// 6 x 6 x 6 colour cube or of the 24-step grey ramp, whichever is closer.
fn nearest_indexed(color: Color32) -> u8 {
    /// The channel levels of the colour cube's six steps.
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let channels = [color.r(), color.g(), color.b()];
    let step = |v: u8| {
        (0..LEVELS.len())
            .min_by_key(|&i| LEVELS[i].abs_diff(v))
            .unwrap_or(0)
    };
    let distance = |rgb: [u8; 3]| {
        channels
            .iter()
            .zip(rgb)
            .map(|(&a, b)| u32::from(a.abs_diff(b)).pow(2))
            .sum::<u32>()
    };
    let [r, g, b] = channels.map(step);
    let cube = [LEVELS[r], LEVELS[g], LEVELS[b]];
    let mean = channels.iter().map(|&c| u32::from(c)).sum::<u32>() / 3;
    let grey_step = (mean.saturating_sub(3) / 10).min(23) as u8;
    let grey = 8 + grey_step * 10;
    if distance([grey; 3]) < distance(cube) {
        232 + grey_step
    } else {
        (16 + 36 * r + 6 * g + b) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventState;

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn q_esc_and_ctrl_c_leave_on_press_only() {
        let press = |code, modifiers| key(code, modifiers, KeyEventKind::Press);
        assert!(quits(press(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(quits(press(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(quits(press(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!quits(press(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert!(!quits(press(KeyCode::Char('x'), KeyModifiers::NONE)));
        assert!(!quits(key(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release
        )));
    }

    #[test]
    fn the_nearest_indexed_colour_is_a_cube_step_or_a_grey() {
        assert_eq!(nearest_indexed(Color32::from_rgb(0, 0, 0)), 16);
        assert_eq!(nearest_indexed(Color32::from_rgb(255, 255, 255)), 231);
        assert_eq!(nearest_indexed(Color32::from_rgb(255, 0, 0)), 196);
        // Amber's accent, #ffa31a, lands on the cube's orange (255, 175, 0).
        assert_eq!(nearest_indexed(Color32::from_rgb(0xff, 0xa3, 0x1a)), 214);
        // A dark neutral is closer to the grey ramp than to the cube.
        assert_eq!(nearest_indexed(Color32::from_rgb(0x12, 0x12, 0x12)), 233);
    }

    #[test]
    fn without_colour_warnings_keep_a_marker() {
        let palette = |depth| Palette {
            theme: Theme::for_id(crate::config::ThemeId::Amber),
            depth,
            glyphs: Glyphs::FULL,
        };
        let mono = palette(Depth::Mono);
        assert_eq!(mono.color(mono.theme.accent), Color::Reset);
        assert!(mono
            .rated(Some(Severity::Hot), mono.theme.ok)
            .add_modifier
            .contains(Modifier::REVERSED));
        let true_color = palette(Depth::TrueColor);
        assert_eq!(
            true_color
                .rated(Some(Severity::Warn), true_color.theme.ok)
                .fg,
            Some(Color::Rgb(0xff, 0xd2, 0x77))
        );
    }

    #[test]
    fn a_sample_extends_every_chart() {
        let mut model = Model::new(
            crate::cli::fixtures::snapshot(crate::cli::fixtures::FULL),
            Palette {
                theme: Theme::for_id(crate::config::ThemeId::Slate),
                depth: Depth::TrueColor,
                glyphs: Glyphs::FULL,
            },
            TempUnit::Celsius,
        );
        model.record(crate::cli::fixtures::snapshot(crate::cli::fixtures::FULL));
        assert_eq!(model.history.cpu.as_ref().map(RingBuffer::len), Some(2));
        assert_eq!(model.gpu_temps.len(), 2);
        assert_eq!(model.gpu_temps[0].len(), 2);
        // The iGPU sends no temperature: its chart keeps gaps, not zeros.
        assert!(model.igpu_temp.iter_oldest_first().all(f32::is_nan));
        assert_eq!(model.igpu_load.iter_oldest_first().last(), Some(3.0));
    }
}
