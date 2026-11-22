# FreePlay YouTube Thumbnail Guide

Reusable visual style, character reference, and prompt template for generating consistent YouTube reaction-style thumbnails for **FreePlay** videos and demos.

---

## Core Assets

- **Mascot Reference Sheet:** `docs/assets/mascot-sheet.jpg`
  - Male chibi tech-creator mascot: spiky black hair, thick eyebrows, brown eyes, black zip-up jacket with subtle blue accents, black shirt.
  - Available sheet expressions: `NEUTRAL`, `HAPPY`, `LAUGH`, `SERIOUS`, `THINKING`, `SURPRISED`.
- **App Screenshots:** `docs/screenshots/`
  - `value-scanner.png` — Live Value Scanner
  - `trainer-controls.png` — Live Trainer Controls & One-Click Attach
  - `library.png` — Installed Library
  - `browse-repos.png` — Community Repositories Browser

---

## Visual Style & Layout Rules

- **Aspect Ratio:** `16:9` (YouTube standard)
- **Overall Vibe:** Classic high-energy, slightly clickbaity YouTube reaction-video thumbnail that stays clean and readable at small sizes.
- **Right Side (Mascot Foreground):**
  - Place the chibi mascot on the **right** side, angled **3/4 toward the left**.
  - **Gaze Direction:** Eyes looking **up-left directly at the app screenshot** (not staring blankly forward).
  - **Pose:** Expressive reaction pose matching the video topic (e.g., hand-on-chin `THINKING`, `SURPRISED`, or `HAPPY` pointing).
  - **Cutout Treatment:** Thick bright white sticker outline + subtle electric-blue rim light so the mascot pops off the background.
- **Left Side (App Showcase):**
  - Display a **real FreePlay screenshot** from `docs/screenshots/` angled slightly in 3D perspective with a glowing blue border and soft drop shadow.
- **Typography (Top-Left / Top-Center):**
  - 2 short, punchy lines max (2–4 words per line), huge bold impact font with thick black outlines and drop shadows.
  - Line 1: Bright white or yellow.
  - Line 2: Electric cyan/blue or bright yellow-red accent.
  - Optional small pill badge in the bottom-left corner (e.g., `FREE & OPEN SOURCE`, `NO PAYWALLS`, `NEW UPDATE`).
- **Accents & Background:**
  - Classic bright red curved YouTube arrow pointing toward the app screenshot or highlighted feature.
  - Dark navy/slate tech-gaming gradient background with a subtle radial glow behind the app window (avoid clutter so the mascot's gaze toward the app is the main visual hook).

---

## Reusable Prompt Template

When asking an AI image generator to create a new thumbnail, fill in the bracketed fields:

```text
Create a 16:9 YouTube reaction-style thumbnail.

Reference images:
1. Character sheet (use this exact chibi character): open-trainer/docs/assets/mascot-sheet.jpg
2. App screenshot to show on the left side: open-trainer/docs/screenshots/<SCREENSHOT_FILENAME>.png

Composition & layout:
- Aspect ratio: 16:9 YouTube thumbnail.
- RIGHT SIDE (foreground): Place the chibi male tech-creator mascot from Reference 1 (spiky black hair, thick eyebrows, brown eyes, black zip-up jacket with blue accents) angled 3/4 to the left, in a [<POSE>, e.g., hand-on-chin THINKING / SURPRISED / EXCITED] pose, eyes looking directly UP-LEFT at the FreePlay app window on the left. Give him a clean white sticker outline and subtle blue rim lighting.
- LEFT SIDE: Display the actual FreePlay screenshot from Reference 2 angled slightly in 3D perspective with a glowing blue border and drop shadow.
- TOP-LEFT / TOP-CENTER bold clickbait YouTube text (huge, super legible, thick black outline):
  - Line 1 (bright white/yellow): "<LINE_1>"
  - Line 2 (electric cyan/blue accent): "<LINE_2>"
- Add a classic bright red curved YouTube arrow pointing toward the FreePlay app screenshot, and a small "<BADGE_TEXT>" pill badge in the bottom-left corner.
- Background: Clean dark navy/slate tech-gaming gradient with a subtle radial glow behind the app screenshot.
```

---

## Example Text Ideas for Future Videos

| Video Topic | Line 1 | Line 2 | Mascot Expression | Badge |
| :--- | :--- | :--- | :--- | :--- |
| Value Scanner Demo | `NEW CHEAT ENGINE` | `ALTERNATIVE?!` | `THINKING` (hand on chin) | `FREE & OPEN SOURCE` |
| Community Repos | `TACHIYOMI FOR` | `GAME TRAINERS?!` | `SURPRISED` | `ZERO PAYWALLS` |
| `.CT` Import/Export | `IMPORT ANY .CT` | `IN 1 CLICK!` | `HAPPY` (pointing) | `NO LUA SCRIPTS` |
| Major Update | `FREEPLAY v2` | `IS INSANE!` | `SURPRISED` | `100% FREE` |
