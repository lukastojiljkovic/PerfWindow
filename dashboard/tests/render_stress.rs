//! Headless render-stress reproduction harness for the 0xc0000409 crash.
//!
//! Drives the WHOLE dashboard UI (title bar, footer, card grid + every panel,
//! and the modals) through a raw `egui::Context` — `ctx.run()` for layout +
//! `ctx.tessellate()` for the CPU mesh build — with NO wgpu/GPU involved. This
//! deliberately avoids the `egui_kittest` wgpu harness, which crashes on this
//! machine inside the WARP software renderer (a separate GPU-stack issue).
//!
//! Each scenario prints a marker to stderr *before* it runs, so if a scenario
//! aborts the process (an allocation failure / `__fastfail`, which no panic
//! hook catches) the last marker pinpoints the culprit. Run with:
//!
//! ```powershell
//! cargo test --test render_stress -- --nocapture --test-threads=1
//! ```
//!
//! Uses a few egui 0.34 panel/`Context::run` entry points that are now marked
//! deprecated in favour of `run_ui`; the deprecated forms are stable and read
//! the same as production code, so the lint is allowed file-wide (mirroring the
//! `#[allow(deprecated)]` in `tests/ui_snapshot.rs`).
#![allow(deprecated)]

use egui::Vec2;
use perfwindow::app::{PerfApp, Status};
use perfwindow::config::{Config, RefreshRate, ThemeId};
use perfwindow::format::TempUnit;
use perfwindow::ipc::snapshot::{D3DEngineLoad, DimmTemp, DisplayInfo};
use perfwindow::ipc::*;
use perfwindow::theme::{self, Theme};

const THEMES: &[ThemeId] = &[
    ThemeId::Amber,
    ThemeId::Slate,
    ThemeId::Phosphor,
    ThemeId::Light,
    ThemeId::Synthwave,
    ThemeId::Crimson,
];

/// One full frame of the real dashboard chrome + grid, then tessellation.
/// Mirrors the panel nesting `PerfApp::ui` performs in production, minus the
/// `eframe::Frame` (which production never reads anyway).
fn render_one_frame(app: &mut PerfApp, ctx: &egui::Context, size: Vec2, ppp: f32) {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        viewport_id: egui::ViewportId::ROOT,
        ..Default::default()
    };
    raw.viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .native_pixels_per_point = Some(ppp);

    let theme = app.theme.clone();
    let full = ctx.run(raw, |ctx| {
        ctx.set_pixels_per_point(ppp);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg))
            .show(ctx, |ui| {
                egui::TopBottomPanel::top("pw_title_bar")
                    .frame(egui::Frame::NONE)
                    .show_inside(ui, |ui| perfwindow::ui::title_bar(ui, app));
                if perfwindow::ui::update_banner::is_visible(app) {
                    egui::TopBottomPanel::top("pw_update_banner")
                        .frame(egui::Frame::NONE)
                        .show_inside(ui, |ui| {
                            perfwindow::ui::update_banner::update_banner(ui, app)
                        });
                }
                if perfwindow::ui::health_banner::is_visible(app) {
                    egui::TopBottomPanel::top("pw_health_banner")
                        .frame(egui::Frame::NONE)
                        .show_inside(ui, |ui| {
                            perfwindow::ui::health_banner::health_banner(ui, app)
                        });
                }
                egui::TopBottomPanel::bottom("pw_footer")
                    .frame(egui::Frame::NONE)
                    .show_inside(ui, |ui| perfwindow::ui::footer(ui, app));
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme.bg))
                    .show_inside(ui, |ui| {
                        perfwindow::ui::effects::paint_grid(ui, &theme);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, true])
                            .show(ui, |ui| perfwindow::ui::card_grid(ui, app));
                    });
            });
        perfwindow::ui::settings::settings_modal(ctx, app);
        perfwindow::ui::update_modal::update_modal(ctx, app);
        let mut show_changelog = app.show_changelog;
        perfwindow::ui::changelog_modal::changelog_modal(ctx, &theme, &mut show_changelog);
        perfwindow::ui::effects::paint_effects(ctx, &theme);
    });

    // The mesh build — where a degenerate Rect / huge vertex allocation would
    // bite if any geometry slipped past the widget guards.
    let _ = ctx.tessellate(full.shapes, full.pixels_per_point);
}

