# dijmo-web

The website for **Dijmo**, a fast, offline video editor you talk to. Today it's a coming-soon page with a double opt-in waitlist. Later the same server will handle licences, purchases, accounts, desktop sign-in, updates, downloads and the plugin store.

- **Server:** Rust, Axum 0.8, Tokio, tower-http. Pages are rendered on the server with Askama, and the templates are checked at compile time.
- **Database:** Postgres 16 through sqlx. Migrations live in `migrations/` and run at startup.
- **Email:** the `EmailSender` trait, with `Log` for development and Resend for production. Mail goes out through a database outbox with retries.
- **Front end:** hand-written CSS and one small ES2020 file, with no framework. See `docs/design.md`.

The full requirements are in `docs/spec.md`. The interface between the server and the page is `docs/contract.md`.

## Layout
```
Cargo.toml            workspace
crates/core/          domain + persistence: config, keys/tokens, waitlist, outbox
crates/web/           the binary: routes, page structs, assets, security, rate limits, email, worker
migrations/           sqlx migrations (embedded in the binary)
templates/            Askama templates (embedded in the binary)
assets/               css/site.css, js/site.js (hashed at startup), fonts/, img/, sounds/
Dockerfile            cargo-chef build, then a distroless non-root runtime
```

## Run it locally
You need Rust 1.88 or later and Postgres 16.
```sh
createdb dijmo_web_dev
cp .env.example .env
# Fill in SECRET_KEY:  openssl rand -base64 64 | tr -d '\n'
set -a && . ./.env && set +a
cargo run -p dijmo-web
open http://localhost:8080
```
With `EMAIL_PROVIDER=log`, the confirmation email is written to the log; click the link from there. The recipient is masked, and nothing is sent.

## Test
```sh
createdb dijmo_web_test          # once
cargo test                       # unit + integration tests against dijmo_web_test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
- The tests use `TEST_DATABASE_URL`, which defaults to `postgres://localhost/dijmo_web_test`.
- They cover the waitlist flow (signup → outbox → confirm → leave, where only the POST deletes), duplicates, expired and forged tokens, rate limits, the honeypot and timing check, the Origin check, the body cap and the security headers.
- They also check that every template renders and obeys the CSP, the page-weight budget, and that every `/assets/…` reference resolves.

## Configuration
The server reads the environment only. `.env` is for local development and is git-ignored.

| Variable | Required | Meaning |
|---|---|---|
| `DATABASE_URL` | yes | Postgres URL. TLS works too (rustls). |
| `PUBLIC_BASE_URL` | yes | The public address with no trailing slash, e.g. `https://dijmo.com`. Email links use it, and form POSTs must come from its origin. |
| `SECRET_KEY` | yes | 64+ random bytes, base64. Purpose-specific keys are derived from it to sign leave links and to hash IPs. Changing it breaks the leave links in emails already sent. |
| `PORT` | no | Default `8080`. |
| `EMAIL_PROVIDER` | no | `log` or `resend`. If unset, it's `resend` when `RESEND_API_KEY` is set and `log` otherwise. |
| `RESEND_API_KEY` | prod | The Resend API key (sending access only). |
| `EMAIL_FROM` | prod | e.g. `Dijmo <hello@dijmo.com>`, on a domain verified in Resend. Use a real inbox: the privacy page tells people to reply. |
| `PROXY_HOPS` | no | How many reverse proxies are in front. Use `0` locally; the Docker image sets `1` for Coolify's proxy; use `2` if Cloudflare (proxied) is in front of Coolify too. The client IP is the Nth address from the right of `X-Forwarded-For`. |
| `ASSETS_DIR` | no | Default `assets`. The Docker image uses `/app/assets`. |
| `LOG_FORMAT` | no | `text` (default) or `json` (set in the Docker image). |
| `RUST_LOG` | no | Default `info,sqlx=warn`. |

## Deploy on Coolify
1. **Database.** In the project, add a **PostgreSQL 16** resource. Copy its *internal* connection URL (`postgres://…@<service>:5432/<db>`).
2. **Application.** Add a new resource from this GitHub repository (branch `main`) and choose the **Dockerfile** build pack.
   - Set the port to `8080`. Coolify's proxy terminates TLS.
   - Set your domain, e.g. `https://dijmo.com`.
   - Redirect `www` to the apex, or the other way round. Use only one origin, and make it match `PUBLIC_BASE_URL`; the form's Origin check depends on it.
