<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/unsupported-uri
summary: The path is a URI scheme red-ale does not use.
---
**Cause.** A book path may be a plain path, `file://`, `xml://` or `sqlite3://`. Any other scheme is refused.

**Fix.** Pass a file path, or one of those URIs.
