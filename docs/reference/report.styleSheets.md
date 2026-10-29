<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.styleSheets
related: method/report.createStyleSheet, method/report.export
---
Lists the style sheet templates (Plain, CSS, Easy, Fancy, Footer, Head or Tail) and the style sheets made from them, with
their options. The predefined style sheets are `Default`, `CSS-based stylesheet (experimental)`, `Easy`, `Technicolor`,
`Footer` and `Head or Tail`; yours are added to them. A report is rendered through one by
[report.export](method/report.export).

## Examples

```example book=none
{"method": "report.styleSheets", "params": {}}
{"result": {"templates": [{"name": "CSS", "…": "…"}, "…"], "styleSheets": [{"name": "CSS-based stylesheet (experimental)", "template": "CSS", "predefined": true, "…": "…"}, "…"]}}
```
