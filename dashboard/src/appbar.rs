//! Windows application-desktop-toolbar ("appbar") ownership for the mini strip.
//!
//! The strip is a borderless window docked to a monitor's top edge. A plain
//! `SetWindowPos` would leave it floating *over* other windows; instead the
//! window is registered with `SHAppBarMessage` as an appbar on `ABE_TOP`, which
//! makes the shell shrink that monitor's work area by the strip's height — the
//! same mechanism the taskbar itself uses — so maximized windows start below
//! the strip instead of underneath it.
//!
//! Appbar registration is process-wide shell state: an appbar that is never
//! removed keeps its band reserved. Every path that can end strip mode sends
//! `ABM_REMOVE`:
//!
//! * [`StripSession::release`] — leaving strip mode.
//! * [`StripSession::drop`] — the session being torn down: the app's normal
//!   close path, a relaunch, and any unwind that drops it.
//! * the window subclass's `WM_DESTROY` arm — the window is going away and the
//!   session has not been dropped yet.
//! * [`emergency_remove`] from the panic hook and the SEH filter in `main.rs` —
//!   best effort, allocation-free and `catch_unwind`-safe, because those run
//!   while the process is already in trouble.
//!
//! A process killed from outside (Task Manager, `TerminateProcess`, a power
//! loss) runs none of these; no in-process code can cover that case.
//!
//! Everything above the Win32 seam is pure geometry, so the dock maths is
//! unit-tested without a desktop: the calls themselves sit behind the small
//! [`AppBarShell`] trait, which the real implementation and the tests both
//! satisfy.

/// Height of the strip in logical (DPI-independent) pixels. The window itself
/// is sized in device pixels: `STRIP_HEIGHT_LOGICAL * monitor scale`.
pub const STRIP_HEIGHT_LOGICAL: f32 = 28.0;

/// The screen edge the strip is docked to. Only [`AppBarEdge::Top`] is used
/// today; the other edges keep the rectangle maths complete and testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppBarEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// A rectangle in virtual-screen pixels, in the same shape as Win32's `RECT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    /// Width in pixels; never negative for a degenerate (inverted) rectangle.
    pub fn width(self) -> i32 {
        (self.right - self.left).max(0)
    }

    /// Height in pixels; never negative for a degenerate (inverted) rectangle.
    pub fn height(self) -> i32 {
        (self.bottom - self.top).max(0)
    }
}

/// `STRIP_HEIGHT_LOGICAL` at `scale`, as whole device pixels and never below 1:
/// a nonsense monitor scale must not ask the shell for a zero-height appbar.
pub fn strip_thickness_px(scale: f32) -> i32 {
    let px = STRIP_HEIGHT_LOGICAL * scale;
    if !px.is_finite() || px < 1.0 {
        1
    } else {
        px.round() as i32
    }
}

/// The band to ask the shell for: `thickness` deep and spanning all of
/// `monitor` along the edge, so the strip covers its monitor exactly.
pub fn requested_rect(monitor: Rect, edge: AppBarEdge, thickness: i32) -> Rect {
    let t = thickness.max(1);
    match edge {
        AppBarEdge::Top => Rect {
            left: monitor.left,
            top: monitor.top,
            right: monitor.right,
            bottom: monitor.top + t,
        },
        AppBarEdge::Bottom => Rect {
            left: monitor.left,
            top: monitor.bottom - t,
            right: monitor.right,
            bottom: monitor.bottom,
        },
        AppBarEdge::Left => Rect {
            left: monitor.left,
            top: monitor.top,
            right: monitor.left + t,
            bottom: monitor.bottom,
        },
        AppBarEdge::Right => Rect {
            left: monitor.right - t,
            top: monitor.top,
            right: monitor.right,
            bottom: monitor.bottom,
        },
    }
}

