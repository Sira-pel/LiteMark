//! Window moving and maximizing for the custom title bar on Windows.
//!
//! gpui's Windows backend has no `start_window_move`, and its caption
//! (`HTCAPTION`) path skips the native move whenever any handler (including
//! gpui's own focus-on-click on the root element) marks the press as handled.
//! So the title bar asks Windows for the move directly, the way custom title
//! bars do: release the mouse capture and post a caption press.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;

const WM_SYSCOMMAND: u32 = 0x0112;
const SM_CXDRAG: i32 = 68;
const SM_CYDRAG: i32 = 69;
/// `SC_MOVE` with the low bits set to `HTCAPTION`: a mouse-driven move.
const SC_MOVE_FROM_CAPTION: usize = 0xF010 | 2;
const SW_MAXIMIZE: i32 = 3;
const SW_RESTORE: i32 = 9;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[link(name = "user32")]
extern "system" {
    fn ReleaseCapture() -> i32;
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn IsZoomed(hwnd: isize) -> i32;
    fn ShowWindowAsync(hwnd: isize, command: i32) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
}

thread_local! {
    /// Screen position of the title-bar press that armed the drag.
    static PRESS: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
}

fn cursor_pos() -> Option<(i32, i32)> {
    let mut cursor = Point { x: 0, y: 0 };
    // SAFETY: a local out parameter.
    (unsafe { GetCursorPos(&mut cursor) } != 0).then_some((cursor.x, cursor.y))
}

/// Remembers where the press that arms the drag happened.
pub fn arm_drag() {
    PRESS.with(|press| press.set(cursor_pos()));
}

/// True once the pointer has left the system drag rectangle around the
/// press, like native title bars. Without this, a pixel of jitter during the
/// first click of a double-click starts the OS move loop, which then swallows
/// the second click.
pub fn drag_past_threshold() -> bool {
    let (Some(press), Some(now)) = (PRESS.with(Cell::get), cursor_pos()) else {
        return true;
    };
    // SAFETY: plain metric queries.
    let size = unsafe { (GetSystemMetrics(SM_CXDRAG), GetSystemMetrics(SM_CYDRAG)) };
    past_threshold(press, now, size)
}

/// The drag rectangle is `size` wide and tall, centered on the press.
fn past_threshold(press: (i32, i32), now: (i32, i32), size: (i32, i32)) -> bool {
    (now.0 - press.0).abs() > size.0 / 2 || (now.1 - press.1).abs() > size.1 / 2
}

fn hwnd(window: &Window) -> Option<isize> {
    // `Window` also has an inherent `window_handle`, so name the trait.
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// Starts the OS window move for the press that is happening now.
///
/// This is `WM_SYSCOMMAND`, not a posted `WM_NCLBUTTONDOWN`: gpui would hand
/// that to the app as a second click at the same spot, which the title bar
/// reads as a double-click.
pub fn start_window_move(window: &Window) {
    let Some(hwnd) = hwnd(window) else { return };
    let mut cursor = Point { x: 0, y: 0 };
    // SAFETY: plain user32 calls on this window's own handle and a local out
    // parameter. The message is posted, not sent, so the move loop runs after
    // gpui has finished handling this press instead of re-entering it.
    unsafe {
        if GetCursorPos(&mut cursor) == 0 {
            return;
        }
        ReleaseCapture();
        let lparam = (((cursor.y as i16 as u16 as u32) << 16) | (cursor.x as i16 as u16 as u32))
            as i32 as isize;
        PostMessageW(hwnd, WM_SYSCOMMAND, SC_MOVE_FROM_CAPTION, lparam);
    }
}

/// Maximizes the window, or restores it when it is already maximized.
/// (`Window::zoom_window` only ever maximizes on Windows.)
pub fn toggle_maximize(window: &Window) {
    let Some(hwnd) = hwnd(window) else { return };
    // SAFETY: as in `start_window_move`.
    unsafe {
        let command = if IsZoomed(hwnd) != 0 { SW_RESTORE } else { SW_MAXIMIZE };
        ShowWindowAsync(hwnd, command);
    }
}

#[cfg(test)]
mod tests {
    use super::past_threshold;

    #[test]
    fn drag_waits_for_the_system_threshold() {
        // Default SM_CXDRAG / SM_CYDRAG is 4: two pixels either way is jitter.
        assert!(!past_threshold((100, 100), (100, 100), (4, 4)));
        assert!(!past_threshold((100, 100), (102, 98), (4, 4)));
        assert!(past_threshold((100, 100), (103, 100), (4, 4)));
        assert!(past_threshold((100, 100), (100, 97), (4, 4)));
    }
}
