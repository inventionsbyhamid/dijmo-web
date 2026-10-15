# dijmo-web: landing page design plan

Owner direction: classy, inspirational, minimal, haptic, ASMR, keystrokes. Sell the vision like a keynote: problem, turn, product, outcome.

**Subject:** a video editor you talk to, for creators who aren't editors.

**The one memorable thing:** the text caret is the playhead. Everything else stays quiet.

## Palette
| Token | Porcelain (light, default) | Stage (dark, `prefers-color-scheme`) | Use |
|---|---|---|---|
| `--ground` | `#F2F3F1` (cool porcelain, not cream) | `#000000` (true black) | page |
| `--ink` | `#17181A` | `#F2F2EF` | text |
| `--quiet` | `#5E6166` (5.6:1) | `#8F8F8A` (6.4:1) | secondary text |
| `--line` | `#D9DBD8` | `#262626` | hairlines, tracks |
| `--caret` | `#E0301A` (red caret) | `#F2F2EF` (white caret) | caret, playhead, focus ring |

The only other colour on the page comes from the video frame: a coast at golden hour, drawn in SVG. In Stage, the frame is the only colour on the page.

## Type
- **One family:** Schibsted Grotesk, the variable font limited to weights 400–700, Latin subset, self-hosted woff2 (SIL OFL 1.1).
  - It was chosen for the approved prototype. It's a newsroom grotesk: sober, slightly condensed, and warm without being cute.
- **Scale:** perfect fourth (1.333), fluid with `clamp()`. Body 18 px.
  - Statements are 42–88 px at weight 500, with −0.035 em tracking and 1.02 leading.
  - Quiet lines are 18–24 px at weight 400.
- **Type does the work:**
  - Sentences type themselves, letter by letter, with the caret leading.
  - There are no eyebrows and no all-caps labels.
  - No single word is accented in italic or colour.
  - Measure is ≤ 20 ch for statements and ≤ 60 ch for prose.

## Layout
- **One left axis for the whole page.** Text is left-aligned because typed text grows from a fixed left edge, and centred text would shuffle as it types.
- **Spacing:** the gutter is 16 px on phones and grows to 64 px. Sections are tall (most fill about 80–100 vh), so each sentence has room to breathe, like keynote slides.

```
dijmo                                             (( Sound off
                    ┌──────────────────────────────────┐
                    │  the frame: a clip that re-edits │   ← hero
                    └──────────────────────────────────┘
                    ▁▂▅▃▂▁▁▁▁▂▅▇▅▃  (its track)
                    make it vertical for tiktok▌
Say what you want,
and it's done.
A video editor you talk to.  Join the waitlist

You have something worth showing.                         ← problem
Then the software asks you to become an editor first.
Timelines. / Keyframes. / Export settings. / Hours.
So most videos never get made.

What if editing were as simple as saying it?▌             ← turn (types itself)

Meet Dijmo.                                               ← product
Record.   one line           [ 4 tracks separate      ]
Say it.   one line           [ request → preview → undo ]
Post it.  one line           [ 9:16  1:1  16:9 fan out ]

It runs on your computer. / Your videos never leave it. / It opens instantly.

──────────────────────────────▌ (playhead reaches the end)
Your ideas, finished. The videos you meant to make.

Be the first to try it.
you@example.com▌                                [ Join ⏎ ]
Coming to Mac first, then Windows and Linux.
```

On phones everything stacks in one column, and the beat diagrams sit under their lines.

## Motion
There is one orchestrated moment per section. They are triggered once by an IntersectionObserver and animate only `transform` and `opacity`.
1. **Hero:** a request types itself. On "Enter", the caret becomes the playhead and sweeps the frame.
   - The edit lands behind it: shutters crop to 9:16, captions pop in word by word, a pause in the track collapses, a warm grade wipes in.
   - Then "undo all" reverts everything, and the loop repeats.
   - The loop pauses off-screen and in hidden tabs, and has a visible Pause button (WCAG 2.2.2).
2. **Problem:** the four jargon nouns drop into a stack, one key-tick each.
3. **Turn:** the question types itself with the red caret.
4. **Product:**
   - *Record:* four merged tracks separate.
   - *Say it:* a request previews its cuts, then they collapse.
   - *Post it:* one frame fans into three shapes.
5. **Promise:** the first two lines fade in slowly. "It opens instantly." appears instantly; the joke is the point.
6. **Outcome:** the playhead runs along a hairline to its end.
7. **Close:** nothing moves until the visitor types. On Enter the line lands, the playhead sweeps, and you hear the thock.

Easing: `cubic-bezier(.7,0,.15,1)` for edits (an editor's snap) and `cubic-bezier(.2,.7,.2,1)` for reveals.

With `prefers-reduced-motion`, there is no typing, no loop and no haptics; the final states show at once. Without JavaScript, everything is visible and the form posts.

## Sound (ASMR, off by default)
- **Sounds:**
  - six to eight soft, close-miked keystrokes (a random variant each time, with ±4 % pitch and ±15 % gain);
  - a deep thock on Enter;
  - a faint projector tick when the frame re-edits.
- **Format:** one sprite per format (`assets/sounds/sfx.webm` Opus and `sfx.m4a` AAC), fetched only after the visitor turns sound on.
- **Fallback:** a Web Audio synth if decoding fails.
- **Rules:**
  - no music and no autoplay;
  - `navigator.audioSession.type = "ambient"` respects the iPhone mute switch;
  - the choice is remembered in localStorage (in try/catch).
- **Sources:** see "Sound sources" below.

## Haptics
- **Android (`navigator.vibrate`):** 5 ms per typed key, 18 ms on Enter, and a 3 ms detent when a section's moment fires.
- **iPhone:** on Enter, a hidden `<input type=checkbox switch>` is toggled through its label, which plays the native haptic (iOS 18+).
- Only the visitor's own actions buzz; the self-typing demo never does. Haptics are off under reduced motion.

## Copy rules
- Short sentences, plain words, sentence case.
- No hype words, numbers, dates or testimonials.
- Platforms: "Coming to Mac first, then Windows and Linux."

## Review against the generic tells
- **Cream and serif with a terracotta accent:** avoided. The ground is a cool porcelain grey with a grotesk; red appears only as the caret, never as decoration.
- **Near-black with an acid accent:** Stage is true `#000` with a white caret, the spec's own choice. Colour only comes from the video.
- **Broadsheet hairlines and the SaaS card kit:** there are no cards, no shadows and no gradient washes. The only hairlines are tracks and the playhead's rail, which are things an editor really has.
- **Eyebrows, middle-dot meta, mono labels, "→" links:** none. The three product beats are a real sequence (record → say → post), but they are named by verbs, not "01/02/03".
- **Fade-up on every section:** replaced by one specific moment per section, each tied to what the sentence says.
- **What I changed after review:**
  - The first sketch centred everything like keynote slides. I changed it to a left axis, because centred typing jitters.
  - I dropped a scroll-progress bar: the playhead appears only where it means something.

## Sound sources
(Filled in once the sounds are made: generator, prompt and licence note.)

## Fonts
Schibsted Grotesk by Bakken & Bæck, SIL Open Font License 1.1. The licence is copied to `assets/fonts/OFL.txt`.
