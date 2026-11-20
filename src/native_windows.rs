#[cfg(target_os = "windows")]
mod implementation {
    use std::ffi::c_void;

    use eframe::Frame;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const HWND_TOPMOST: isize = -1;

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct MonitorInfo {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MonitorFromWindow(window: *mut c_void, flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(monitor: *mut c_void, monitor_info: *mut MonitorInfo) -> i32;
        fn GetWindowRect(window: *mut c_void, rect: *mut Rect) -> i32;
        fn SetWindowPos(
            window: *mut c_void,
            insert_after: *mut c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }

    pub fn position_top_right(frame: &Frame) {
        let Ok(handle) = frame.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as *mut c_void;

        // SAFETY: all pointers refer to initialized structs and the HWND is live for this frame.
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return;
            }

            let mut monitor_info = MonitorInfo {
                size: std::mem::size_of::<MonitorInfo>() as u32,
                ..Default::default()
            };
            let mut window_rect = Rect::default();
            if GetMonitorInfoW(monitor, &mut monitor_info) == 0
                || GetWindowRect(hwnd, &mut window_rect) == 0
            {
                return;
            }

            let width = window_rect.right - window_rect.left;
            let x = monitor_info.work.right - width - 16;
            let y = monitor_info.work.top + 16;
            SetWindowPos(
                hwnd,
                HWND_TOPMOST as *mut c_void,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
    }
}

#[cfg(target_os = "windows")]
pub use implementation::position_top_right;

#[cfg(not(target_os = "windows"))]
pub fn position_top_right(_frame: &eframe::Frame) {}
