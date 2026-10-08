//! Topmost overlay placement for the mini strip.
//!
//! The strip is a borderless window covering the top `STRIP_HEIGHT_LOGICAL`
//! logical pixels of one monitor. It is deliberately *not* registered with the
//! shell as an appbar: nothing is reserved, the monitor's work area is never
//! touched, and a maximized window or a game still reads the full desktop size.
//! The strip simply renders on top of everything on that monitor, the taskbar
//! included where the taskbar is at the top.
//!
//! Nothing is reserved, so there is nothing to hand back on the way out. The
//! only OS state the strip changes is the window's extended style, and
//! [`StripWindow::release`] puts the bits it borrowed back.
//!
//! The geometry is pure and unit-tested ([`band_rect`], [`strip_thickness_px`]);
//! the Win32 calls sit behind a non-Windows stub so the crate still builds off
//! Windows.

/// Height of the strip in logical (DPI-independent) pixels. The window itself
/// is sized in device pixels: `STRIP_HEIGHT_LOGICAL * monitor scale`.
pub const STRIP_HEIGHT_LOGICAL: f32 = 28.0;

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
/// a nonsense monitor scale must not ask `SetWindowPos` for a zero-height
/// window.
pub fn strip_thickness_px(scale: f32) -> i32 {
    let px = STRIP_HEIGHT_LOGICAL * scale;
    if !px.is_finite() || px < 1.0 {
        1
    } else {
        px.round() as i32
    }
}

/// The band the strip covers on `monitor`: the monitor's top edge, its full
/// width, [`strip_thickness_px`] device pixels tall.
///
/// The band starts at the monitor's full rectangle (`rcMonitor`), not its work
/// area, so the strip covers whatever is under it and reserves nothing. It is
/// in device pixels, so a monitor to the left of or above the primary keeps its
/// negative coordinates.
pub fn band_rect(monitor: Rect, scale: f32) -> Rect {
    Rect {
        left: monitor.left,
        top: monitor.top,
        right: monitor.right,
        bottom: monitor.top + strip_thickness_px(scale),
    }
}

#[cfg(not(windows))]
/// Non-Windows stub: there is no Win32 window to place, so strip mode runs
/// without an overlay.
#[derive(Debug)]
pub struct StripWindow;

#[cfg(not(windows))]
impl StripWindow {
    /// No Win32 window on this platform, so there is nothing to place.
    pub fn attach(_hwnd: isize, _target_device: Option<&str>) -> Option<Self> {
        None
    }
    pub fn refresh(&mut self) {}
    pub fn device_name(&self) -> Option<&str> {
        None
    }
    pub fn release(&mut self) {}
}

#[cfg(windows)]
pub use win32::StripWindow;

#[cfg(windows)]
mod win32 {
    //! The Win32 side of the overlay: the monitor/DPI lookups that feed the
    //! band maths, the extended-style swap and the `SetWindowPos` placements.

