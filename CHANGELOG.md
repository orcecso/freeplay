# Changelog

## v0.3.0 — Linux, Steam Deck & Handheld Edition (Experimental)

> **Testing Notice:** The Linux, SteamOS / Steam Deck, and handheld PC builds have **not yet been fully tested on physical Linux or Steam Deck hardware**. Community testers, bug reports, pull requests, and [Buy Me a Coffee](https://buymeacoffee.com/orcecso) donations (to help fund hardware testing and ongoing development) are warmly welcomed!

### Added
- **Linux & SteamOS Memory Engine (`trainer-core`):**
  - Implemented `/proc/<pid>/maps`, `/proc/<pid>/mem`, and `process_vm_readv` / `process_vm_writev` backend for live memory reading, writing, Value Scanner (`All`, `i32`, `f32`, `f64`, etc.), Freezing, Pointer Chains, and reversible AoB Byte Patches (`FOLL_FORCE`).
  - Added automatic **Proton / Wine `.exe` & module detection** so Windows games running via Steam Proton report their real `.exe` and `.dll` names, enabling Windows `.CT` tables and community trainer repos on Steam Deck.
  - Added Linux/SteamOS system process filtering (hides OS/desktop/Steam daemons while keeping Proton games, native Steam games, and `/run/media` microSD titles visible) and actionable `CAP_SYS_PTRACE` permission guidance.
  - Enabled live `dummy-target` integration tests on Linux in CI.
- **Handheld PC & Steam Deck Accessibility (`app`):**
  - Added built-in **Gamepad API navigation** (`useGamepadNav.ts`): **LB/RB** switch tabs, **D-Pad / Left Stick** navigate focus, **A** select, **B** back/close modal, **Right Stick** scroll, plus an automatic controller shortcut hint pill.
  - Added responsive handheld layouts (`≤ 960px` compact sidebar and `≤ 760px` top-bar nav), `720×500` minimum window size, touch-friendly `40px` control targets, and crisp `:focus-visible` rings.
  - Added close confirmation modal so closing FreePlay while attached cleanly detaches and stops freezes/patches.
  - Added Linux `xdg-open` support and XDG Base Directory data storage (`$XDG_DATA_HOME/FreePlay` / `~/.local/share/FreePlay`).
- **Linux & Steam Deck Packaging & Release CI:**
  - Added `scripts/install-steamdeck.sh` for immutable-OS-safe installation into `~/.local/bin/freeplay`, desktop shortcut registration (ready for "Add to Steam"), and one-time `CAP_SYS_PTRACE` setup.
  - Updated `.github/workflows/release.yml` to build Windows (`.msi`, `.exe`, portable) and Linux (`ubuntu-22.04` `.AppImage`, `.deb`, and `FreePlay-linux-x64-steamdeck.tar.gz`) in parallel with unified `SHA256SUMS.txt`.

### Known Limitations on Linux
- Auto Assembler remote code caves (`alloc`) are currently Windows-only; Value entries, Pointer Chains, Freezing, and AoB Byte Patches work natively on Linux.
