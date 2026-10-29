# API: the Manual (`help.*`)

The Manual's JSON-RPC door ([`../manual.md`](../manual.md), "The doors"), and through it MCP's tools and the CLI's
`exec`. Domain code: `red-ale-docs` (the Pages, rendering, search, Examples and coverage checks); methods:
`red-ale-rpc::methods::help`, over the Manual `red-ale-rpc::manual::manual()` builds once per process from the API's
own schema document and the built-in report templates. From `redAleApi` 0.6.0.

## Methods

All three have Effect `read` and no target, read nothing but the Manual compiled into the binary, and work with no
book open.

| Method        | Effect | Does |
|---------------|--------|------|
| `help.topics` | read   | Every Page (`id`, `kind`, `title`, `summary`), by Page ID; `kind` keeps one kind. |
| `help.get`    | read   | One Page by Page ID, or by a method's, error code's or report template's name, rendered as `format` with its Examples for `door`. |
| `help.search` | read   | Pages ranked for `query`, best first, each with a `snippet`; `kind` and `limit` optional. |

**Kinds:** `start`, `concept`, `guide`, `method`, `error`, `report`, `gnucash`, `legal` (see
[`../glossary.md`](../glossary.md), "The Manual"). A Page ID is the kind, `/` and a name (`method/transaction.post`,
`error/unbalanced`, `report/balance-sheet`), except the Start Page, `start`. A report's name is its template's name
in lower case, each run of other characters than letters and digits made one `-`.

### `help.get`

**Params:** `id`; `format`, `markdown` (the default) or `text`; `door`, `jsonrpc` (the default), `cli` or `mcp`.
An `id` that names no Page is `not-found` with `data.kind` `page`.

**Result:** `id`, `kind`, `title`, `summary`, `related` (Page IDs), `from` (the section of GnuCash's documentation the
Page adapts, or `null`), `format`, `door` and `content`. `content` is the Page: its title, summary and body, its
related Pages and a one-line licence notice naming `legal/notices` and `legal/gfdl`, as the GFDL asks of a Page
served on its own ([`../manual.md`](../manual.md), "Licence"). `text` drops the markup: headings underlined, code
indented, each link followed by its target in parentheses.

Each Example is shown for the door:

| Door      | Shows |
|-----------|-------|
| `jsonrpc` | each request (`jsonrpc`, `id` from 1, `method`, `params`) and its response |
| `cli`     | a `red-ale exec` command line (`--book book.gnucash` when the Example's book is open; several calls as `--script -` with a here-document), then what it prints, or the error it prints on standard error with exit status 2 |
| `mcp`     | each `tools/call`'s `name` and `arguments`, and the result: `structuredContent` for an object, a text block otherwise, `isError` with the error object and its code's `name` for an error |

### `help.search`

**Params:** `query`, any words; `kind`; `limit`, 1 to 100, 10 by default (another value is `invalid-params`).

**Result:** `hits`, each `id`, `kind`, `title`, `summary` and `snippet`: the first line of the Page outside code with
a word of the query, or its summary.

Ranking is BM25F (k1 1.2, b 0.75) over each Page's title (weight 3), method name (3), summary (2) and body (1),
each field normalised by its own length, so a long body does not bury a match in the title.
Words are lower-cased, split at anything but letters and digits, made singular (`entries` to `entry`) and stripped
of stop words; dotted names (`transaction.post`) also count whole, and camelCase names (`closeBooks`) also by part.
Words a synonym brings in count half. Ties go by Page ID, so a query always gives the same list. No embeddings, no
network.

The synonym table is generated from the glossary's ⚠ notes: in a ⚠ note, each *italic* phrase and `code` span is
another name for the row's term ("GnuCash calls it the *transaction log* (`TransLog`)" maps both to *journal*),
except in a sentence starting "Not", which says what the term is not; the code name beside a term with a ⚠ note
counts too. `docs/synonyms.txt` adds a short hand-kept list (`bill => business document`). The glossary is compiled
in and parsed when the Manual is built, so the table never falls behind it.

## Pages in `api.schema` and on errors

- Each method entry of `api.schema` has `help`, its Page ID (`method/<name>`).
- Every error from every door has `data.help`, the Page ID of the Page that explains its code (`error/<name>`),
  added by the dispatcher after the method runs; an error whose `data` was absent gets `{"help": …}`. A method that
  sets a more specific Page keeps it. Both are additive within 0.x ([`../api.md`](../api.md), "Versioning").

## Coverage

`crates/red-ale-rpc/tests/manual_coverage.rs` runs `red_ale_docs::coverage::all` over the served Manual: every method
and report template has a hand-written part and an Example, every error code a hand-written cause and fix, every link
and `related` Page ID exists, and every `from` names a checklist entry (none yet: the checklist arrives with the
GnuCash documentation work). Gaps known while the Manual is written are listed in `docs/coverage-pending.txt`, which
may only shrink: the test fails on a gap not listed and on a listed entry that is no longer a gap. The same file
requires the compiled-in sources to have no problems, every method's `help` and every error code's Page to exist,
and every built-in report template to have a Page with its options.

`crates/red-ale-rpc/tests/manual_examples.rs` runs every Example (see "Examples" in
[`../manual.md`](../manual.md)).
