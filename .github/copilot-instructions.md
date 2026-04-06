# Copilot Instructions for OCR Project

## Build & Test Commands

### Building
- **Debug build**: `cargo build` (with minimal debug info per profile settings)
- **Release build**: `cargo build --release` (optimized with LTO and single codegen unit)
- **Run**: `cargo run --release` (starts the OCR application)

### Testing
- **Run all tests**: `cargo test`
- **Run single test**: `cargo test scan_line_bytes_count_with_padding_test -- --exact`
- **Run with output**: `cargo test -- --nocapture`

Tests are located in `src/clipboard.rs` and cover bitmap padding calculation and bit iteration logic.

## High-Level Architecture

This is a Windows-only OCR (Optical Character Recognition) monitoring application that:

1. **Clipboard Monitoring** (`main.rs` - `wnd_proc`): 
   - Registers a Win32 window to listen for clipboard change notifications (`WM_CLIPBOARDUPDATE`)
   - Filters updates from specific applications (Adobe PDF, Firefox, Cut & Sketch, Snipping Tool, IrfanView)

2. **Image Extraction** (`clipboard.rs`):
   - Reads bitmap data from Windows clipboard (DIB format)
   - Converts various bitmap formats (1-bit, 24-bit, 32-bit) to BGRA format
   - Handles bitmap padding alignment (32-bit boundaries per scanline)

3. **OCR Processing** (`ocr.rs`):
   - Uses Windows Media OCR Engine to recognize text in the extracted image
   - Supports multiple language packs selected via combobox UI
   - Processes output line-by-line with intelligent spacing for ASCII text

4. **UI Components** (`main.rs`):
   - Rich text edit control: displays OCR results
   - Language combobox: selects OCR language from available system language packs
   - Right-click context menu: copy OCR result to clipboard

5. **Text Pipeline**:
   - OCR result → formatted with proper spacing → stored in buffer → copied to clipboard
   - Result also inserted at end of rich text control with scroll to bottom

## Key Conventions

### Windows Interop Patterns
- Uses `windows` crate v0.62 with specific feature set for graphics, OCR, and UI
- All Win32 FFI calls wrapped with `unsafe` blocks (required for raw window handles)
- `HWND` handles stored in `OnceLock<Hwnd>` statics for thread-safe global access
- `Hwnd` wrapper struct implements `Send + Sync` to work with static storage

### Memory & Buffer Management
- Fixed buffer size: `BUF_SIZE = 8192` bytes for OCR output (UTF-16LE format)
- UTF-16 null-terminated strings used throughout (matching Windows conventions)
- Bitmap pixel format: BGRA (Blue-Green-Red-Alpha), 4 bytes per pixel
- Custom `Drop` impls for `Clipboard`, `Handle`, and `MemoryHandle` ensure resource cleanup

### Text Encoding
- All text is UTF-16LE (wide characters, 2 bytes per character)
- Whitespace handling in OCR: inserts spaces around ASCII words, preserves non-ASCII formatting
- Buffer position tracked with `Cursor<&mut [u8]>` for safe writes

### Language Pack Support
- `DISPLAY_NAMES` maps display names to language tags (cached in `OnceLock`)
- Only language packs installed on system appear in combobox
- Current language selected by default via `OcrEngine::TryCreateFromUserProfileLanguages()`

### Single Instance Check
- Application prevents multiple instances by enumerating windows for matching window title
- Existing instance brought to foreground if already running
- Window title includes crate name and version via macros

### Error Handling
- Uses `anyhow::Result` throughout for context-based error messages
- Includes `c!()` macro (defined in main.rs) that formats location info for debugging
- Application continues on non-critical errors (e.g., missing language packs)

## Windows-Only Target

This project exclusively targets **Windows 10/11** and requires:
- Windows Media OCR Engine (included in Windows 10+)
- OCR language packs installed via Settings (e.g., `Language.OCR*en-US*`)
- Recent Rust toolchain with Windows support

The `windows_subsystem = "windows"` attribute removes console window on startup.
