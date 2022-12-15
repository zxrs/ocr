use anyhow::{ensure, Result};
use std::io::{Cursor, Write};
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

    let mut buf = [0u8; 4096];
    let mut cur = Cursor::new(buf.as_mut_slice());
    engine
        .RecognizeAsync(&bmp)?
        .get()?
        .Lines()?
        .First()?
        .try_for_each(|line| -> Result<()> {
            line.Text()?
                .as_wide()
                .split(|num| num == &32)
                .try_for_each(|data| -> Result<()> {
                    let data = unsafe {
                        slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2)
                    };
                    // if ascii text, insert a space.
                    if data.chunks(2).all(|n| n[0] < 0x80 && n[1] == 0) {
                        let pos = cur.position() as usize;
                        let r = cur.get_ref();
                        // if the previous 4 bytes are "\r\n", do not insert a space.
                        if pos > 3 && (*r)[pos - 4..pos] != [0x0d, 0x00, 0x0a, 0x00] {
                            let _ = cur.write(&[32, 0])?;
                        }
                        let _ = cur.write(data)?;
                    } else {
                        let _ = cur.write(data)?;
                    }
                    Ok(())
                })?;
            // add "\r\n"
            let _ = cur.write(&[0x0d, 0x00, 0x0a, 0x00])?;
            Ok(())
        })?;
    // null termination.
    let _ = cur.write(&[0, 0])?;

    let len = cur.position() as usize / 2;
    let result = unsafe { slice::from_raw_parts(&buf[..] as *const [u8] as *const u16, len) };
    Ok(result.to_owned())
}
