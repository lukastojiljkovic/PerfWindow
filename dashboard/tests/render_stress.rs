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

use egui::{Pos2, Vec2};
use perfwindow::app::{PerfApp, Status};
use perfwindow::config::{Config, RefreshRate, ThemeId};
use perfwindow::format::TempUnit;
use perfwindow::ipc::snapshot::{D3DEngineLoad, DimmTemp, DisplayInfo};
use perfwindow::ipc::*;
use perfwindow::theme::{self, Theme};
use std::sync::atomic::{AtomicU64, Ordering};

const THEMES: &[ThemeId] = &[
    ThemeId::Amber,
    ThemeId::Slate,
    ThemeId::Phosphor,
    ThemeId::Light,
    ThemeId::Synthwave,
    ThemeId::Crimson,
];

/// What one harness frame produced — enough to assert "something rendered"
/// and "this text is on screen" without a GPU.
struct FrameOutput {
    /// Every text shape's contents, newline-joined, with the letter-spacing
    /// thin spaces (U+2009) stripped so assertions can match plain strings.
    text: String,
    shape_count: usize,
}

/// Process-wide frame clock: each frame gets a strictly later `time`, with a
/// 1 s step so hover delays (tooltip ~0.5 s) elapse between any two frames.
static FRAME_CLOCK: AtomicU64 = AtomicU64::new(0);

fn next_time() -> f64 {
    FRAME_CLOCK.fetch_add(1, Ordering::Relaxed) as f64
}

fn collect_text(shapes: &[egui::epaint::ClippedShape]) -> String {
    fn walk(shape: &egui::Shape, out: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = String::new();
    for clipped in shapes {
        walk(&clipped.shape, &mut out);
    }
    out.replace('\u{2009}', "")
}

/// One full frame of the real dashboard chrome + grid, then tessellation.
/// Mirrors the panel nesting `PerfApp::ui` performs in production, minus the
/// `eframe::Frame` (which production never reads anyway). `events` lets a
/// scenario feed pointer input (hover sweeps, clicks) through the same raw
/// path winit would use.
fn run_frame(
    app: &mut PerfApp,
    ctx: &egui::Context,
    size: Vec2,
    ppp: f32,
    events: Vec<egui::Event>,
) -> FrameOutput {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        viewport_id: egui::ViewportId::ROOT,
        time: Some(next_time()),
        events,
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

    let output = FrameOutput {
        text: collect_text(&full.shapes),
        shape_count: full.shapes.len(),
    };
    // The mesh build — where a degenerate Rect / huge vertex allocation would
    // bite if any geometry slipped past the widget guards.
    let _ = ctx.tessellate(full.shapes, full.pixels_per_point);
    output
}

/// Event-less [`run_frame`], for the bulk matrix scenarios.
fn render_one_frame(app: &mut PerfApp, ctx: &egui::Context, size: Vec2, ppp: f32) {
    let _ = run_frame(app, ctx, size, ppp, Vec::new());
}

/// The press/release/move triple that lands one primary-button click at `pos`.
fn click_events(pos: Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ]
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
        bus_clock_mhz: Some(100.6),
        core_clocks_mhz: Some(
            (0..n_cores)
                .map(|i| Some(3600.0 + i as f64 * 25.0))
                .collect(),
        ),
    }
}

/// Integrated-GPU section as sensord reports it on dual-GPU laptops: shared
/// memory instead of dedicated VRAM, several headline readings absent.
fn igpu() -> GpuInfo {
    GpuInfo {
        name: "Intel UHD Graphics".into(),
        kind: "integrated".into(),
        load: Some(7.5),
        temp: None,
        vram_used_mb: None,
        vram_total_mb: None,
        clock_mhz: Some(350.0),
        fan_rpm: None,
        power_w: None,
        memory_load: None,
        hot_spot_temp: None,
        memory_junction_temp_c: None,
        pcie_rx_bps: None,
        pcie_tx_bps: None,
        dedicated_vram_used_mb: None,
        shared_vram_used_mb: Some(332.0),
        voltage_v: None,
        d3d_engines: Some(vec![D3DEngineLoad {
            name: "D3D Video Decode".into(),
            load: 4.0,
        }]),
        memory_clock_mhz: None,
        video_engine_load: None,
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
        memory_clock_mhz: Some(8001.0),
        video_engine_load: Some(18.6),
    }
}

