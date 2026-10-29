# Milestone 6: red-ale v0.6

**Status: in progress, decisions settled.** The scope comes from [`roadmap.md`](roadmap.md) ("v0.6") and
[`manual.md`](manual.md), as narrowed by the maintainer during planning ("Decide before code"). The research for the
invoice and the Dashboard is in [`notes/report-invoice.md`](notes/report-invoice.md) ("the note" below); its open
questions are settled in "Decided", item 11.

**Goal:** red-ale documents itself. The Manual answers an agent's or a user's "how do I" through JSON-RPC, MCP, the
CLI and a published website, with every Example run as a test, and shows how to write a User report. red-ale
prints an invoice as GnuCash's Print Invoice does by default, and offers GnuCash's Dashboard.

This file tracks progress towards v0.6. Feature names and versions come from [`features.md`](features.md); when a
feature here is done, set its Ported column there to `0.6`.

## Scope

The two open rows left in v0.6:

| Area                | Rows              |
|---------------------|-------------------|
| §8 Business reports | Printable invoice |
| §8 Report system    | Dashboard         |

Fifteen rows once planned here are ❌ ("Decided", items 1 and 2): PDF export; every other business report; every tax
report.

Also in v0.6:

- **the Manual** ("The Manual"), which documents v0.1 to v0.6 at once, with a section on User reports;
- the v0.5 follow-ups taken up here ("Carried forward from v0.5");
- **two Book settings rows the invoice needs**, found by the research (the note, open question 8): *Business options*
  (company name, address, ID, contact, phone, fax, email, URL, tax number) and *Default invoice report*. No method read
  or wrote them before; `book.options` and `book.setOptions` gain them ("Business book options");
- the `sql-book` error no longer names a version for MySQL and PostgreSQL, which are ❌ (done:
  `crates/red-ale-service/src/error.rs:44`, `tests/session.rs`,
  `database_server_books_are_refused_before_anything_happens`).

Why these together: the Manual changes no file red-ale writes and no method's behaviour, and from v0.6 a report counts
as done only with its Manual page ([`roadmap.md`](roadmap.md), "How the order is chosen", rule 6). Most of GnuCash's
remaining reports are left for users to write as their own scripts, so the Manual must teach that. The Printable
invoice reads data red-ale has modelled since v0.1 and is what GnuCash prints when no other invoice report is chosen.
The Dashboard combines reports v0.5 already ported.

**Out:**

- **Everything ❌ in "Decided".**
- **A method that saves a user report script from its text.** User scripts stay files in the report directory ("Decided", item 3).
- **eguile templates**, of any report ("Decided", item 2).
- **Interactive charts:** a v1.0 decision, proposed ❌ ([`roadmap.md`](roadmap.md), "v1.0").
- **Windows**, as in v0.5 ([`milestone5.md`](milestone5.md), "Out").
- **The v0.5 release steps** (pushing `v0.5.0`, the harness smoke tests, `install.sh` on macOS) are the maintainer's
  and don't wait for v0.6.

## Compatibility in v0.6

Every v0.1 to v0.5 rule holds. Reports only read: no report changes a Book or makes it dirty, on XML or SQLite. The
Manual reads nothing of a Book. Two rules are new:

- **The server reads no file a report option names.** The invoice's Picture Location option names an image
  (`5.17:gnucash/report/reports/standard/invoice.scm:405-407`). red-ale puts the string into the HTML's `<img src>`,
  as GnuCash does, and never opens it ("Decided", item 4).
- **Book options the invoice reads are read, not set.** The business options (company name, address, ID, contact,
  phone, fax, email, URL) and the Default Invoice Report are kept in the Book's Slots
  (`5.17:libgnucash/engine/qofbook.cpp:1034-1112`), which `book.setOptions` writes from v0.6 as GnuCash's Book Options
  dialog does (the note §1.6). No report run writes one, and GnuCash's migration of the old invoice report preference into the Book
  (`5.17:gnucash/gnome/business-gnome-utils.c:115-128`) is a GUI step red-ale doesn't copy.

## The Manual

As [`manual.md`](manual.md) designs it and [`roadmap.md`](roadmap.md) ("The manual") scopes it, with its open items
settled ("Decided", item 7). The API to document is `redAleApi` 0.5.0: 183 methods and 32 error codes in the 0.4.0
document (`crates/red-ale-rpc/tests/schemas/`), the same in 0.5.0, plus what v0.6 adds, and about 45 report templates.

