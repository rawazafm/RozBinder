# RozBinder

**RozBinder** is a Windows desktop app that merges multiple `.exe` files into a single launcher. Drop your executables onto the app, set a save path, and it produces one portable `.exe` that unpacks and runs all the bundled programs in the order you choose.

> **Windows only** — the launcher stub and merged output are Windows executables.

---

## Download

Grab the latest installer or portable executable from the [**Releases**](https://github.com/rawazafm/RozBinder/releases) page.

| File | Description |
|------|-------------|
| `Binder_x.x.x_x64-setup.exe` | NSIS installer — installs RozBinder to Program Files |
| `Binder.exe` | Portable — run directly, no installation needed |

---

## Features

- **Drag-and-drop** — drop `.exe` files directly onto the window
- **Reorder** — move programs up/down to control launch order
- **Custom icon** — drop or browse an `.ico` file to embed a custom icon in the merged launcher
- **Portable output** — the merged `.exe` extracts its payloads to a temp folder and starts them; original files are never moved or modified
- **Deduplication** — adding the same file twice is silently skipped
- **Up to 100 executables** per merge

---

## How it works

1. **Add executables** — click *Add executables* or drag `.exe` files onto the panel
2. **Set an icon** *(optional)* — drag an `.ico` file or click *Set icon*
3. **Choose output** — type a path or click *Browse*
4. **Merge** — click *Merge*; a single `.exe` is written to the output path

When the merged launcher runs it:
1. Derives a unique key from the bundle index
2. Extracts all payloads into `%TEMP%\rozbinder\<key>\`
3. Launches every `.exe` in list order from that temporary folder

---

## Build from source

**Prerequisites**

- [Node.js](https://nodejs.org/) 20+
- [Rust](https://rustup.rs/) stable toolchain
- Windows (required for the Tauri Windows target)

```bash
# Install JS dependencies
npm install

# Development mode (hot-reload)
npm run tauri dev

# Production build
npm run tauri build
```

Output files are placed in:
```
src-tauri/target/release/Binder.exe
src-tauri/target/release/bundle/nsis/*-setup.exe
```

---

## Tech stack

| Layer | Technology |
|-------|-----------|
| Frontend | React 19 + TypeScript + Vite |
| Backend | Rust + Tauri 2 |
| Bundler stub | Custom Rust binary (`rozbinder-stub`) |
| CI / Releases | GitHub Actions (`windows-latest`) |

---

## License

MIT
