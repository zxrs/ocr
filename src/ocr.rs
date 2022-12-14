use anyhow::{ensure, Result};
use std::ptr;
use std::slice;
use windows::{
    core::Interface,
    Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Win32::System::WinRT::IMemoryBufferByteAccess,
};

pub fn scan(width: i32, height: i32, bytes_per_pixel: usize, bgr: Vec<u8>) -> Result<Vec<u16>> {
    ensure!(bgr.len() > bytes_per_pixel, "no data");

    let bmp = SoftwareBitmap::Create(BitmapPixelFormat::Bgra8, width, height)?;
    {
        let bmp_buf = bmp.LockBuffer(BitmapBufferAccessMode::Write)?;
        let array: IMemoryBufferByteAccess = bmp_buf.CreateReference()?.cast()?;

        let mut data = ptr::null_mut();
        let mut capacity = 0;
        unsafe {
            array.GetBuffer(&mut data, &mut capacity)?;
        }
        assert_eq!((width * height * 4).abs(), capacity as i32);

        let slice = unsafe { slice::from_raw_parts_mut(data, capacity as usize) };
        slice.chunks_mut(4).enumerate().for_each(|(i, c)| {
            c[0] = bgr[bytes_per_pixel * i];
            c[1] = bgr[bytes_per_pixel * i + 1];
            c[2] = bgr[bytes_per_pixel * i + 2];
            c[3] = 0;
        });
    }

    let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
    let result = engine
        .RecognizeAsync(&bmp)?
        .get()?
        .Lines()?
        .First()?
        .filter_map(|line| {
            // remove unnecessary whitespace in japanese text.
            // eg. あ い abc d ef え お => あい abc d ef えお
            Some(
                line.Text()
                    .ok()?
                    .to_string_lossy()
                    .split_ascii_whitespace()
                    .map(|s| {
                        if s.chars().all(|c| c.is_ascii()) {
                            format!(" {s} ")
                        } else {
                            s.to_owned()
                        }
                    })
                    .collect::<String>()
                    .trim()
                    .chars()
                    .chain(Some('\r'))
                    .chain(Some('\n'))
                    .collect::<String>(),
            )
        })
        .collect::<String>()
        .replace("  ", " ")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    Ok(result)
}