    use super::{band_rect, Rect};
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFOEXW,
        MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_SHOWWINDOW,
        WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    };

    /// The two extended-style bits the strip owns while it is on: `NOACTIVATE`
    /// so clicking the slider or a control never pulls focus away from the game
    /// or app the user is in, and `APPWINDOW` so the strip keeps its taskbar
    /// button, which a `NOACTIVATE` window otherwise loses.
    const OVERLAY_STYLE_BITS: u32 = WS_EX_NOACTIVATE.0 | WS_EX_APPWINDOW.0;

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

    /// One monitor, as the strip needs it.
    struct Monitor {
        /// GDI device name (`\\.\DISPLAY1`), the value persisted in
        /// `Config::mini_strip_monitor`.
        device: String,
        rect: Rect,
        /// The monitor's own scale, as a factor of 96 DPI.
        scale: f32,
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
            scale: dpi_scale(handle),
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

    /// The monitor to cover: `target_device` when that monitor still exists,
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

    /// The effective scale of `handle`, as a factor of 96 DPI. It is the target
    /// monitor's own scale, not the one the window is on before it moves there,
    /// so the band is sized right on the first placement.
    fn dpi_scale(handle: HMONITOR) -> f32 {
        let (mut x, mut y) = (0u32, 0u32);
        match unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut x, &mut y) } {
            Ok(()) if x > 0 => x as f32 / 96.0,
            _ => 1.0,
        }
    }

    /// The window's current outer rectangle, or `None` when the API fails.
    fn window_rect(hwnd: HWND) -> Option<Rect> {
        let mut rc = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rc) }
            .ok()
            .map(|()| local_rect(rc))
    }

    fn ex_style(hwnd: HWND) -> u32 {
        unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
    }

    fn set_ex_style(hwnd: HWND, style: u32) {
        unsafe {
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        }
    }

    /// Move the window onto `band` and raise it into the topmost band. The
    /// strip is borderless, so the window covers the band exactly. `NOACTIVATE`
    /// must not steal focus, and `NOOWNERZORDER` keeps a subsequent topmost
    /// placement from z-ordering the strip's owner.
    fn place_window(hwnd: HWND, band: Rect) {
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                band.left,
                band.top,
                band.width(),
                band.height(),
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW,
            )
        };
    }

    /// Re-assert the topmost band without moving or resizing the window, so a
    /// game or another topmost window that came up later does not stay above
    /// the strip.
    fn assert_topmost(hwnd: HWND) {
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
        };
    }

    /// A live mini-strip overlay.
    ///
    /// Dropping it — or calling [`StripWindow::release`] — puts back the
    /// extended-style bits the strip took.
    pub struct StripWindow {
        hwnd: HWND,
        /// The monitor the strip covers, by GDI device name.
        device_name: Option<String>,
        /// `GWL_EXSTYLE` as it was before the strip's bits went on, so
        /// `release` can restore the exact values those bits had.
        original_ex_style: u32,
    }

    impl StripWindow {
        /// Take over `hwnd`: add the overlay styles, resolve the monitor to
        /// cover (the configured device when it still exists, the monitor the
        /// window is on otherwise) and move the window onto its band.
        ///
        /// `None` means there is no window to place — a zero handle, as in
        /// headless tests.
        pub fn attach(hwnd: isize, target_device: Option<&str>) -> Option<Self> {
            if hwnd == 0 {
                return None;
            }
            let hwnd = HWND(hwnd as *mut core::ffi::c_void);
            let original_ex_style = ex_style(hwnd);
            set_ex_style(hwnd, original_ex_style | OVERLAY_STYLE_BITS);
            let mut window = Self {
                hwnd,
                device_name: target_device.map(str::to_owned),
                original_ex_style,
            };
            window.refresh();
            Some(window)
        }

        /// Recompute the band and make sure the window is on it and on top.
        ///
        /// Called at most once a second: it covers a monitor that went away,
        /// a resolution or DPI change, and another topmost window that came up
        /// later. A place costs one `GetWindowRect` when the geometry is
        /// unchanged; a move only happens when the band actually differs.
        pub fn refresh(&mut self) {
            self.reposition();
            // winit rewrites the whole `GWL_EXSTYLE` word whenever it applies
            // one of its own window attributes, so the strip's bits are put
            // back when they went missing. One read, a write only when needed.
            let current = ex_style(self.hwnd);
            if current & OVERLAY_STYLE_BITS != OVERLAY_STYLE_BITS {
                set_ex_style(self.hwnd, current | OVERLAY_STYLE_BITS);
            }
        }

        fn reposition(&mut self) {
            let Some(monitor) = preferred_monitor(self.hwnd, self.device_name.as_deref()) else {
                return;
            };
            self.device_name = Some(monitor.device);
            let band = band_rect(monitor.rect, monitor.scale);
            if window_rect(self.hwnd) == Some(band) {
                assert_topmost(self.hwnd);
            } else {
                place_window(self.hwnd, band);
            }
        }

        /// The device name of the monitor the strip covers.
        pub fn device_name(&self) -> Option<&str> {
            self.device_name.as_deref()
        }

        /// Put back the extended-style bits the strip took. Idempotent.
        pub fn release(&mut self) {
            let current = ex_style(self.hwnd);
            let restored =
                (current & !OVERLAY_STYLE_BITS) | (self.original_ex_style & OVERLAY_STYLE_BITS);
            if restored != current {
                set_ex_style(self.hwnd, restored);
            }
        }
    }

    impl Drop for StripWindow {
        fn drop(&mut self) {
            self.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_band_spans_the_primary_monitor_from_its_top_edge() {
        let monitor = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert_eq!(
            band_rect(monitor, 1.0),
            Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 28
            }
        );
        // The band is device pixels, so the scale changes only its height.
        assert_eq!(band_rect(monitor, 1.5).height(), 42);
    }

    #[test]
    fn a_monitor_left_of_and_above_the_primary_keeps_its_coordinates() {
        let monitor = Rect {
            left: -1920,
            top: -120,
            right: 0,
            bottom: 1080,
        };
        assert_eq!(
            band_rect(monitor, 1.0),
            Rect {
                left: -1920,
                top: -120,
                right: 0,
                bottom: -92
            }
        );
    }

    #[test]
    fn the_band_is_never_a_zero_height_window() {
        let monitor = Rect {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 720,
        };
        assert_eq!(band_rect(monitor, 0.0).height(), 1);
        assert_eq!(band_rect(monitor, f32::NAN).height(), 1);
    }
}
