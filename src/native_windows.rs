#[cfg(target_os = "windows")]
mod implementation {
    use std::ffi::c_void;
    use std::ptr;

    use eframe::{CreationContext, Frame};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const HWND_TOPMOST: isize = -1;
    const IMAGE_ICON: u32 = 1;
    const LR_SHARED: u32 = 0x8000;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const SM_CXICON: i32 = 11;
    const SM_CYICON: i32 = 12;
    const SM_CXSMICON: i32 = 49;
    const SM_CYSMICON: i32 = 50;

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

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> *mut c_void;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MonitorFromWindow(window: *mut c_void, flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(monitor: *mut c_void, monitor_info: *mut MonitorInfo) -> i32;
        fn GetWindowRect(window: *mut c_void, rect: *mut Rect) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn LoadImageW(
            instance: *mut c_void,
            name: *const u16,
            image_type: u32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> *mut c_void;
        fn SendMessageW(
            window: *mut c_void,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
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

    pub fn apply_window_icon(creation_context: &CreationContext<'_>) {
        let Ok(handle) = creation_context.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as *mut c_void;
        // Resource ID 1 matches the default icon embedded by winresource in build.rs.
        const APP_ICON_RESOURCE: *const u16 = 1usize as *const u16;

        // SAFETY: hwnd is a valid live Win32 window handle and APP_ICON_RESOURCE is an integer resource ID.
        unsafe {
            let instance = GetModuleHandleW(ptr::null());
            if instance.is_null() {
                return;
            }
            for (icon_slot, metric_w, metric_h) in [
                (ICON_BIG, SM_CXICON, SM_CYICON),
                (ICON_SMALL, SM_CXSMICON, SM_CYSMICON),
            ] {
                let width = GetSystemMetrics(metric_w);
                let height = GetSystemMetrics(metric_h);
                let icon = LoadImageW(
                    instance,
                    APP_ICON_RESOURCE,
                    IMAGE_ICON,
                    width,
                    height,
                    LR_SHARED,
                );
                if !icon.is_null() {
                    SendMessageW(hwnd, WM_SETICON, icon_slot, icon as isize);
                }
            }
        }
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
pub use implementation::{apply_window_icon, position_top_right};

#[cfg(not(target_os = "windows"))]
pub fn apply_window_icon(_creation_context: &eframe::CreationContext<'_>) {}

#[cfg(not(target_os = "windows"))]
pub fn position_top_right(_frame: &eframe::Frame) {}
