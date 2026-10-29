<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/invalid
summary: The request is not valid for the book as it stands.
---
**Cause.** The params are well formed but the change they ask for breaks a rule of the book: a commodity the
account cannot hold, an account type that cannot sit under that parent, a value that is out of range, a date before
the auto read-only threshold, and so on. The message names the rule. Nothing was changed.

**Fix.** Read the message and change the request to respect it. The method's Reference page lists its rules.
