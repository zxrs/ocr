use super::{BUF_SIZE, DISPLAY_NAMES, ID_COMBO};
use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use std::ptr;
use std::slice;
use windows::{
    core::{Interface, HSTRING},
    Globalization::Language,
    Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::WinRT::IMemoryBufferByteAccess,
        UI::WindowsAndMessaging::{
            GetDlgItem, SendMessageW, CB_GETCURSEL, CB_GETLBTEXT, CB_GETLBTEXTLEN,
        },
    },
};

pub fn scan(hwnd: HWND, width: i32, height: i32, bgra: Vec<u8>, buf: &mut [u8]) -> Result<usize> {
    let bmp = SoftwareBitmap::Create(BitmapPixelFormat::Bgra8, width, height)?;
    {
        let bmp_buf = bmp.LockBuffer(BitmapBufferAccessMode::Write)?;
        let array: IMemoryBufferByteAccess = bmp_buf.CreateReference()?.cast()?;

        let mut data = ptr::null_mut();
        let mut capacity = 0;
        unsafe { array.GetBuffer(&mut data, &mut capacity)? };

        assert_eq!((width * height * 4).abs(), capacity as i32);

        let slice = unsafe { slice::from_raw_parts_mut(data, capacity as usize) };
        slice.clone_from_slice(&bgra);
    }

    //let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;

    let display_name = unsafe {
        let hctrl = GetDlgItem(Some(hwnd), ID_COMBO)?;
        let index = SendMessageW(
            hctrl,
            CB_GETCURSEL,
            Some(WPARAM::default()),
            Some(LPARAM::default()),
        )
        .0 as usize;
        //dbg!(index);
        let len = SendMessageW(
            hctrl,
            CB_GETLBTEXTLEN,
            Some(WPARAM(index)),
            Some(LPARAM::default()),
        )
        .0 as usize;
        //dbg!(len);
        let mut buf = vec![0u16; len + 1];
        SendMessageW(
            hctrl,
            CB_GETLBTEXT,
            Some(WPARAM(index)),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        );
        buf
    };

    let lang_tag = DISPLAY_NAMES
        .get()
        .context(c!())?
        .get(&display_name)
        .context(c!())?;

    let lang = Language::CreateLanguage(&HSTRING::from_wide(&lang_tag[..lang_tag.len() - 1]))?;

    let engine = OcrEngine::TryCreateFromLanguage(&lang)?;
    let mut cur = Cursor::new(buf);
    engine
        .RecognizeAsync(&bmp)?
        .get()?
        .Lines()?
        .First()?
        .try_for_each(|line| -> Result<()> {
            line.Text()?
                // split by whitespace
                .split(|num| num == &0x0020)
                .try_for_each(|data| -> Result<()> {
                    let data = unsafe {
                        slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2)
                    };
                    // if ascii text, insert a space.
                    if data.chunks(2).all(|n| n[0] < 0x80 && n[1] == 0) {
                        let pos = cur.position() as usize;
                        let r = cur.get_ref();
                        // if the previous 4 bytes are "\r\n" or 2bytes are " ", do not insert a space.
                        if pos > 3
                            && ((*r)[pos - 4..pos] != [0x0d, 0x00, 0x0a, 0x00]
                                && (*r)[pos - 2..pos] != [0x20, 0x00])
                        {
                            cur.write_all(&[0x20, 0x00])?;
                        }
                        cur.write_all(data)?;
                        cur.write_all(&[0x20, 0x00])?;
                    } else {
                        cur.write_all(data)?;
                    }
                    Ok(())
                })?;
            // if the last 2bytes are " ", remove it.
            let pos = cur.position() as usize;
            let r = cur.get_ref();
            if pos > 2 && (*r)[pos - 2..pos] == [0x20, 0x00] {
                cur.set_position(pos as u64 - 2);
            }
            // add "\r\n"
            cur.write_all(&[0x0d, 0x00, 0x0a, 0x00])?;
            Ok(())
        })?;
    // null termination.
    cur.write_all(&[0, 0])?;

    // the last 2 bytes of buffer should be null terminatation.
    if cur.position() as usize > BUF_SIZE - 2 {
        cur.set_position((BUF_SIZE - 2) as u64);
        cur.write_all(&[0, 0])?;
    }

    Ok(cur.position() as usize)
}
