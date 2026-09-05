//! Windows overlay window configuration: non-activating, topmost, click-through.
//!
//! Uses extended window styles (WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_EX_TOOLWINDOW,
//! WS_EX_TRANSPARENT) to make the overlay float above other windows without
//! stealing focus. Per-pixel click-through uses SetWindowRgn from the sprite's
//! alpha mask. WDA_EXCLUDEFROMCAPTURE hides the overlay from screen captures.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, SetWindowRgn, HRGN, RGN_AND, RGN_OR,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, SetWindowDisplayAffinity, SetWindowLongW, SetWindowPos, GWL_EXSTYLE,
    HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WDA_EXCLUDEFROMCAPTURE,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

/// Float above other windows, non-activating, excluded from screen capture.
///
/// Returns Err when the window handle is not available yet, so the caller can
/// retry on subsequent frames once the window is realized.
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    let raw_window_handle = match window.window_handle() {
        Ok(handle) => handle,
        Err(e) => {
            return Err(format!("Window handle not available yet: {}", e));
        }
    };

    let hwnd = match raw_window_handle.as_raw() {
        RawWindowHandle::Win32(win32_window) => win32_window.hwnd.get() as HWND,
        _ => {
            return Err("Not a Windows window handle".to_string());
        }
    };

    set_window_styles(hwnd)?;
    set_window_topmost(hwnd)?;
    exclude_from_capture(hwnd)?;

    Ok(())
}

/// SetWindowRgn carves the click-through region from the sprite's alpha mask.
///
/// `None` makes the entire window click-through by applying WS_EX_TRANSPARENT.
/// `Some` creates a region from the opaque pixels and removes WS_EX_TRANSPARENT
/// so clicks hit the sprite and pass through everywhere else.
pub fn update_input_region(
    window: &tauri::WebviewWindow,
    mask_data: Option<&ai_buddy_core::overlay::AlphaMask>,
    sprite_x: i32,
    sprite_y: i32,
    sprite_facing: i32,
    scale: i32,
) -> Result<(), String> {
    let raw_window_handle = match window.window_handle() {
        Ok(handle) => handle,
        Err(e) => {
            return Err(format!("Window handle not available yet: {}", e));
        }
    };

    let hwnd = match raw_window_handle.as_raw() {
        RawWindowHandle::Win32(win32_window) => win32_window.hwnd.get() as HWND,
        _ => {
            return Err("Not a Windows window handle".to_string());
        }
    };

    if let Some(mask) = mask_data {
        apply_input_mask(hwnd, mask, sprite_x, sprite_y, sprite_facing, scale)?;
    } else {
        clear_input_region(hwnd)?;
    }

    Ok(())
}

/// Apply the extended window styles: non-activating, topmost, toolwindow, transparent.
fn set_window_styles(hwnd: HWND) -> Result<(), String> {
    unsafe {
        let current_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_style = current_style
            | (WS_EX_NOACTIVATE as i32)
            | (WS_EX_TOPMOST as i32)
            | (WS_EX_TOOLWINDOW as i32)
            | (WS_EX_TRANSPARENT as i32)
            | (WS_EX_LAYERED as i32);

        if SetWindowLongW(hwnd, GWL_EXSTYLE, new_style) == 0 && current_style != new_style {
            return Err("Failed to set extended window styles".to_string());
        }

        if SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
        ) == 0
        {
            return Err("Failed to apply topmost flag".to_string());
        }
    }

    Ok(())
}

/// Make the window topmost without changing its size or position.
fn set_window_topmost(hwnd: HWND) -> Result<(), String> {
    unsafe {
        if SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER)
            == 0
        {
            return Err("Failed to set window topmost".to_string());
        }
    }
    Ok(())
}

/// Exclude the overlay from screen capture.
///
/// WDA_EXCLUDEFROMCAPTURE makes the window invisible to screen recording and
/// screen sharing, matching macOS's NSWindowSharingType::None. This is DESIGN.md
/// decision 8's screen-share rule.
fn exclude_from_capture(hwnd: HWND) -> Result<(), String> {
    unsafe {
        if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
            return Err("Failed to exclude window from capture".to_string());
        }
    }
    Ok(())
}

/// Apply the alpha mask as the input region using SetWindowRgn.
///
/// Creates a region from the opaque pixels in the sprite's alpha mask, applying
/// scale and facing. The region is positioned at sprite_x, sprite_y in window
/// coordinates. Facing < 0 mirrors the mask horizontally.
fn apply_input_mask(
    hwnd: HWND,
    mask: &ai_buddy_core::overlay::AlphaMask,
    sprite_x: i32,
    sprite_y: i32,
    sprite_facing: i32,
    scale: i32,
) -> Result<(), String> {
    let (width, height, opaque) = mask.raw();

    unsafe {
        let mut combined_rgn: HRGN = std::ptr::null_mut();

        let mirror = sprite_facing < 0;
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if opaque[idx] {
                    let draw_x = if mirror {
                        (width - 1 - x) * scale
                    } else {
                        x * scale
                    };
                    let scaled_y = y * scale;

                    let rect_rgn = CreateRectRgn(
                        sprite_x + draw_x,
                        sprite_y + scaled_y,
                        sprite_x + draw_x + scale,
                        sprite_y + scaled_y + scale,
                    );

                    if rect_rgn.is_null() {
                        if !combined_rgn.is_null() {
                            DeleteObject(combined_rgn);
                        }
                        return Err("Failed to create region rectangle".to_string());
                    }

                    if combined_rgn.is_null() {
                        combined_rgn = rect_rgn;
                    } else {
                        let temp_rgn = CreateRectRgn(0, 0, 0, 0);
                        if temp_rgn.is_null() {
                            DeleteObject(combined_rgn);
                            DeleteObject(rect_rgn);
                            return Err("Failed to create temp region".to_string());
                        }

                        if windows_sys::Win32::Graphics::Gdi::CombineRgn(
                            temp_rgn,
                            combined_rgn,
                            rect_rgn,
                            RGN_OR,
                        ) == 0
                        {
                            DeleteObject(combined_rgn);
                            DeleteObject(rect_rgn);
                            DeleteObject(temp_rgn);
                            return Err("Failed to combine regions".to_string());
                        }

                        DeleteObject(combined_rgn);
                        DeleteObject(rect_rgn);
                        combined_rgn = temp_rgn;
                    }
                }
            }
        }

        if combined_rgn.is_null() {
            return clear_input_region(hwnd);
        }

        let current_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_style = current_style & !(WS_EX_TRANSPARENT as i32);
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_style);

        if SetWindowRgn(hwnd, combined_rgn, 1) == 0 {
            DeleteObject(combined_rgn);
            return Err("Failed to set window region".to_string());
        }
    }

    Ok(())
}

/// Clear the input region, making the entire window click-through.
///
/// Applies WS_EX_TRANSPARENT so all clicks pass through, then removes any
/// existing window region.
fn clear_input_region(hwnd: HWND) -> Result<(), String> {
    unsafe {
        let current_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_style = current_style | (WS_EX_TRANSPARENT as i32);
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_style);

        SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
    }

    Ok(())
}
