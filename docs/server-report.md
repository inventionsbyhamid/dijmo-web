# Server report (2026-10-15)

The Rust server for dijmo-web is built to `docs/spec.md` and `docs/contract.md`. The contract file is unchanged.

## What's there
- **`crates/core`**: domain and persistence.
  - Config from the environment, with secrets redacted from `Debug`.
  - Keys derived from `SECRET_KEY`.
  - Email normalisation.
  - The waitlist: join, confirm, leave and prune.
  - The email outbox: leases, backoff and prune.
- **`crates/web`**: the binary.
  - Axum routes per the contract, and the Askama page structs with the contract's exact field names.
  - Hashed asset serving.
  - Security headers, the Origin check, the client IP behind proxies, and GCRA rate limits.
  - The `EmailSender` trait, with `Log`, `Resend` and an in-memory sender for tests, plus the outbox worker.
  - `--healthcheck`, graceful shutdown, and a retry while connecting to the database.
- **`migrations/`**:
  - `waitlist_signups`: `citext` email, the SHA-256 of the token, `confirmed_at`, `last_enqueued_at`, the `source` jsonb and `ip_hash`.
  - `email_outbox`.
- **Build files:** `Dockerfile` (cargo-chef, then distroless `cc-debian12:nonroot`, plus `HEALTHCHECK`), `.dockerignore`, `.env.example` and `README.md`, which has the Coolify and Resend steps.

## Contract compliance
- **Templates:** `index`, `confirm`, `leave`, `privacy` and `error` structs, each with `title`, `description`, `canonical`, `base_url`, `css`, `js` and `og_image`, plus the extra fields in the contract table.
  - Askama's escaping is on, and nothing uses `|safe`.
  - Index title and description are as the page builder asked.
- **`POST /waitlist`:**
  - Form: `303` to `/?status=sent|invalid|slow_down#join`. On `invalid`, the typed email is passed back as `&email=` so the index can re-fill it.
  - JSON: `200 {"ok":true}`, `422 {"ok":false,"error":"invalid"}` or `429 {"ok":false,"error":"slow_down"}`.
  - A filled honeypot, or a post less than 1.5 s after render, gets the success answer silently. A missing `t` counts as too fast.
- **Assets:**
  - `site.css` and `site.js` are hashed at startup (8 hex chars of SHA-256) and served at `/assets/site.<hash>.<ext>` with `public, max-age=31536000, immutable`.
  - The rest of `assets/` is served from disk with `public, max-age=86400`. Range requests work, which Safari needs for audio.
- **CSP:** the contract's string, exactly, on every response.
- **Routes:** all of them. 404s, 405s, 413s, timeouts and panics render `error.html`, or JSON when the request wants JSON.

**Additions (outside the contract, so the page needs nothing new):**
- RFC 8058 one-click unsubscribe: a POST to `/waitlist/leave?token=` with `List-Unsubscribe=One-Click`. Emails carry `List-Unsubscribe` and `List-Unsubscribe-Post`.
- `GET /waitlist` answers `303` to `/#join`.
- A cross-origin POST gets `403`: the error page, or `{"ok":false,"error":"forbidden"}` for JSON.
- Status codes on the outcome pages: confirm answers `200` for confirmed or already, `410` for expired, `400` for invalid. Leave answers `200` for left, `400` for invalid. The page renders in every case.

## Design decisions
- **The token is minted when the email is sent, not when the request arrives.**
  - The worker generates the 32 random bytes and stores only their SHA-256, just before it sends. The raw token never touches the database, and the 72 h expiry starts when the email goes out.
  - A re-send after the 10-minute cooldown replaces the link, so only the newest link works.
  - Unsent queued mail for the same signup is dropped, so a recovered provider doesn't send a burst.
