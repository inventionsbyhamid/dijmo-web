# dijmo-web: design spec (v1, 2026-10-15)

The website for Dijmo. Today it is a coming-soon page with a waitlist. Later the same server handles licences, purchases, accounts, desktop-app sign-in, updates and downloads, and, after the editor's M9, the plugin store.

It is its own git repository, pushed to its own GitHub repo. Coolify builds the `Dockerfile` and deploys on every push to `main`.

## What Dijmo is (for copy and design)
- **The product.** Dijmo is a fast, offline video editor for creators who aren't editors: people posting tutorials, social clips and gameplay to YouTube, Instagram and TikTok.
- **How you edit.** You can edit by saying what you want, in a chat inside the app: "make it vertical for TikTok", "cut the silences", "add captions". Every edit is shown before it is applied, and can be undone.
- **Recording.** It records screen, camera, game and mic, each kept separate so they stay editable.
- **Offline and private.** The AI runs on your computer. Your videos never leave it.
- **Instant.** It opens instantly and scrubs smoothly, even on older Macs.
- **Platforms.** Mac first, then Windows and Linux.
- **Built to grow.** Plugins and effects, an MCP server so other AI apps can edit with Dijmo, and a plugin store later.

## Owner's direction (2026-10-15)
- "really classy, and inspirational, like think from steve jobs perspective, highly ambitious, aesthetic, haptic, asmr"
- "minimalistic, aesthetic, haptics, asmr audio, keystrokes maybe"
- "sell the vision in steve jobs style, the goal, outcome, problem etc, make it really good with asmr audio and build final"
- "think long term, plan and build long term from a large video editor project perspective"
- The server is Rust; the UI is fast. The waitlist uses double opt-in. Postgres, not SQLite. Login comes later.

## Stack (decided)
- **Server:** Rust stable, Axum 0.8, Tokio, tower-http (compression br/gzip, static files with hashed names and long cache, security headers, request limits, tracing).
- **Templates:** Askama, checked at compile time. Pages are rendered on the server.
- **Database:** Postgres 16 via sqlx, using the Postgres feature and runtime queries checked by tests. Migrations live in `migrations/` and run at startup.
- **Front end:** hand-written CSS, plus a small amount of TypeScript compiled by `esbuild` (or plain ES modules if no build is needed). There is no front-end framework. htmx is allowed later for account pages.
- **Email:** an `EmailSender` trait.
  - **Production:** Resend's HTTP API (`RESEND_API_KEY`), with `EMAIL_FROM` for the sender address.
  - **Development:** `Log` writes the email to the log.
  - The provider is chosen by environment. Never commit keys.
- **Config:** read only from the environment (12-factor).
  - `DATABASE_URL`, `PUBLIC_BASE_URL`, `RESEND_API_KEY`, `EMAIL_FROM`, `PORT` (default 8080).
  - `SECRET_KEY`: 64+ random bytes, base64. It signs confirmation tokens and HMACs IP hashes.
  - `.env` is for local development only and is git-ignored. Ship a `.env.example`.

## Layout (long-term shape)
```
dijmo-web/
  Cargo.toml            workspace
  crates/
    web/                Axum app: routes, templates, static assets, middleware (the binary)
    core/               domain + persistence: waitlist now; licences, accounts and purchases later as new modules
  migrations/           sqlx migrations (Postgres)
  templates/            Askama: base.html, index.html, confirm.html, privacy.html, errors
  assets/               css/, ts/ (or js/), sounds/ (only if samples are used), fonts/, img/
  Dockerfile            multi-stage: build with cargo-chef caching, then a distroless/debian-slim runtime as non-root
  .env.example, README.md, docs/spec.md
```

**Conventions for later pieces:**
- Public JSON APIs live under `/api/v1/…`.
- Webhooks live under `/hooks/<provider>`.
- Account pages live under `/account/…`.
- Tables are named in plural snake_case, with `created_at` / `updated_at` as timestamptz.
- Emails are stored lower-cased in a `citext` column.

## The waitlist (double opt-in)
1. `POST /waitlist` takes an email, a honeypot field and a timestamp field. It works as a plain form post and also as a fetch from JavaScript (JSON in and out). Its answer is the same whether or not the address is already known, so nobody can probe whether an address is on the list.
2. The server stores a pending signup with:
   - the email;
   - a SHA-256 of the confirmation token (never the token itself);
   - the created time;
   - the source (UTM parameters, if present);
   - a coarse IP hash (HMAC with the secret, not the raw IP).

   It then sends a confirmation email with a single link: `/waitlist/confirm?token=…`.
3. **The token:** 32 random bytes, base64url, expiring after 72 hours. Confirming is idempotent. Confirmed addresses get `confirmed_at`.
4. **Unsubscribe:** an unsubscribe link (`/waitlist/leave?token=…`), sent in the confirmation email and every later email, removes the signup.
5. **Rate limits:** per IP hash and per email, in memory with a bounded map (a shared store comes when there is more than one instance). Re-sends to the same address are limited to one per 10 minutes.
6. **Email:** plain, beautiful HTML plus a text version, in Dijmo's voice. There are no tracking pixels.

