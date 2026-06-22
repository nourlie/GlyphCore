# GlyphCore

[Русский](README.md) · **English**

### Guild Wars 2 localization — non-Latin script rendering + text translation

> **Guild Wars 2 localization** into any language: render **non-Latin scripts**
> (Cyrillic, **Korean / Hangul**, and more) in the game + translate the game text —
> without modifying `Gw2.dat`. *Guild Wars 2 localization (Russian / Korean / …).*

<img width="237" height="247" alt="image" src="https://github.com/user-attachments/assets/2d33dddc-26aa-4c0f-bf60-b0dcd12c216c" />

An early-load proxy DLL that makes **Guild Wars 2** **render the letters the stock
client shows as empty boxes** (Cyrillic, Korean, etc.) and **translates the game
text** into your language. Substitution is **language-neutral** — you can put a
translation into any language into the dictionary. This is an accessibility /
localization project: it teaches the live font engine to draw the needed glyphs and
swaps text **in memory**, **without changing the on-disk `Gw2.dat` file**, so
ArenaNet's archive integrity checks pass and the game never triggers a "repair".

> **Status:** ✅ Non-Latin text (Cyrillic, Korean Hangul) renders without crashing.
> ✅ Text translation, including the previously unreadable encrypted (RC4) strings
> (~70% of all text). ✅ **In-game overlay** (the **Insert** key): dictionary editor,
> string collection and settings right in the game — with an **interface language
> switch (Русский / English)**.

