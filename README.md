# ocr

A tiny [OcrEngine](https://learn.microsoft.com/ja-jp/uwp/api/windows.media.ocr.ocrengine?view=winrt-22621) application written in Rust. This application monitors the Windows clipboard, performs OCR when any image is copied to the clipboard, and copies the result as a string to the clipboard.

![ocr](https://user-images.githubusercontent.com/60449021/208563347-15c88f52-f07f-4921-8a31-7d1386244702.png)

## How to install an OCR language pack

The following commands on PowerShell install the OCR pack for "en-US":

```
$Capability = Get-WindowsCapability -Online | Where-Object { $_.Name -Like 'Language.OCR*en-US*' }
```

```
$Capability | Add-WindowsCapability -Online
```

See more details [here](https://learn.microsoft.com/en-us/windows/powertoys/text-extractor#how-to-install-an-ocr-language-pack).
