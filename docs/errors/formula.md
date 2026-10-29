<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/formula
summary: A scheduled transaction's formula could not be used.
---
**Cause.** A formula in a scheduled transaction does not parse, uses a function red-ale does not compute, or cannot
be evaluated (for example it divides by zero or names a variable that has no value). `data.reason`, `data.formula`
and `data.position` say what and where.

**Fix.** Correct the formula at that position, or give the variable a value.
