# Server ↔ page contract (controller, 2026-10-15)

Two builders work in parallel. This file is the only interface between them. If either side needs to change it, edit this file in the same commit and say so in the report.

## Who owns what
- **The server builder** owns `Cargo.toml`, `crates/`, `migrations/`, `Dockerfile`, `.env.example`, `README.md` and the Rust tests.
- **The page builder** owns `templates/`, `assets/` and `docs/design.md`.
- Neither edits the other's files.

## Templates (Askama, in `templates/`)
- Every page extends `base.html`. That file has the blocks `title`, `head` (extra head tags) and `body`.
- Every page struct has these common fields:
  - `title: String`
  - `description: String`
  - `canonical: String`: the absolute URL of the page.
  - `base_url: String`: `PUBLIC_BASE_URL`, with no trailing slash.
  - `css: String`: the hashed URL of `assets/css/site.css`, e.g. `/assets/site.3f2a9c1e.css`.
  - `js: String`: the hashed URL of `assets/js/site.js`.
  - `og_image: String`: the absolute URL of `assets/img/og.png`.

The server builder defines one Rust struct per page with these exact field names. The page builder uses only these fields.

| Template | Extra fields |
|---|---|
| `index.html` | `status: String`: `""`, `"sent"`, `"invalid"` or `"slow_down"` (the outcome of a no-JS form post); `email: String` (re-filled on `invalid`); `t: String` (the render time in ms, for the form's timing field) |
| `confirm.html` | `outcome: String`: `"confirmed"`, `"already"`, `"expired"` or `"invalid"` |
| `leave.html` | `outcome: String`: `"left"` or `"invalid"` |
| `privacy.html` | (none) |
| `error.html` | `code: u16`, `heading: String`, `body: String` |

**Escaping:** Askama escapes HTML by default; leave it on. No `|safe` on any field that carries user data.

## The waitlist form
**The HTML form (works with no JavaScript):**
```html
<form method="post" action="/waitlist">
  <input type="email" name="email" required autocomplete="email">
  <input type="text" name="website" tabindex="-1" autocomplete="off" hidden>   <!-- honeypot, must stay empty -->
  <input type="hidden" name="t" value="{{ t }}">
</form>
```
**The server's answer:** `303 See Other` to `/?status=sent#join` (or `status=invalid` / `slow_down`). Index reads `status` from the query.

**The JavaScript path:** the same POST, but with `Accept: application/json` and `Content-Type: application/json`, sending `{"email","website","t"}`.
- `200 {"ok":true}` for both accepted and already-known addresses, so nobody can probe whether an address is on the list.
- `422 {"ok":false,"error":"invalid"}`.
- `429 {"ok":false,"error":"slow_down"}`.

The server treats a form posted less than 1.5 s after render (from `t`), or with the honeypot filled, as a silent success.

## Assets
- **The two source files:** `assets/css/site.css` and `assets/js/site.js`, plain ES2020 and one file each. There is no build step; hand-written and small.
- **Serving:**
  - The server hashes these two files at startup and serves them at `/assets/<name>.<hash>.<ext>` with `Cache-Control: public, max-age=31536000, immutable`.
  - Everything else under `assets/` (fonts, sounds, images) is served at `/assets/<path>` with `max-age=86400`.
- **Sounds:** `assets/sounds/*.webm` (Opus) plus `*.m4a` (AAC, for Safari). The JS picks whichever `canPlayType` supports. Total sounds ≤ 30 KB.
- **Fonts:** self-hosted woff2 in `assets/fonts/`.
- **Images:** `assets/img/og.png` (1200×630) and `assets/img/icon.svg` (the favicon).

## Content Security Policy (the server sends this exactly)
`default-src 'self'; img-src 'self' data:; media-src 'self'; font-src 'self'; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'`

So the page builder may use **no inline `<script>` and no inline `style=""` attributes**. Everything goes in `site.css` and `site.js`. Use `data-*` attributes for JS hooks.

## Routes
- `GET /`, `GET /privacy`
- `POST /waitlist`
- `GET /waitlist/confirm?token=`, `GET /waitlist/leave?token=`
- `GET /healthz`, `GET /readyz`
- `GET /robots.txt`, `GET /sitemap.xml`
- `/assets/*`
- The 404 and 500 pages render `error.html`.
