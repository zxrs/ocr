use anyhow::{ensure, Result};
use std::ptr;
use std::slice;
use windows::Win32::{
    Foundation::HANDLE,
    Graphics::Gdi::BITMAPINFO,
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalFree, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        SystemServices::{CF_DIB, CF_UNICODETEXT},
    },
};

struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

#[derive(Debug)]
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { GlobalUnlock(self.0 .0) };
    }
}

#[derive(Debug)]
struct MemoryHandle(isize);
impl Drop for MemoryHandle {
    fn drop(&mut self) {
        unsafe { GlobalFree(self.0) };
    }
}

struct Dib {
    width: i32,
    height: i32,
    bits_per_pixel: u16,
    data: Vec<u8>,
}

impl Dib {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn bytes_per_pixel(&self) -> usize {
        (self.bits_per_pixel / 8) as usize
    }

    fn scan_line_bytes_count(&self) -> usize {
        self.width as usize * self.bytes_per_pixel()
    }

    fn scan_line_bytes_count_with_padding(&self) -> usize {
        if self.bytes_per_pixel() == 4 {
            return self.scan_line_bytes_count();
        }
        (self.width as usize * self.bits_per_pixel as usize + 31) / 32 * 4
    }

    fn to_bgr(&self) -> Vec<u8> {
        self.data
            .chunks(self.scan_line_bytes_count_with_padding())
            .rev()
            .flat_map(|c| &c[0..self.scan_line_bytes_count()])
            .cloned()
            .collect()
    }
}

pub fn get() -> Result<(i32, i32, usize, Vec<u8>)> {
    ensure!(is_bitmap_on_clipboard_data(), "not bitmap data");

    let dib = read_bitmap_from_clipboard()?;

    let width = dib.width();
    let height = dib.height();
    let bytes_per_pixel = dib.bytes_per_pixel();
    let bgr = dib.to_bgr();

    Ok((width, height, bytes_per_pixel, bgr))
}

pub fn set(src: &[u16]) -> Result<()> {
    unsafe { OpenClipboard(None).ok()? };
    let _clip = Clipboard;

    unsafe { EmptyClipboard().ok()? };

    let h_mem = unsafe { GlobalAlloc(GMEM_MOVEABLE, src.len() * 2) };
    ensure!(h_mem != 0, "failed to global alloc.");
    let h_mem = MemoryHandle(h_mem);

    let dst = unsafe { GlobalLock(h_mem.0) } as *mut u8;
    ensure!(!dst.is_null(), "failed to global lock.");

    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr() as *const u8, dst, src.len() * 2);
        GlobalUnlock(h_mem.0);
        SetClipboardData(CF_UNICODETEXT.0, HANDLE(h_mem.0))?;
    }
    Ok(())
}

fn is_bitmap_on_clipboard_data() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_DIB.0).as_bool() }
}

fn read_bitmap_from_clipboard() -> Result<Dib> {
    unsafe { OpenClipboard(None).ok()? };
    let _clip = Clipboard;

    let handle = unsafe { GetClipboardData(CF_DIB.0)? };
    let bitmap = unsafe { GlobalLock(handle.0) };
    ensure!(!bitmap.is_null(), "failed to global lock.");
    let _handle = Handle(handle);

    let bitmap = unsafe { &mut *(bitmap as *mut BITMAPINFO) };
    ensure!(bitmap.bmiHeader.biHeight > 0, "not yet supported!");

    let data = unsafe {
        slice::from_raw_parts(
            bitmap.bmiColors.as_ptr() as *mut u8,
            bitmap.bmiHeader.biSizeImage as usize,
        )
    };

    Ok(Dib {
        width: bitmap.bmiHeader.biWidth,
        height: bitmap.bmiHeader.biHeight,
        bits_per_pixel: bitmap.bmiHeader.biBitCount,
        data: data.to_owned(),
    })
}
