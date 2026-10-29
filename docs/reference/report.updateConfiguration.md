<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/report.updateConfiguration
related: method/report.saveConfiguration, method/report.renameConfiguration
---
Changes options of a saved configuration. Options you give are set; an option given as `null` goes back to the
template's default. The configuration keeps its GUID and name. It finds the configuration among saved ones only, by name
or GUID, so a template of the same name does not make it ambiguous.

## Examples

```example book=none
{"method": "report.saveConfiguration", "params": {"name": "Net worth", "report": "Net Worth Linechart", "options": {"General": {"Step Size": "QuarterDelta"}}}}
{"result": "…"}
{"method": "report.updateConfiguration", "params": {"configuration": "Net worth", "options": {"General": {"Step Size": null}}}}
{"result": {"name": "Net worth", "kind": "saved", "…": "…"}}
```
