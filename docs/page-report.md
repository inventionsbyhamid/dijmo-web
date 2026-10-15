# dijmo-web: landing page report (2026-10-15)

The page builder's report: what was built, the final copy, sound, haptics, measured weight and what's left. The design plan and the sound sources are in `docs/design.md`.

## Status
- **Built:** all five pages and the shared partial `templates/partials/scene.html` (the hero's video scene), plus `site.css`, `site.js`, the font, the sound sprite, the favicon, the touch icon and the OG card. Everything follows `docs/contract.md`: only the contract's fields, no inline scripts or styles, `data-*` hooks.
- **How it was checked:** I rendered the templates with Jinja (after converting Askama's `else if` and `||`), served them behind the exact CSP, and drove headless Chrome. I checked desktop (1024–1920), phones (375 and 390), dark mode, reduced motion, no JavaScript, the form's JSON path (invalid, sent) and the sound toggle.
  - The console was clean and there was no CSP violation.
  - Measured layout shift was 0. There's no horizontal scroll at any size.
  - Lighthouse itself was not run.

## The design
- **One idea:** the caret is the playhead. A request types itself under a video frame. On "Enter", the red caret's twin sweeps the frame as a playhead, and the edit lands behind it.
- **The hero's edits, in order:**
  1. Shutters crop the frame to 9:16, and the cropped edges stay faintly visible.
  2. Captions pop in word by word.
  3. The pause in the clip's waveform turns red, then closes up.
  4. A warm grade wipes in behind the playhead.
  5. "undo all" runs the playhead backwards, and everything returns.
- **Ground and type:** a cool Porcelain ground (`#F2F3F1`) with a red caret, or true-black Stage with a white caret under `prefers-color-scheme: dark`. One typeface, Schibsted Grotesk.
- **Layout:** one left axis for the whole page, because typed text grows from a fixed left edge. Colour only comes from the video.
- **Motion:** one moment per section, each tied to what its sentence says.
  - **Problem:** the jargon drops into a pile.
  - **Turn:** the question types itself, then lands with a thock.
  - **Record:** four takes stacked as one separate onto their own tracks.
  - **Say it:** a request previews its cuts in red; "Apply" closes them and "Undo" appears.
  - **Post it:** one frame fans out into vertical, square and wide.
  - **Promise:** two lines fade in slowly; "It opens instantly." appears instantly.
  - **Outcome:** the playhead runs a hairline to its end.
  - **Close:** Enter thocks and the rail fills.
- **Motion rules:** only `transform` and `opacity` animate. The hero loop pauses off-screen and in hidden tabs, and has a visible Pause button. Reduced motion shows the end states with no typing.

## Final copy
**Hero**
- Say what you want, and it’s done.
- Dijmo is a video editor you talk to, made for people who aren’t editors.
- Join the waitlist
- *(The demo types these requests: make it vertical for tiktok / add captions / cut the long pause / warmer, like film / undo all.)*

**Problem**
- You have something worth showing.
- Then the software asks you to become an editor first.
- Timelines. Keyframes. Export settings. Hours.
- So most videos never get made.

**Turn**
- What if editing were as simple as saying it?

**Product:** "Meet Dijmo."
- **Record.** Screen, camera, game and voice. Each stays on its own track, so you can change it later.
- **Say it.** Type what you want, in your own words. You see every edit before it happens, and you can always undo it.
- **Post it.** Vertical, square and wide, all from one project. Ready for TikTok, Instagram and YouTube.

**Promise**
- Everything runs on your computer. Even the AI.
- Your videos never leave it.
- It opens instantly.
- And it stays smooth, even on older Macs.

**Outcome**
- Your ideas, finished.
- The videos you meant to make.

**Close**
- Be the first to try it.
- The field is labelled "Your email", with the placeholder "you@example.com". The button is "Join the waitlist".
- Under the field: "We’ll send one link to confirm. You can leave any time."
- **Status messages** (identical with and without JS):
  - Sent: "Check your inbox. Open the link we sent to finish joining."
  - Invalid: "That email doesn’t look right. Check it and try again."
  - Slow down: "Too many tries in a short time. Wait a few minutes, then try again."
  - JS only, server error: "Something went wrong on our side. Try again in a moment."
  - JS only, network failure: "That didn’t go through. Check your connection and try again."

**Footer (every page):** Coming to Mac first, then Windows and Linux. Links: Home, Privacy.

**Confirm page**
- **Confirmed:** "You’re on the list." / "Thank you. We’ll write when Dijmo is ready for you to try." A small celebratory edit plays: the playhead sweeps and the caption "you’re in." pops in.
- **Already:** "You’re already on the list." / "Nothing more to do. We’ll write when Dijmo is ready for you to try."
- **Expired:** "This link has expired." / "Links last 72 hours. Join again and we’ll send you a new one." Link: "Join again".
- **Invalid:** "This link doesn’t work." / "It may have been cut short. Copy the whole link from the email, or join again."

**Leave page**
- **Left:** "You’ve left the waitlist." / "We’ve removed your email. You won’t hear from us again." Link: "Changed your mind? Join again".
- **Invalid:** "This link doesn’t work." / "It may have been cut short. Copy the whole link from the email and try again."

**Privacy page:** "Privacy". It covers what we keep, why, who else sees it, what we don’t do, how long we keep it, how to leave, and questions, all in plain words. See `templates/privacy.html`.

**Error page:** the server's `heading` and `body` under SMPTE colour bars showing the `code`, with "Back to the home page".

**Suggested server strings** (`title` and `description` are set by the server):
- **Title:** "Dijmo: say what you want, and it’s done"
- **Description:** "Dijmo is a video editor you talk to. It runs on your computer, and your videos never leave it. Coming to Mac first, then Windows and Linux."

## Sound design
- **Sounds:**
  - eight soft keystrokes, with a random variant each time (never the same one twice in a row), pitch ±4 % and gain ±15 %;
  - a deep thock on Enter;
  - a faint projector tick when an edit lands.
  - There is no music and no room tone.
- **Volume:** the demo types more quietly (0.4) than the visitor's own keys (1.0).
- **Off by default.** The "Sound" button (`aria-pressed`) turns it on.
  - The choice is remembered in `localStorage` (`dijmo:sound`, in try/catch).
  - Nothing is fetched until sound is on. If it was on last time, the sprite is prefetched at idle and decoded on the first gesture.
- **Behaviour:**
  - `navigator.audioSession.type = "ambient"` follows the iPhone mute switch.
  - At most 15 voices play at once.
  - Opus WebM is used where `canPlayType` allows, and AAC otherwise, falling back to AAC if WebM won't decode, and to a Web Audio synth if neither will.
  - The script measures the decoded onset to correct for AAC encoder delay.
- **Checked in Chrome:** both files decode, with an onset shift of 0. The toggle fetches 8.5 KB, and the choice survives a reload.
- **Sources and licence:** see "Sound sources" in `docs/design.md`. ElevenLabs commercial use needs a paid plan.

## Haptics
- **Android (`navigator.vibrate`):**
  - 5 ms per key the visitor types;
  - 18 ms on Enter;
  - a 3 ms detent when each section's moment fires.
  - It only vibrates after the visitor has touched the page, so there are no blocked-call warnings.
- **iPhone:** on Enter, a hidden `<input type=checkbox switch>` is toggled through its label, which plays the native haptic (iOS 18+). This is only used where `vibrate` is missing and `switch` is supported.
- **Never:** the self-typing demo never buzzes, and everything is off under `prefers-reduced-motion`.

## Page weight (measured)
First visit, before optional media. HTML is the rendered index.

| File | Raw (B) | gzip 6 (B) | brotli q4 (B) |
|---|---:|---:|---:|
| index.html | 12,445 | 3,989 | 3,944 |
| site.css | 30,250 | 7,649 | 7,969 |
| site.js | 21,901 | 6,892 | 7,132 |
| schibsted-grotesk.woff2 (Latin subset, wght 400–700) | 23,748 | 23,748 | 23,748 |
| icon.svg | 349 | 241 | 221 |
| **Total** | **88,693** | **42,519** | **43,014** |

- **Against the budget:** about 42 KB transferred and 89 KB uncompressed, against 150 KB. JavaScript is 21.9 KB raw (6.9 KB gzip), against 25 KB.
- **Optional (opt-in or never fetched by the page):**
  - `sfx.webm`: 8,241 B; `sfx.m4a`: 13,640 B. Together that's 21.9 KB, under the 30 KB budget.
  - `og.png`: 69,542 B. `apple-touch-icon.png`: 2,246 B.
- **The server's budget test** (`crates/web/tests/budget.rs`) counts HTML, CSS, JS, fonts, icon and the larger sound uncompressed. By that count it's about 102 KB.

## To-dos and concerns
1. **Listen to the sounds.** I picked them by waveform and spectrum analysis, not by ear. The owner should approve or re-roll them; the generation script is quick to re-run.
2. **ElevenLabs plan.** Commercial use of the generated sounds needs a paid ElevenLabs plan.
3. **Test on real devices.** The iOS switch haptic, Safari's WebM/AAC choice and onset correction, and Android vibration were not tested on a phone. Only headless Chrome was used.
4. **Privacy promises must stay true:**
   - no cookies;
   - no analytics;
   - no tracking pixels;
   - Resend is the only processor;
   - "Reply to any email from us, and a person will answer" means `EMAIL_FROM` must accept and route replies.
   - A contact address would be better later.
5. **Lighthouse** was not run (it's not installed). The structure targets 100: one font file preloaded, no layout shift measured, AA contrast, labels and a skip link.
6. **The JSON body** sends `t` as a string, exactly like the form. The server's `JsonJoin.t` takes a `serde_json::Value`, so both work.
7. **Partial template.** `templates/partials/scene.html` is included by `index.html` and `confirm.html`. It needs no struct; the server's `askama.toml` `dirs` already covers it.
8. **Later:**
   - a short real video loop could replace the drawn scene (it would be optional media);
   - translations are out of scope (the site is English-only for now).
