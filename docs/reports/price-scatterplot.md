<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/price-scatterplot
related: method/report.run, method/report.options, report/securities, report/investment-portfolio
---
A scatter plot of the price of one commodity in the report currency over time. Use it to see how a security or a
foreign currency has moved. The commodity and the currency must differ, or it says `Identical commodities`. In
GnuCash: Reports ▸ Assets & Liabilities ▸ Price Scatterplot.

The options that matter are `Price/Price of Commodity`, `Price/Report's currency`, `Price/Price Source` (actual
transactions or the price database), `Price/Invert prices` and `General/Start Date` and `End Date`.

## Examples

```example fixture=reports/stock-test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Price Scatterplot", "options": {"General": {"Start Date": {"absolute": "1980-01-01"}, "End Date": {"absolute": "1981-12-31"}}, "Price": {"Price of Commodity": "NASDAQ::AAPL"}}}}
{"result": {"title": "Price Scatterplot", "sections": [{"blocks": [{"kind": "chart", "type": "scatter", "title": ["Price Scatterplot", "AAPL - 01/01/80 to 12/31/81"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
