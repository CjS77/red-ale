<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/report-limit
summary: A report script passed one of its limits.
---
**Cause.** Report scripts run in a sandbox with limits on operations, time and call depth. `data.limit` says which
was passed. This guards the server against runaway scripts.

**Fix.** Narrow the report (a shorter date range, fewer accounts) or simplify the script so it loops less.
