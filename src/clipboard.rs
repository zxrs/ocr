use anyhow::{anyhow, ensure, Result};
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

struct BitIterator<'a> {
    slice: &'a [u8],
    index: usize,
}

impl<'a> BitIterator<'a> {
    fn new(slice: &'a [u8]) -> Self {
        Self { slice, index: 0 }
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        let byte_index = self.index / 8;
        let bit_index = self.index % 8;

        let bits = self.slice.get(byte_index)?;
        let bit = (bits << bit_index) >> 7;

        self.index += 1;

        Some(bit)
    }
}

#[derive(Debug, Default)]
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

    fn scan_line_bytes_count_with_padding(&self) -> usize {
        (self.width as usize * self.bits_per_pixel as usize + 31) / 32 * 4
    }

    fn to_bgra(&self) -> Result<Vec<u8>> {
        let iter = self
            .data
            .chunks(self.scan_line_bytes_count_with_padding())
            .rev();
        let result = match self.bits_per_pixel {
            32 => iter.flatten().cloned().collect(),
            24 => iter
                .flat_map(|s| {
                    s[0..self.width as usize * 3]
                        .chunks(3)
                        .flat_map(|p| [p[0], p[1], p[2], 255])
                })
                .collect(),
            1 => iter
                .flat_map(|s| {
                    BitIterator::new(s).take(self.width as usize).flat_map(|n| {
                        if n > 0 {
                            [255; 4]
                        } else {
                            [0; 4]
                        }
                    })
                })
                .collect(),
            _ => {
                return Err(anyhow!(
                    "{} bits per pixel image is not yet supported.",
                    self.bits_per_pixel
                ));
            }
        };
        Ok(result)
    }
}

pub fn get() -> Result<(i32, i32, Vec<u8>)> {
    ensure!(is_bitmap_on_clipboard_data(), "not bitmap data");
    let dib = read_bitmap_from_clipboard()?;
    Ok((dib.width(), dib.height(), dib.to_bgra()?))
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
    let size = bitmap.bmiHeader.biSizeImage as usize;
    ensure!(size > 0, "no data.");

    let bits_per_pixel = bitmap.bmiHeader.biBitCount;
    ensure!(bitmap.bmiHeader.biHeight > 0, "not yet supported!");

    let data = unsafe { slice::from_raw_parts(bitmap.bmiColors.as_ptr() as *mut u8, size) };

    Ok(Dib {
        width: bitmap.bmiHeader.biWidth,
        height: bitmap.bmiHeader.biHeight,
        bits_per_pixel,
        data: data.to_owned(),
    })
}

#[test]
fn scan_line_bytes_count_with_padding_test() {
    let dib = Dib {
        width: 9,
        bits_per_pixel: 1,
        ..Default::default()
    };
    assert_eq!(dib.scan_line_bytes_count_with_padding(), 4);

    let dib = Dib {
        width: 53,
        bits_per_pixel: 24,
        ..Default::default()
    };
    assert_eq!(dib.scan_line_bytes_count_with_padding(), 160);

    let dib = Dib {
        width: 53,
        bits_per_pixel: 32,
        ..Default::default()
    };
    assert_eq!(dib.scan_line_bytes_count_with_padding(), 212);
}

#[test]
fn bit_iterator_test() {
    let s: [u8; 4] = [0b1001_1110, 0b1100_1100, 0, 0];
    let mut iter = BitIterator::new(&s).take(10);
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(0));
    assert_eq!(iter.next(), Some(0));
    assert_eq!(iter.next(), Some(1));

    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(0));

    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), None);
}
