# red-ale design documents

Design comes before code. Read in this order:

1. [`mission-statement.md`](mission-statement.md): what red-ale is for, and what it will not do.
2. [`glossary.md`](glossary.md): the ubiquitous language. Every name used in docs and code is defined here once.
3. [`features.md`](features.md): every GnuCash feature, with the red-ale version that ports it.
4. [`milestone1.md`](milestone1.md): the v0.1 scope and its progress.
5. [`milestone2.md`](milestone2.md): the v0.2 scope (MCP server, reports, the rest of books, transactions and business) and its progress. Draft.
6. [`architecture.md`](architecture.md): bounded contexts, crates, the context map and the main decisions.
7. [`file-fidelity.md`](file-fidelity.md): how an XML book round-trips without loss.
8. [`api.md`](api.md): the JSON-RPC published language: conventions, error codes, methods.
9. [`reports.md`](reports.md): report scripting: the Rhai engine, the query API, options, the report result,
   rendering, saved configurations, and the plan for translating and testing GnuCash's reports.
10. [`mcp.md`](mcp.md): the MCP server: tools generated from the methods, their hints, results and errors.
11. [`testing.md`](testing.md): fixtures, round-trip tests and the GnuCash oracle.
12. [`open-questions.md`](open-questions.md): decisions not yet made.

`GNUCASH_REV` pins the GnuCash release red-ale matches (tag, then commit).

## Research notes

`notes/` holds the research behind the design: what GnuCash 5.17 does, cited to its source as
`5.17:path/to/file:line`. They describe GnuCash, not red-ale; the documents above say what red-ale does with that.

| Note                                                                         | Covers                                                          |
|------------------------------------------------------------------------------|-----------------------------------------------------------------|
| [`xml-format-core.md`](notes/xml-format-core.md)                             | XML envelope, libxml2 serialisation, core entities, write order |
| [`xml-format-business.md`](notes/xml-format-business.md)                     | XML of business objects                                         |
| [`storage-and-safety.md`](notes/storage-and-safety.md)                       | Lock, save, backups, `.log` journal, replay, book features      |
| [`engine-accounts-transactions.md`](notes/engine-accounts-transactions.md)   | Numeric rules, accounts, transactions, void, scrubs, reconcile  |
| [`engine-commodities-prices-lots.md`](notes/engine-commodities-prices-lots.md) | Commodities, ISO table, prices, lots, capital gains, splits   |
| [`engine-business.md`](notes/engine-business.md)                             | Owners, documents, entries, posting, payments, reminders        |
