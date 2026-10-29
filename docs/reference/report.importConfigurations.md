<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.importConfigurations
related: method/report.importStyleSheets, method/report.saveConfiguration
---
Imports the saved report configurations from GnuCash's `saved-reports-2.8` file (or `saved-reports-2.4` if there is no
2.8 one), given as its `text`. The server never reads a path: the CLI's `--file` reads the file on your side. The text
is data, never run, although it is Scheme.

Each entry becomes a saved configuration with GnuCash's GUID and name, configuring the template it names, storing only
the options that differ from the template's defaults. Entries go in file order, each on its own, so one that cannot be
imported does not stop the others. `skipped` says why for each: `malformed`, `untranslated` (a report red-ale does not
have), `exists` (importing twice adds nothing), `conflict` (the name is taken) or `invalid-option`. Options the template
does not have are listed in `ignored`. With `dryRun` nothing is written.

## Examples

```example book=none
{"method": "report.importConfigurations", "params": {"text": "(let ()\n  (define (options-gen)\n    (let\n         (\n           (options (gnc:report-template-new-options/report-guid \"d8b63264186b11e19038001558291366\" \"Net Worth Linechart\"))\n           (new-embedded-report-ids '()) ;; only used with Multicolumn View Reports\n         )\n; Section: General\n\n(let ((option (gnc:lookup-option options\n                                 \"General\"\n                                 \"Step Size\")))\n  ((lambda (o) (if o (gnc:option-set-value o 'QuarterDelta))) option))\n\n\n      options\n    )\n  )\n  (gnc:define-report\n    'version 1\n    'name \"Quarterly net worth\"\n    'report-guid \"5e1f0a2b3c4d4e5f8a9b0c1d2e3f4a5b\"\n    'parent-type \"d8b63264186b11e19038001558291366\"\n    'options-generator options-gen\n    'menu-path (list gnc:menuname-custom)\n    'renderer (gnc:report-template-renderer/report-guid \"d8b63264186b11e19038001558291366\" \"Net Worth Linechart\")\n  )\n)\n", "dryRun": true}}
{"result": {"dryRun": true, "imported": [{"line": 1, "configuration": {"guid": "5e1f0a2b3c4d4e5f8a9b0c1d2e3f4a5b", "name": "Quarterly net worth", "kind": "saved", "…": "…"}, "options": {"General": {"Step Size": "QuarterDelta"}}, "ignored": []}], "skipped": []}}
```