- **Leave links** are `base64url(signup id ‖ HMAC(id)[..16])`, keyed by a subkey of `SECRET_KEY`. Every later email can carry one without storing anything. Rotating `SECRET_KEY` breaks the leave links already sent.
- **Duplicates:** known, cooling-down and confirmed addresses all get the same answer. A confirmed address gets no new email.
- **Rate limits** (in memory, bounded at 50k keys, keyed by HMAC so no raw IPs or emails are held):
  - per IP (IPv4 address or IPv6 /64): 8 at once, then 1 a minute;
  - per email: 3 at once, then 1 every 20 minutes;
  - the 10-minute re-send cooldown is in the database.
- **IP hash stored with a signup:** HMAC of IPv4 /24 or IPv6 /48, 16 bytes.
- **Client IP:** `PROXY_HOPS` (the image sets 1 for Coolify) picks the Nth address from the right of `X-Forwarded-For`.
- **Outbox:**
  - Claims use `FOR UPDATE SKIP LOCKED` with a 120 s lease, so it's safe across several instances and across crashes.
  - Backoff runs from 15 s, doubling, capped at 1 h, for 20 attempts (about 12 h).
  - Resend's 401, 403, 408, 409, 429 and 5xx are retried, because a bad key or an unverified domain can be fixed. Other 4xx errors are permanent.
  - The worker wakes immediately on a new signup; otherwise it polls every 5 s.
- **Housekeeping (hourly):**
  - Unconfirmed signups with no request for 30 days are deleted.
  - Sent and failed outbox rows older than 30 days are deleted.
- **Logs:**
  - Spans carry the method and path only, never the query, where the tokens live.
  - Emails appear masked (`j***@example.com`). Provider errors are scrubbed of addresses. IPs are never logged.
  - The `Log` sender prints the text body so the links can be clicked in development. It warns at startup if the site is https and the sender is `log`.
- **Limits:**
  - Bodies are capped at 16 KiB (tower-http and axum).
  - Handlers time out after 10 s (`503`).
  - Getting a database connection times out after 5 s; statements after 10 s, and locks after 5 s.
  - Graceful shutdown is capped at 20 s, then the worker gets 10 s.
- **Toolchain:** sqlx 0.8.6, because 0.9 needs Rust 1.94 and the Mac has 1.90. The Docker image pins `rust:1.90`.

## Tests (44, against `dijmo_web_test`)
- **core: 12 unit tests** covering config, keys, email parsing, UTM and backoff.
- **core: 4 database tests** covering join → issue → confirm → leave, duplicates and the cooldown, expired and replaced tokens, and outbox claim, retry, send and prune.
- **web: 10 unit tests** covering assets, the rate limiter, headers, the client IP, the Origin check, and the email content.
- **web: 17 HTTP tests** (in-process):
  - signup → outbox → confirm → leave;
  - duplicates get the same answer and one email;
  - a re-send replaces the link;
  - an expired token, and forged tokens;
  - one-click unsubscribe;
  - an invalid email, and escaped re-fill;
  - the honeypot and timing check;
  - per-IP and per-email limits;
  - Origin and Referer, plus UTM capture;
  - the body cap;
  - security headers on every kind of response;
  - every page and outcome renders through the real templates and obeys the CSP;
  - JSON errors, 405 and the redirect;
  - hashed assets, caching and traversal;
  - health, robots and the sitemap.
- **web: 1 budget test** against the real `templates/` and `assets/`:
  - The first visit (HTML, CSS, JS, font, favicon and one prefetched sound) is 99.3 KB uncompressed, against a 150 KB budget.
  - `site.js` is 24,504 B, against 25 KB (25,600 B). That's close: the page builder has about 1 KB of headroom.
  - The sounds total 21.4 KB, against 30 KB.
  - Every `/assets/…` reference in the templates, CSS and JS is served.

`cargo build --release`, `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` all pass.

## Local verification (curl, against the running server)
The server ran the release binary on :8080, with the Log sender and `dijmo_web_dev`. A curl script made 48 checks. All passed, except two greps in my own script that matched the `<title>` as well as the `<h1>`; checking the `<h1>` directly confirmed those pages.

