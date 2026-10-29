<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/report-script
summary: A report script failed while running.
---
**Cause.** A report's Rhai script raised an error at run time. `data.source`, `data.line` and `data.message` say
where and why, and `data.log` holds what the script logged before it stopped.

**Fix.** For a report you wrote, fix the script at that line. For a built-in report, the options are probably
unusual: reset them and report the failure.