## Security and reliability
- **Headers:**
  - a strict Content-Security-Policy (self only; Google Fonts if used, otherwise self-hosted fonts are preferred);
  - HSTS;
  - `X-Content-Type-Options`;
  - `Referrer-Policy strict-origin-when-cross-origin`;
  - `Permissions-Policy`: camera, microphone and geolocation off.
- **Forms:** POSTs check Origin/Referer against `PUBLIC_BASE_URL`. There is no cookie-based state yet; CSRF tokens arrive with login.
- **Limits:** request body cap 16 KiB on forms. Timeouts on every handler.
- **Database and email:** sqlx pool timeouts. Email sending runs outside the request, using a database-backed outbox table plus a worker with retries, so a provider outage never loses a signup or fails the page.
- **Health and shutdown:** `/healthz` (process up) and `/readyz` (the database answers) for Coolify. The server shuts down gracefully on SIGTERM.
- **Logs:** structured, with no emails or IPs in plain text.

## The landing page: "The Keystroke", grown into a story
**Direction (owner-approved spirit):** minimal, aesthetic, haptic, ASMR, keystrokes. One typeface, a quiet palette, one accent. The text caret is also the editor's playhead.

**Sell the vision the way a Jobs keynote does.** Use a few short sentences, each with room to breathe. Give the problem, then a turn, then the product, then the outcome. No feature grids, no "trusted by" logos, no stock gradients.

**Story arc** (suggested; the implementer writes the final copy in this spirit, short sentences in plain words):
1. **Hero.**
   - A single video frame and a line that types itself.
   - The frame re-edits as each request "lands": vertical, captions, cut the pause, warmer.
   - The headline is one short sentence about saying what you want and it's done.
2. **The problem.** You have something worth showing. Then the software asks you to become an editor first: timelines, keyframes, export settings, hours. Most videos never get made.
3. **The turn.** What if editing were as simple as saying it?
4. **The product, in three beats:**
   - **Record:** screen, camera, game, voice; each kept separate.
   - **Say it:** chat edits, shown before they happen and always undoable.
   - **Post it:** vertical, square and wide, from one project.
5. **The promise.** It runs on your computer. Your videos never leave it. It opens instantly.
6. **The outcome.** Your ideas, finished. The videos you meant to make.
7. **The close.** The waitlist line: type your email; Enter lands. Then "Coming to Mac first, then Windows and Linux."

**Audio (ASMR):**
- Synthesised with Web Audio, or tiny samples (≤ 30 KB total): soft close-miked keystrokes, each one slightly varied, never a loop.
- A low "thock" on Enter, and a faint film tick when the frame re-edits.
- A very quiet room tone is optional; there is never music.
- Sound is OFF until the visitor turns it on. It's remembered per device (localStorage, wrapped in try/catch). It respects the OS mute; there is no autoplay.

**Haptics:**
- Android: `navigator.vibrate` — 5 ms per key, 18 ms on Enter, a 3 ms detent per story section.
- iPhone: Safari has no vibration API, so Enter uses the native `<input type="checkbox" switch>` haptic where available (iOS 17.4+). Everything else is carried by motion and sound.
- Haptics are off when `prefers-reduced-motion` is set.

**Motion:** one orchestrated moment per section, at most. Scroll-linked reveals are subtle, and everything respects `prefers-reduced-motion`. 60 fps: only transform and opacity animate. Nothing moves that the story doesn't need.

**Performance:**
- The hero is interactive within 1 s on 4G.
- Total transfer is under 150 KB before any optional video: fonts subset and self-hosted (woff2), CSS inline or one file, JavaScript ≤ 25 KB.
- No layout shift. Lighthouse 100 on performance, accessibility and best practices is the target.

**Accessibility:**
- Works fully without JavaScript: the form posts and the story is plain HTML.
- Keyboard and screen-reader friendly. The self-typing demo is `aria-hidden`, with a static description.
- Visible focus, AA contrast.

**Light and dark:** the light "Porcelain" ground with a red caret is the default; a dark "Stage" ground (true black, white caret) follows `prefers-color-scheme`. Colour comes from the video frame. The visual direction can be refined as long as it stays minimal, classy and distinctive.

**Pages:**
- `/` (the story and waitlist);
- `/waitlist/confirm` (a thank-you with a small celebratory edit);
- `/waitlist/leave`;
- `/privacy` (a plain-language privacy note for the waitlist);
- 404 and 500 pages in the same voice.

**SEO and social:** title, description, Open Graph and Twitter cards (with a generated OG image or a static one), `robots.txt`, `sitemap.xml`, a favicon from `docs/dijmo-icon.svg`.

## Testing (lean)
Integration tests against a real Postgres (`DATABASE_URL` from the test environment; locally that's Homebrew Postgres 16):
- the waitlist flow: signup → outbox → confirm → leave;
- duplicates;
- an expired token;
- rate limits;
- the honeypot;
- the security headers.

Plus:
- A test that every template renders.
- A script or test that checks the page weight budget.

## Out of scope now (later specs)
Licences and activation, purchases (provider and webhooks), accounts (email-link and passkey login, desktop-app sign-in via OAuth PKCE), updates and downloads, the plugin store, and analytics (if ever, privacy-friendly and cookieless).