- **Pages:**
  - `/`, `/?status=sent` and `/privacy` answer 200.
  - `/nope` answers 404 with `error.html` ("There’s nothing here.").
  - `GET /waitlist` answers 303.
  - The index title is "Dijmo: say what you want, and it’s done".
- **Headers:** every response carries the exact CSP, `HSTS max-age=31536000`, `nosniff`, `strict-origin-when-cross-origin` and `Permissions-Policy camera=(), microphone=(), geolocation=(), browsing-topics=()`. HTML is `no-cache`, and outcome pages are `no-store`.
- **Operations and SEO:** `/healthz` and `/readyz` return `{"ok":true}`. `robots.txt` names the sitemap, and `sitemap.xml` lists `/` and `/privacy`.
- **Assets:**
  - The hashed CSS and JS are served immutable, with the right types.
  - `icon.svg` is `max-age=86400`, and the font answers 200.
  - A ranged request for the sound answers 206.
  - A missing file and path traversal both answer 404.
  - Pages are compressed with brotli.
- **Waitlist:**
  - A form post answers 303 to `/?status=sent#join`. A duplicate JSON post answers 200 `{"ok":true}`, and only one email is logged for the two posts.
  - An invalid email gets 422 `{"ok":false,"error":"invalid"}`. No Origin, or a foreign Origin, gets 403. The honeypot gets a silent `{"ok":true}`. A 20 KB body gets 413.
  - The fourth post for the same address gets 429.
- **The confirmation email in the log:**
  - It appears as `to=q***@…`, with the subject, the confirm link and the leave link. The full address is never logged.
  - The confirm link gives "You’re on the list."; clicking it again gives "You’re already on the list."; a bad token gives 400.
  - The leave link gives "You’ve left the waitlist." and the row is deleted. A forged leave token gives 400.
- **Stored row** (checked with psql before leaving): `token_hash` is 32 bytes, `ip_hash` is 16 bytes, and there is no raw token or IP.
- **Graceful shutdown:** SIGTERM logs "shutting down", then "outbox worker stopped", then "stopped".
- **Template check:** the live page matches the current `templates/` (page v2, `b55517c`), and the served asset hashes match the files on disk.

## Follow-ups and risks
1. **Mail scanners and leave links: resolved (contract change, 2026-10-15).** `GET /waitlist/leave?token=` now deletes nothing and renders `outcome: "confirm"` with the `token`. The page's button POSTs the token, which deletes the signup and answers `303` to `/waitlist/leave?done=1` (`outcome: "left"`). One-click unsubscribe still deletes directly. Tests cover each path: GET deletes nothing, POST deletes, a bad token on either path, cross-origin POSTs are refused, and one-click works.
   - The original note follows. `GET /waitlist/leave` used to delete the signup, as the contract said. Corporate link scanners (for example Microsoft Defender Safe Links) sometimes fetch every link in an email, which could silently unsubscribe someone.
   - Suggested contract change: GET shows a "Leave the waitlist?" button (`outcome: "ask"` plus a `token` field), and the POST does the deletion. The POST route already exists.
   - Confirm links have the same exposure, but the effect is milder: an auto-confirm.
2. **Privacy copy.** The privacy page says "How long we keep it: Until you leave." The server also deletes *unconfirmed* signups after 30 days. The page builder may want to add one line about that.
3. **Docker.** The Dockerfile has not been built here, because this Mac has no Docker. Coolify's first build is the real test of it. The build needs only the official `rust:1.90-slim-bookworm` image and crates.io.
4. **HSTS** is `max-age=31536000`, without `includeSubDomains` or `preload`, so other subdomains aren't affected. Add those once every subdomain is https.
5. **One origin.** `PUBLIC_BASE_URL` must be the only public origin. Redirect `www` to the apex (or the reverse) in Coolify, or form posts from the other host get `403`.
6. **Rate limits are per instance** (in memory). A shared store is needed when there is more than one instance.