/// Re-assert the strip's own band after `ABM_QUERYPOS`.
///
/// The shell moves the proposed band perpendicular to the edge when something
/// else already owns that strip (the taskbar, another appbar, an autohide
/// bar); that offset is kept, because it is the shell telling us where the
/// band may start. The band's thickness and its length along the edge stay
/// ours, so the strip is never handed a slot it cannot draw in. `ABM_SETPOS`
/// may still trim the result, and the rectangle it returns is the one that
/// counts.
pub fn adjust_queried(requested: Rect, queried: Rect, edge: AppBarEdge) -> Rect {
    match edge {
        AppBarEdge::Top => Rect {
            top: queried.top,
            bottom: queried.top + requested.height(),
            ..requested
        },
        AppBarEdge::Bottom => Rect {
            top: queried.bottom - requested.height(),
            bottom: queried.bottom,
            ..requested
        },
        AppBarEdge::Left => Rect {
            left: queried.left,
            right: queried.left + requested.width(),
            ..requested
        },
        AppBarEdge::Right => Rect {
            left: queried.right - requested.width(),
            right: queried.right,
            ..requested
        },
    }
}

/// The window rectangle for a band the shell granted.
///
/// The strip is borderless, so the window covers the band exactly: the
/// position comes from the band's origin and the size from its edges. A band
/// the shell trimmed to nothing (a monitor narrower than the strip) is widened
/// to one pixel rather than handed to `SetWindowPos` as a zero or negative
/// size.
pub fn window_rect(granted: Rect) -> Rect {
    Rect {
        left: granted.left,
        top: granted.top,
        right: granted.left + granted.width().max(1),
        bottom: granted.top + granted.height().max(1),
    }
}

/// The Win32 calls the dock sequence needs. Split out so [`dock`] can be
/// driven by a fake shell in the unit tests.
pub trait AppBarShell {
    /// `ABM_NEW` on `edge`; `false` when the shell refuses the registration.
    fn register(&mut self, edge: AppBarEdge) -> bool;
    /// `ABM_QUERYPOS`: the shell's proposed band for `requested`.
    fn query_pos(&mut self, requested: Rect) -> Rect;
    /// `ABM_SETPOS`: commit `band` and return the rectangle actually granted.
    fn set_pos(&mut self, band: Rect) -> Rect;
    /// `ABM_REMOVE`.
    fn remove(&mut self);
    /// Move the window onto the granted band.
    fn place_window(&mut self, window: Rect);
}

/// Register the appbar, reserve `edge` on `monitor` and move the window onto
/// the granted band. Returns the window rectangle that was applied, or `None`
/// when the shell refused the registration.
///
/// This is the whole `ABM_NEW` → `ABM_QUERYPOS` → adjust → `ABM_SETPOS` →
/// `SetWindowPos` sequence the appbar documentation prescribes, minus the
/// Win32 calls themselves.
pub fn dock<A: AppBarShell>(
    shell: &mut A,
    monitor: Rect,
    edge: AppBarEdge,
    thickness: i32,
) -> Option<Rect> {
    if !shell.register(edge) {
        return None;
    }
    let requested = requested_rect(monitor, edge, thickness);
    let queried = shell.query_pos(requested);
    let band = adjust_queried(requested, queried, edge);
    let granted = window_rect(shell.set_pos(band));
    shell.place_window(granted);
    Some(granted)
}

/// What the shell told the strip since the last frame was drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripFlags {
    /// The band must be re-reserved (`ABN_POSCHANGED`, `WM_DISPLAYCHANGE`,
    /// `WM_DPICHANGED`).
    pub redock: bool,
    /// A fullscreen application is running (`ABN_FULLSCREENAPP`); the strip
    /// leaves the topmost band so the game can cover it.
    pub fullscreen_app: bool,
}

/// Best-effort `ABM_REMOVE` for the crash paths.
///
/// Called from the panic hook and the SEH filter in `main.rs`, where the heap
/// may already be damaged: it reads one atomic, makes one `SHAppBarMessage`
/// call and allocates nothing. A process killed from outside never gets here.
#[cfg(not(windows))]
pub fn emergency_remove() {}

/// See the non-Windows stub of the same name: this one removes the live
/// registration, if there is one.
#[cfg(windows)]
pub fn emergency_remove() {
    win32::emergency_remove();
}

