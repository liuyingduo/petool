#[cfg(target_os = "windows")]
use chrono::Utc;
#[cfg(target_os = "windows")]
use core::ffi::c_void;
#[cfg(target_os = "windows")]
use image::{ImageBuffer, Rgba};
#[cfg(target_os = "windows")]
use std::fs;
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    SRCCOPY,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, IsWindow, SM_CXSCREEN, SM_CYSCREEN,
};

#[cfg(target_os = "windows")]
fn hwnd_from_i64(hwnd_value: i64) -> HWND {
    HWND(hwnd_value as isize as *mut c_void)
}

#[cfg(target_os = "windows")]
fn rect_for_capture(target_hwnd: Option<i64>) -> Result<RECT, String> {
    if let Some(hwnd_value) = target_hwnd {
        let hwnd = hwnd_from_i64(hwnd_value);
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return Err(format!("Window not found: {}", hwnd_value));
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }
            .map_err(|_| "Failed to get window rect".to_string())?;
        Ok(rect)
    } else {
        let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        Ok(RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        })
    }
}

#[cfg(target_os = "windows")]
fn capture_rgba(target_hwnd: Option<i64>) -> Result<(u32, u32, Vec<u8>), String> {
    let rect = rect_for_capture(target_hwnd)?;
    let width = (rect.right - rect.left).max(1);
    let height = (rect.bottom - rect.top).max(1);
    let null_hwnd = HWND(std::ptr::null_mut());

    let hdc_screen = unsafe { GetDC(null_hwnd) };
    if hdc_screen.0.is_null() {
        return Err("Failed to acquire screen DC".to_string());
    }

    let hdc_mem = unsafe { CreateCompatibleDC(hdc_screen) };
    if hdc_mem.0.is_null() {
        unsafe {
            ReleaseDC(null_hwnd, hdc_screen);
        }
        return Err("Failed to create compatible DC".to_string());
    }

    let hbitmap = unsafe { CreateCompatibleBitmap(hdc_screen, width, height) };
    if hbitmap.0.is_null() {
        unsafe {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(null_hwnd, hdc_screen);
        }
        return Err("Failed to create compatible bitmap".to_string());
    }

    let old_obj = unsafe { SelectObject(hdc_mem, HGDIOBJ(hbitmap.0)) };
    let bitblt_ok = unsafe {
        BitBlt(
            hdc_mem, 0, 0, width, height, hdc_screen, rect.left, rect.top, SRCCOPY,
        )
    }
    .is_ok();

    if !bitblt_ok {
        unsafe {
            SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(HGDIOBJ(hbitmap.0));
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(null_hwnd, hdc_screen);
        }
        return Err("Failed to capture pixels with BitBlt".to_string());
    }

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut bgra = vec![0u8; (width as usize) * (height as usize) * 4];
    let copied = unsafe {
        GetDIBits(
            hdc_mem,
            hbitmap,
            0,
            height as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };

    unsafe {
        SelectObject(hdc_mem, old_obj);
        let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(null_hwnd, hdc_screen);
    }

    if copied == 0 {
        return Err("GetDIBits returned no data".to_string());
    }

    let mut rgba = vec![0u8; bgra.len()];
    for i in (0..bgra.len()).step_by(4) {
        rgba[i] = bgra[i + 2];
        rgba[i + 1] = bgra[i + 1];
        rgba[i + 2] = bgra[i];
        rgba[i + 3] = 255;
    }

    Ok((width as u32, height as u32, rgba))
}

#[cfg(target_os = "windows")]
fn cleanup_old_screenshots(dir: &Path, keep_count: usize) -> Result<(), String> {
    let mut files = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if files.len() <= keep_count {
        return Ok(());
    }

    files.sort_by_key(|entry| {
        entry
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    let remove_count = files.len().saturating_sub(keep_count);
    for entry in files.into_iter().take(remove_count) {
        let _ = fs::remove_file(entry.path());
    }

    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn capture_to_png(
    screenshot_dir: &Path,
    target_hwnd: Option<i64>,
    keep_count: usize,
) -> Result<PathBuf, String> {
    fs::create_dir_all(screenshot_dir).map_err(|e| e.to_string())?;

    let (width, height, rgba) = capture_rgba(target_hwnd)?;
    let image: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| "Failed to construct image buffer".to_string())?;

    let suffix = if let Some(hwnd) = target_hwnd {
        format!("window-{}", hwnd)
    } else {
        "desktop".to_string()
    };

    let filename = format!(
        "desktop-shot-{}-{}.png",
        Utc::now().format("%Y%m%d-%H%M%S-%3f"),
        suffix
    );
    let output_path = screenshot_dir.join(filename);
    image.save(&output_path).map_err(|e| e.to_string())?;

    let keep = keep_count.max(20);
    let _ = cleanup_old_screenshots(screenshot_dir, keep);

    Ok(output_path)
}
