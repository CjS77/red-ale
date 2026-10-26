# red-ale design documents

Design comes before code. Read in this order:

1. [`mission-statement.md`](mission-statement.md): what red-ale is for, and what it will not do.
2. [`glossary.md`](glossary.md): the ubiquitous language. Every name used in docs and code is defined here once.
3. [`features.md`](features.md): every GnuCash feature, with the red-ale version that ports it.
4. [`milestone1.md`](milestone1.md): the v0.1 scope and its progress.
5. [`milestone2.md`](milestone2.md): the v0.2 scope (MCP server, reports, the rest of books, transactions and business) and its progress. Draft.
6. [`roadmap.md`](roadmap.md): the versions from v0.3 to parity, and which `features.md` rows each one ports. Proposed.
7. [`milestone3.md`](milestone3.md): the v0.3 scope (memory and scale, import and export, online quotes, locales)
   and its progress. Draft.
8. [`architecture.md`](architecture.md): bounded contexts, crates, the context map and the main decisions.
9. [`file-fidelity.md`](file-fidelity.md): how an XML book round-trips without loss.
10. [`api.md`](api.md): the JSON-RPC published language: conventions, error codes, methods.
11. [`reports.md`](reports.md): report scripting: the Rhai engine, the query API, options, the report result,
    rendering, saved configurations, and the plan for translating and testing GnuCash's reports.
12. [`mcp.md`](mcp.md): the MCP server: tools generated from the methods, their hints, results and errors.
13. [`manual.md`](manual.md): the Manual: user documentation for agents, users and the website, from one core. Proposed.
14. [`testing.md`](testing.md): fixtures, round-trip tests and the GnuCash oracle.
15. [`open-questions.md`](open-questions.md): decisions not yet made.

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
| [`time-zones.md`](notes/time-zones.md)                                       | The local zone, day-neutral and local times, relative dates     |
