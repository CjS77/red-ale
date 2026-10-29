<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/import.discard
related: method/import.preview, method/import.apply
---
Drops a held Import proposal without applying it. Proposals are also dropped when applied, when 16 newer ones push
them out and when the book closes, so discarding is only for tidiness. It changes nothing in the book.

## Examples

```example
{"method": "import.discard", "params": {"proposal": "imp-999999"}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "importProposal", "id": "imp-999999", "help": "error/not-found"}}}
```

A held proposal is dropped, and is then gone:

```example
{"method": "import.preview", "params": {"format": "csvPrices", "files": [{"text": "2024-06-28,1.0850\n"}], "layout": {"format": "csv", "separators": ",", "skipStartLines": 0, "dateFormat": "y-m-d", "currencyFormat": "period", "encoding": "utf-8", "columns": ["Date", "Amount"], "fromCommodity": "CURRENCY::EUR", "toCurrency": "USD"}}}
{"result": {"id": "imp-1", "…": "…"}}
{"method": "import.discard", "params": {"proposal": "imp-1"}}
{"result": {"discarded": true}}
{"method": "import.apply", "params": {"proposal": "imp-1"}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "importProposal", "id": "imp-1", "help": "error/not-found"}}}
```