#[cfg(not(windows))]
/// Non-Windows stub: there is no shell appbar to own, so the strip is never
/// docked and nothing is ever reserved.
#[derive(Debug)]
pub struct StripSession;

#[cfg(not(windows))]
impl StripSession {
    /// No appbar on this platform, so there is nothing to attach to.
    pub fn attach(_hwnd: isize, _target_device: Option<&str>) -> Option<Self> {
        None
    }
    pub fn redock(&mut self) {}
    pub fn take_flags(&mut self) -> StripFlags {
        StripFlags {
            redock: false,
            fullscreen_app: false,
        }
    }
    pub fn set_topmost(&self, _topmost: bool) {}
    pub fn device_name(&self) -> Option<&str> {
        None
    }
    pub fn release(&mut self) {}
}

#[cfg(windows)]
pub use win32::StripSession;

#[cfg(windows)]
mod win32 {
    //! The Win32 side of the appbar: `SHAppBarMessage`, the appbar callback
    //! subclass, and the monitor/DPI lookups that feed the dock maths.

    use super::{dock, AppBarEdge, AppBarShell, Rect};
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFOEXW,
        MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::Shell::{
        DefSubclassProc, RemoveWindowSubclass, SHAppBarMessage, SetWindowSubclass, ABE_BOTTOM,
        ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
        ABM_WINDOWPOSCHANGED, ABN_FULLSCREENAPP, ABN_POSCHANGED, APPBARDATA,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, WM_ACTIVATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCDESTROY,
        WM_WINDOWPOSCHANGED,
    };

    /// Our private appbar callback message. `WM_APP` and above are reserved
    /// for application-private messages, so nothing else in this process can
    /// collide with it.
    const APP_BAR_CALLBACK: u32 = 0x8000 + 1;

    /// Subclass identifier, unique per (window, proc) pair in this process.
    const SUBCLASS_ID: usize = 1;

    /// `HWND` value of the live registration, or 0. The panic hook has no
    /// route to the session, so it reads this to send its own `ABM_REMOVE`.
    pub(super) static LIVE_HWND: AtomicIsize = AtomicIsize::new(0);

    fn win_rect(r: Rect) -> RECT {
        RECT {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }

    fn local_rect(rc: RECT) -> Rect {
        Rect {
            left: rc.left,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        }
    }

    fn edge_code(edge: AppBarEdge) -> u32 {
        match edge {
            AppBarEdge::Left => ABE_LEFT,
            AppBarEdge::Top => ABE_TOP,
            AppBarEdge::Right => ABE_RIGHT,
            AppBarEdge::Bottom => ABE_BOTTOM,
        }
    }

    /// An `APPBARDATA` for this window and callback message. `cbSize` is the
    /// struct's real size on the running target (48 bytes on x64), which is
    /// what the shell validates before it reads the rest of the struct.
    fn bar_data(hwnd: HWND, edge: AppBarEdge, rc: RECT) -> APPBARDATA {
        APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: APP_BAR_CALLBACK,
            uEdge: edge_code(edge),
            rc,
            lParam: LPARAM(0),
        }
    }

    /// The real shell, one per dock attempt.
    struct WinAppBarShell {
        hwnd: HWND,
        edge: AppBarEdge,
    }

    impl AppBarShell for WinAppBarShell {
        fn register(&mut self, edge: AppBarEdge) -> bool {
            self.edge = edge;
            let mut data = bar_data(self.hwnd, edge, RECT::default());
            unsafe { SHAppBarMessage(ABM_NEW, &mut data) != 0 }
        }

        fn query_pos(&mut self, requested: Rect) -> Rect {
            let mut data = bar_data(self.hwnd, self.edge, win_rect(requested));
            unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
            local_rect(data.rc)
        }

        fn set_pos(&mut self, band: Rect) -> Rect {
            let mut data = bar_data(self.hwnd, self.edge, win_rect(band));
            unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
            local_rect(data.rc)
        }

