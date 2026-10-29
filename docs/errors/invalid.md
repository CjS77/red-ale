<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/invalid
summary: The request is not valid for the book as it stands.
---
**Cause.** The params are well formed but the change they ask for breaks a rule of the book: a commodity the
account cannot hold, an account type that cannot sit under that parent, a value that is out of range, a posted date
before the auto read-only threshold (`data.reason` `auto-read-only`, with `data.date` and `data.threshold`), and so
on. The message names the rule. Nothing was changed. Changing a transaction that is already before the threshold is a
[conflict](error/conflict) instead.

**Fix.** Read the message and change the request to respect it. The method's Reference page lists its rules.
