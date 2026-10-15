# Dijmo logo concepts

There are four directions for the new logo. Open `index.html` to compare them side by side: mark, wordmark, app icon, 16 px loupes, the Dock, Finder and a browser tab, in light and dark.

Nothing here touches the live site. `assets/img/icon.svg` and the templates stay as they are until a direction is chosen.

## The four directions

| Direction | Idea | Weak spot |
|---|---|---|
| **Caret** | The d's stem is the caret: cut free of its bowl, red, and running a little under the baseline the way a playhead crosses a track. | A letter with one coloured stroke is a familiar monogram move. The idea lives in the colour and the cut more than in the silhouette. |
| **Selection** | The name arrives already selected, with highlight and handles, because in Dijmo selecting words is selecting the video. | It needs three colours and some fuss. At 16 px it is a d on a pink tile, and it reads as interface before brand. |
| **Cut** | The record dot, cut once at the caret. The take and the edit are one shape, and the cut leaves a D. | Split circles are common, and it says record and cut rather than words. The wordmark's o reads as a dot first. |
| **Thock** | A porcelain keycap whose legend is the caret: the site's thock made into an object, at home on a gamer's desk. | It says keyboard before video. Without the material icon, the flat mark is a rounded square anyone could own. |

**Recommendation: Caret.** It is the site's own idea, made into a letter. It is the quietest of the four and the clearest at 16 px. It also keeps its meaning in one colour, because the cut, not the red, separates the caret from the bowl.

## Files

Each direction folder (`caret/`, `selection/`, `cut/`, `thock/`) holds:

- `mark-light.svg`, `mark-dark.svg`, `mark-mono.svg`: the symbol, tight to its bounds, on a transparent ground.
  - `light` is for Porcelain.
  - `dark` is for Stage black.
  - `mono` is one colour (ink), with every idea carried by shape alone.
- `wordmark-light.svg`, `wordmark-dark.svg`, `wordmark-mono.svg`: "dijmo" as outlines.
- `app-icon-light.svg`, `app-icon-dark.svg`: 1024 × 1024 macOS icons.
- `favicon.svg`: a 32-unit grid drawn for 16 px. The key edges land on whole pixels.
- `png/`: renders at 16, 32, 128 and 1024 of the icons and marks, favicons at 16, 32 and 64, and the wordmarks.
- `preview.png`: one proof sheet with light, dark, one colour and 16 px loupes.

## Craft notes

- **Letterforms.** All letterforms come from Schibsted Grotesk (OFL), the site's typeface, as outlines via fontTools. No SVG references a font.
  - The Caret bowl is the font's own o, clipped flat where the stem would join. The caret is exactly one stem wide.
- **Icon shape.** The icon body is Apple's grid: an 824 px continuous-corner squircle at 100 px inside the 1024 canvas.
  - The shape was checked against the mask of the system's own TextEdit and Calculator icons. They match to the anti-aliased edge.
  - The drop shadow is close to Apple's template: 10 px down, soft, about 28% black.
- **Palette.** The palette is the site's: Porcelain `#F2F3F1`, ink `#17181A`, caret red `#E0301A` and Stage `#000`. There are two additions:
  - Selection's highlight is a pale red (`#F2C4BC`), and `#5E1E15` on Stage. That is what macOS draws for a selection when the accent colour is red.
  - Thock's icon uses soft lighting for its material: a concave dish and a deep front skirt. Its flat mark and favicon are plain.
- **Small sizes.** Every mark was rendered at 16, 32, 128 and 1024 and checked at true pixels, in colour and in one colour.
  - Favicons are drawn separately from the 1024 art, so strokes stay whole pixels at 16 px.

## Rebuilding

From this folder:

```
python3 source/build.py .
```

- **Needs:** Python with fontTools (and brotli for woff2) and Pillow, plus `rsvg-convert` on the PATH.
- **The script:** `source/build.py` holds every dimension, so a chosen direction can be tuned there and rebuilt in one step.
- **Page font:** `index.html` inlines a subset of Schibsted Grotesk, so it also renders correctly when opened straight from disk.
