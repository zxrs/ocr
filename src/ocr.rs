use anyhow::Result;
use std::ptr;
use std::slice;
use windows::{
    core::Interface,
    Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Win32::System::WinRT::IMemoryBufferByteAccess,
};

pub fn scan(width: i32, height: i32, bytes_per_pixel: usize, bgr: Vec<u8>) -> Result<Vec<u16>> {
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
            Some(
                line.Text()
                    .ok()?
                    .as_wide()
                    .iter()
                    .chain(Some(&0x000d))
                    .chain(Some(&0x000a))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .chain(Some(0))
        .collect();
    Ok(result)
}
