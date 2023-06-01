use super::{BUF_SIZE, DISPLAY_NAMES, ID_COMBO};
use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use std::ptr;
use std::slice;
use windows::{
    core::{ComInterface, HSTRING},
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
        let hctrl = GetDlgItem(hwnd, ID_COMBO);
        let index =
            SendMessageW(hctrl, CB_GETCURSEL, WPARAM::default(), LPARAM::default()).0 as usize;
        //dbg!(index);
        let len = SendMessageW(hctrl, CB_GETLBTEXTLEN, WPARAM(index), LPARAM::default()).0 as usize;
        //dbg!(len);
        let mut buf = vec![0u16; len + 1];
        SendMessageW(
            hctrl,
            CB_GETLBTEXT,
            WPARAM(index),
            LPARAM(buf.as_mut_ptr() as isize),
        );
        buf
    };

    let lang_tag = DISPLAY_NAMES
        .get()
        .context("no display names.")?
        .get(&display_name)
        .context("no lang tag.")?;

    let lang = Language::CreateLanguage(&HSTRING::from_wide(&lang_tag[..lang_tag.len() - 1])?)?;

    let engine = OcrEngine::TryCreateFromLanguage(&lang)?;
    let mut cur = Cursor::new(buf);
    engine
        .RecognizeAsync(&bmp)?
        .get()?
        .Lines()?
        .First()?
        .try_for_each(|line| -> Result<()> {
            line.Text()?
                .as_wide()
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
                            let _ = cur.write(&[0x20, 0x00])?;
                        }
                        let _ = cur.write(data)?;
                        let _ = cur.write(&[0x20, 0x00])?;
                    } else {
                        let _ = cur.write(data)?;
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
            let _ = cur.write(&[0x0d, 0x00, 0x0a, 0x00])?;
            Ok(())
        })?;
    // null termination.
    let _ = cur.write(&[0, 0])?;

    // the last 2 bytes of buffer should be null terminatation.
    if cur.position() as usize > BUF_SIZE - 2 {
        cur.set_position((BUF_SIZE - 2) as u64);
        let _ = cur.write(&[0, 0])?;
    }

    Ok(cur.position() as usize)
}
