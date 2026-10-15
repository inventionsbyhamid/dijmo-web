# dijmo-web: landing page report (v2, 2026-10-15)

The page builder's report. v2 replaces v1's illustrated "video frame" with **"The words are the edit"**: pure typography, sound and touch. The plan is in `docs/design.md`.

## Status
- **Built:** all six templates, `site.css`, `site.js` (now loaded as an ES module), the font, the sound sprite, the favicon and the OG card.
- **Removed:** the scene partial and every faux-video element.
- **Contract:** followed throughout, including the new leave `confirm` state with its `token` field.
- **How it was checked:** I rendered the templates with Jinja (Askama's `else if` and `||` converted), served them behind the exact CSP, and drove headless Chrome. I checked:
  - 1280×720, 1440×900 and 1920×1080 on desktop, plus phones at 390×844 and 375×667;
  - dark mode, reduced motion and no JavaScript;
  - the waitlist JSON flow (invalid, then sent) and the sound toggle (opt-in fetch, remembered after a reload);
  - the confirm, leave-confirm, privacy and error pages.
- **Results:**
  - The console was clean, with no CSP violation and no horizontal scroll at any size.
  - Layout shift was 0.0001 over 30 seconds of the hero editing. That's the caret and the time digits; the reflow itself never shifts layout.
  - Lighthouse was not run.

## The design
- **The hero:** a creator's raw spoken transcript, a cooking tip of 50 words, with fillers and long pauses shown as quiet gap marks. Its running time, `1:12`, rides the end of a hairline at the top right; it's the only "video" on the page.
- **The command line** with the red caret types requests key by key, and each lands with a thock:
  1. "cut the ums": each filler is struck in red, with a tick and a 4 ms tap, then fades. The paragraph reflows and the time ticks down to 1:06.
  2. "remove the long pauses": the gap marks snap shut with soft clicks (0:34).
  3. "make it 30 seconds": the two weakest sentences fade and fold away (0:28).
  4. "add captions": the words light up at speaking rhythm, with a red underline walking word to word.
  5. "undo all" restores the raw take, and it loops.
  - **Controls:** a Pause button; the demo pauses off-screen and in hidden tabs.
- **How the reflow works:** a hidden copy of the paragraph is laid out normally; the visible words are absolutely placed over it and moved with `transform` only.
  - Words on the same line slide; a word that changes line fades out, moves and fades back in. Nothing crosses.
  - The paragraph keeps its raw height.
- **No motion or no JavaScript:** the raw take (1:12) and the edited take (0:28) show as two static paragraphs.
- **The other moments, one per section:**
  - **Problem:** the jargon stacks (Keyframes, Bitrate, Render queue, Nested sequences, LUTs), and "So most videos never get made." lands. Then a command types "make it simple": the pile is struck line by line and the section folds. The command stays, with its caret, as a first glimpse of the answer.
  - **Turn:** the question types itself and lands with a thock.
  - **Record:** "Screen. Camera. Game. Voice." Each ring fills with a red dot, with a relay click.
  - **Say it:** "cut the silences" types, then "Cuts 14 pauses. 0:41 shorter." appears. Apply is pressed and the line becomes "14 pauses cut. 0:41 shorter.", with Undo offered.
  - **Post it:** "Salt the onions first." sets itself into 9:16, 1:1 and 16:9 outlines.
  - **Promise:** "It opens instantly." appears instantly.
  - **Outcome:** a hairline runs to its end.
  - **Close:** the waitlist line (keystrokes, thock, tap).
- **Confirm page:** "You’re um, on the list." The "um," is struck and the line closes up. Screen readers hear only "You’re on the list."
- **Error page:** the code in large quiet figures with a caret.

## Final copy
**Hero**
- Say what you want, and it’s done.
- Dijmo is a video editor you talk to. Change the words, and the video follows.
- Join the waitlist
- **The transcript (raw, 1:12):** "Okay, so, um, this is the trick my grandmother taught me. — 8s — You salt the onions first, like, before the pan is even hot. And uh, they go soft and sweet, you know, instead of burnt. — 9s — I didn’t believe her for years. — 7s — So, um, anyway. — 8s — And honestly, like, you’ll never go back."
- **Edited (0:28):** "Okay, so, this is the trick my grandmother taught me. You salt the onions first, before the pan is even hot. And they go soft and sweet, instead of burnt. And honestly, you’ll never go back."

**Problem**
- You have something worth showing.
- Then the software asks you to become an editor first.
- Keyframes. Bitrate. Render queue. Nested sequences. LUTs.
- So most videos never get made.
- *(Command: make it simple)*

**Turn**
- What if editing were as simple as saying it?

**Product:** "Meet Dijmo."
- **Record.** Screen, camera, game and voice, each on its own track, so every part stays editable. *(Screen. Camera. Game. Voice.)*
- **Say it.** Ask in your own words. You see every edit before it happens, and you can always undo it.
- **Post it.** Vertical, square and wide, all from one project. Ready for TikTok, Instagram and YouTube.

**Promise**
- It runs on your computer, AI and all.
- Your videos never leave it.
- It opens instantly.
- And it stays smooth, even on older Macs.

**Outcome**
- Your ideas, finished.
- The videos you meant to make.

**Close**
- Be the first to try it.
- The field is labelled "Your email". The button is "Join the waitlist".
- Under the field: "We’ll send one link to confirm. You can leave any time."
- The status messages are unchanged from v1.

**Leave page**
- **Confirm (new):** "Leave the waitlist?" / "We’ll remove your email, and you won’t hear from us again."
  - Buttons: "Leave the waitlist" (it POSTs the token) and "Stay on the list".
- **Left and invalid:** as before.

**Privacy:** adds "…and we delete the signup after 30 days" for unconfirmed signups.

**Footer:** Coming to Mac first, then Windows and Linux.

## Sound and haptics
- **Sound:** the same sprite (ElevenLabs; sources in `docs/design.md`), off until turned on.
  - The demo types at 0.4 gain and lands with a 0.45 thock.
  - A cut filler ticks; a closing gap is a soft key click.
  - The visitor's own typing is full gain, and Enter is a full thock.
- **Haptics:**
  - The visitor's keys tap 5 ms, Enter 18 ms (or the iPhone switch haptic), and each section's detent 3 ms.
  - The demo's cuts tap 4 ms, as the ruling asked, but only once the visitor has touched the page and turned sound on. A visitor who just reads is never buzzed.
  - Everything is off under reduced motion.

## Page weight (measured)
First visit, before optional media. HTML is the rendered index.

| File | Raw (B) | gzip 6 (B) | brotli q4 (B) |
|---|---:|---:|---:|
| index.html | 11,334 | 3,323 | 3,273 |
| site.css | 28,186 | 6,779 | 7,099 |
| site.js | 24,505 | 8,311 | 8,593 |
| schibsted-grotesk.woff2 | 23,748 | 23,748 | 23,748 |
| icon.svg | 349 | 241 | 221 |
| **Total** | **88,122** | **42,402** | **42,934** |

- **Against the budget:** about 42 KB transferred and 88 KB uncompressed, against 150 KB.
- **JavaScript:** 24.5 KB raw against the 25 KB limit. That's tight; see the to-dos.
- **Optional:** `sfx.webm` is 8,241 B and `sfx.m4a` 13,640 B. `og.png` (typographic now) is 78,249 B; the page never fetches it.

## To-dos and concerns
1. **JS headroom is small:** 24.5 of 25 KB. Adding anything to `site.js` means removing something, or agreeing a build step that minifies.
2. **Unchanged from v1:**
   - The sounds were chosen by analysis, not by ear.
   - Commercial use needs a paid ElevenLabs plan.
   - Not tested on real phones (the iPhone switch haptic, Safari audio, Android vibration).
   - The privacy promises must stay true.
3. **A shared git index.** The server builder's commit `d19d4fd` swept up my staged deletion of `templates/partials/scene.html`. The result is right, but we share one index, so each of us should stage only our own paths.