/// Build a fresh app + context for `theme`, with fonts installed and a couple
/// of warm-up frames so the queued font swap settles before the real render.
fn fresh(theme_id: ThemeId, unit: TempUnit, heat_map: bool) -> (PerfApp, egui::Context) {
    let cfg = Config {
        theme: theme_id,
        unit,
        cpu_heat_map: heat_map,
        refresh: RefreshRate::Ms500,
        ..Config::default()
    };
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    Theme::for_id(theme_id).apply(&ctx);
    let mut app = PerfApp::for_tests(cfg);
    app.theme = Theme::for_id(theme_id);
    (app, ctx)
}

fn run_matrix(label: &str, make_snap: impl Fn() -> Option<Snapshot>) {
    let sizes: &[(Vec2, f32)] = &[
        (Vec2::new(1180.0, 600.0), 1.0),
        (Vec2::new(1180.0, 600.0), 1.5),
        (Vec2::new(1180.0, 600.0), 2.0),
        (Vec2::new(2400.0, 1400.0), 3.0),
        (Vec2::new(720.0, 500.0), 1.0),
        (Vec2::new(360.0, 500.0), 1.25),
        (Vec2::new(200.0, 400.0), 1.0),
        (Vec2::new(120.0, 300.0), 2.0),
    ];
    for &theme_id in THEMES {
        for unit in [TempUnit::Celsius, TempUnit::Fahrenheit] {
            for heat_map in [false, true] {
                for &(size, ppp) in sizes {
                    let (mut app, ctx) = fresh(theme_id, unit, heat_map);
                    if let Some(snap) = make_snap() {
                        app.status = Status::Running;
                        app.history.record(&snap);
                        app.history.record(&snap);
                        app.latest = Some(snap);
                    } else {
                        app.status = Status::Running; // Running + no snapshot => loading-sensors fallback
                    }
                    eprintln!(
                        "STRESS {label} theme={theme_id:?} unit={unit:?} heat={heat_map} size={:?} ppp={ppp}",
                        size
                    );
                    // Two frames: first builds atlas/layout, second renders steady state.
                    render_one_frame(&mut app, &ctx, size, ppp);
                    render_one_frame(&mut app, &ctx, size, ppp);
                }
            }
        }
    }
}

// ---- snapshot builders ----------------------------------------------------

fn cpu(load: Option<f64>, n_cores: usize, p_cores: Option<u32>) -> CpuInfo {
    CpuInfo {
        name: "Test CPU 14900HX".into(),
        load,
        cores: Some((0..n_cores).map(|i| (i as f64 * 3.7) % 100.0).collect()),
        temp: Some(58.0),
        clock_mhz: Some(4400.0),
        power_w: Some(52.0),
        core_temps: Some(
            (0..n_cores)
                .map(|i| if i % 2 == 0 { Some(60.0) } else { None })
                .collect(),
        ),
        voltage_v: Some(1.21),
        distance_to_tjmax_c: Some(12.0),
        power_cores_w: Some(30.0),
        power_memory_w: Some(5.0),
        power_platform_w: Some(40.0),
        p_core_count: p_cores,
        e_core_count: p_cores.map(|p| (n_cores as u32).saturating_sub(p)),
    }
}

fn gpu(load: Option<f64>) -> GpuInfo {
    GpuInfo {
        name: "RTX 4070 Laptop".into(),
        kind: "discrete".into(),
        load,
        temp: Some(71.0),
        vram_used_mb: Some(6348.0),
        vram_total_mb: Some(12288.0),
        clock_mhz: Some(2610.0),
        fan_rpm: Some(1480.0),
        power_w: Some(140.0),
        memory_load: Some(42.5),
        hot_spot_temp: Some(83.0),
        memory_junction_temp_c: Some(78.0),
        pcie_rx_bps: Some(1_200_000.0),
        pcie_tx_bps: Some(800_000.0),
        dedicated_vram_used_mb: Some(6000.0),
        shared_vram_used_mb: Some(348.0),
        voltage_v: Some(0.95),
        d3d_engines: Some(vec![
            D3DEngineLoad {
                name: "D3D 3D".into(),
                load: 51.0,
            },
            D3DEngineLoad {
                name: "D3D Copy".into(),
                load: 3.0,
            },
        ]),
    }
}

