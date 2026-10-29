<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/report-option
summary: A report option value is not valid.
---
**Cause.** An option was given a value its type does not allow: an unknown name, a number out of range, a choice
that is not on the list, an account that does not suit the report. `data.section`, `data.name` and `data.reason`
say which.

**Fix.** Look up the option and its allowed values on the report's Page, or via `report.options`, and send a value
of that kind.
