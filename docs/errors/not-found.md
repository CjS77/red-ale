<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/not-found
summary: An entity named in the request does not exist.
---
**Cause.** An account, transaction, split, lot, price, commodity or other entity named in the request is not in the
book. `data.kind` says what kind and `data.id` what was asked for. Account names are full names with `:` between
levels, and are case-sensitive.

**Fix.** Look the entity up first ([account.list](method/account.list), [transaction.find](method/transaction.find),
[commodity.list](method/commodity.list) and so on) and pass its GUID or exact name.
