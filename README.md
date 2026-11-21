<div align="center">

<img src="app/public/logo.svg" alt="FreePlay Logo" width="84" />

# FreePlay

**Free, open-source single-player game trainer hub powered by community repositories.**  
No subscriptions. No paywalls. Import Cheat Engine `.CT` tables, scan values live, or subscribe to community trainer repos—just like Tachiyomi.

[![Download Latest Release](https://img.shields.io/badge/Download-Latest%20Release%20(.exe)-5b8cff?style=for-the-badge&logo=windows)](../../releases/latest)
[![Buy Me a Coffee](https://img.shields.io/badge/Buy%20Me%20a%20Coffee-Support%20FreePlay-FFDD00?style=for-the-badge&logo=buy-me-a-coffee&logoColor=black)](https://buymeacoffee.com/orcecso)
[![License: MIT](https://img.shields.io/badge/License-MIT-34d399?style=for-the-badge)](LICENSE)

</div>

---

## Quick Start (Just Download & Play)

No coding or setup required:

1. Open the **[Releases](../../releases/latest)** page on the right side of this repository.
2. Download the latest **`FreePlay.exe`** (or Windows installer) and run it.
3. Launch your single-player game, then do **any** of the following:
   - **Add a Community Repo** in **Repositories** → install trainers with one click from **Browse Repos**.
   - **Import a Cheat Engine `.CT` file** in **Import / Export .CT** → click **Pick Process...** to choose your running game (no typing `.exe` names!).
   - **Find values yourself** using the built-in **Value Scanner** (`All: 4 Bytes + Float + Double`), test them live, and export as `.CT` or `.trainer.json`.

> **Single-Player Only:** FreePlay uses standard Windows user-mode memory APIs for offline/single-player games. It does not include kernel drivers or anti-cheat bypasses.

---

## Coming from Cheat Engine? (`.CT` Users & Table Creators)

If you've used **Cheat Engine** before—or if you're a creator who already builds and shares `.CT` tables—you'll feel right at home:

- **Import your existing `.CT` files directly:** FreePlay reads Cheat Engine `.CT` XML tables out of the box—including static addresses, multi-level pointer chains, `aobscanmodule` byte patches, and opt-in x64 Auto Assembler code caves (`alloc`, `registersymbol`, `movss`/`mulss`, etc.).
- **Safer by default:** `<LuaScript>` and `LuaCall` blocks are blocked automatically so community tables can't silently run arbitrary scripts on your PC.
- **Create once, share everywhere:** Use the built-in **Live Value Scanner** to find HP, Gold, or XP in a running game, then export both a **Cheat Engine `.CT` file** and a **`.trainer.json`** (complete with a ready-to-paste `index.json` snippet + SHA-256 hash) to share in any community GitHub repository.

---

## Screenshots

### Live Trainer Controls & One-Click Attach
Attach to your running game, toggle AoB patches and opt-in x64 Auto Assembler scripts, or read/set/freeze live values—and export back to `.CT` anytime.

![FreePlay Trainer Controls](docs/screenshots/trainer-controls.png)

### Built-In Live Value Scanner (with Real-Time Progress)
Pick any running game from the filtered Process Picker (Windows system processes hidden automatically), scan in **All (`4 Bytes + Float + Double`)** mode with a live progress bar, test values in-game, and capture them to export as `.CT` or `.trainer.json`.

![FreePlay Live Value Scanner](docs/screenshots/value-scanner.png)

### Library & Tachiyomi-Style Community Repositories
Keep all your installed trainers in one clean library with automatic version-update badges, or browse and install SHA-256 verified community trainers by game title, `.exe`, or Steam AppID.

| Installed Library | Browse Community Catalog |
| :---: | :---: |
| ![FreePlay Library](docs/screenshots/library.png) | ![FreePlay Browse Repos](docs/screenshots/browse-repos.png) |

---

## Key Features

- **Zero Bundled Trainers (Tachiyomi Model):** Add any GitHub `index.json` URL in **Repositories**, refresh catalogs, and install/update trainers with SHA-256 integrity checks.
- **Smart Process Picker:** Lists your running third-party apps and games with their window titles right at the top—no need to guess or type `.exe` filenames.
- **Cheat Engine `.CT` Import & Export:** Bring existing `.CT` tables into FreePlay or export your own discoveries back to `.CT`.
- **Live Value Scanner:** Supports `All (4 Bytes + Float + Double)`, `i32`, `f32`, `f64`, `i64`, `i16`, `u8`, plus `Exact`, `Increased`, `Decreased`, `Changed`, and `Unchanged` filtering with live progress.
- **Clean Cleanup:** Disabling a toggle or detaching automatically unfreezes values, restores original bytes, and frees allocated memory caves.

---

## Hosting Your Own Community Trainer Repo

Anyone can host a trainer repository on GitHub for free (zero backend required):

1. Fork or copy the template in [`examples/sample-repo/`](examples/sample-repo/).
2. Put your `.trainer.json` files inside `trainers/` and list them in `index.json` (FreePlay's **Value Scanner** generates the exact `index.json` entry and SHA-256 hash for you!).
3. Share your raw GitHub `index.json` URL (e.g. `https://raw.githubusercontent.com/<user>/<repo>/main/index.json`) so anyone can paste it into **Repositories** in FreePlay.

---

## Support FreePlay

FreePlay is 100% free, open-source, and has zero paywalls or subscriptions. If it saved you time or money, consider supporting ongoing development:

[![Buy Me a Coffee](https://img.shields.io/badge/Buy%20Me%20a%20Coffee-ffdd00?style=for-the-badge&logo=buy-me-a-coffee&logoColor=black)](https://buymeacoffee.com/orcecso)

---

## Developer Guide (Building from Source)

### Prerequisites
- **Windows 10/11** (64-bit)
- **Rust** (`rustup` / `cargo`)
- **Node.js 20+** (`npm`)

### Project Layout
```text
open-trainer/
├── Cargo.toml
├── docs/screenshots/             # README showcase screenshots
├── examples/
│   ├── sample.trainer.json       # Example declarative trainer manifest
│   └── sample-repo/              # Template community repo (index.json + trainers/)
├── crates/
│   ├── trainer-core/             # User-mode Windows memory engine, pointer chains, AoB & live scanner
│   ├── trainer-schema/           # JSON trainer schema, .CT XML converter & x64 Auto Assembler runtime
│   ├── trainer-repo/             # Community repo catalog, HTTP/local fetcher, SHA-256 verifier & store
│   └── dummy-target/             # Local test process for end-to-end integration tests
└── app/                          # Tauri v2 + React + TypeScript desktop app
    ├── src/                      # React frontend
    └── src-tauri/                # Tauri v2 Rust backend
```

### Run Backend Tests
```powershell
cargo test --workspace
```

### Run Desktop App in Dev Mode
```powershell
cd app
npm install
npm run tauri dev
```

### Build Release Executable
```powershell
cd app
npm install
npm run tauri build
```
