<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/arithmetic
summary: Exact arithmetic overflowed or was undefined.
---
**Cause.** Amounts are exact fractions of 64-bit integers, and the calculation needed more than they hold or
divided by zero. Nothing was changed.

**Fix.** Use smaller amounts, or a coarser denominator (`"12.34"` rather than 20 decimal places), or a non-zero rate
or divisor.
