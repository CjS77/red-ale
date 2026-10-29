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

A held proposal is dropped with `{"method": "import.discard", "params": {"proposal": "imp-1"}}`, which answers
`{"discarded": true}`.