fn base(cpu: Option<CpuInfo>, gpu: Option<Vec<GpuInfo>>) -> Snapshot {
    Snapshot {
        v: 1,
        ts: 1_747_645_200,
        ts_ms: Some(1_747_645_200_123),
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
            modules: Some(vec![perfwindow::ipc::RamModule {
                label: "Kingston KF556S40-16 #0".into(),
                capacity_gb: Some(16.0),
                temp_c: Some(44.0),
                timings: Some("CL40-40-40-80 @ 5602 MT/s".into()),
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
            percentage_used_pct: Some(2.0),
            temp_warn_c: Some(79.0),
            temp_crit_c: Some(82.0),
            data_read_gb: Some(33676.0),
            data_written_gb: Some(26245.0),
        }]),
        board: Some(BoardInfo {
            temp: Some(38.0),
            vrm_temp: Some(61.0),
            name: Some("ASUS FX507VI".into()),
            bios_version: Some("FX507VI.316".into()),
            bios_date: Some("2023-11-15".into()),
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
            wifi: None,
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
            model: Some("ROG NEBULA".into()),
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
            adapter: "Wi-Fi".into(),
            down_bps: Some(nan),
            up_bps: Some(inf),
            link_bps: Some(i64::MAX),
            down_pct: Some(nan),
            up_pct: Some(inf),
            wifi: Some(perfwindow::ipc::WifiInfo {
                ssid: Some("".into()),
                signal_pct: Some(nan),
                phy_mbps: Some(inf),
                band: None,
            }),
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
                    percentage_used_pct: Some(i as f64),
                    temp_warn_c: Some(70.0),
                    temp_crit_c: Some(85.0),
                    data_read_gb: Some(10_000.0),
                    data_written_gb: Some(8_000.0),
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

#[test]
fn igpu_populated_snapshot_renders() {
    run_matrix("igpu", || {
        let mut s = base(
            Some(cpu(Some(34.0), 24, Some(8))),
            Some(vec![gpu(Some(51.0))]),
        );
        s.igpu = Some(igpu());
        Some(s)
    });
}

// ---- pointer-event, degenerate-viewport and content scenarios -------------

const SIZE: Vec2 = Vec2::new(1180.0, 600.0);

fn realistic_snapshot() -> Snapshot {
    base(
        Some(cpu(Some(34.0), 24, Some(8))),
        Some(vec![gpu(Some(51.0))]),
    )
}

/// App in the steady Running state with a populated snapshot + history.
fn running_app(theme_id: ThemeId) -> (PerfApp, egui::Context) {
    let (mut app, ctx) = fresh(theme_id, TempUnit::Celsius, false);
    let snap = realistic_snapshot();
    app.status = Status::Running;
    app.history.record(&snap);
    app.history.record(&snap);
    app.latest = Some(snap);
    (app, ctx)
}

/// Park a live (never-polled) connect receiver on the app so RETRY / RESPAWN
/// clicks exercise the real `restart_connect` guard instead of spawning a
/// genuine connect worker (which would try the production pipe and the UAC
/// elevation path from inside a test).
fn arm_connect_guard(app: &mut PerfApp) -> std::sync::mpsc::Sender<connect::ConnectEvent> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.connect_rx = Some(rx);
    tx
}

#[test]
fn frames_contain_expected_content() {
    // Running grid: chrome + cards + footer text all present.
    let (mut app, ctx) = running_app(ThemeId::Amber);
    let _ = run_frame(&mut app, &ctx, SIZE, 1.0, Vec::new());
    let out = run_frame(&mut app, &ctx, SIZE, 1.0, Vec::new());
    assert!(out.shape_count > 0, "running frame produced no shapes");
    for needle in ["PERFWINDOW", "CPU", "STORAGE", "NETWORK", "BATTERY"] {
        assert!(
            out.text.contains(needle),
            "running frame is missing {needle:?}; text was:\n{}",
            out.text
        );
    }

    // Running with no snapshot: the loading-sensors fallback.
    let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
    app.status = Status::Running;
    let _ = run_frame(&mut app, &ctx, SIZE, 1.0, Vec::new());
    let out = run_frame(&mut app, &ctx, SIZE, 1.0, Vec::new());
    assert!(out.text.contains("Loading sensors"));

    // Sensord down: the error overlay replaces the grid.
    let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
    app.status = Status::SensordDown;
    let _ = run_frame(&mut app, &ctx, SIZE, 1.0, Vec::new());
    let out = run_frame(&mut app, &ctx, SIZE, 1.0, Vec::new());
    assert!(out.text.contains("SENSOR FEED STOPPED"));
    assert!(out.text.contains("RESPAWN"));
}

#[test]
fn hover_sweep_exercises_tooltips_without_crashing() {
    // Sweep the pointer across the whole window in each headline state. The
    // 1 s frame-clock step exceeds egui's tooltip delay, so a second frame at
    // the same position actually lays the tooltip out (stat rows, donut,
    // disk rows, footer uptime).
    type StateFactory = fn() -> (PerfApp, egui::Context);
    let states: &[(&str, StateFactory)] = &[
        ("running", || running_app(ThemeId::Amber)),
        ("loading", || {
            let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
            app.status = Status::Running;
            (app, ctx)
        }),
        ("overlay", || {
            let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
            app.status = Status::SensordDown;
            (app, ctx)
        }),
    ];
    for (label, make) in states {
        let (mut app, ctx) = make();
        let _guard = arm_connect_guard(&mut app);
        render_one_frame(&mut app, &ctx, SIZE, 1.0);
        let mut y = 8.0;
        while y < SIZE.y {
            let mut x = 8.0;
            while x < SIZE.x {
                eprintln!("STRESS hover state={label} x={x} y={y}");
                let pos = Pos2::new(x, y);
                let _ = run_frame(
                    &mut app,
                    &ctx,
                    SIZE,
                    1.0,
                    vec![egui::Event::PointerMoved(pos)],
                );
                // Tooltip frame: same position, hover delay elapsed.
                let _ = run_frame(&mut app, &ctx, SIZE, 1.0, Vec::new());
                x += 72.0;
            }
            y += 48.0;
        }
    }
}

#[test]
fn loading_failed_click_sweep_drives_retry_and_exit() {
    let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
    app.status = Status::Connecting(connect::ConnectPhase::Failed(
        connect::FailedReason::UacCancelled,
    ));
    let _guard = arm_connect_guard(&mut app);
    render_one_frame(&mut app, &ctx, SIZE, 1.0);

    // Click a grid of points across the central band (below the title bar,
    // above the footer) — dense enough that both buttons are guaranteed hits.
    let mut y = 140.0;
    while y < 420.0 {
        let mut x = 16.0;
        while x < SIZE.x {
            let _ = run_frame(&mut app, &ctx, SIZE, 1.0, click_events(Pos2::new(x, y)));
            x += 20.0;
        }
        y += 12.0;
    }

    // EXIT was hit somewhere in the sweep.
    assert!(app.want_quit, "the click sweep never landed on EXIT");
    // RETRY was hit too, but the live connect receiver makes restart_connect
    // a no-op: no fresh machine was spawned and the receiver is untouched.
    assert!(app.connect_rx.is_some());
    assert!(matches!(
        app.status,
        Status::Connecting(connect::ConnectPhase::Failed(_))
    ));
}

#[test]
fn error_overlay_click_sweep_keeps_the_respawn_guard() {
    let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
    app.status = Status::SensordDown;
    let _guard = arm_connect_guard(&mut app);
    render_one_frame(&mut app, &ctx, SIZE, 1.0);

    let mut y = 60.0;
    while y < 360.0 {
        let mut x = 16.0;
        while x < SIZE.x {
            let _ = run_frame(&mut app, &ctx, SIZE, 1.0, click_events(Pos2::new(x, y)));
            x += 20.0;
        }
        y += 12.0;
    }

    // RESPAWN routes through restart_connect, whose re-entrancy guard sees
    // the live receiver and declines — status must not have been reset.
    assert!(matches!(app.status, Status::SensordDown));
    assert!(app.connect_rx.is_some());
    assert!(!app.want_quit);
}

#[test]
fn degenerate_viewports_render() {
    let sizes: &[(Vec2, f32)] = &[
        (Vec2::ZERO, 1.0),
        (Vec2::splat(1.0), 1.0),
        (Vec2::splat(1.0), 2.0),
    ];
    for &(size, ppp) in sizes {
        for state in ["running", "loading", "overlay"] {
            let (mut app, ctx) = match state {
                "running" => running_app(ThemeId::Amber),
                "loading" => {
                    let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
                    app.status = Status::Running;
                    (app, ctx)
                }
                _ => {
                    let (mut app, ctx) = fresh(ThemeId::Amber, TempUnit::Celsius, false);
                    app.status = Status::SensordDown;
                    (app, ctx)
                }
            };
            eprintln!("STRESS degenerate state={state} size={size:?} ppp={ppp}");
            render_one_frame(&mut app, &ctx, size, ppp);
            render_one_frame(&mut app, &ctx, size, ppp);
        }
    }
}
