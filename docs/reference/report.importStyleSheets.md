<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.importStyleSheets
related: method/report.importConfigurations, method/report.styleSheets
---
Imports the style sheets from GnuCash's `stylesheets-2.0` file, given as its `text`; the server never reads a path and
never runs the text. An entry that names a predefined style sheet (`Default`, `Easy`, …) holds the user's changes to
it; any other name is a user style sheet. `skipped` says why an entry was left out: `malformed`, `untranslated` (a style
sheet template red-ale does not have), `exists` (the Configuration already has an entry for that name, so edits made in
red-ale are kept), `invalid-option` or `unchanged` (a predefined style sheet at its defaults). Image options are kept
but not rendered. With `dryRun` nothing is written.

## Examples

```example book=none
{"method": "report.importStyleSheets", "params": {"text": "(let ((template (gnc:html-style-sheet-template-find \"Footer\")))\n  (if template\n    (let ((options ((gnc:html-style-sheet-template-options-generator template))))\n\n; Section: Tables\n\n(let ((option (gnc:lookup-option options\n                                 \"Tables\"\n                                 \"Table border width\")))\n  ((lambda (o) (if o (gnc:option-set-value o '2.0))) option))\n\n (gnc:restore-html-style-sheet \"Acme\" \"Footer\" options))))\n"}}
{"result": {"dryRun": false, "imported": [{"line": 1, "styleSheet": {"name": "Acme", "template": "Footer", "predefined": false, "…": "…"}, "…": "…"}], "skipped": []}}
```
