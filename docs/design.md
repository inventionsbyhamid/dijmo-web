# dijmo-web: landing page design (v2, "The words are the edit")

**Owner direction:** classy, inspirational, minimal, haptic, ASMR, keystrokes. Sell the vision like a keynote.

**v1 was rejected.** Its drawn "video frame" pretended to be footage. v2 drops every faux-video element.

**The concept:** pure typography, sound and touch, and honest about the product. Dijmo's Script view edits a video by editing its words, and auto-polish cuts fillers and pauses. The caret is still the playhead.

## Palette (unchanged)
| Token | Porcelain (light, default) | Stage (dark) | Use |
|---|---|---|---|
| `--ground` | `#F2F3F1` | `#000000` | page |
| `--ink` | `#17181A` | `#F2F2EF` | text |
| `--quiet` | `#5E6166` (5.6:1) | `#8F8F8A` (6.4:1) | secondary text, fillers' gaps |
| `--line` | `#D9DBD8` | `#262626` | hairlines |
| `--caret` | `#E0301A` | `#F2F2EF` | caret, strike, record dot, focus |

There are no images and no colour other than these. Red appears only as the caret, the strike through a cut, the record dot and the caption underline.

## Type
- **One family:** Schibsted Grotesk, self-hosted (OFL).
- **Statements:** 32–60 px at weight 500.
- **The transcript:** a reading size, 20–30 px at weight 400, 1.55 leading, a measure of at most 60 ch.
- **Times:** tabular figures, in the same face; no monospace.
- **No tells:** no eyebrows, no capitals, no accent word.

## Layout
- **One left axis.**
- **The hero:** the headline, then the transcript paragraph with its running time at top right (the only "video" on the page), then the command line.

```
dijmo                                                   Sound off
Say what you want,                   Dijmo is a video editor you
and it's done.                       talk to. Join the waitlist
────────────────────────────────────────────────────── 1:12
Okay, so, um, this is the trick my grandmother taught me. — 8s —
You salt the onions first, like, before the pan is even hot. And
uh, they go soft and sweet, you know, instead of burnt. — 9s — …
cut the ums▌                                       Pause the demo
```

- **Below the hero,** one idea per screen: problem, turn, the three beats (art to the right, type only), promise, outcome, close.

## Motion: one orchestrated moment per section
1. **Hero:** commands type themselves; each lands with the thock.
   1. "cut the ums": each filler is struck in red and fades with a tick, then the paragraph reflows. The duration falls 1:12 → 1:06.
   2. "remove the long pauses": the gap marks snap shut with soft clicks (→ 0:34).
   3. "make it 30 seconds": the two weakest sentences fade and fold away (→ 0:28).
   4. "add captions": the words light up at speaking rhythm, with a red underline sweeping word to word.
   5. "undo all" restores the raw take, and it loops.
   - **How the reflow works:** a hidden copy of the paragraph is laid out normally; the visible words are absolutely placed and glide to their new places with `transform`. Layout never shifts. The paragraph keeps its raw height.
   - **The duration line** shrinks with the time, and the time rides its end.
2. **Problem:**
   - The jargon stacks (Keyframes, Bitrate, Render queue, Nested sequences, LUTs). Then "So most videos never get made."
   - Then a command types "make it simple": the pile is struck line by line and folds away, as a first glimpse of the answer.
3. **Turn:** the question types itself and lands with a thock.
4. **Record:** "Screen. Camera. Game. Voice." Each word arms: its ring fills with a red dot, with a relay click.
5. **Say it:** a request types; a one-line preview appears; Apply is pressed; Undo is offered.
6. **Post it:** the same caption sentence sets itself into three outlined frames, 9:16, 1:1 and 16:9 (outlines and type only).
7. **Promise:** "It opens instantly." appears instantly, after two slow lines.
8. **Outcome:** a hairline runs to its end.
9. **Close:** the waitlist line, with its keystrokes, thock and tap.

**Easing:** `cubic-bezier(.7,0,.15,1)` for edits, `cubic-bezier(.2,.7,.2,1)` for reveals.

**Fallbacks:** with reduced motion or without JS, the hero shows the raw take (1:12) and the edited take (0:28) as two static paragraphs, and every section shows its end state.

## Sound and touch
- **Sound:** the same sprite, off until turned on.
  - The demo types softly.
  - Cuts tick, and gaps close with a quieter key click.
  - Enter thocks.