> ⬇️ **Prebuilt files are on the [Releases](https://github.com/nourlie/GlyphCore/releases/latest) page.**
> No need to build from source — just download and drop into the game folder.

> ⚠️ **A ready-made translation is NOT included.** The release ships `version.dll`
> (glyph rendering + overlay), `gw2-cyrillic-gui.exe` (dictionary editor) and
> `cyrillic-string-dump-tool.exe` (text dump). **You fill in the
> `cyrillic_strings.csv` dictionary yourself** — extract the English text with the
> dump tool and translate it (or collect strings live in the game). Without a filled
> dictionary the glyphs render, but the text stays English.

---

## 📦 Installation

1. Download **`version.dll`** from the [latest release](https://github.com/nourlie/GlyphCore/releases/latest).
2. Drop it into the Guild Wars 2 folder — next to `Gw2-64.exe`.
3. Launch the game. The glyphs render. Done.

```
Guild Wars 2\
├─ Gw2-64.exe
└─ version.dll      ← the downloaded file
```

> The genuine `version.dll` exports are forwarded to the real system `version.dll`
> from `System32` automatically — no separate `version_orig.dll` copy is needed.

> 🔄 **Auto-update.** On launch `version.dll` checks the
> [GitHub releases](https://github.com/nourlie/GlyphCore/releases) and, if a newer
> one exists, quietly updates itself for the next launch (in the background, when
> online).
>
> **To roll back:** delete `version.dll` from the game folder.

---

## 🪟 In-game overlay (the Insert key)

`version.dll` embeds a full **overlay** into the game — opened with the **Insert**
key. It is the main way to work with the localization without leaving the game.
Vertical tabs, dark theme, **interface language switchable (Русский / English)** on
the *Interface* tab:

- **Translation** — status and general controls.
- **Found words** — strings collected while playing, grouped by file.
- **Dictionary editor** — edit any `dict_*.csv` / `pn_*.csv`, the main
  `cyrillic_strings.csv`, and collected strings right in the game: click-to-edit,
  filters (all / translated / untranslated), search, progress indicator, quick
  "Go to untranslated" jump. **Changes apply live immediately on save** — no restart
  or reopening of panels.
- **Interface** — **overlay interface language (Русский / English)**, HUD icon
  position and size, overlay font scale, **game glyph font selection and its size**
  (applied on the fly).
- **Log** — diagnostic log.

> The editor supports **paste/copy (Ctrl+V / Ctrl+C)**, and Korean input/text renders
> with glyphs (via a system font) instead of "boxes".

> String collection (`autocollect` / `seed_decode`) can be **toggled right in the
> overlay** — no game restart.

---

## 📝 Game text translation

Besides drawing non-Latin glyphs, `version.dll` **translates the game text**. The
translation comes from the **`cyrillic\cyrillic_strings.csv`** dictionary (columns
`english,translate` — translate into **any** language; editable in any spreadsheet,
in the GUI, or in the overlay). The English string is replaced with the translation
**in memory**, after the integrity checks — the `.dat` is untouched. **Translation
length is unlimited.** Encrypted (RC4) strings — ~70% of all game text — are
translated too.

> Substitution is **language-neutral**: the `translate` column can hold Russian,
> Korean, or any other language. By default the game stays in its **original
> English** unless your OS language is Russian (`translate` auto-enables only on
> Russian systems; elsewhere enable it in the overlay or in `config.ini`).

```
Guild Wars 2\
├─ version.dll
├─ cyrillic-string-dump-tool.exe   ← dictionary dump tool (from the release)
└─ cyrillic\
   ├─ cyrillic_strings.csv         ← main dictionary (english,translate) — you fill the translation
   ├─ dict_*.csv                   ← per-source dictionaries (wiki/API) — also loaded
   ├─ discovered_strings.csv       ← auto-collected new strings (temporary)
   ├─ cyrillic.ttf                 ← your own font (optional)
   ├─ config.ini                   ← settings
   └─ version_proxy_log.txt        ← log
```

### Get a base dictionary — `cyrillic-string-dump-tool.exe`

**Unencrypted** strings (item names and part of the UI — the smaller share of the
text) can be extracted from `Gw2.dat` offline with
**`cyrillic-string-dump-tool.exe`** (downloaded from the same
[release](https://github.com/nourlie/GlyphCore/releases/latest)):

1. **Fully close the game** (and the launcher — it needs exclusive access to the archive).
2. Drop `cyrillic-string-dump-tool.exe` into the Guild Wars 2 folder (next to
   `Gw2.dat`) and run it (double-click).
3. It scans the archive (a few minutes) and **appends** the found English strings to
   `cyrillic\cyrillic_strings.csv` (merge — existing rows and your translations are kept).

Then fill in the `translate` column (translation into your language).

> ℹ️ **Encrypted (RC4) strings — ~70% of the text — are NOT extracted offline** by
> this tool (they need the game's runtime keys). They land in the dictionary **as you
> play** via string collection (see below): `version.dll` decodes them on the fly and
> writes them down. Bottom line: **full dictionary = base offline dump (raw) + RC4
> collection in-game.**

### More text sources — the official wiki and the GW2 API

`cyrillic-string-dump-tool.exe` extracts English text not only from `Gw2.dat`, but
also from the **official wiki** and the **GW2 API** — the content that is encrypted
in the `.dat` or sent by the server (dialogue, lines). Run the tool with no
arguments — a **menu** opens (arrows / space / Enter) to pick sources:

- **Wiki:** story dialogue (by expansion), renown-heart lines, events, ambient (zone)
  dialogue, boss lines, NPC dialogue (conversation window).
- **GW2 API:** achievements, items, skills, traits, skins, world-map names
  (zones / points of interest / waypoints) and a bundle of short lists (titles,
  currencies, dyes, mounts, minis, guild, WvW, etc.).

Each source is written to its own **`dict_<source>.csv`** file (format
`english,translate`, merge — your translations are kept). **`version.dll` loads all
`dict_*.csv`** from the `cyrillic\` folder together with `cyrillic_strings.csv`, so
keeping translations in separate files is convenient and breaks nothing (manual
translations in `cyrillic_strings.csv` take priority).

> Long dumps (events, dialogue, NPCs — thousands of pages) are written **as they go**
> and support **resume**: if interrupted, the next run continues from where it left
> off instead of re-downloading everything.

### Collecting strings as you play

`version.dll` decodes the strings the game shows (including encrypted ones) and
appends them to `cyrillic\discovered_strings.csv`. On the next launch the new strings
are automatically folded into `cyrillic_strings.csv`. This way the dictionary grows
with what you actually see in the game. Two modes (enabled in the overlay or in
`config.ini`):

- **`autocollect`** — collects strings **as they appear on screen**.
- **`seed_decode`** — decrypts **all** strings sent by the server (far more than
  what's on screen, ~18× coverage).

### Count declensions — `[one|few|many]`

Some languages inflect nouns by count (Russian: 1 snowflak**e**, 3 snowflak**es**,
5 snowflak**es** all differ), which the English `[s]` cannot express. For those, put
**three forms separated by `|`** in square brackets — `version.dll` picks the right
one by the count the game supplies:

```
[form_for_1 | form_for_2-4 | form_for_5+]
```

| When | Which number | Form used |
|------|--------------|-----------|
| **one** | 1, 21, 31, 101 … (but not 11) | `[`**form1**`|…|…]` |
| **few** | 2–4, 22–24 … (but not 12–14)  | `[…|`**form2**`|…]` |
| **many** | 5–20, 0, 25–30 …            | `[…|…|`**form3**`]` |

**Example.** The English counter string `%num1% Snowflake[s]` → into the `translate`
column. It's convenient to keep the common stem **outside** the brackets and leave
only the differing endings inside:

```csv
english,translate
%num1% Snowflake[s],%num1% снежин[ка|ки|ок]
```

In the game it renders: `1 снежинка`, `3 снежинки`, `7 снежинок`.
(Whole words work too — `[снежинка|снежинки|снежинок]`; both are equivalent.)

> A couple of extra notes:
> - two forms `[ый|ые]` — adjective/suffix: the form for **1**, otherwise the second
>   (e.g. `золот[ой|ые] [ключ|ключа|ключей]`);
> - a single `[ы]`/`[и]` without `|` yields **the singular only** (the engine can't
>   change the word stem) — for counters always use three forms.

Declensions apply both on dictionary load and **live** when you save an edit in the
overlay editor.

---

## 🅰️ Glyph font

By default the game glyphs are drawn with the bundled **Tahoma** font (covers Latin
and Cyrillic). For Korean (Hangul), `version.dll` additionally uses the **Hangul
glyphs already shipped inside the game itself**, so Korean renders across the whole UI
without plugging in a separate font. To render Cyrillic with **your own** font — two
ways:

- **In the overlay:** the **Interface** tab → pick a `.ttf` and size. Applied on the
  fly (on the next font-asset load — a zone/map change), no restart.
- **Drop-in:** put a **`cyrillic.ttf`** file into the **`cyrillic\`** subfolder next
  to `version.dll`.

```
Guild Wars 2\
├─ Gw2-64.exe
├─ version.dll
└─ cyrillic\
   └─ cyrillic.ttf   ← your font
```
> If you put `cyrillic.ttf` / `cyrillic_strings.csv` directly into the game folder,
> on the next launch `version.dll` moves them into `cyrillic\` itself.

> [!IMPORTANT]
> **A custom font must contain the script's glyphs.** If a `.ttf` has no Cyrillic,
> version.dll **rejects it** and falls back to the built-in font — no crash. Check the
> `cyrillic\version_proxy_log.txt` log.

> [!WARNING]
> **Don't trust the Windows font preview** — it shows Cyrillic even for fonts without
> it (system font fallback) while the file itself has no such letters. Use fonts with
> real Cyrillic coverage:
> [Roboto](https://fonts.google.com/specimen/Roboto),
> [Open Sans](https://fonts.google.com/specimen/Open+Sans),
> [PT Sans](https://fonts.google.com/specimen/PT+Sans),
> [Noto Sans](https://fonts.google.com/noto/specimen/Noto+Sans).

---

## ⚙️ Settings — `cyrillic\config.ini`

Settings can be changed **in the overlay** (the Interface tab) or by hand in
`cyrillic\config.ini` (created on first launch). Main options:

```ini
# Auto-update version.dll from GitHub releases.
autoupdate=true
# Apply the translation from cyrillic_strings.csv (show the translation in-game).
# Defaults to on only on a Russian-locale system, otherwise the original English.
translate=true
# Overlay interface language: ru | en (defaults to en unless the OS is Russian).
ui_lang=ru
# Auto-collect shown strings into discovered_strings.csv.
autocollect=false
# Full collection: decrypt ALL server-sent strings (~18× coverage).
seed_decode=false
# Translate character names and location names (the pn_*.csv layer).
translate_proper_nouns=false
```

| Option | Default | What it does |
|--------|:---:|------------|
| `autoupdate` | `true` | checks and installs version.dll updates from GitHub |
| `translate` | *by locale* | loads the dictionary and shows the translation in-game (on for Russian systems, off otherwise) |
| `ui_lang` | *by locale* | overlay interface language: `ru` / `en` (`en` unless the OS is Russian) |
| `autocollect` | `false` | collects shown strings into `discovered_strings.csv` |
| `seed_decode` | `false` | decrypts all server-sent strings (full collection) |
| `translate_proper_nouns` | `false` | translates names/locations from the `pn_*.csv` layer |

> String collection (`autocollect` / `seed_decode`) and most settings apply **on the
> fly** when changed in the overlay — no restart needed.

---

## 🖥️ GUI — dictionary editor, settings and status

Besides the in-game overlay, the release includes **`gw2-cyrillic-gui.exe`** — a
standalone graphical app to work with the dictionary outside the game. Put it next to
`version.dll` (in the game folder) or run it from anywhere and point it at the GW2
folder — it finds `cyrillic\` automatically (including by searching Steam libraries).

```
Guild Wars 2\
├─ version.dll
├─ gw2-cyrillic-gui.exe   ← graphical editor (from the release)
└─ cyrillic\
   └─ cyrillic_strings.csv
```

Tabs:

- **Dictionary** — an `English / Translation` table over `cyrillic_strings.csv`:
  search, filters (**all / translated / untranslated / with broken markup**),
  highlighting of untranslated rows and rows with broken placeholders, row selection
  with checkboxes and bulk operations, undo (**Ctrl+Z**), translation progress and
  jump to the next untranslated row. Changes are saved into the same CSV.
- **Settings** — edit `cyrillic\config.ini` in a couple of clicks, plus a theme
  (dark/light, accent).
- **Status** — parses `cyrillic\version_proxy_log.txt`: what loaded, whether the
  signatures were found, whether the dictionary is visible — handy for diagnostics.

> The GUI breaks nothing in the game: it only edits the dictionary and `config.ini`
> and reads the log. The translation is still applied by `version.dll`.

---

## 🔧 Advanced — `gw2-dat-tool` (optional)

The repository has a CLI [`gw2-dat-tool/`](gw2-dat-tool/) for working with **your own**
`Gw2.dat`: exporting the string dictionary, inspecting `AFNT` font chunks, generating
glyph atlases. Most people **don't need it** — `version.dll` +
`cyrillic-string-dump-tool.exe` are enough. Details in
[`gw2-dat-tool/README.md`](gw2-dat-tool/README.md).

```powershell
# export the whole string dictionary to CSV (game closed):
cargo run --release -- --dat "C:\path\to\Guild Wars 2\Gw2.dat" strs-export-all --out dict.csv
```

---

## How it works (in brief)

Guild Wars 2 stores fonts and text inside the `Gw2.dat` archive. The direct
approaches don't work: **editing `Gw2.dat`** is impossible (the client checks the CRC
and deletes modified files → re-download), and a **regular add-on** loads *after* the
game has already read the data.

`version.dll` sidesteps both: GW2 imports it statically, so it loads **before** the
engine; the game reads and verifies the *genuine* data, and then the DLL swaps the
**decompressed font and string bytes in RAM** — after all integrity checks, but before
the parser. The on-disk `Gw2.dat` is **not modified**. The overlay is drawn on top of
the game via a Direct3D hook.

The source of the injected `version.dll` is closed; the releases ship a prebuilt
binary.

---

## ⚠️ Legal / game data

The repository contains the `gw2-dat-tool` tool and Cyrillic data. It contains **no**
data extracted from `Gw2.dat` — that is ArenaNet's property; you obtain the prebuilt
files from *your own* legally installed copy of the game (via the dump tool) or from
the releases.

Guild Wars 2 is a trademark of ArenaNet, LLC. This is an unofficial, non-commercial
fan project, not affiliated with or endorsed by ArenaNet.

## License

[MIT](LICENSE) for the code in this repository.
