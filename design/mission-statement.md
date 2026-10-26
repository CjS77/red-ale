# Mission statement

red-ale is a headless reimplementation of GnuCash in Rust. People and programs drive it over JSON-RPC 2.0, and AI agents
drive it over MCP. It works on the user's existing GnuCash books, in place, and GnuCash keeps working on them afterwards.

## Goals, in priority order

1. **Never damage a book.** A user copies their `.gnucash` file, red-ale opens and edits it, and GnuCash 5.17 opens the
   result with no warnings and no lost data. When red-ale can't edit a book safely it opens it read-only and says why.
   This goal outranks every other.
2. **Feature parity with GnuCash 5.17**, tracked feature by feature in [`features.md`](features.md).
3. **Everything over JSON-RPC.** No operation exists only inside the code. The CLI is a front end for the same
   operations.
4. **An MCP server** over the same operations, from v0.2.

## Non-goals

- A graphical interface. GnuCash's desktop-only features are marked won't-do in `features.md`.
- Guile. GnuCash's reports are Guile Scheme; red-ale translates them into its own Rhai scripts from v0.2 and never
  embeds or runs Guile.
- Linking. Programs that use red-ale talk to it over JSON-RPC or MCP; no red-ale crate is shipped as a library.
- Being "better" than GnuCash in ways that change a book's meaning. Extensions beyond parity are stored where GnuCash
  keeps and ignores them, and each gets a design doc first.

## Releases

| Version | Scope                                                                                              |
|---------|----------------------------------------------------------------------------------------------------|
| v0.1    | GnuCash ≤ 2.0 features (see [`milestone1.md`](milestone1.md)) on XML books; JSON-RPC API and CLI    |
| v0.2    | MCP server; GnuCash ≤ 1.6 reports in Rhai; the rest of books, transactions and business (see [`milestone2.md`](milestone2.md)) |
| v0.3    | Import and export; online price quotes (see [`roadmap.md`](roadmap.md))                            |
| v0.4    | SQLite books; scheduled transactions and budgets                                                   |