fn base(cpu: Option<CpuInfo>, gpu: Option<Vec<GpuInfo>>) -> Snapshot {
    Snapshot {
        v: 1,
        ts: 1_747_645_200,
        cpu,
        gpu,
        igpu: None,
        ram: Some(RamInfo {
            used_mb: Some(15462.0),
            total_mb: Some(32768.0),
            available_mb: Some(17306.0),
            load: Some(47.2),
            cached_mb: Some(4403.0),
            pagefile_used_mb: Some(2150.0),
            pagefile_total_mb: Some(8192.0),
            dimm_temps: Some(vec![DimmTemp {
                label: "DIMM #0".into(),
                temp_c: 44.0,
            }]),
        }),
        storage: Some(vec![StorageInfo {
            name: "Samsung 980 Pro".into(),
            kind: "nvme".into(),
            temp: Some(48.0),
            activity: Some(22.0),
            used_gb: Some(412.0),
            total_gb: Some(931.5),
            health: Some(98.0),
            read_bps: Some(4_404_019.0),
            write_bps: Some(629_145.0),
            power_on_hours: Some(5057),
            power_on_count: Some(1103),
            available_spare_pct: Some(100.0),
        }]),
        board: Some(BoardInfo {
            temp: Some(38.0),
            vrm_temp: Some(61.0),
        }),
        fans: Some(vec![FanInfo {
            name: "CPU".into(),
            rpm: Some(920.0),
        }]),
        voltages: Some(vec![VoltageInfo {
            name: "+12V".into(),
            volts: Some(12.08),
        }]),
        net: Some(NetInfo {
            adapter: "Ethernet".into(),
            down_bps: Some(4_404_019.0),
            up_bps: Some(629_145.0),
            link_bps: Some(1_000_000_000),
            down_pct: Some(35.2),
            up_pct: Some(5.0),
        }),
        battery: Some(BatteryInfo {
            charge_pct: Some(64.0),
            rate_w: Some(-12.4),
            voltage_v: Some(11.4),
            design_capacity_mwh: Some(90000.0),
            full_capacity_mwh: Some(78000.0),
            time_remaining_sec: Some(8100),
        }),
        uptime_sec: Some(360_000),
        atk_fans: Some(vec![FanInfo {
            name: "CPU Fan".into(),
            rpm: Some(3200.0),
        }]),
        display: Some(DisplayInfo {
            name: "\\\\.\\DISPLAY1".into(),
            width: 2560,
            height: 1600,
            refresh_hz: 240,
        }),
        displays: None,
        health: Some(HealthInfo {
            pawnio: "ok".into(),
            degraded: false,
            notes: None,
        }),
    }
}

#[test]
fn realistic_full_snapshot_renders() {
    run_matrix("realistic", || {
        Some(base(
            Some(cpu(Some(34.0), 24, Some(8))),
            Some(vec![gpu(Some(51.0))]),
        ))
    });
}

#[test]
fn running_without_snapshot_shows_loading() {
    run_matrix("no_snapshot", || None);
}

#[test]
fn non_finite_values_everywhere() {
    run_matrix("nan_inf", || {
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        let mut s = base(
            Some(cpu(Some(nan), 16, Some(6))),
            Some(vec![gpu(Some(inf))]),
        );
        if let Some(c) = s.cpu.as_mut() {
            c.temp = Some(nan);
            c.clock_mhz = Some(inf);
            c.power_w = Some(nan);
            c.voltage_v = Some(nan);
            c.distance_to_tjmax_c = Some(inf);
            c.cores = Some(vec![nan, inf, f64::NEG_INFINITY, -50.0, 1e30]);
            c.core_temps = Some(vec![Some(nan), Some(inf), None, Some(-1e30)]);
        }
        if let Some(g) = s.gpu.as_mut().and_then(|v| v.first_mut()) {
            g.temp = Some(nan);
            g.vram_used_mb = Some(inf);
            g.vram_total_mb = Some(nan);
            g.clock_mhz = Some(inf);
            g.power_w = Some(nan);
            g.memory_load = Some(inf);
            g.pcie_rx_bps = Some(inf);
            g.pcie_tx_bps = Some(nan);
            g.voltage_v = Some(nan);
        }
        if let Some(r) = s.ram.as_mut() {
            r.load = Some(nan);
            r.used_mb = Some(inf);
            r.pagefile_used_mb = Some(inf);
            r.pagefile_total_mb = Some(nan);
            r.dimm_temps = Some(vec![DimmTemp {
                label: "X".into(),
                temp_c: nan,
            }]);
        }
        if let Some(d) = s.storage.as_mut().and_then(|v| v.first_mut()) {
            d.temp = Some(nan);
            d.activity = Some(inf);
            d.used_gb = Some(nan);
            d.total_gb = Some(inf);
            d.health = Some(nan);
            d.read_bps = Some(inf);
            d.write_bps = Some(nan);
        }
        s.net = Some(NetInfo {
            adapter: "Ethernet".into(),
            down_bps: Some(nan),
            up_bps: Some(inf),
            link_bps: Some(i64::MAX),
            down_pct: Some(nan),
            up_pct: Some(inf),
        });
        s.battery = Some(BatteryInfo {
            charge_pct: Some(nan),
            rate_w: Some(inf),
            voltage_v: Some(nan),
            design_capacity_mwh: Some(0.0),
            full_capacity_mwh: Some(inf),
            time_remaining_sec: Some(i64::MIN),
        });
        Some(s)
    });
}