- **Haptics:**
  - The visitor's own keys tap 5 ms, Enter 18 ms (or the iPhone switch), and each section's detent 3 ms.
  - Demo cuts tap 4 ms, but only once the visitor has touched the page and turned sound on, so a silent visitor is never buzzed.
- **Only in view:** a demo's sounds belong to it. The hero's transcript and each story moment play their keys, ticks and thocks only while they are on screen and the tab is visible.
  - Scroll one out of view, or hide the tab, and its sounds stop at once (even a key already sounding) and its typing pauses, so sound and motion stay in step and nothing runs unseen.
  - Bring it back, and it carries on where it stopped. With sound off, it carries on silently.
  - One IntersectionObserver and `visibilitychange` drive this (`hold` and `live` in `site.js`). The visitor's own sounds, and the sound setting itself, are untouched.

## Review against the generic tells
- **Avoided:** cream and serif, acid-on-black, the broadsheet look, the card kit, eyebrows, middle dots, mono labels, arrows, and fade-up on everything.
- **The coordinator's preview line used a middle dot.** It's written as two short sentences instead: "Cuts 14 pauses. 0:41 shorter."
- **Kept on purpose:**
  - Hairlines appear only where they mean something: a clip's duration, a gap of silence, the outcome's run to the end.
  - Numbers appear only as the clip's own times and the demo's own preview, never as marketing claims.

## Sound sources
All three sounds come from the **ElevenLabs Sound Effects API** (`POST /v1/sound-generation`). I generated 11 candidates: three typing takes, two single keys, three thocks and three ticks.

I chose by analysis rather than by ear: onsets, tail length and spectral centroid. Soft keys sit at a 1.9–3.2 kHz centroid; the deep thock is about 630 Hz.

- **Keys, eight variants:** sliced from one typing take, so they share one keyboard's character.
  - Prompt: "Unhurried ASMR typing on a thocky custom mechanical keyboard, soft separate keystrokes with half-second pauses, close-miked, dry" (4 s, `prompt_influence` 0.7).
  - Processing: high-pass at 70 Hz, low-pass at 8.5 kHz, tail trimmed at −42 dB, loudness matched by RMS, peak ≤ −6 dBFS.
- **Thock:** the main hit of one take, 0.53–0.70 s.
  - Prompt: "One deep, satisfying thock of a large mechanical keyboard Enter key pressed firmly, close-miked ASMR, warm low end, dry studio, no reverb, single press" (0.8 s, influence 0.85).
  - Processing: high-pass at 35 Hz, low-pass at 9 kHz, peak −3 dBFS.
- **Projector tick:** a single click of one take, 0.17–0.24 s.
  - Prompt: "One faint single click of a vintage film projector shutter, very short dry tick, close-miked, quiet, no reverb" (0.5 s, influence 0.7).
  - Processing: high-pass at 900 Hz, low-pass at 12 kHz, peak −6 dBFS. It plays at about 0.4 gain.
- **Packaging:** one 2.0 s sprite with 120 ms gaps.
  - `sfx.webm`: Opus, 32 kb/s VBR, mono, 8,241 B.
  - `sfx.m4a`: AAC via AudioToolbox, 48 kb/s, mono, 13,640 B.
  - The script measures the first onset after decoding and corrects for any AAC encoder delay.
- **Fallback:** a Web Audio synth (filtered noise click plus sine body) if neither file decodes.
- **Licence:** ElevenLabs allows commercial use of generated sound effects only on a **paid plan**; the free tier is non-commercial and needs attribution. Confirm the account's plan before launch.

## Fonts
Schibsted Grotesk by Bakken & Bæck, SIL Open Font License 1.1. The licence is copied to `assets/fonts/OFL.txt`.

## The share card (Open Graph)
`assets/img/og.png` (1200 × 630) holds three things, centred on Porcelain:
- the Caret lockup;
- one very large line, "The video editor you talk to.", in Schibsted Grotesk 600, kerned from the font's own tables;
- `dijmo.com`, small, in the quiet grey.

Everything sits inside a centred 1000 × 500 safe area, so the centre crops of X, iMessage and WhatsApp keep it, and it still reads at about 300 px wide. Every letter is an outline, so the render needs no installed font. `og:image:alt` matches the line.

- **Source:** `docs/brand/og.svg`, written by `docs/brand/final/source/build_final.py`.
- **Rebuild:** `python3 docs/brand/final/source/build_final.py og` (from the repo root). It writes the SVG, renders it with `rsvg-convert -w 1200 -h 630 -b '#F2F3F1' docs/brand/og.svg -o assets/img/og.png`, then reduces the PNG to a 64-colour palette (about 16 KB).
