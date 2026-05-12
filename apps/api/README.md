# Verdict API

## Commands

```bash
cargo check
cargo clippy -- -D warnings
cargo test
```

## Local Postgres

```bash
docker compose up -d
```

Set `DATABASE_URL` from `.env.example` before starting the API.

## Phase 2 — Gemini + PDF fetch (local)

- **`GEMINI_API_KEY`** / **`GEMINI_MODEL`:** required; `POST /events/from-fda-briefing` calls Gemini with the PDF bytes and a JSON response schema, then validates and inserts an `events` row.
- **`FDA_PDF_MAX_BYTES`:** max PDF size when fetching by URL (default ~25MiB).
- **`FDA_PDF_ALLOWED_HOST_SUFFIXES`:** comma-separated host suffix allowlist (default `fda.gov`).
- **`FDA_PDF_ALLOW_INSECURE_LOCALHOST`:** set `true` only for local stub servers in tests/dev; keep `false` otherwise.

To exercise a **real** FDA PDF URL locally, use `https://www.fda.gov/...` and keep the allowlist at its default; automated tests use a loopback stub and do not hit the public internet.

### Phase 2 — FDA briefing verification (PR 7)

Reproduce **three successful ingests** on a local stack (no Fly/Vercel). Before each run, confirm the URL returns a real PDF (`Content-Type: application/pdf` and bytes start with `%PDF`); FDA sometimes serves HTML landing pages at `/media/.../download` depending on `Accept` headers and redirects.

1. From repo root: `docker compose up -d`
2. `cd apps/api` and copy `.env.example` to `.env` with `DATABASE_URL`, `GEMINI_API_KEY`, and optional `GEMINI_MODEL` (default `gemini-2.5-flash-lite`).
3. Apply migrations (e.g. `sqlx migrate run` with the same `DATABASE_URL`, or rely on `cargo run` which migrates on startup).
4. `cargo run` (API on `http://127.0.0.1:3000` by default).
5. For each URL below, run:

```bash
curl -sS -X POST "http://127.0.0.1:3000/events/from-fda-briefing" \
  -H "Content-Type: application/json" \
  -d '{"pdf_url":"<PASTE_URL_HERE>"}'
```

6. Confirm a `201` JSON body with `title` shaped like `{drug} PDUFA {YYYY-MM-DD}` and `source_url` equal to the pasted URL.
7. `curl -sS "http://127.0.0.1:3000/events?status=upcoming"` should list the new rows.

**Example FDA.gov PDFs (agency-published documents; extraction quality depends on layout — drug advisory briefings usually yield cleaner `drug_name` / PDUFA fields than cross-program reports).** Maintainer: re-verify URLs periodically; replace if FDA reorganizes media IDs.

| # | PDF URL (HTTPS) | Verified (UTC) | Notes |
|---|-----------------|----------------|-------|
| 1 | `https://www.fda.gov/media/191112/download` | 2026-05-09 | Large agency PDF; confirm still `application/pdf` before ingest. |
| 2 | `https://www.fda.gov/media/187841/download` | 2026-05-09 | Financial / performance style report; may stress sponsor/indication phrasing. |
| 3 | `https://www.fda.gov/media/190768/download` | 2026-05-09 | Mid-size PDF; check `decision_date` aligns with document headings. |

**Caveats:** OCR-heavy scans, multi-product PDFs, or tables without an explicit PDUFA date can cause validation failures (the API returns `400` with a message — no silent insert). Gemini rate limits and quota apply; retries are limited to three parse/validation attempts per request for the same PDF bytes.

**Regression fixture (no network):** JSON shape used after Gemini returns text is checked in `tests/fixtures/briefing_valid_min.json` and parsed in `ingest::gemini_briefing` unit tests.
