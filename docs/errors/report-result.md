<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/report-result
summary: A report script returned a malformed result.
---
**Cause.** The script finished but what it returned is not a valid report document. `data.path` says where in the
result the first problem is.

**Fix.** Fix the script so it returns the report structure the Manual's custom-report Pages describe.