3. **Environment variables** (Coolify → the application → *Environment Variables*):
   ```
   DATABASE_URL=<internal URL from step 1>
   PUBLIC_BASE_URL=https://dijmo.com
   SECRET_KEY=<openssl rand -base64 64 | tr -d '\n'>
   RESEND_API_KEY=<from Resend>
   EMAIL_FROM=Dijmo <hello@dijmo.com>
   ```
   Leave `PROXY_HOPS` at the image default (`1`) unless Cloudflare proxies the domain, in which case set `2`. Keep `SECRET_KEY` stable across deploys.
4. **Health check.** The image has a `HEALTHCHECK` (`/app/dijmo-web --healthcheck`, which probes `/healthz`). If you configure Coolify's own health check instead, use path `/healthz`, or `/readyz` to also check the database, on port `8080`.
5. **Deploy.** Every push to `main` deploys.
   - Migrations run at startup. Several instances are safe, because sqlx takes an advisory lock.
   - On SIGTERM, the server stops accepting connections, lets in-flight requests finish (20 s at most), then stops the outbox worker and closes the pool.

## Resend (production email)
1. Create a Resend account, then **Domains → Add domain**. Use the sending domain, e.g. `dijmo.com`, or a subdomain such as `mail.dijmo.com`.
2. Add the DNS records Resend shows (SPF via `MX`/`TXT` on the `send` subdomain, plus the DKIM `TXT`). Wait until the domain shows **Verified**.
3. Add a DMARC record if you have none, e.g. `_dmarc` `TXT` `v=DMARC1; p=none; rua=mailto:you@dijmo.com`.
4. **API Keys → Create** a key with *Sending access* for that domain. Put it in `RESEND_API_KEY`, and set `EMAIL_FROM` to an address on the verified domain.
5. Test it: join the waitlist on the live site and check the logs.
   - `outbox: done … sent=true` means the mail was accepted.
   - Errors are logged with the recipient masked.
   - A bad key or an unverified domain is retried with backoff (for about 12 hours), so fixing the setting delivers the queued mail.

## How the waitlist works
- **`POST /waitlist`** takes `email`, the honeypot `website` and `t` (the render time), as a form or as JSON.
  - The form answers `303` to `/?status=sent|invalid|slow_down#join`.
  - JSON answers `200 {"ok":true}`, `422 {"ok":false,"error":"invalid"}` or `429 {"ok":false,"error":"slow_down"}`.
  - Known and unknown addresses get the same answer, so nobody can probe the list.
  - A filled honeypot, or a post less than 1.5 s after render, is a silent success.
- **Rate limits** are kept in memory with a bounded map: 8 posts at once per IP (IPv6 /64), then 1 per minute; 3 per address, then 1 per 20 minutes. A confirmation email goes out at most once per 10 minutes per address, enforced in the database.
- **Storage.** A signup stores the lower-cased email (`citext`), the UTM parameters from the page it came from, and a coarse HMAC of the IP (IPv4 /24, IPv6 /48). It never stores the IP itself.
- **Confirmation tokens** are 32 random bytes in base64url. The worker mints one just before sending, and only its SHA-256 is stored. A token is valid for 72 hours, and confirming is idempotent.
- **Leave links** (`/waitlist/leave?token=…`) are HMAC-signed signup ids, so every later email can carry one.
  - Opening the link deletes nothing: mail scanners open every link. The page asks first, and its button POSTs the token, which deletes the signup and redirects to `/waitlist/leave?done=1`.
  - Emails also carry `List-Unsubscribe` with one-click POST (RFC 8058), which deletes directly.
- **Housekeeping.** Unconfirmed signups with no new request for 30 days are deleted. Sent outbox rows are kept for 30 days.

Useful queries:
```sql
SELECT count(*) FILTER (WHERE confirmed_at IS NOT NULL) AS confirmed, count(*) AS total FROM waitlist_signups;
SELECT id, kind, attempts, last_error, next_attempt_at FROM email_outbox WHERE sent_at IS NULL;
```

## Security notes
- Every response carries the contract's exact CSP, plus HSTS, `nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, a `Permissions-Policy` that turns off camera, microphone and geolocation, `X-Frame-Options: DENY` and COOP.
- POSTs must come from `PUBLIC_BASE_URL`'s origin, checked by `Origin`, or by `Referer` when there is no `Origin`. The exception is the one-click leave POST, which is authorised by its signed token.
- Request bodies are capped at 16 KiB. Handlers time out after 10 s, the database pool after 5 s, and statements after 10 s.
- Logs are structured. They never contain emails (only masked ones), IPs or query strings (where the tokens live).
