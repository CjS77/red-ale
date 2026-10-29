<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.discard
related: method/scheduledTransaction.sinceLastRun, method/scheduledTransaction.apply
---
Drops a held Since Last Run proposal without applying it. Proposals are also dropped when applied, when 16 newer ones
push them out and when the book closes, so discarding is only for tidiness. It changes nothing in the book.

## Examples

```example
{"method": "scheduledTransaction.discard", "params": {"proposal": "slr-999999"}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "sinceLastRunProposal", "id": "slr-999999", "help": "error/not-found"}}}
```

A held proposal is dropped with `{"method": "scheduledTransaction.discard", "params": {"proposal": "slr-1"}}`, which
answers `{"discarded": true}`.