        fn remove(&mut self) {
            let mut data = bar_data(self.hwnd, self.edge, RECT::default());
            unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
        }

        fn place_window(&mut self, window: Rect) {
            // `SWP_NOZORDER` keeps whatever the strip's topmost state is;
            // `SWP_NOACTIVATE` must not steal focus from the user's window.
            let _ = unsafe {
                SetWindowPos(
                    self.hwnd,
                    None,
                    window.left,
                    window.top,
                    window.width(),
                    window.height(),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            };
        }
    }

    /// One monitor, as the strip needs it.
    struct Monitor {
        /// GDI device name (`\\.\DISPLAY1`), the value persisted in
        /// `Config::mini_strip_monitor`.
        device: String,
        rect: Rect,
    }

    fn monitor_info(handle: HMONITOR) -> Option<Monitor> {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        // `MONITORINFOEXW` starts with a `MONITORINFO`, which is the view the
        // API's `*mut MONITORINFO` parameter expects.
        let ok = unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo as *mut _) };
        if !ok.as_bool() {
            return None;
        }
        Some(Monitor {
            device: utf16_device_name(&info.szDevice),
            rect: local_rect(info.monitorInfo.rcMonitor),
        })
    }

    /// A NUL-terminated `WCHAR[32]` device name as a `String`.
    fn utf16_device_name(buf: &[u16]) -> String {
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    /// Enumeration state for [`monitor_matching`]: `HMONITOR` values do not
    /// survive a relaunch, so the persisted device name is resolved fresh.
    struct FindMonitor<'a> {
        name: &'a str,
        found: Option<HMONITOR>,
    }

    unsafe extern "system" fn find_monitor_cb(
        handle: HMONITOR,
        _hdc: HDC,
        _clip: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // Safety: `data` is the `&mut FindMonitor` that `monitor_matching`
        // passes to `EnumDisplayMonitors` for the duration of that call.
        let ctx = unsafe { &mut *(data.0 as *mut FindMonitor<'_>) };
        if let Some(monitor) = monitor_info(handle) {
            if monitor.device == ctx.name {
                ctx.found = Some(handle);
                // Stop enumerating: the device name is unique per monitor.
                return BOOL(0);
            }
        }
        BOOL(1)
    }

    /// The monitor whose GDI device name is `name`, or `None` when it is gone.
    fn monitor_matching(name: &str) -> Option<HMONITOR> {
        let mut ctx = FindMonitor { name, found: None };
        let _ = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(find_monitor_cb),
                LPARAM(&mut ctx as *mut FindMonitor<'_> as isize),
            )
        };
        ctx.found
    }

    /// The monitor to dock on: `target_device` when that monitor still exists,
    /// the monitor the window is on otherwise.
    fn preferred_monitor(hwnd: HWND, target_device: Option<&str>) -> Option<Monitor> {
        if let Some(monitor) = target_device
            .and_then(monitor_matching)
            .and_then(monitor_info)
        {
            return Some(monitor);
        }
        monitor_info(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) })
    }

    /// The PerMonitorV2 scale for the monitor the window is on, as a factor of
    /// 96 DPI. `GetDpiForWindow` reports 0 for an invalid window.
    fn dpi_scale(hwnd: HWND) -> f32 {
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        if dpi == 0 {
            1.0
        } else {
            dpi as f32 / 96.0
        }
    }

    /// Run the full dock sequence on `monitor`: `ABM_NEW`, `ABM_QUERYPOS`,
    /// adjust, `ABM_SETPOS`, `SetWindowPos`.
    fn dock_on(hwnd: HWND, monitor: &Monitor, edge: AppBarEdge) -> Option<Rect> {
        let thickness = super::strip_thickness_px(dpi_scale(hwnd));
        let mut shell = WinAppBarShell { hwnd, edge };
        dock(&mut shell, monitor.rect, edge, thickness)
    }

    /// State the subclass proc needs. Owned by [`StripSession`] in a `Box`, so
    /// the `dwRefData` pointer handed to `SetWindowSubclass` stays valid for
    /// exactly as long as the subclass is installed.
    struct SubclassState {
        hwnd: HWND,
        /// Set by `ABN_POSCHANGED` and the display/DPI change messages; the
        /// strip re-runs the dock sequence on its next frame.
        redock: AtomicBool,
        /// Mirrors `ABN_FULLSCREENAPP`'s data: a fullscreen app owns the
        /// topmost band while it runs.
        fullscreen_app: AtomicBool,
        /// Cleared as soon as this window's `ABM_REMOVE` has been sent, so the
        /// `WM_DESTROY` arm and the session's `Drop` cannot double-remove.
        registered: AtomicBool,
    }

    /// Read an appbar callback's notification code and its data.
    ///
    /// The shell sends the `ABN_*` notification code in `wParam` and the
    /// event-specific value in `lParam` — `ABN_FULLSCREENAPP`'s is `TRUE`/`FALSE`
    /// for whether a fullscreen application is running. The two arguments
    /// cannot be told apart heuristically (`ABN_POSCHANGED` is 1, the same as
    /// a true flag), so the documented order is the only order read here.
    fn decode_callback(wparam: usize, lparam: isize) -> (u32, bool) {
        (wparam as u32, lparam != 0)
    }

    /// The appbar callback, plus the two messages the appbar documentation
    /// requires an appbar to forward. Everything else falls through to the
    /// window's own proc — winit's.
    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        ref_data: usize,
    ) -> LRESULT {
        if ref_data != 0 {
            // Safety: `ref_data` is the live `SubclassState` for as long as the
            // subclass is installed, and `StripSession::release` removes the
            // subclass before the box is dropped.
            let state = unsafe { &*(ref_data as *const SubclassState) };
            match msg {
                APP_BAR_CALLBACK => {
                    let (code, data) = decode_callback(wparam.0, lparam.0);
                    match code {
                        ABN_POSCHANGED => state.redock.store(true, Ordering::Relaxed),
                        ABN_FULLSCREENAPP => state.fullscreen_app.store(data, Ordering::Relaxed),
                        _ => {}
                    }
                    // A registered appbar answers its callback message with TRUE.
                    return LRESULT(1);
                }
                WM_ACTIVATE => {
                    forward_bar_message(hwnd, state, ABM_ACTIVATE, wparam.0 as isize);
                }
                WM_WINDOWPOSCHANGED => {
                    forward_bar_message(hwnd, state, ABM_WINDOWPOSCHANGED, 0);
                }
                WM_DISPLAYCHANGE | WM_DPICHANGED => {
                    state.redock.store(true, Ordering::Relaxed);
                }
                WM_DESTROY => {
                    // The band goes back to the desktop even when the app never
                    // reaches its own exit path.
                    remove_registration_if_live(state);
                }
                WM_NCDESTROY => {
                    // Windows drops the subclass with the window; forget the
                    // cached handle so the session's `Drop` cannot act on a
                    // recycled one.
                    LIVE_HWND.store(0, Ordering::Relaxed);
                }
                _ => {}
            }
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Forward an activate/window-position change to the shell, as the appbar
    /// documentation requires: the shell needs both to lay the bars out again.
    fn forward_bar_message(hwnd: HWND, state: &SubclassState, message: u32, value: isize) {
        let mut data = bar_data(hwnd, AppBarEdge::Top, RECT::default());
        data.hWnd = state.hwnd;
        data.lParam = LPARAM(value);
        unsafe { SHAppBarMessage(message, &mut data) };
    }

    fn remove_registration_if_live(state: &SubclassState) {
        if state.registered.swap(false, Ordering::Relaxed) {
            unregister(state.hwnd);
            let raw = state.hwnd.0 as isize;
            let _ = LIVE_HWND.compare_exchange(raw, 0, Ordering::Relaxed, Ordering::Relaxed);
        }
    }

    /// Send `ABM_REMOVE` for `hwnd`. The only `ABM_REMOVE` call in the crate,
    /// shared by the strip session, the subclass's `WM_DESTROY` arm and the
    /// crash path, so every exit route releases the band the same way.
    fn unregister(hwnd: HWND) {
        let mut shell = WinAppBarShell {
            hwnd,
            edge: AppBarEdge::Top,
        };
        shell.remove();
    }

    pub(super) fn emergency_remove() {
        let raw = LIVE_HWND.swap(0, Ordering::Relaxed);
        if raw == 0 {
            return;
        }
        unregister(HWND(raw as *mut core::ffi::c_void));
    }

    /// A live mini-strip appbar registration.
    ///
    /// Dropping it — or calling [`StripSession::release`] — sends `ABM_REMOVE`
    /// and unsubclasses the window.
    pub struct StripSession {
        state: Box<SubclassState>,
        /// The monitor the strip is reserved on, by GDI device name.
        device_name: Option<String>,
        subclassed: bool,
    }

    impl StripSession {
        /// Subclass `hwnd`, register the appbar on the top edge of
        /// `target_device`'s monitor (the monitor `hwnd` is on when the device
        /// name is absent or gone) and move the window onto the granted band.
        ///
        /// `None` means no strip session could be set up at all: a zero window
        /// handle, or a window that refuses to be subclassed. A registration
        /// the shell refuses still yields a session — the strip then shows
        /// without reserving its band, which beats not showing at all.
        pub fn attach(hwnd: isize, target_device: Option<&str>) -> Option<Self> {
            if hwnd == 0 {
                return None;
            }
            let hwnd = HWND(hwnd as *mut core::ffi::c_void);
            let state = Box::new(SubclassState {
                hwnd,
                redock: AtomicBool::new(false),
                fullscreen_app: AtomicBool::new(false),
                registered: AtomicBool::new(false),
            });
            let ref_data = (&*state as *const SubclassState) as usize;
            let installed =
                unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, ref_data) };
            if !installed.as_bool() {
                eprintln!(
                    "PerfWindow: could not subclass the window for the mini strip appbar; \
                     continuing without a reserved band"
                );
                return None;
            }

            let monitor = preferred_monitor(hwnd, target_device);
            let granted = monitor
                .as_ref()
                .and_then(|m| dock_on(hwnd, m, AppBarEdge::Top));
            if granted.is_none() {
                eprintln!(
                    "PerfWindow: the shell refused the mini strip appbar \
                     (SHAppBarMessage ABM_NEW); continuing without a reserved band"
                );
            }
            state.registered.store(granted.is_some(), Ordering::Relaxed);
            if granted.is_some() {
                LIVE_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            }

            Some(Self {
                state,
                device_name: monitor.map(|m| m.device),
                subclassed: true,
            })
        }

        /// Re-run the dock sequence: another appbar moved, the display layout
        /// changed, the DPI changed, or the strip moved to another monitor.
        pub fn redock(&mut self) {
            let hwnd = self.state.hwnd;
            let monitor = preferred_monitor(hwnd, self.device_name.as_deref());
            let granted = monitor
                .as_ref()
                .and_then(|m| dock_on(hwnd, m, AppBarEdge::Top));
            self.state
                .registered
                .store(granted.is_some(), Ordering::Relaxed);
            if granted.is_some() {
                LIVE_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            }
            if let Some(monitor) = monitor {
                self.device_name = Some(monitor.device);
            }
        }

        /// Drain the notifications the shell sent since the last call.
        pub fn take_flags(&mut self) -> super::StripFlags {
            super::StripFlags {
                redock: self.state.redock.swap(false, Ordering::Relaxed),
                fullscreen_app: self.state.fullscreen_app.load(Ordering::Relaxed),
            }
        }

        /// Raise the strip into (or drop it out of) the topmost band. The
        /// shell asks for this on `ABN_FULLSCREENAPP`: `HWND_NOTOPMOST` is
        /// enough to let a game cover the strip, and `HWND_BOTTOM` is neither
        /// needed nor wanted (it would put the strip under everything else
        /// when the game exits).
        pub fn set_topmost(&self, topmost: bool) {
            let after = if topmost {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            let _ = unsafe {
                SetWindowPos(
                    self.state.hwnd,
                    Some(after),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
        }

        /// The device name of the monitor the strip is reserved on.
        pub fn device_name(&self) -> Option<&str> {
            self.device_name.as_deref()
        }

        /// Send `ABM_REMOVE` and drop the subclass. Idempotent.
        pub fn release(&mut self) {
            remove_registration_if_live(&self.state);
            if self.subclassed {
                let _ = unsafe {
                    RemoveWindowSubclass(self.state.hwnd, Some(subclass_proc), SUBCLASS_ID)
                };
                self.subclassed = false;
            }
        }
    }

    impl Drop for StripSession {
        fn drop(&mut self) {
            self.release();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn callback_reads_the_notification_from_wparam() {
            assert_eq!(
                decode_callback(ABN_POSCHANGED as usize, 0),
                (ABN_POSCHANGED, false)
            );
            assert_eq!(
                decode_callback(ABN_FULLSCREENAPP as usize, 1),
                (ABN_FULLSCREENAPP, true)
            );
            assert_eq!(
                decode_callback(ABN_FULLSCREENAPP as usize, 0),
                (ABN_FULLSCREENAPP, false)
            );
        }

        #[test]
        fn callback_ignores_notifications_the_strip_does_not_act_on() {
            // `ABN_STATECHANGE` (0) and `ABN_WINDOWARRANGE` (3) have no
            // effect on the strip, but must still decode without disturbing
            // the fullscreen flag they carry in `lParam`.
            use windows::Win32::UI::Shell::{ABN_STATECHANGE, ABN_WINDOWARRANGE};
            assert_eq!(
                decode_callback(ABN_STATECHANGE as usize, 0),
                (ABN_STATECHANGE, false)
            );
            assert_eq!(
                decode_callback(ABN_WINDOWARRANGE as usize, 1),
                (ABN_WINDOWARRANGE, true)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shell that records the calls it receives and hands back a fixed
    /// grant, so the dock sequence's shape can be asserted without a desktop.
    #[derive(Default)]
    struct FakeShell {
        calls: Vec<&'static str>,
        edge: Option<AppBarEdge>,
        queried: Option<Rect>,
        granted: Option<Rect>,
        placed: Option<Rect>,
    }

    impl AppBarShell for FakeShell {
        fn register(&mut self, edge: AppBarEdge) -> bool {
            self.calls.push("register");
            self.edge = Some(edge);
            true
        }
        fn query_pos(&mut self, requested: Rect) -> Rect {
            self.calls.push("query");
            self.queried = Some(requested);
            // The shell nudges the band down, as if a taskbar owned the top.
            Rect {
                top: requested.top + 40,
                bottom: requested.bottom + 40,
                ..requested
            }
        }
        fn set_pos(&mut self, band: Rect) -> Rect {
            self.calls.push("set");
            self.granted = Some(band);
            band
        }
        fn remove(&mut self) {
            self.calls.push("remove");
        }
        fn place_window(&mut self, window: Rect) {
            self.calls.push("place");
            self.placed = Some(window);
        }
    }

    #[test]
    fn thickness_scales_with_the_monitor_dpi() {
        assert_eq!(strip_thickness_px(1.0), 28);
        assert_eq!(strip_thickness_px(1.25), 35);
        assert_eq!(strip_thickness_px(1.5), 42);
        assert_eq!(strip_thickness_px(2.0), 56);
        // Degenerate scales never produce a zero-height band.
        assert_eq!(strip_thickness_px(0.0), 1);
        assert_eq!(strip_thickness_px(f32::NAN), 1);
    }

    #[test]
    fn requested_rect_spans_the_monitor_along_the_edge() {
        let monitor = Rect {
            left: -1920,
            top: 120,
            right: 0,
            bottom: 1200,
        };
        assert_eq!(
            requested_rect(monitor, AppBarEdge::Top, 35),
            Rect {
                left: -1920,
                top: 120,
                right: 0,
                bottom: 155
            }
        );
        assert_eq!(
            requested_rect(monitor, AppBarEdge::Bottom, 35),
            Rect {
                left: -1920,
                top: 1165,
                right: 0,
                bottom: 1200
            }
        );
        assert_eq!(
            requested_rect(monitor, AppBarEdge::Left, 35),
            Rect {
                left: -1920,
                top: 120,
                right: -1885,
                bottom: 1200
            }
        );
        // A zero thickness is widened to one pixel rather than reserving nothing.
        assert_eq!(requested_rect(monitor, AppBarEdge::Top, 0).height(), 1);
    }

    #[test]
    fn adjust_queried_keeps_the_shells_offset_and_our_thickness() {
        let monitor = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let requested = requested_rect(monitor, AppBarEdge::Top, 28);
        // Shell proposes the band 40 px lower (its taskbar owns the top edge).
        let queried = Rect {
            top: 40,
            bottom: 68,
            ..requested
        };
        assert_eq!(
            adjust_queried(requested, queried, AppBarEdge::Top),
            Rect {
                left: 0,
                top: 40,
                right: 1920,
                bottom: 68
            }
        );
        // A shell that proposes a shorter band is not obeyed: the strip keeps
        // its own thickness and the shell's offset.
        let short = Rect {
            top: 40,
            bottom: 50,
            ..requested
        };
        assert_eq!(
            adjust_queried(requested, short, AppBarEdge::Top).height(),
            28
        );
    }

    #[test]
    fn window_rect_covers_the_granted_band() {
        let granted = Rect {
            left: 0,
            top: 40,
            right: 1920,
            bottom: 68,
        };
        assert_eq!(window_rect(granted), granted);
        // Degenerate grants are widened instead of handed to SetWindowPos.
        let empty = Rect {
            left: 10,
            top: 10,
            right: 10,
            bottom: 10,
        };
        let window = window_rect(empty);
        assert_eq!(window.width(), 1);
        assert_eq!(window.height(), 1);
    }

    #[test]
    fn dock_runs_the_documented_call_order() {
        let monitor = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let mut shell = FakeShell::default();
        let window = dock(&mut shell, monitor, AppBarEdge::Top, 28).expect("registration succeeds");

        assert_eq!(shell.calls, ["register", "query", "set", "place"]);
        assert_eq!(shell.edge, Some(AppBarEdge::Top));
        // Reserved band: full monitor width, our thickness, at the shell's offset.
        let granted = shell.granted.expect("a band was committed");
        assert_eq!(granted.left, 0);
        assert_eq!(granted.right, 1920);
        assert_eq!(granted.top, 40);
        assert_eq!(granted.height(), 28);
        // The window lands on exactly the band that was granted.
        assert_eq!(window, shell.placed.expect("the window was moved"));
        assert_eq!(window.bottom, 68);
    }

    #[test]
    fn a_scaled_monitor_gets_a_scaled_band() {
        let monitor = Rect {
            left: 1920,
            top: 0,
            right: 5760,
            bottom: 2160,
        };
        let mut shell = FakeShell::default();
        let thickness = strip_thickness_px(2.0);
        let window = dock(&mut shell, monitor, AppBarEdge::Top, thickness).unwrap();
        assert_eq!(thickness, 56);
        assert_eq!(shell.queried.unwrap().height(), 56);
        assert_eq!(window.height(), 56);
        assert_eq!(window.left, 1920);
        assert_eq!(window.width(), 3840);
    }

    #[test]
    fn a_refused_registration_never_touches_the_window() {
        struct Refusing;
        impl AppBarShell for Refusing {
            fn register(&mut self, _edge: AppBarEdge) -> bool {
                false
            }
            fn query_pos(&mut self, requested: Rect) -> Rect {
                requested
            }
            fn set_pos(&mut self, band: Rect) -> Rect {
                band
            }
            fn remove(&mut self) {}
            fn place_window(&mut self, _window: Rect) {
                panic!("a refused registration must not move the window");
            }
        }
        let monitor = Rect {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 720,
        };
        assert!(dock(&mut Refusing, monitor, AppBarEdge::Top, 28).is_none());
    }
}