#[test]
fn extreme_magnitudes() {
    run_matrix("huge", || {
        let big = 1e300_f64;
        let mut s = base(Some(cpu(Some(big), 8, None)), Some(vec![gpu(Some(big))]));
        if let Some(c) = s.cpu.as_mut() {
            c.clock_mhz = Some(big);
            c.power_w = Some(big);
            c.cores = Some(vec![big, -big, f64::MAX, f64::MIN]);
        }
        if let Some(r) = s.ram.as_mut() {
            r.used_mb = Some(big);
            r.total_mb = Some(big);
        }
        Some(s)
    });
}

#[test]
fn empty_and_degenerate_collections() {
    run_matrix("empty_collections", || {
        let mut s = base(Some(cpu(Some(0.0), 0, Some(0))), Some(vec![]));
        if let Some(c) = s.cpu.as_mut() {
            c.cores = Some(vec![]);
            c.core_temps = Some(vec![]);
        }
        s.storage = Some(vec![]);
        s.fans = Some(vec![]);
        s.voltages = Some(vec![]);
        s.gpu = Some(vec![]);
        s.battery = None;
        s.net = None;
        Some(s)
    });
}

#[test]
fn inconsistent_core_counts() {
    run_matrix("bad_core_counts", || {
        // p_core_count greater than the actual core list — exercises the
        // hybrid P/E split index math under inconsistent sensord data.
        let mut s = base(Some(cpu(Some(50.0), 4, Some(64))), None);
        if let Some(c) = s.cpu.as_mut() {
            c.core_temps = Some(vec![Some(60.0)]); // shorter than cores
        }
        Some(s)
    });
}

#[test]
fn many_cores_gpus_disks() {
    run_matrix("many_everything", || {
        let mut s = base(
            Some(cpu(Some(80.0), 256, Some(128))),
            Some((0..8).map(|_| gpu(Some(60.0))).collect()),
        );
        s.storage = Some(
            (0..16)
                .map(|i| StorageInfo {
                    name: format!(
                        "Disk number {i} with a very very long name that should ellipsise"
                    ),
                    kind: "nvme".into(),
                    temp: Some(40.0 + i as f64),
                    activity: Some(10.0),
                    used_gb: Some(500.0),
                    total_gb: Some(2000.0),
                    health: Some(90.0),
                    read_bps: Some(1e6),
                    write_bps: Some(1e6),
                    power_on_hours: Some(1000),
                    power_on_count: Some(50),
                    available_spare_pct: Some(99.0),
                })
                .collect(),
        );
        s.fans = Some(
            (0..12)
                .map(|i| FanInfo {
                    name: format!("FAN{i}"),
                    rpm: Some(1000.0 + i as f64 * 100.0),
                })
                .collect(),
        );
        s.voltages = Some(
            (0..12)
                .map(|i| VoltageInfo {
                    name: format!("V{i}"),
                    volts: Some(1.0 + i as f64),
                })
                .collect(),
        );
        if let Some(g) = s.gpu.as_mut().and_then(|v| v.first_mut()) {
            g.d3d_engines = Some(
                (0..16)
                    .map(|i| D3DEngineLoad {
                        name: format!("Engine {i}"),
                        load: i as f64,
                    })
                    .collect(),
            );
        }
        Some(s)
    });
}

#[test]
fn modals_and_overlays_render() {
    // Settings open, changelog open, sensord-down overlay, each across themes.
    for &theme_id in THEMES {
        let (mut app, ctx) = fresh(theme_id, TempUnit::Celsius, false);
        app.settings_open = true;
        app.show_changelog = true;
        app.status = Status::SensordDown;
        eprintln!("STRESS modals theme={theme_id:?}");
        render_one_frame(&mut app, &ctx, Vec2::new(1180.0, 600.0), 1.0);
        render_one_frame(&mut app, &ctx, Vec2::new(1180.0, 600.0), 2.0);
        // also a tiny window so the modal's scroll-area clamp engages
        render_one_frame(&mut app, &ctx, Vec2::new(360.0, 240.0), 1.5);
    }
}
