# The manual

**Status: proposed** (see [`roadmap.md`](roadmap.md), v0.6). How red-ale documents itself, so that:

1. an AI agent can find out by itself how to do something with red-ale;
2. a user can ask an agent how to do something, and the agent has the whole documentation at hand;
3. the project can publish a manual, as a static website, covering the MCP server, the JSON-RPC API and the CLI.

All three read one corpus, the **Manual**, built by one crate and served through every door red-ale already has.

## What exists today

- `api.schema` (and `red-ale schema`): every method with a one-line summary, its Effect and target, and params and
  result schemas whose field descriptions come from doc comments; every error code by name.
- `red-ale methods`: the method list with summaries.
- MCP: one tool per method with the summary as its description, and a three-sentence `instructions` string
  (`crates/red-ale-mcp/src/server.rs:21`). Tools only, no resources ([`mcp.md`](mcp.md)).
- `design/`: thorough, but written for red-ale's developers: GnuCash citations, rationale and deviations, not "how
  do I".

What is missing: what a method is *for* and when to use it, worked examples, how methods combine into tasks, the
concepts a user needs (value and amount, read-only reasons, trading accounts), how GnuCash's menus and dialogs map
onto red-ale, and a way to search any of it.

## Terms

Proposed for [`glossary.md`](glossary.md):

| Term | Meaning |
|------|---------|
| **Manual** | red-ale's user documentation: every Page, compiled into the binary and served through the API, MCP, the CLI and the website. **⚠** Not the design documents in `design/`, which are for red-ale's developers. |
| **Page** | One unit of the Manual, with a stable Page ID, a kind, a title, a one-line summary and a Markdown body. |
| **Page ID** | A Page's stable name, such as `method/transaction.post`, `guide/reconcile-an-account` or `concept/value-and-amount`. Kept within a major version, so links from agents and other sites keep working. |
| **Reference page** | The Page for one method, error code or report template, generated from the registries and completed by hand. |
| **Guide** | A Page that shows how to do one task, as a sequence of method calls. |
| **Concept page** | A Page that explains an idea the methods rely on. |
| **Example** | A method call with its params and expected result, inside a Page, that the tests run. |

## The common core: `red-ale-docs`

