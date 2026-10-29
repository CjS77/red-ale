<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/invalid-params
related: method/help.get
summary: The params do not match the method's schema.
---
**Cause.** The params are not what the method's schema allows: a name the method does not have (so a typo never
passes silently), a missing required field, a value of the wrong type, positional params, or a JSON number given as
an amount. The message names the field. Nothing was changed.

**Fix.** Check the method's Reference page or its schema (`api.schema`). Send amounts as strings (`"12.34"`) or
as `{"num": "1234", "denom": "100"}`, never as JSON numbers, which could lose digits. Spell params in camelCase.
