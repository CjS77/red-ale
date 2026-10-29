<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/method-not-found
summary: No method has that name.
---
**Cause.** The request named a method red-ale does not have. Method names are case-sensitive and namespaced:
`account.create`, not `createAccount` or `Account.Create`.

**Fix.** List the methods with `red-ale methods` or [help.topics](method/help.topics), or search for the task with
[help.search](method/help.search). A name from a newer red-ale than the server you are talking to also lands here.
