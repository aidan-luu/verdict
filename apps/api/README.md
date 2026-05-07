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

- **`GEMINI_API_KEY`** / **`GEMINI_MODEL`:** required to run the API process today (ingestion routes will use them in later PRs).
- **`FDA_PDF_MAX_BYTES`:** max PDF size when fetching by URL (default ~25MiB).
- **`FDA_PDF_ALLOWED_HOST_SUFFIXES`:** comma-separated host suffix allowlist (default `fda.gov`).
- **`FDA_PDF_ALLOW_INSECURE_LOCALHOST`:** set `true` only for local stub servers in tests/dev; keep `false` otherwise.

To exercise a **real** FDA PDF URL locally, use `https://www.fda.gov/...` and keep the allowlist at its default; automated tests use a loopback stub and do not hit the public internet.
