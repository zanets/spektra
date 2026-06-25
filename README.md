# Spektra

A desktop audio spectrogram viewer built with Rust and [egui](https://github.com/emilk/egui).

![Spektra screenshot](assets/example.png)

**[中文說明 →](README.zh-TW.md)**

---

## Features

- **Drag & drop** or click **Open** to load audio files
- Supports MP3, WAV, FLAC, OGG, M4A, AAC, Opus, WMA, AIFF, AU
- **Linear / Logarithmic** frequency axis toggle
- **dB range sliders** — adjustable ceiling and floor with reset
- **Overlap control** — 0 %, 50 %, or 75 % FFT window overlap
- **Channel modes** — Mix, L, R, or L+R split view
- **Hover info** — time, frequency, nearest musical note, and dB at cursor
- **Export PNG** — 1400 × 800 spectrogram image (Cmd/Ctrl+S or Save PNG button)
- SoX-style color palette

---

## Installation

**macOS (Homebrew)**

```sh
brew tap zanets/tap
brew install spektra
```

**Other platforms** — pre-built binaries for macOS (Apple Silicon) and Windows (x64) are available on the [Releases](../../releases) page.

---

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- FFmpeg libraries

**macOS (Homebrew)**

```sh
brew install ffmpeg lame x264 x265 svt-av1 dav1d opus libvpx libvmaf
cargo build --release
```

**Windows (vcpkg)**

```powershell
vcpkg install ffmpeg:x64-windows-static
$env:FFMPEG_DIR = "$env:VCPKG_INSTALLATION_ROOT\installed\x64-windows-static"
cargo build --release
```

The binary is written to `target/release/spektra` (or `spektra.exe` on Windows).

---

## Usage

| Action | How |
|--------|-----|
| Open file | Drag & drop onto the window, or click **Open** |
| Save PNG  | Press **Cmd/Ctrl+S**, or click **Save PNG** |
| Toggle frequency scale | Click **Freq: Lin / Freq: Log** |
| Adjust dB range | Move the **Ceil** and **Floor** sliders |
| Reset dB range | Click the **↺** button |
| Change overlap | Select from the **OL** dropdown |
| Change channel | Select from the **Mix / L+R / L / R** dropdown |

Hover over the spectrogram to see a crosshair with time, frequency, musical note, and dB value.

---

## Technical Details

| Parameter | Value |
|-----------|-------|
| FFT size | 2048 points |
| Window | Hann |
| Frequency bands | 1025 |
| Time columns | 800 |
| Color palette | SoX |
| Export resolution | 1400 × 800 px |

Audio decoding is handled by [ffmpeg-next](https://crates.io/crates/ffmpeg-next). Mono files are upmixed to stereo internally.

---

## License

MIT
