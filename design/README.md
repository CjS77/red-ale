# red-ale design documents

Design comes before code. Read in this order:

1. [`mission-statement.md`](mission-statement.md): what red-ale is for, and what it will not do.
2. [`glossary.md`](glossary.md): the ubiquitous language. Every name used in docs and code is defined here once.
3. [`features.md`](features.md): every GnuCash feature, with the red-ale version that ports it.
4. [`milestone1.md`](milestone1.md): the v0.1 scope and its progress.
5. [`milestone2.md`](milestone2.md): the v0.2 scope (MCP server, reports, the rest of books, transactions and
   business) and its progress. Done.
6. [`roadmap.md`](roadmap.md): the versions from v0.4 to parity, and which `features.md` rows each one ports. Proposed.
7. [`milestone3.md`](milestone3.md): the v0.3 scope (memory and scale, import and export, online quotes, locales,
   the Configuration API), its progress, the evidence for each "Done when" item, and follow-ups. Done.
8. [`milestone4.md`](milestone4.md): the v0.4 scope (SQLite books, scheduled transactions, budgets and their
   reports), its decisions and progress. In progress.
9. [`architecture.md`](architecture.md): bounded contexts, crates, the context map and the main decisions.
10. [`file-fidelity.md`](file-fidelity.md): how an XML book round-trips without loss.
11. [`api.md`](api.md): the JSON-RPC published language: conventions, error codes, methods. Its `api/` pages cover
    each area; v0.3 added [`import.md`](api/import.md), [`export.md`](api/export.md), [`quotes.md`](api/quotes.md)
    and [`configuration.md`](api/configuration.md), and v0.4 adds [`budgets.md`](api/budgets.md) and
    [`scheduled-transactions.md`](api/scheduled-transactions.md).
12. [`reports.md`](reports.md): report scripting: the Rhai engine, the query API, options, the report result,
    rendering, saved configurations, and the plan for translating and testing GnuCash's reports.
13. [`mcp.md`](mcp.md): the MCP server: tools generated from the methods, their hints, results and errors.
14. [`manual.md`](manual.md): the Manual: user documentation for agents, users and the website, from one core. Proposed.
15. [`testing.md`](testing.md): fixtures, round-trip tests, fuzzing and the GnuCash oracle.
16. [`open-questions.md`](open-questions.md): decisions not yet made.

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