A new bounded context and crate, `red-ale-docs`, holds the Manual's model and everything that reads it. It does no
I/O and depends on no other red-ale crate. The registries it documents reach it as data:
- the `api.schema` document (a `serde_json::Value`, from `Dispatcher::schema`);
- the report templates and their option schemas (from `red-ale-report`'s registry).

So `red-ale-rpc` can depend on it to serve the `help.*` methods without a cycle.

```
   docs/ (Markdown, hand-written) ──┐
   api.schema (methods, errors) ────┼──► red-ale-docs: Manual = Pages + index
   report registry (templates) ─────┘        │
                                             ├─► help.* methods (JSON-RPC, so MCP tools and CLI too)
                                             ├─► MCP resources and instructions
                                             ├─► red-ale help (terminal text)
                                             └─► red-ale manual build (static site, llms.txt)
```

### Sources

| Kind | Source | Written by |
|------|--------|------------|
| Reference: method | Generated from `api.schema` (name, Effect, target, params, result, errors), merged with `docs/reference/<method>.md`: what it is for, when to use it, pitfalls, Examples, related methods | Generated and hand-written |
| Reference: error | Generated from the error table, merged with `docs/errors/<name>.md`: what causes it and how to fix it | Generated and hand-written |
| Reference: report | Generated from the report registry: template name and GUID, options with GnuCash's help texts and defaults, merged with `docs/reports/<file>.md` | Generated and hand-written |
| Concept | `docs/concepts/*.md`. The terms come from the glossary's user-facing entries, so the Manual and the design use one vocabulary | Hand-written |
| Guide | `docs/guides/*.md`: one task each, made of Examples | Hand-written |
| From GnuCash | `docs/gnucash/*.md`: GnuCash's menus, dialogs and assistants mapped to red-ale methods ("Actions › Stock Split…" → `stock.split`), following `features.md`'s rows | Hand-written |
| From GnuCash's documentation | GnuCash's *Tutorial and Concept Guide* and *Help Manual* at the `5.17` tag of `Gnucash/gnucash-docs`: their concepts and how-tos, translated for the API, MCP and the CLI, GUI material left out ([`roadmap.md`](roadmap.md), v0.6). A checklist in `docs/` maps each section to the Pages covering it | Hand-written |
| Start | `docs/start.md`: what red-ale is, opening a book, saving, amounts on the wire, where to look next | Hand-written |

Hand-written files are Markdown with a short front matter (`id`, `title`, `summary`, `related`). They live in
`docs/` at the repository root, beside `design/`, and are compiled in with `include_str!`, as the built-in reports
are. The Manual works offline and matches the binary that serves it.

### Examples are tests

An Example is a fenced block in a Page:

````markdown
```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "account.balance", "params": {"account": "Root:Bank", "asOf": "1971-01-01"}}
{"result": {"balances": [{"commodity": "CURRENCY::USD", "balance": {"num": "218600", "denom": "100"}}]}}
```
````

- The test suite runs every Example on a temporary copy of the named fixture (or a new book), with the fixed clock
  and zone, and compares each result exactly or, where a field is marked `"…"`, by shape.
- An Example that stops matching the code fails the build, so the Manual can't fall behind the API.
- The renderers show each Example three ways: as a JSON-RPC request and response, as a `red-ale exec` command line,
  and as an MCP tool call.

### Coverage is checked

Tests like the MCP comparison test ([`mcp.md`](mcp.md)) check that the Manual covers the registries:
- every method has a Reference page with a hand-written part and at least one Example;
- every error code has a cause and a fix;
- every report template has a Page;
- every Page ID that a Page links to exists;
- every glossary term a Page uses is defined.

The check also runs over error data: `data.help` (below) must name an existing Page.

### Search

`help.search` ranks Pages for a query with BM25 over titles, summaries, bodies and method names, built once from the
compiled-in corpus. It is deterministic and offline: no embeddings, no network.
- **GnuCash's words find red-ale's Pages:** a synonym table drawn from the glossary maps "quantity" to *amount*,
  "bill" to *document*, "scheduled transaction" to its Pages, and so on.
- **Menu paths find Pages too:** searching "Stock Split…" finds the From GnuCash Page for that menu item.

## The doors

### JSON-RPC (and through it MCP tools and the CLI)

New `help.*` methods. All have Effect `read` and no target, and all work with no book open:

| Method | Does |
|--------|------|
| `help.topics` | Every Page: ID, kind, title, summary. Optional `kind` filter. |
| `help.get` | One Page by ID, as Markdown or plain text (`format`), with its Examples rendered for one door (`door`: `jsonrpc`, `cli`, `mcp`). |
| `help.search` | Pages ranked for `query`, each with a snippet. `kind` and `limit` optional. |

- Errors gain an optional `data.help`, the ID of the Page that explains them (`error/unbalanced`,
  `concept/read-only-books`). This is additive within the major version.
- `api.schema` stays as it is; each method entry gains `help`, its Page ID.

### MCP

The adapter stays thin. It still has no logic of its own; it adds MCP's own ways to offer context:

- **Resources.** Every Page is a resource, `red-ale://manual/<page-id>`, `text/markdown`, through `resources/list`
  and `resources/read`, with the resource template `red-ale://manual/method/{name}`
  ([2025-11-25 spec, "Resources"](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)).
  Clients that load resources give the agent the whole Manual; the others still have the `help.*` tools.
- **Instructions** are the Start Page's summary section, generated, not a string in the adapter.
- **Tool descriptions** are the summary plus the Reference page's "When to use it" paragraph, within a fixed budget
  (about 60 words), ending with the Page ID.
  - `red-ale mcp --tool-descriptions short` keeps the one-line summary, for clients that are short of context:
    146 tools and growing.
- **No prompts** for now. Guides are resources; a prompt per Guide can be added later without changing anything
  else.

[`mcp.md`](mcp.md)'s "tools only" changes to "tools and resources".

### CLI

- `red-ale help` prints the Start Page.
- `red-ale help <page-id or method>` prints a Page as terminal text, with its Examples as `exec` command lines.
- `red-ale help --search <words>` lists matching Pages.
- `red-ale methods` gains each method's Page ID.

### The static site

`red-ale manual build <dir>` writes the website from the same Manual: one HTML file per Page, an index per kind and
a search page. It also writes:
- `schema.json`, the `api.schema` document;
- `llms.txt`, an index of Page IDs, titles and summaries with links, for LLMs reading the site;
- `llms-full.txt`, every Page as Markdown in one file.

The site follows the report renderer's rules ([`reports.md`](reports.md) §8):
- one stylesheet;
- no script except an optional search page;
- deterministic bytes, so a rebuild with no changes is a no-op.

Markdown becomes HTML through `pulldown-cmark` (MIT). CI builds the site for each release; the published site matches
the release.

## Writing it

- **Two audiences, two places.** `design/` explains why, with GnuCash citations, for developers; `docs/` explains
  how, for users and agents. A deviation from GnuCash that a user can see appears in both: the design page gives the
  reason, the Manual says what to expect.
- **The ubiquitous language holds.** The Manual uses the glossary's terms and flags GnuCash's differing words as the
  glossary does.
- **Every version ships documented.** From the version that brings the Manual, a method, error code or report
  counts as done only when its Reference page and Examples pass the coverage tests, as it now needs its oracle
  tests.

## Licence

`docs/` is licensed under the GNU Free Documentation License, version 1.1 or any later version, with no Invariant
Sections, no Front-Cover Texts and no Back-Cover Texts: the terms GnuCash's books use (`5.17:C/guide/legal.docbook`,
`5.17:C/manual/legal.docbook`, with the licence text in `5.17:COPYING-DOCS` of `Gnucash/gnucash-docs`). So Pages may
adapt the books' text, not only their ideas.

