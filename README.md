# RustPaint

A modern, Microsoft-Paint-style image editor written in Rust. One codebase builds a
native desktop app for **Windows**, **macOS** and **Linux**, producing a small,
self-contained executable with fast startup and low idle CPU usage.

![RustPaint — dark theme](docs/screenshot-dark.png)

![RustPaint — light theme](docs/screenshot-light.png)

*(Both screenshots are the real app running; `examples/landscape.png` was drawn entirely
inside RustPaint with the fill, ellipse, polygon, rectangle and text tools.)*

---

## Highlights

- **Paint 11-flavoured UI** — icon tool rail, ribbon-free command bar with real menus,
  floating cards and dialogs, rounded corners, light **and** dark themes.
- **13 tools** — Select, Pencil, Brush, Eraser, Airbrush, Fill, Colour picker, Text,
  Line, Rectangle, Ellipse, Polygon, Pan.
- **Text with real fonts** — every font installed on the machine is discovered in the
  background and listed in a searchable picker (plus two bundled faces that always work),
  with size, opacity, true bold weight and live preview on the canvas.
- **Drag & drop** — drop an image file anywhere on the window to open it (with the usual
  unsaved-changes prompt), or pass a file path on the command line.
- **System image clipboard** — copy, cut and paste RGBA images directly to and from
  other desktop applications via `arboard`, with an automatic in-app fallback on
  headless systems.
- **Remembers where you left off** — theme, canvas font, file-dialog preference, last
  folder and an "Open recent" list (8 files) are stored atomically in a tiny settings
  file and restored on the next launch.
- **Real selection workflow** — marquee select (`Shift` for square), floating move,
  cut / copy / paste, delete, select all, crop, nudge with the arrow keys (`Shift` for
  10 px). An active selection confines painting and bucket fills to itself; starting a
  stroke outside it drops the selection instead of trapping you.
- **Numeric control** — size, opacity, hardness, anti-alias toggle, shape fill toggle,
  fill tolerance, font size + bold, `#RGB` / `#RRGGBB` / `#RRGGBBAA` hex input and a
  hue-preserving HSV picker.
- **Pixel-accurate canvas** — nearest-neighbour zoom at ≥ 1:1 (crisp pixels, optional
  pixel grid), smooth linear zoom below 1:1, zoom to cursor (including trackpad pinch),
  pan with Space / middle-drag / Pan tool, fit to window, actual size.
- **Full undo / redo** — bounding-box diff and canvas-resize history (80 steps / 256 MB
  cap) covering strokes, fills, selections, flips, 90° rotations, crops and canvas
  resizes, with accurate saved-state tracking.
- **Files** — PNG / JPEG / BMP open & crash-safe atomic save, EXIF orientation support
  on open, background I/O for large canvases, unsaved-changes guard, premultiplied-alpha
  canvas scaling, flip / rotate / invert / grayscale.
- **Native file panels, with a safety net** — Open/Save use window-parented system
  dialogs (`IFileDialog` on Windows, `NSOpenPanel`/`NSSavePanel` on macOS, the XDG
  desktop portal on Linux). If no native dialog can be shown, the app falls back to its
  own themed, virtualised file browser. Switch any time from
  *File → Use system file dialogs*.
- **Per-tool brush sizes** — Pencil, Brush, Eraser, Airbrush and each shape keep their
  own thickness across tool switches.
- **Big canvases stay smooth** — sparse 64×64-tiled coverage masks and incremental
  dirty-rectangle GPU uploads keep even 40-megapixel canvases responsive; dimensions
  above 8192 px per side or 40 MP are rejected cleanly before allocating.

## Build & run

Requires a Rust toolchain (**1.95+** recommended; current stable works out of the box) —
<https://rustup.rs>. No other SDKs beyond the platform's usual graphics libraries are
needed.

```bash
git clone https://github.com/aleksve/rustpaint.git && cd rustpaint
cargo run --release --locked
```

Default release builds are **portable** across CPUs of the target architecture and use
fat LLVM LTO, a single codegen unit, symbol stripping and `panic = "abort"`. If you are
building strictly for the current machine and want host-specific SIMD instructions, opt
in with:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --locked
```

### Windows

```powershell
cargo build --release --locked
.\target\release\rustpaint.exe
```

Release builds set `windows_subsystem = "windows"`, so no console window appears. The
resulting `.exe` is portable across x86-64 Windows machines (with the standard VC++
runtime when built with the `msvc` toolchain).

### macOS

```bash
cargo build --release --locked
./target/release/rustpaint          # or: cargo install --path . --locked
```

Build for Apple Silicon with `--target aarch64-apple-darwin` or Intel with
`--target x86_64-apple-darwin`, and combine with `lipo` if distributing a universal app
bundle.

### Linux

```bash
sudo apt install build-essential pkg-config libxkbcommon-dev libgl1-mesa-dev \
                 libx11-dev libxcursor-dev libxrandr-dev libxi-dev libwayland-dev
