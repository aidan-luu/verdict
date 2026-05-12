# Verdict — Agent Instructions

You are the primary coding agent on this project. Read SPEC.md before doing
anything else. Re-read it if a session has been idle or compacted.

## Author context

The author is new to Rust. They have shipped TypeScript / Node and React
before. Default to short explanations when introducing Rust idioms; do not
assume familiarity with lifetimes, traits, async runtimes, or the borrow
checker. When you write Rust, prefer clarity over cleverness. Comment
non-obvious borrow-checker workarounds inline with one sentence on the why.

## Stack (locked, do not propose alternatives)

- **Backend:** Rust (latest stable), Axum, Tokio, SQLx (compile-time-checked
  queries), `tracing` for logs, `thiserror` for errors, `serde` + `validator`
  for input validation, `reqwest` for outbound HTTP.
- **Database:** Postgres 16. Migrations via `sqlx-cli`.
- **Background jobs:** Apalis with Redis backend. Run as a separate worker
  binary in the same Cargo workspace.
- **Auth:** Clerk. The Rust backend verifies Clerk-issued JWTs via middleware.
  Do not roll auth from scratch.
- **Frontend:** Next.js 15 (App Router), TypeScript strict, Tailwind,
  shadcn/ui, Recharts. Type the API client from the Rust side via `utoipa`
  (Rust → OpenAPI) and `openapi-typescript` (OpenAPI → TS).
- **LLM:** Google Gemini API. Default to `gemini-2.5-flash-lite` for Phase 2 PDF
  ingestion (budget-friendly); pin the exact model id in env when you need
  reproducibility.
- **Deployment:** Local-first for now; when you ship publicly, Fly.io for the
  Rust services and Vercel for the Next.js app are the intended targets.
- **Testing:** `cargo test` + `tokio::test` for Rust. Vitest for the frontend.
  Playwright for end-to-end on the critical demo path. No more than that.

## Conventions

- **No `unwrap()` or `expect()` in production paths.** Use `?` and propagate
  typed errors via `thiserror`. Tests can `unwrap`.
- **No raw SQL.** Use SQLx macros (`query!`, `query_as!`) for compile-time
  checking against the schema.
- **No `Arc<Mutex<...>>` unless justified in a code comment.** The agent will
  reach for this by default; don't. Prefer message passing, ownership
  transfer, or `Arc<RwLock<...>>` for read-heavy state.
- **Validate at every boundary.** External JSON, LLM outputs, query params,
  request bodies — all validated before they touch business logic.
- **Probabilities are exact.** Stored as `numeric(5,4)`, mapped to
  `rust_decimal::Decimal` on the backend. Never `f64` for stored values.
  Brier components compute in `Decimal` and cast to `f64` only for display.
- **Tests live alongside source.** `#[cfg(test)]` modules at the bottom of
  each Rust file. Frontend test files alongside components.
- **No commented-out code.** Delete it. Git remembers.

## Workflow

After every logical unit of work, in order:
1. `cargo check` (must be clean)
2. `cargo clippy -- -D warnings` (must be clean)
3. `cargo test` (relevant tests must pass)
4. `cargo fmt`
5. For frontend changes: `pnpm typecheck && pnpm lint && pnpm test`

Before declaring any task done, all five must pass. If they don't, fix, don't
suppress.

## Plan mode discipline

For any task that will touch more than one file or more than ~50 lines, START
in plan mode. Produce a plan, list the files you will create or modify, list
the tests you will add, and stop. Wait for the author to approve before
writing code.

For one-line fixes or single-function additions, plan mode is overkill — just
write the change.

## PR discipline

- Smaller PRs than you would write in TypeScript. Each PR should compile,
  pass tests, and represent one coherent change.
- A typical Rust PR here is 100–300 lines. If you're at 600+ lines, you've
  bundled unrelated work — split it.
- Every PR title starts with the phase tag: `[P1]`, `[P2]`, `[P3]`, `[P4]`.

## When you don't know

- If a request is ambiguous, ask one specific question. Don't guess.
- If you suggest a library not listed above, justify it in writing first.
- If a Rust idiom would surprise the author, comment it inline with one
  sentence explaining the why.

## What is out of scope

Refer to SPEC.md "Out of scope." If you find yourself wanting to add anything
listed there because "it would be quick," stop and ask.

## Critical: math correctness

Brier score and reliability diagram bucketing have unit tests with
hand-computed expected values. Do not modify those tests to make them pass.
If your implementation disagrees with a hand-computed test, the implementation
is wrong, not the test. The math is small and well-defined.