Only the text carries the GFDL: the Markdown in `docs/`, and the Pages, site and `llms*.txt` built from it. All code
stays GPL-3.0-or-later, including `red-ale-docs`, the `help.*` methods and the site generator. Each file in `docs/`
starts with `SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later`. `red-ale-docs` packages that text, so its
`license` field is `GPL-3.0-or-later AND GFDL-1.1-no-invariants-or-later`, and `deny.toml` has the GFDL on its
allow-list. The allow-list is global, so a dependency licensed under the GFDL alone (code under a documentation
licence) would pass `cargo deny`; review catches that.

The Manual is a Modified Version of those books in the GFDL's sense, so GFDL 1.1 §4 asks it to:
- carry a title of its own ("red-ale Manual"), not GnuCash's;
- keep the books' copyright notices and add red-ale's beside them, followed by the licence notice;
- include the GFDL's full text, as a Page (`legal/gfdl`) and as `docs/COPYING-DOCS`;
- keep a History Page listing the books it draws on (title, year, authors, publisher, and where their source is
  published) and an item for each red-ale release of the Manual;
- name at least five of the books' principal authors with red-ale's;
- keep the substance of any Acknowledgements section it carries over.

How the doors meet that:
- Every Page that adapts the books' text says so in its front matter (`from: guide/ch_txns.docbook`), and its
  footer names the source section and links `legal/notices`. Coverage tests check that each `from` names a file in
  the checklist.
- `help.get`, the MCP resources and `red-ale help` serve Pages one at a time, so each served Page ends with a
  one-line licence notice and the Page IDs of the notices and the licence.
- The static site and `llms-full.txt` carry the notices, History and licence in full.

The binary carries the Manual beside the code as separate works, which GFDL 1.1 §7 treats as an aggregate: the
GFDL covers the Manual and the GPL covers the code, and neither licence reaches the other. Examples are JSON
method calls, not code to copy into red-ale, so they raise no question.

## Settled for v0.6

Decided with [`milestone6.md`](milestone6.md):

- The website publishes the latest release only.
- The synonym table is generated from the glossary's ⚠ entries, with a short hand-kept supplement.
- Every method, however rarely used, needs a "what it is for" paragraph and one Example; nothing more is required.
- The Manual has a short section on writing custom report scripts (a Concept page and a Guide), since most of
  GnuCash's reports are left for users to write as their own scripts.