| Status | Item                           | Notes                                                                                                                                                                                                                                                                                                                                 |
|:------:|--------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | `red-ale-docs`                 | The Manual's common core: Pages from `api.schema`, the error table and the report registry, merged with `docs/`; the index; deterministic offline search (BM25), with synonyms generated from the glossary's ⚠ entries and a short hand-kept supplement. No I/O, no red-ale dependency ([`manual.md`](manual.md), "The common core"). Done: `crates/red-ale-docs` (`Manual::build`, `search`, `render`, `examples`, `coverage`); BM25F, synonyms parsed from the compiled-in glossary plus `docs/synonyms.txt`; `docs/` compiled in by its build script |
|   ✅   | `help.*` methods               | `help.topics`, `help.get`, `help.search`: Effect `read`, no target, no book needed. `data.help` on errors; `help` on each method in `api.schema`. Done: `methods/help.rs`, `redAleApi` 0.6.0, the 0.5.0 document committed (`tests/help.rs`, `api_additive.rs`); [`api/help.md`](api/help.md) |
|   ✅   | MCP                            | Pages as resources (`red-ale://manual/<page-id>`); instructions from the Start Page; tool descriptions from the Reference pages, within about 60 words; `--tool-descriptions short`. [`mcp.md`](mcp.md) changes from "tools only" to "tools and resources". Done: `crates/red-ale-mcp/src/manual.rs`; `red-ale-mcp/tests/manual.rs` (`every_page_is_a_markdown_resource`, `reading_a_resource_gives_its_page_with_examples_as_tool_calls`, `the_method_template_is_listed`, `the_instructions_come_from_the_start_page`, `tool_descriptions_lead_with_the_reference_page_within_the_budget`, `short_tool_descriptions_are_the_summary`), `tests/tools.rs` (`every_method_is_a_tool_with_its_params_schema_and_hints`), `red-ale/tests/mcp.rs` (`tool_descriptions_short_keeps_the_summary_only`) |
|   ✅   | CLI                            | `red-ale help`, `red-ale help <page>`, `red-ale help --search`; `red-ale methods` lists Page IDs. Done: `crates/red-ale/src/help.rs`; `red-ale/tests/help.rs` (`help_prints_the_start_page_as_text`, `help_prints_a_page_with_its_examples_as_exec_lines`, `help_finds_errors_and_reports_by_name`, `help_search_lists_matching_pages`, `an_unknown_page_fails_with_the_closest_pages`, `help_with_a_command_prints_its_options`, `methods_lists_each_methods_page_id`) |
|   ✅   | Static site                    | `red-ale manual build <dir>`: one page per Page, indexes, search, `schema.json`, `llms.txt`, `llms-full.txt`; byte-for-byte the same on a rebuild. The latest release only. Done: `red-ale-docs`'s `Manual::site` (`site/`), `crates/red-ale/src/manual.rs`; `red-ale/tests/manual_site.rs` (`building_twice_gives_the_same_bytes`, `the_command_writes_the_site_into_an_empty_or_new_directory`, `every_page_has_a_file_with_its_title_and_the_licence_line`, `every_internal_link_resolves_to_a_file_and_anchor`, `llms_txt_lists_every_page_and_llms_full_holds_them`, `the_search_index_parses_and_has_every_page`, `only_the_search_page_runs_a_script_and_nothing_is_fetched_from_elsewhere`); unit tests in `site/markdown.rs` |
|   ✅   | Deployment                     | A CI workflow builds the site at each release tag and publishes it to GitHub Pages; a manual run builds without publishing. Done: `.github/workflows/manual.yml` (tag `vX.Y.Z` builds and deploys; `workflow_dispatch` builds and keeps the site as an artifact, and deploys only with the `publish` input). Not yet run on GitHub: the repository's Pages source must be set to "GitHub Actions" |
|        | Reference pages                | Every method, error code and report template: a "what it is for" paragraph and at least one Example each, nothing more required                                                                                                                                                                                                       |
|        | Start, Concept and Guide pages | The Start Page; Concept pages for what the methods rely on; Guides for common tasks, including a report to PDF through its HTML export and an outside converter such as pandoc                                                                                                                                                        |
|        | User reports                   | Below                                                                                                                                                                                                                                                                                                                                 |
|        | From GnuCash pages             | Each ✅ row of `features.md` maps its menus, dialogs and assistants to methods ("Actions › Stock Split…" → `stock.split`; "Print Invoice" → `report.run`, "Decided", item 6). A ❌ report's page says to write it as a User report                                                                                                   |
|        | GnuCash's documentation        | The *Tutorial and Concept Guide* and *Help Manual* at the `5.17` tag of `Gnucash/gnucash-docs`, chapter by chapter, GUI material left out; a checklist, [`gnucash-docs-checklist.md`](gnucash-docs-checklist.md), maps every section to its Pages or marks it GUI-only. GnuCash's glossary (`C/guide/gnc-glossary.docbook`) checked against [`glossary.md`](glossary.md)              |
|   ✅   | Licence                        | `docs/` under GFDL-1.1-or-later with no invariant sections; notices, History and the licence text as Pages; `red-ale-docs` is `GPL-3.0-or-later AND GFDL-1.1-no-invariants-or-later`; `deny.toml` allows the GFDL ([`manual.md`](manual.md), "Licence"). Done: `docs/COPYING-DOCS` (from `gnucash-docs` at 5.17), `legal/gfdl` generated from it, `legal/notices` and `legal/history`; the books' notices join them with the first Page that adapts their text |
|   ✅   | Coverage tests                 | Every method, error and template has its Page; every Example passes; every link and every `from` resolves; every glossary term a Page uses is defined. Done: `red-ale-rpc/tests/manual_coverage.rs` (`the_only_gaps_are_the_pending_ones` runs every check of `red-ale-docs`'s `coverage` against the shrinking `docs/coverage-pending.txt`: methods, errors, templates, links, `from` against the checklist's rows and the checklist's Page IDs against the Pages, and bold terms in Concept pages against the glossary, [`manual.md`](manual.md), "Settled in building it"; `the_checklist_has_a_row_for_every_section`, `every_method_and_error_names_an_existing_page`, `every_report_template_has_a_page`, `the_manual_sources_have_no_problems`) and `tests/manual_examples.rs` (`every_example_passes`); unit tests in `coverage.rs` and `synonyms.rs` |

The `gnucash-docs` repository isn't checked out on the development machine yet. It is cloned at the `5.17` tag
beside `$GNUCASH_SOURCE` before the GnuCash documentation work starts, and the checklist records its commit, as
`GNUCASH_REV` records the source's.

### User reports

User report scripts have worked since v0.2 ([`reports.md`](reports.md) §1, §2): a `.rhai` file in the report
directory (`reports.directory` in the Configuration) is loaded at every report method, `report.list` lists its
templates or its errors with line and column under `problems`, and `report.run` and `report.export` run it. Scripts
read the Book only through the read-only query API and cannot change it or reach files, the network or the clock.

What is missing is documentation outside `design/`. A short section of the Manual, written for a user or an agent
writing a report:

- **A Concept page, "Report scripts"**: the report directory; the shape of a script (`templates()`, `options`,
  `render`); the option types; the query API's main reads; exact arithmetic with `Rational`; the report result's
  blocks; the limits; importing the built-in library modules. Short, linking to the built-in scripts as worked
  examples rather than restating [`reports.md`](reports.md).
- **A Guide, "Write your own report"**: set the report directory, write a small script (account balances at a
  date, say), check it with `report.list`, run it with `report.run` and `report.export`. Its script is a fixture and
  its Examples run as tests, so the Guide can't fall behind the engine.
- **The From GnuCash pages of the ❌ reports** point there.

The agent must be able to write files where red-ale runs, since no method takes a script's text ("Decided", item 3).
The Guide says so.

## Reports

Translated from GnuCash 5.17's Scheme into Rhai, on the v0.2 engine, as [`reports.md`](reports.md) sets out. Source
files are under `5.17:gnucash/report/reports/`; lines are the file's length at the pin.

| Status | Report            | Source (GUID)                                                                                                                                                                                                                                                                   | Lines | GnuCash's test     |
|:------:|-------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------:|--------------------|
|   ✅   | Printable invoice | `standard/invoice.scm` (`5123a759ceb9483abf2182d01c140e8d`), the built-in default invoice report (`5.17:gnucash/gnome/business-gnome-utils.c:110-113`). The same file defines the Easy and Fancy invoices (`:873-905`), which are ❌: only the Printable template is registered. Done: `report_invoice.rs` (TI's assertions, `inv_1_simple_entry` to `inv_8_combinations_of_entry_options`; the hand-worked cases, XML and SQLite; `a_run_changes_nothing`), `every_v06_report_gives_its_recorded_bytes`, `dates_follow_the_books_fancy_date_format` |   906 | `test-invoice.scm` |
|   ✅   | Dashboard         | Not a template: a Reports menu item (`5.17:gnucash/gnome/report-menus.scm:110-121`) that makes a two-column Multicolumn View of the expense and income pies, the income/expense chart and the Account Summary (`standard/dashboard.scm:40-65`), all ported by v0.5. A built-in saved configuration ("Decided", item 5; [`reports.md`](reports.md) §9, §11). Done: `report_dashboard.rs` (`the_dashboard_runs_by_name_and_guid_as_gnucash_lays_it_out_on_xml_and_sqlite`, `the_dashboard_gives_the_bytes_of_a_multicolumn_view_saved_by_hand`, `the_dashboard_is_listed_as_a_read_only_saved_configuration_with_its_menu_item`, `the_dashboard_cannot_be_changed_renamed_or_deleted_only_copied`, `a_users_own_dashboard_shares_the_name_and_the_guid_tells_them_apart`, `importing_a_gnucash_users_dashboard_keeps_it_beside_the_built_in_one`, `a_configuration_entry_with_the_built_in_guid_is_left_out_and_kept`), `every_v06_report_gives_its_recorded_bytes`; the Manual's `report/dashboard` |    65 |                    |

### What the invoice needs from the engine

- **Invoices in the query API** ([`reports.md`](reports.md) §5): an invoice, bill or voucher with its owner and
  addresses, terms, dates, entries (quantity, price, discount, taxes), tax tables and totals, its posted transaction
  and lot, and the payments against it. The core already computes these for posting and payments (`ops::business`);
  the query API exposes reads only, as for the v0.5 lot reads. The research note lists exactly what
  `standard/invoice.scm` reads.
- **Two option types** ([`reports.md`](reports.md) §6): an invoice (General › Invoice Number, which
  `gnc:invoice-report-create` sets, `5.17:gnucash/report/reports/reports.scm:68-80`; a GUID or `null`, as `budget`)
  and an image (Layout › Picture Location, `gnc-register-pixmap-option`; a string), each with its wire value.
- **One core read**, the due date (the posted transaction's due date, else its date posted), and the query API's
  Invoice, Entry, Owner, Order and BillTerm handles, `book.invoice`, `book.option(path)` and `ctx.print_time` (the
  note §4, §5 items 1–2).
- **Four additions to the report result** (the note §3.2): `numberFormat: "exact"` (GnuCash prints quantities and
  percentage discounts unrounded), `class` on blocks and cells (the invoice's CSS option styles by class), `css` on
  the result, and an `image` block. Additive; no earlier report sets them, so every golden byte stays the same.
- **Built-in saved configurations** (the note §2.3), of which the Dashboard is the first. Done
  ([`reports.md`](reports.md) §9, [`api/reports.md`](api/reports.md), "Types"): `report.list`'s `readOnly` and menu
  fields, the refusals, copying, names not reserved, the GUID clash.

### Business book options

| Status | Item                   | Notes |
|:------:|------------------------|-------|
|   ✅   | Business options       | `book.options` and `book.setOptions` read and write the Business tab's strings (company name, address, contact, phone, fax, email, URL, ID) and the Tax tab's Tax Number, in the Slots GnuCash's Book Options dialog writes (the note §1.6, open question 8). Done ([`api/book-options.md`](api/book-options.md)): `book_options.rs` (unit), `business_options_are_written_only_when_they_change`; `book_options.rs` (API), `business_options_are_set_read_and_saved`; `oracle_business_options.rs`, `gnucash_reads_the_business_options_red_ale_writes_on_xml`, `…_on_sqlite`, `red_ale_writes_the_slots_gnucash_writes`, `cleared_options_read_as_unset_in_gnucash` |
|   ✅   | Default invoice report | `defaultInvoiceReport`: `{ guid, name }` or `null`, read and written as GnuCash parses and writes the slot (the note §1.6). Done: `the_default_invoice_report_is_parsed_as_gnucash_parses_it`, `the_default_invoice_report_is_written_as_gnucash_writes_it` (unit); `a_default_invoice_report_needs_a_guid_and_a_name` (API); the oracle tests above read it through `qof_book_get_default_invoice_report_guid` and `-name` |

## Carried forward from v0.5

From [`milestone5.md`](milestone5.md), "Follow-ups" ("Decided", item 8):

| Status | Follow-up                                                     | In v0.6                                                                                                                                                                              |
|:------:|---------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | Report functions that reuse an engine function's name         | Yes, and more pressing now that users write their own scripts: enable Rhai's `metadata` feature outside tests if it costs little, and refuse such clashes at load with `report-load`. Done: `metadata` is on in `red-ale-report` (3% on the binary, 5% on a registry load, measured in [`reports.md`](reports.md) §2, "Build"); a script defining a function of an engine function's name and arity, or of one fewer (a method call reaches it), is `report-load` at the function, or `internal-error` if built in. The Budget Report's `range` became `period_range`; golden bytes unchanged. `engine_name_clash.rs`: `a_report_defining_an_engine_function_does_not_load`, `a_function_a_method_call_would_reach_does_not_load`, `a_function_of_the_report_api_is_not_replaced`, `a_user_module_defining_an_engine_function_does_not_load_its_importer`, `a_built_in_script_defining_an_engine_function_is_an_internal_error`, `a_report_reusing_no_engine_name_loads`; `rhai_closure_limits.rs`, `a_module_calls_the_importers_function_in_place_of_the_engines` |
|   ✅   | Duplicate GUIDs across transactions and template transactions | Yes: a save that would write one is refused, on XML as on SQLite, since GnuCash keeps both in one collection. Done: the Session refuses the save before anything is written, no backup included, and a Mutation that would make such a pair, both as `invalid` with `data.reason` `shared-guid` (SQLite gave `file-format` from its primary key, after the backup); the session goes on ([`file-fidelity.md`](file-fidelity.md), "Writing"; [`sql-fidelity.md`](sql-fidelity.md) §8; [`differences-from-gnucash.md`](differences-from-gnucash.md)). `shared_guid.rs`: `an_xml_book_read_with_a_shared_guid_opens_but_does_not_save_until_it_is_removed`, `a_mutation_giving_a_transaction_a_template_transaction_s_guid_is_refused_on_xml`, `…_on_sqlite`; `session.rs` (unit), `an_sqlite_save_of_a_shared_guid_is_refused_before_anything_is_written`; `dispatch.rs`, `a_save_of_a_shared_transaction_guid_is_invalid` |
|        | The SQLite reader's split lists                               | No: a memory change with nothing in v0.6 depending on it                                                                                                                             |
|        | `setup --remove` and an empty servers object                  | No: harmless to every known harness                                                                                                                                                  |
|        | `setup` updates through a harness CLI                         | No, unless the v0.5 smoke tests find a harness that fails the add                                                                                                                    |
|        | Rhai closure bugs                                             | Unchanged: upstream's                                                                                                                                                                |

## Interfaces

`redAleApi` 0.6.0, additive over 0.5.0:

| Status | Interface    | Notes                                                                                                                                                                                                                                 |
|:------:|--------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | JSON-RPC API | `help.topics`, `help.get`, `help.search`; `data.help` on errors; `help` on each method in `api.schema`; the invoice and image option types. The 0.5.0 document is committed beside the others and `api_additive.rs` checks against it. Done: the `help.*` methods, `data.help` and `help` in `api.schema` ("The Manual"); the 0.5.0 document (`api_additive.rs`); the `invoice` and `image` option types (`invoice_and_image_options_read_and_write_their_wire_form`); the report result's `exact` number display, `class`, `css` and `image` blocks (`additions_c_finish_and_round_trip`, `additions_c_render_classes_css_and_pictures`); the Business book options and `defaultInvoiceReport` (`business_options_are_set_read_and_saved`); built-in saved configurations and `ReportInfo.readOnly` (`report_dashboard.rs`) |
|   ✅   | MCP server   | Pages as resources; generated instructions; longer tool descriptions; `--tool-descriptions short`. The invoice and the Dashboard are reached through the report tools. Done: "The Manual", MCP; the invoice and the Dashboard through `report.run` and `report.export`, the same methods as JSON-RPC, and their Pages (`report/printable-invoice`, `report/dashboard`) as resources |
|   ✅   | CLI          | `red-ale help`; `red-ale manual build`; `red-ale methods` with Page IDs. Done: `red-ale help` and `red-ale methods`, and `red-ale manual build` ("The Manual", Static site) |

## Testing

As v0.2, v0.4 and v0.5 did for reports ([`reports.md`](reports.md) §13), and as [`manual.md`](manual.md) sets out for
the Manual:

- **GnuCash's own test, ported.** `test-invoice.scm` is read as text and each assertion becomes a Rust test on the
  invoice data GnuCash's tests build with `create-test-invoice-data`
  (`5.17:bindings/guile/test/test-engine-extras.scm:487-849`), already a fixture (`reports/transaction.gnucash`,
  v0.5). **No test runs Guile.**
- **Hand-worked cases** for every invoice option and branch the ported test doesn't reach (bills, vouchers, credit
  notes, payments, each display column), each citing the `.scm` lines it follows.
- **The Dashboard** runs as GnuCash's four children laid out in two columns, and its bytes equal those of a
  Multicolumn View saved by hand with the same children.
- **Golden bytes:** `report_golden.rs` records the invoice and the Dashboard on every fixture book, XML and SQLite,
  and every earlier report's bytes stay unchanged.
- **Reports only read:** a run leaves the Book's revision, the dirty flag and the file unchanged, XML and SQLite. No
  run opens a file: the Picture Location set to a path that doesn't exist gives the same bytes as any other string.
- **The Manual:** every Example runs as a test, the custom-report Guide's script among them; the coverage tests; the
  site built twice gives the same bytes; the MCP resources list every Page; `help.search` ranks a fixed set of
  queries as recorded.

## Order of work

1. **The research note** on the invoice.
2. **`red-ale-docs`, `help.*` and the doors** (MCP, CLI, site), with the Start Page, so later work writes its Pages
   as it goes.
3. **The v0.5 follow-ups** taken up here, before new scripts land.
4. **Invoice reads and option types** in the engine, then **the Printable invoice**.
5. **The Dashboard.**
6. **User reports** in the Manual.
7. **Reference pages** for every method, error and template; Concept pages, Guides and From GnuCash pages; the
   GnuCash documentation checklist.
8. **Deployment**, interfaces and the audit.

v0.6 ships whole: the Manual and the two reports together, not split into v0.6 and v0.6.x.

## Decide before code

### Decided

The maintainer's decisions, during the planning of v0.6. Each ❌ row is ❌ in `features.md`, and
[`roadmap.md`](roadmap.md) is updated.

1. **PDF export is ❌.** GnuCash exports PDF by printing its WebKit report view (`5.17:gnucash/gnome/gnc-plugin-page-report.cpp:2021`).
   `report.export` already gives a report as self-contained
   HTML, and general tools such as pandoc or an HTML-to-PDF converter do that job better than a renderer red-ale would
   have to build and keep. A Guide shows the way.
2. **Most reports are ❌; users write their own.** A user, or an agent on their behalf, writes the report they need as
   a user script ("User reports"), so red-ale ports only what GnuCash itself relies on: the Printable invoice, which
   Print Invoice runs by default, and the Dashboard, which costs a saved configuration. ❌:
    - the customer, vendor and employee reports, the Owner report (new) and the Job Report (`standard/new-owner-report.scm`); Receivable and
      payable aging (`standard/new-aging.scm`); the Customer Summary (`standard/customer-summary.scm`);
    - the Easy and Fancy invoices (`standard/invoice.scm`); the Tax invoice and Australian Tax Invoice (`standard/taxinvoice.scm`); the
      Receipt (`standard/receipt.scm`); the Lot Viewer (`standard/lot-viewer.scm`,
      experimental in GnuCash, `:203-205`);
    - every tax report: US income tax (TXF), German income tax, Income and GST Statement. Each also encodes one
      country's forms and rules, which red-ale would have to keep current: the German report still follows the 2011
      VAT return (`5.17:libgnucash/tax/de_DE/txf.scm:15-17`). The per-account tax-related flag and TXF codes stay ✅:
      red-ale keeps and edits them, so GnuCash's reports still work on the book.

   With the Tax invoice and the Receipt gone, red-ale needs no eguile templates; the Balance Sheet (eguile) was
   already ❌, and the Balance Sheet of v0.2 gives its figures.
3. **User scripts stay files.** No method takes a script's text; an agent that writes reports needs file access where
   red-ale runs.
4. **Image options hold a string**, a file name, which the HTML's `<img src>` refers to as GnuCash's does: GnuCash's
   `make-file-url` puts `file:///` in front of whatever the option holds (the note §1.4), so the page never fetches
   from the network. The server never reads the file. [`reports.md`](reports.md) §8's rule that an exported page holds
   no image or URL gains this one exception.
5. **The Dashboard is a built-in saved configuration** of the Multicolumn View: named "Dashboard", read-only, two
   columns, with GnuCash's four children and spans (`standard/dashboard.scm:44-63`). `report.list` shows it, and
   `report.run { report: "Dashboard" }` runs it. Its GUID is the menu item's (`ad80271c890b11dfa79f2dcedfd72085`,
   `5.17:gnucash/gnome/report-menus.scm:117`), since GnuCash gives the view none of its own. A user copies it into
   their own saved configuration to change it.
6. **Printing an invoice** is `report.run` with an invoice template and its Invoice Number option, as
   `gnc:invoice-report-create` does (`5.17:gnucash/report/reports/reports.scm:68-80`). No new method. The Book's
   Default Invoice Report, falling back to the Printable invoice, is named on the invoice's From GnuCash page. GnuCash
   prints the Printable invoice when the Default Invoice Report is not an invoice report
   (`5.17:gnucash/gnome/dialog-invoice.c:957-963`), so the page tells clients to do the same when it names a template
   red-ale doesn't have (the Easy or Fancy invoice, say).
7. **The Manual's open items** ([`manual.md`](manual.md), "Settled for v0.6"): the site publishes the latest release
   only; synonyms are generated from the glossary's ⚠ entries with a short hand-kept supplement; every method needs
   a "what it is for" paragraph and one Example, and nothing more is required.
8. **v0.5 follow-ups:** the two in "Carried forward from v0.5" are taken up; the rest wait.
9. **No split:** v0.6 ships the Manual and the two reports together.
10. **MySQL and PostgreSQL books are ❌**, so v0.7 has no rows ([`roadmap.md`](roadmap.md), "v0.7"); the `sql-book`
    error no longer names a version.
11. **The note's open questions** (§6) are settled as it proposes: the invoice's result has an empty title and a
    heading block, as GnuCash's document has none (1); an invoice GUID that names no document reads as `()` and
    renders the no-invoice warning (2); refused CSS in the CSS option is left out with a `debug` log entry (3); class
    names are in scope (4); built-in saved configuration names are not reserved (5); an invoice report is a template
    whose General › Invoice Number is of type `invoice` (6); the Business book options are added (8); clients fall
    back to the Printable invoice (9, item 6); the new number display is `exact` (10). Question 7, porting the Easy
    and Fancy invoices as built-in saved configurations, stays ❌ under item 2.

### Glossary

New terms enter [`glossary.md`](glossary.md) before code: *Manual*, *Page*, *Page ID*, *Reference page*, *Guide*, *Concept page*, *Example*
(from [`manual.md`](manual.md)). Clash to flag: an *Example* in the Manual is a test, not
sample data.

## Done when

- Both rows above are ✅, with Ported `0.6` in `features.md`, and the ❌ rows of "Decided" are ❌ there.
- The Printable invoice gives the figures `test-invoice.scm` and the hand-worked cases hold, on XML and SQLite
  fixtures; the Dashboard runs as GnuCash lays it out; every earlier report's golden bytes are unchanged.
- No report changes a Book or opens a file.
- Every method, error code and report template has its Reference page with an Example; the custom-report Guide's
  script runs; every Example passes; the coverage tests pass; the site rebuilds byte for byte and publishes from a
  release tag.
- Every JSON-RPC method is an MCP tool with the same name, params schema and hints; every Page is an MCP resource;
  any API change is additive, checked by a test.
- v0.1's to v0.5's done-when checks still pass.

## Follow-ups

- **Accounts and splits shared with templates.** GnuCash keeps template accounts in the Book's one account collection
  and template splits in its one split collection, and an SQL book keeps each in one table, as it does transactions.
  v0.6 checks only transactions against template transactions ("Carried forward from v0.5"). Accounts would cost the
  same lookup per template account; splits a pass over every split of the Book at each save.
- **Script functions reached by a method call between scripts.** A method call `x.f(a)` reaches a script `fn f(a)`
  reading `x` as `this` ([`reports.md`](reports.md) §2, "One name, one function"), so a report `fn f()` could also
  stand in for a module's own `fn f(x)` called as `x.f()`. The load check compares scripts by name and arity only;
  it refuses this between a script and the engine, not between two scripts.
