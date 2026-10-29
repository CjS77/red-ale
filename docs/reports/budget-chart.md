<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/budget-chart
related: method/report.run, method/report.options, report/budget-report, report/budget-flow
---
One bar chart for each of the selected accounts, comparing the budgeted and the actual amount in each budget period.
Use it to see at a glance which accounts are on track. The charts show the account's name and its totals for the whole
budget in their title. In GnuCash: Reports ▸ Budget ▸ Budget Chart.

The options that matter are `General/Budget`, `Accounts/Accounts` and `Levels of Subaccounts`, the range of periods
(`General/Range start` and `Range end`) and `Display/Running Sum`, which charts running totals instead of each period.

## Examples

```example fixture=reports/budget now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Budget Chart"}}
{"result": {"title": "Budget Chart", "sections": [{"blocks": [{"kind": "chart", "type": "bar", "title": ["Bank", "Bgt: $120.00 Act: $159.00"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