cargo run --release --locked
```

Both Wayland and X11 are supported; the OpenGL (`glow`) backend also runs smoothly under
Mesa software rendering, VMs and remote desktops.

## Keyboard shortcuts

Tool selection uses plain keys; commands use `Ctrl` on Windows/Linux and `⌘` on macOS.

| Command | Shortcut | | Tool | Shortcut |
| --- | --- | --- | --- | --- |
| New image | `Ctrl N` | | Select | `V` |
| Open image | `Ctrl O` | | Pencil | `P` |
| Save / Save as | `Ctrl S` / `Ctrl ⇧ S` | | Brush | `B` |
| Canvas size | `Ctrl E` | | Eraser | `E` |
| Undo / Redo | `Ctrl Z` / `Ctrl ⇧ Z` or `Ctrl Y` | | Airbrush | `A` |
| Cut / Copy / Paste | `Ctrl X` / `Ctrl C` / `Ctrl V` | | Fill | `G` |
| Select all / Deselect | `Ctrl A` / `Ctrl D` | | Colour picker | `I` |
| Delete selection | `Del` | | Text | `T` |
| Zoom in / out | `Ctrl +` / `Ctrl -` | | Line | `L` |
| Actual size / Fit | `Ctrl 0` / `Ctrl 9` | | Rectangle | `R` |
| Toggle pixel grid | `Ctrl '` | | Ellipse | `O` |
| Brush size − / + | `[` / `]` | | Polygon | `K` |
| Swap / reset colours | `X` / `D` | | Pan | `H` |
| Pan canvas | `Space` + drag, or middle-drag | | Finish polygon / text | `Enter` / `Ctrl Enter` |
| Pick colour anytime | `Alt` + click | | Nudge selection | Arrow keys (`⇧` = 10 px) |
| Keyboard shortcuts | `F1` | | Cancel | `Esc` |

### Files & settings

| Platform | Settings file |
| --- | --- |
| Windows | `%APPDATA%\RustPaint\settings.ini` |
| macOS | `~/Library/Application Support/RustPaint/settings.ini` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/rustpaint/settings.ini` |

### Text tool

Click on the canvas, type, drag the handle (or the painted letters) to reposition, and
press `Ctrl+Enter` or click *Apply* to commit. Filter installed families in the **Font**
dropdown search box; *Bold* selects a true bold face when available and scales a
synthetic bold pass otherwise. Both bundled faces (`Inter (bundled)` and
`DejaVu Sans (bundled)`) are always present.

## Architecture

```text
src/
  main.rs            window setup (eframe / glow), Windows GUI subsystem flag
  app.rs             application state, frame lifecycle, headless test harness
  app/files.rs       EXIF-aware image loading, atomic saving, background I/O worker
  app/dialogs.rs     New, Canvas Size, Unsaved Changes, Shortcuts and About modals
  app/overlays.rs    file-drop overlay, live text bar/editor, I/O spinner, toasts
  app/shortcuts.rs   modifier-accurate keyboard and clipboard dispatch
  app/transient.rs   in-progress gesture and selection state
  app/ui/topbar.rs   menu bar and quick-action toolbar
  app/ui/sidebar.rs  tool rail, per-tool options, HSV picker, palette and swatches
  app/ui/canvas.rs   checkerboard texture, partial GPU uploads, grid, pointer input
  app/ui/statusbar.rs dimensions, cursor coordinates, selection size, zoom controls
  paint.rs           typed edit sessions (Stroke/Shape/Polygon/Text/Move) & actions
  doc.rs             RGBA buffer, integer Porter-Duff compositing, flood fill, resize
  mask.rs            sparse pooled 64×64-tiled coverage mask & clipped rasterisers
  history.rs         bounding-box diff + resize undo/redo stack with saved-state IDs
  clipboard.rs       OS RGBA image clipboard (arboard) with in-app fallback
  fonts.rs           async system font discovery (fontdb) + cached face loading
  text.rs            glyph layout, kerning and direct mask rasterisation (ab_glyph)
  filedlg.rs         virtualised themed open/save file browser with overwrite prompt
  native.rs          window-parented system file dialogs (rfd)
  prefs.rs           escaped INI preferences with atomic persistence
  tools.rs           tool metadata, options, static palette and hex colour parser
  widgets.rs         custom egui controls, modal helper and two-pass HSV picker
  theme.rs           dark/light palettes, bundled font setup and egui visuals
  icons.rs           runtime vector icons, app icon and clipped marching ants
  geom.rs            shared segment-distance geometry helper
  ir.rs              saturating integer rectangle (`IRect`) primitives
  assets.rs          embedded Inter and DejaVu Sans font binaries
```

## Notes & limitations

- Single layer, like classic Paint (no layer stack or blend modes).
- Text is rasterised onto the canvas when committed; it is not re-editable afterwards.
- Only upright (non-italic) font faces are listed.
- On Linux, native file panels require a running `xdg-desktop-portal` backend; otherwise
  the built-in browser opens automatically.
- JPEG does not store alpha: transparent pixels are composited onto white when saving as
  `.jpg` / `.jpeg`.

## License

RustPaint is free software licensed under the **GNU General Public License v3.0 or later**
(`GPL-3.0-or-later`) — see [`LICENSE`](LICENSE).

Bundled font files in `assets/` (`Inter-Variable.ttf` under SIL Open Font License 1.1 and
`DejaVuSans.ttf` under the Bitstream Vera / Arev permissive license with public-domain
DejaVu changes) are covered by their respective upstream licenses in
[`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md).
