# Dijmo logo

The logo is the Caret. The d's stem is the text caret, cut free of its bowl, red, and running a little under the baseline, the way a playhead crosses a track. The caret is the playhead.

The four directions it was chosen from are in `../concepts/`.

## Files

| File | Use |
|---|---|
| `wordmark-light.svg` | The default logo, on Porcelain or white. |
| `wordmark-dark.svg` | On Stage black or dark grounds. |
| `wordmark-mono.svg` / `wordmark-mono-white.svg` | One colour, for print, embossing, photos and single-ink work. |
| `mark-*.svg` | The d alone, for small square spaces: avatars, stickers, the app. |
| `lockup-*.svg` | The app icon tile beside the wordmark, for "Dijmo for Mac" contexts and social cards. On light grounds the tile is the Stage icon; on dark grounds it is the Porcelain icon. |
| `app-icon-light.svg` / `app-icon-dark.svg` | The 1024 × 1024 macOS icon. |
| `favicon.svg` | A 32-unit grid drawn for 16 px, with edges on whole pixels. |
| `app-icons/` | The desktop app's icon set, staged for `src-tauri/` (see below). |
| `png/` | Reference renders. |
| `source/build_final.py` | Rebuilds everything here, plus the website's favicon, touch icon and OG card. |

- **Outlines.** Every letter is an outline from Schibsted Grotesk (OFL), the site's typeface. No file needs a font.
- **Shared geometry.** The geometry is shared with `../concepts/source/build.py`, so a change there flows into every file.

## Colours

| Name | Hex | Role in the logo |
|---|---|---|
| Porcelain | `#F2F3F1` | Light ground; the light icon. |
| Ink | `#17181A` | Letters and bowl on light grounds; the one-colour logo. |
| Caret red | `#E0301A` | The caret only. |
| Stage | `#000000` | Dark ground; the dark icon. |
| Stage ink | `#F2F2EF` | Letters and bowl on dark grounds. |

- **The caret is always red in colour versions, in dark mode too.** The site's interface caret turns Stage ink in dark mode; the logo's caret does not.
- **One-colour versions are ink or white.** They are never red alone.

## Clear space

- **x** is the height of the bowl: the x-height of the wordmark.
- **Around the wordmark and the lockup:** keep at least x on every side.
- **Around the mark alone:** keep at least ½ x on every side.
- **Inside the lockup:** the space between tile and wordmark is fixed; don't change it.

## Minimum sizes

| Item | Screen | Print |
|---|---|---|
| Wordmark (full height, tittle to descender) | 16 px | 6 mm |
| Lockup (tile height) | 20 px | 7 mm |
| Mark | 20 px | 7 mm |

Below 20 px, use `favicon.svg` or the 16 and 32 px PNGs. They are fitted to the pixel grid. Scaled-down artwork blurs the gap between bowl and caret.

## Don'ts

- **Don't close the gap.** Never join the bowl and caret into an ordinary d.
- **Don't move the caret.** Don't shorten it, thin it, shift it, or drop its dip under the baseline.
- **Don't recolour it.** No other reds, no gradients, outlines, glows or shadows, except the app icon's own material.
- **Don't retype "dijmo".** Not in Schibsted Grotesk or any other font; use the files. Don't capitalise it.
- **Don't distort it.** No stretching, condensing, slanting, rotating or re-spacing.
- **Don't put the red caret on red, mid-tones or busy photos.** Use the one-colour white version there.
- **Don't set the Porcelain tile on Porcelain.** Use the Stage tile on light grounds.
- **Don't use the other concept marks** (Selection, Cut, Thock) as logos.

## App icons for the desktop app

`app-icons/` mirrors `src-tauri/` file for file. The names and pixel sizes match the existing Tauri set.

- **Copy in:**
  - `app-icons/icons/*` goes to `src-tauri/icons/`.
  - `app-icons/icon.svg` goes to `src-tauri/icon.svg`.
- **macOS:**
  - Files: `icon.icns` (built with `iconutil` from a full `.iconset`, 16 to 1024), `icon.png`, `32x32.png`, `64x64.png`, `128x128.png` and `128x128@2x.png`.
  - These use the light icon: Apple's 824 px continuous-corner squircle, centred on the 1024 canvas, with its drop shadow.
  - macOS 26 only swaps in a dark variant for asset-catalog icons, so the light icon is the default.
- **Windows:**
  - `icon.ico` holds PNG entries at 16, 24, 32, 48, 64 and 256. The 16, 24 and 32 sizes come from the pixel-fitted favicon; the larger ones are a full-canvas Porcelain tile with a hairline edge.
  - The `Square*Logo.png` and `StoreLogo.png` tiles use the same rule.
- **Android:**
  - `ic_launcher` and `ic_launcher_round` are the tile and circle.
  - `ic_launcher_foreground` keeps the mark inside the 66 dp safe zone, on the adaptive background `#F2F3F1`.
- **iOS:** the icons are full bleed and flattened, as the system applies the mask.

## The website

- **Favicon and touch icon.**
  - `assets/img/icon.svg` is `favicon.svg`.
  - `apple-touch-icon.png` is the full-bleed 180 px icon.
- **OG card.** `og.png` is rendered from `docs/brand/og.svg`: the lockup, "The video editor you talk to." very large, and `dijmo.com`, centred on Porcelain (see `docs/design.md`).
- **Header.**
  - The header's wordmark is inline SVG in `templates/base.html`, with no style attributes, as the CSP requires.
  - The letters use `currentColor`, so they follow the theme; the caret stays red.
  - The SVG is `aria-hidden`, and the link keeps its accessible name, "Dijmo home".

## Rebuilding

From the repo root:

```
python3 docs/brand/final/source/build_final.py            # brand, icons and site
python3 docs/brand/final/source/build_final.py icons      # just one part: brand, icons or site
```

- **Needs:**
  - Python with fontTools (and brotli) and Pillow.
  - `rsvg-convert` and `iconutil` on the PATH.
- **The header SVG** is written to `source/header.svg.html`. Paste it into `templates/base.html` if the wordmark changes.
