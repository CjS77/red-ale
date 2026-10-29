<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/import-file
summary: An import file cannot be read as its format.
---
**Cause.** The file given to an import is not readable as the format named (QIF, CSV, OFX and so on) at all.
`data.file`, `data.line` and `data.reason` say where it gave up.

**Fix.** Check that you chose the right format and encoding, and that the file is whole. Fix or remove the line
named, then preview the import again.
