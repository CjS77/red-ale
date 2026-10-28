# GnuCash 5.17 XML format: core entities

Scope: the exact bytes GnuCash 5.17 writes, and the rules it applies when it reads, for an XML book, **excluding
business objects** (those are in [`xml-format-business.md`](xml-format-business.md), "the business note"). Covered
here: the file envelope, libxml2 serialisation, value encodings, KVP slots, book, commodity, price database, account,
lot, transaction, split, the out-of-v0.1 entities that must still round-trip (template transactions, scheduled
transactions, budgets, recurrences, legacy freqspecs), entity write order, and what the reader tolerates or rewrites.
File detection, locking, backups, compression policy and the `.log` journal are in
[`storage-and-safety.md`](storage-and-safety.md); this note only touches them where the byte format depends on them.

Citations are `5.17:<path>:<line>` against tag 5.17 (commit 0e302675ff). In tables, `xml/` abbreviates
`libgnucash/backend/xml/` and `engine/` abbreviates `libgnucash/engine/`. "Oracle" means the GnuCash 5.17 library
and Python bindings in `$HOME/.local/opt/gnucash-5.17`, linked against the system **libxml2 2.9.14**
(`libxml2.so.2 => /lib/x86_64-linux-gnu/libxml2.so.2`, package `2.9.14+dfsg-1.3ubuntu3.10`) and zlib 1.3.

## 0. Facts that surprise

1. **GnuCash's load+save is byte-identical for files it wrote** (GnuCash's own `gtest-load-save-files.cpp` checks
   exactly this, §11), *except* that the order of `<gnc:budget>` elements is not reproducible: two saves of the same
   data gave different budget orders (§7.6). Transaction order also depends on the process's `LC_COLLATE` for ties
   (§7.4).
2. Text is written as raw UTF-8. Only `&`, `<`, `>` and `\r` are escaped (`&amp; &lt; &gt; &#13;`); `"`, `'`, tab and
   newline are written raw. Control characters other than tab/LF/CR, and invalid UTF-8 bytes, are replaced by `?`
   **before** writing (§3.4).
3. Two different empty-element spellings exist and depend on which libxml2 call built the element:
   `<trn:description></trn:description>` but `<act:name/>` (§3.3).
4. Pretty-printing indentation is capped at 60 spaces (30 levels) by libxml2 (§3.2).
5. On load GnuCash **normalises split denominators**: values are converted to the transaction currency's fraction and
   quantities to the account's SCU, rounding half away from zero (`-25005/1000` USD → `-2501/100`) (§10.4). Price
   values are not normalised (`12000/10` stays).
6. On load, splits inside each transaction are stably re-ordered: non-negative values first, negative values last
   (§6.6).
7. Unknown child elements inside an account, transaction, split, lot, budget, SX or recurrence, and unknown children of
   `<gnc:book>`, make the whole load fail with a parse error. Unknown children of `<gnc:commodity>` and `<price>` are
   ignored. Comments are dropped everywhere (§10.2).
8. Several element handlers' failures are silently ignored: an unparsable `<act:id>` (e.g. with surrounding
   whitespace) gives the account a **new random GUID** and orphans its splits; an `<act:parent>` that refers to a
   later account re-parents the account under the root (§10.2).
9. `gnc:count-data` for `commodity` excludes the `CURRENCY` namespace but includes `template:template`, so it never
   equals the number of `<gnc:commodity>` elements written (§2.2).
10. A currency used by any account gets `<cmdty:get_quotes/>` set on load (GnuCash 1.8 compatibility hack) unless its
    `auto_quote_control` slot is `"false"`; removing such a `<gnc:commodity>` block from a file makes GnuCash add it
    back (§10.3).

## 1. Experiments and artefacts

All under `/tmp/claude-1001/-home-code-projects-red-ale/bbc44319-413c-4d3a-9c4c-5d9e0c7bf9fe/scratchpad/xmlcore/`:

| File | What |
|---|---|
| `create.py` | Builds a book through the bindings: 21 accounts of all common types (nested, placeholder, hidden, non-standard SCU, `STOCK` in `NASDAQ:ACME` fraction 10000, `MUTUAL` in `FUND:ZETA` fraction 1000, EUR bank), 7 transactions (multi-currency, stock buy/sell in a lot, a voided one, notes, reconciled split), 4 prices, a titled lot, account colour/notes/code/tax/last-num/reconcile slots, and the string `amp& lt< gt> quot" apos' eé euro€ jp日本 tab\there nl\nhere cr\rhere ctl\x01x` in account name, description, notes, transaction description and notes, split memo and action, and lot title. |
| `rich.xml` | `create.py`'s output, decompressed (the oracle compresses by default: `file-compression` defaults to true, `5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:28-29`). |
| `augment.py` | Adds, by hand and deliberately non-canonical (unsorted keys, one-line formatting, offsets, char refs, CDATA, duplicates): every KVP type, a budget, a template-transaction block and an SX with a deferred instance → `aug.xml`. |
| `resave.py`, `rt.sh` | Open a copy with `SESSION_NORMAL_OPEN`, mark the book dirty (`qof_book_mark_session_dirty` through ctypes, no data edit), save, decompress, `cmp` with the input. |
| **`rich-canonical.gnucash`** | `aug.xml` after one oracle load+save: an uncompressed 5.17-written book with every core entity, every slot type, SX, template transactions and a budget. **Use it as a fixture.** |
| `variants.py`, `var/<name>/` | 37 read-quirk variants of `rich-canonical.gnucash` (in.xml, out.xml); results in §10.2. |
| `collate.py`, `c_*` | Transaction-order collation test (§7.4). `budgets.py`, `b*`, `bx*`: budget order test (§7.6). `deep.xml`, `dp*`: indentation cap (§3.2). `esc.c`: direct libxml2 2.9.14 escaping test (§3.4). |

Results: `rich.xml` → load+save → identical; `rich-canonical.gnucash` → load+save → identical; GnuCash's own fixtures
`sample1.gnucash` and `bug-746937-sx-split-order.gnucash` (`libgnucash/backend/xml/test/test-files/load-save/`) →
identical. The compressed output equals `gzip -6 -n` of the plain output on this machine (header
`1f 8b 08 00 00 00 00 00 00 03`: no name, mtime 0, OS Unix).

## 2. File envelope

### 2.1 Bytes before the book

Written with `fprintf`, not libxml2 (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1268-1299`, `:1251-1256`):

```
<?xml version="1.0" encoding="utf-8" ?>\n
<gnc-v2
     xmlns:gnc="http://www.gnucash.org/XML/gnc"
     ...one line per prefix: "\n" + 5 spaces + xmlns:P="http://www.gnucash.org/XML/P"...
     xmlns:vendor="http://www.gnucash.org/XML/vendor">\n
<gnc:count-data cd:type="book">1</gnc:count-data>\n
<gnc:book version="2.0.0">\n
```

Prefix order: core `gnc act book cd cmdty price slot split sx trn ts fs bgt recurrence lot`
(`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1273-1287`), then business plugins in registry order
(`5.17:libgnucash/backend/xml/gnc-backend-xml.cpp:172-182`): `addr billterm bt-days bt-prox cust employee entry
invoice job order owner taxtable tte vendor`. All 29 are always written, whatever the book contains. The `>` closes
the last declaration's line. Note the space before `?>`.

Verbatim (oracle, `rich.xml` lines 1-33, middle lines elided):

```
<?xml version="1.0" encoding="utf-8" ?>
<gnc-v2
     xmlns:gnc="http://www.gnucash.org/XML/gnc"
     xmlns:act="http://www.gnucash.org/XML/act"
     ...
     xmlns:tte="http://www.gnucash.org/XML/tte"
     xmlns:vendor="http://www.gnucash.org/XML/vendor">
<gnc:count-data cd:type="book">1</gnc:count-data>
<gnc:book version="2.0.0">
<book:id type="guid">84a3c7eb9aba48bab0c946d96019b2a0</book:id>
<gnc:count-data cd:type="commodity">3</gnc:count-data>
<gnc:count-data cd:type="account">21</gnc:count-data>
<gnc:count-data cd:type="transaction">7</gnc:count-data>
<gnc:count-data cd:type="price">4</gnc:count-data>
```

The file ends `</gnc:book>\n</gnc-v2>\n\n`: one blank line (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1030`,
`:1330`).

### 2.2 `gnc:count-data`

Each line is `fprintf("<gnc:count-data cd:type=\"%s\">%d</gnc:count-data>\n")`; **a count of 0 is not written**
(`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:871-927`, `:884`, `:913`). Order and computation
(`:989-1008`):

| # | `cd:type` | Value | Notes |
|---|---|---|---|
| 0 | `book` | 1 | Before `<gnc:book>` (`:1311`). |
| 1 | `commodity` | Σ commodities over all namespaces except `CURRENCY` (`5.17:libgnucash/engine/gnc-commodity.cpp:1616-1642`) | Includes `template:template` (always present, §6.2), so ≥ 1. Not the number of `<gnc:commodity>` elements: currencies are excluded from the count but some are written. |
| 2 | `account` | 1 + number of descendants of the root | Root counted; **template accounts not counted** (verified: 21 with 2 template accounts present). |
| 3 | `transaction` | Number of distinct transactions reachable through splits of the main account tree (`5.17:libgnucash/engine/Transaction.cpp:2491-2497`) | **Template transactions not counted** (verified). A transaction with no splits is neither counted nor written. |
| 4 | `schedxaction` | length of the SX list | |
| 5 | `budget` | size of the budget collection | |
| 6 | `price` | number of prices in the price DB | |
| 7… | business types | per type, in registry order | `gnc:GncBillTerm`, `gnc:GncCustomer`, `gnc:GncEmployee`, `gnc:GncEntry`, `gnc:GncInvoice`, `gnc:GncJob`, `gnc:GncOrder`, `gnc:GncTaxTable`, `gnc:GncVendor`; see the business note. |

Lines 1-7… are written **after** `<book:id>` and `<book:slots>` (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:986-1011`).

Read: the value is parsed with `string_to_gint64`; known types fill progress counters only; an unknown type is logged
and ignored (`:367-454`, `:438-449`). Missing, wrong or extra counts change nothing (verified: `no_counts`,
`wrong_counts` variants give canonical output). The writer recomputes them.

### 2.3 Inside `<gnc:book>`, in write order

`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:975-1034`:

1. `<gnc:book version="2.0.0">` (fprintf; version string `5.17:libgnucash/backend/xml/gnc-book-xml-v2.cpp:49`).
2. `<book:id type="guid">…</book:id>` (`5.17:libgnucash/backend/xml/gnc-book-xml-v2.cpp:84-113`).
3. `<book:slots>` only if the book's frame is non-empty.
4. count-data lines (§2.2).
5. Commodities (§6.2).
6. `<gnc:pricedb version="1">` if it holds ≥ 1 price (§6.3).
7. Accounts of the main tree, root first (§6.4).
8. Transactions (§6.5).
9. `<gnc:template-transactions>` if the template root has ≥ 1 descendant (§8.1).
10. `<gnc:schedxaction>`s (§8.2).
11. `<gnc:budget>`s (§8.3).
12. Business objects, per type in registry order (business note).
13. `</gnc:book>\n`, `</gnc-v2>\n\n`.

Every entity of 2, 3, 5, 7, 8, 10, 11 is written by `xmlElemDump(out, NULL, node)` followed by `fprintf("\n")`.

### 2.4 Book element (read)

The main parser accepts under `<gnc-v2>`: `gnc:count-data`, `gnc:book`, and, for pre-book files, `gnc:pricedb`,
`gnc:commodity`, `gnc:account`, `gnc:transaction`, `gnc:schedxaction`, `gnc:template-transactions`. Under
`<gnc:book>`: `book:id`, `book:slots`, `gnc:count-data`, `gnc:pricedb`, `gnc:commodity`, `gnc:account`,
`gnc:budget`, `gnc:transaction`, `gnc:schedxaction`, `gnc:template-transactions`, plus the business tags
(`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:715-756`). Anything else: "Tag <x> not allowed in current context",
load fails (`5.17:libgnucash/backend/xml/sixtp.cpp:419-421`). Element order under `<gnc:book>` is free, but references
must resolve when the referring element ends: commodities before accounts and prices, parents before children,
accounts (and lots, which live in accounts) before transactions, template accounts before SXes.
The `version` attributes of `gnc-v2`, `gnc:book` and all entities are not checked (verified: `version="9.9.9"` and an
extra attribute on an account load fine), except SX, which only logs a warning (§8.2).

## 3. Serialisation (libxml2 2.9.14 as GnuCash drives it)

### 3.1 Who writes which bytes

| Bytes | Mechanism | Citation |
|---|---|---|
| XML declaration, `<gnc-v2 …>`, count-data, `<gnc:book …>`, `<gnc:pricedb version="1">`, `<gnc:template-transactions>`, closing tags of these | `fprintf` | `xml/io-gncxml-v2.cpp:913, 982, 1030, 1107, 1190, 1268-1299, 1330` |
| Every other top-level entity | `xmlElemDump(out, NULL, node)` = `xmlNodeDumpOutput(buf, NULL, node, level 0, format 1, encoding NULL→"UTF-8")`, then `"\n"` | e.g. `xml/io-gncxml-v2.cpp:1068, 1154, 1217`, `xml/io-utils.cpp:45` |
| Each `<price>` | `"  "` + `xmlNodeDumpOutput(outbuf, NULL, price, level 1, format 1, NULL)` + `"\n"` | `xml/io-gncxml-v2.cpp:1121-1131` |

### 3.2 Pretty-printing algorithm

Equivalent pseudo-code (verified against oracle output, including a 35-level KVP frame):

```
dump(node, level, fmt):
    if node is text: out(escape_text(node.content)); return
    out("<" + name); for (k, v) in attrs: out(" " + k + "=\"" + escape_attr(v) + "\"")
    if node has no children: out("/>"); return
    out(">")
    pretty = fmt and none of node.children is a text node      # one text child switches it off
    if pretty: out("\n")
    for c in node.children:
        if pretty: out(indent(level + 1))
        dump(c, level + 1, pretty)
        if pretty: out("\n")
    if pretty: out(indent(level))
    out("</" + name + ">")

indent(n) = "  " * min(n, 30)          # libxml2 caps at 60 spaces
```

Consequences: an element with element children only is one per line, indented 2 spaces per level from the entity
(level 0); an element with a text child is written on one line and its content is never re-indented (a value containing
`\n` continues at column 0 on the next line, see §3.5); an element with no children is `<x/>`. Attribute order is
creation order; GnuCash creates at most one attribute (`version` or `type`) per element, except `gnc:count-data`.
Prices start at level 1, so `<price>` is indented 2 and its children 4 (§6.3 excerpt). Elements nested more than 30
levels below an entity stay at 60 spaces (verified, `dp/out.xml` line 440: `k5` key at column 61).

### 3.3 Empty values: two spellings

The read side cannot tell them apart (both give `""`), but the writer's choice is fixed per field:

| Built with | Empty content gives | Used for |
|---|---|---|
| `text_to_dom_tree` (`xmlNodeAddContent`, adds nothing for `""`) | `<x/>` | `act:name`, `act:type`, `cmdty:space`/`cmdty:id`/`cmdty:name`/`cmdty:xcode`/`cmdty:quote_source`/`cmdty:quote_tz` in `<gnc:commodity>`, `price:source`, `price:type`, `bgt:name`, `bgt:description`, `recurrence:period_type`, numbers (`5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:45-58`) |
| `xmlNewTextChild(parent, NULL, tag, "")` (creates an empty text node) | `<x></x>` | `trn:num`, `trn:description`, `split:memo`, `split:action`, `slot:key`, KVP `string` values, `cmdty:space`/`cmdty:id` inside commodity *references*, `sx:name`, `sx:enabled`… |
| `xmlNewChild(…, NULL)` / node with no children | `<x/>` | `act:non-standard-scu`, `cmdty:get_quotes`, empty `frame`/`list` slot values, `trn:splits` of a split-less transaction, empty `sx:schedule` |

Verified: `<act:name/>` for an empty account name, `<trn:description></trn:description>` for an empty description,
`<bgt:description/>`, `<cmdty:quote_tz/>`, `<slot:value type="string"></slot:value>`, `<slot:value type="frame"/>`,
`<slot:value type="list"/>`. Several of these fields are skipped entirely when empty (§6); the table only matters when
they are written.

### 3.4 Character handling

**Before the tree is built** every user string passes `checked_char_cast`
(`5.17:libgnucash/backend/xml/gnc-xml-helper.cpp:29-48`): while the string is not valid UTF-8, the first invalid
byte is replaced with `?` (so each invalid byte becomes one `?`); then every byte `0x01..0x1F` except `0x09`, `0x0A`,
`0x0D` becomes `?`. The same function is applied on read to character data and attribute values
(`5.17:libgnucash/backend/xml/sixtp-to-dom-parser.cpp:66,95`).

**Text content** (`xmlEscapeContent`, encoding UTF-8):

| Character | Written as |
|---|---|
| `&` | `&amp;` |
| `<` | `&lt;` |
| `>` | `&gt;` (also inside `]]>`) |
| `\r` (U+000D) | `&#13;` |
| `"`, `'`, `\t`, `\n`, DEL, U+00A0, U+FEFF, U+FFFE, any non-ASCII incl. non-BMP | raw UTF-8 bytes |

**Attribute values** (doc NULL, so not UTF-8 mode): `&`→`&amp;`, `<`→`&lt;`, `>`→`&gt;`, `"`→`&quot;`,
`\t`→`&#9;`, `\n`→`&#10;`, `\r`→`&#13;`, non-ASCII → `&#xE9;` (uppercase hex, no padding). GnuCash only writes
constant ASCII attribute values, so this matters only for a generic pass-through.

Verified with `esc.c` against the system libxml2 and in the oracle. Oracle bytes for the account name (`xxd`):

```
3c61 6374 3a6e 616d 653e 4368 6563 6b69   <act:name>Checki
6e67 2061 6d70 2661 6d70 3b20 6c74 266c   ng amp&amp; lt&l
743b 2067 7426 6774 3b20 7175 6f74 2220   t; gt&gt; quot" 
6170 6f73 2720 65c3 a920 6575 726f e282   apos' e.. euro..
ac20 6a70 e697 a5e6 9cac 2074 6162 0968   . jp...... tab.h
6572 6520 6e6c 0a68 6572 6520 6372 2623   ere nl.here cr&#
3133 3b68 6572 6520 6374 6c3f 783c 2f61   13;here ctl?x</a
6374 3a6e 616d 653e 0a                    ct:name>.
```

On read, `&#13;` gives back `\r`; character references, predefined entities and CDATA sections are decoded and
re-written by the rules above (verified: input `&#xE9;&#233;&#x20AC;&#x9;tab&#13;&#10;&quot;&apos;` is written
`éé€<TAB>tab&#13;<LF>"'`; `<![CDATA[<cdata & stuff>]]>` is written `&lt;cdata &amp; stuff&gt;`). Only the five
predefined entities resolve (`5.17:libgnucash/backend/xml/sixtp.cpp:611-614`). XML end-of-line normalisation applies
on read: a raw CR LF or lone CR in the file becomes LF (verified: a whole file converted to CRLF loads and saves to the
canonical LF file).

### 3.5 Multi-line text

A value with `\n` is written raw inside a single-line element, so the continuation starts at column 0:

```
  <act:description>desc amp&amp; lt&lt; gt&gt; quot" apos' eé euro€ jp日本 tab	here nl
here cr&#13;here ctl?x</act:description>
```

### 3.6 libxml2 version dependence

Everything in §3.2-3.4 is libxml2 2.9.14 behaviour. GnuCash 5.17 builds for Windows/macOS/Flatpak may bundle a
different libxml2. Files GnuCash writes there can differ in escaping or empty-element spelling if libxml2's serialiser
changed (open question 1). red-ale must therefore **read** any well-formed escaping, and should write the 2.9.14 form.

## 4. Value encodings

| Kind | Written | Read | Citation |
|---|---|---|---|
| GUID | 32 lowercase hex digits, element attribute `type="guid"` | `type` must be `guid` or `new`, else failure; text exactly 32 hex digits (either case), else Boost's UUID parser (accepts `{…}` and hyphenated forms); surrounding whitespace fails | `xml/sixtp-dom-generators.cpp:88-107`, `xml/sixtp-dom-parsers.cpp:47-69`, `engine/guid.cpp:91-111, 217-234` |
| Numeric (`gnc_numeric`) | `printf("%" PRId64 "/%" PRId64, num, denom)`: sign on the numerator, denominator as stored (never reduced), zero is `0/<denom>` | Fast path `[-]digits[/digits]` with denom ≥ 1 (`"7"` → 7/1); else `GncNumeric(string)` accepts decimals (`"1.50"` → 150/100), hex and surrounding junk (`" 2500/100 "` works); failure → **0/1** with a warning | `engine/gnc-numeric.cpp:1376-1385, 1406-1426, 216-245`, `xml/sixtp-dom-parsers.cpp:429-437` |
| Integer | `%" G_GINT64_FORMAT` (`-9223372036854775808` round-trips) | trim ASCII space both ends, `from_chars`, all characters consumed | `xml/sixtp-dom-generators.cpp:61-71`, `xml/sixtp-utils.cpp:144-182` |
| Unsigned (`guint`, `guint16`) | `%u` | as integer | `xml/sixtp-dom-generators.cpp:74-85` |
| Double | `g_strdup_printf("%24.18g")` then strip spaces: `0.1` → `0.100000000000000006`, `12345678901234567890` → `1.23456789012345672e+19`, `3` → `3` | trim, `from_chars<double>` | `xml/sixtp-dom-generators.cpp:189-202`, `xml/sixtp-utils.cpp:161-173` |
| Time (`time64`) | `<TAG>\n  <ts:date>YYYY-MM-DD HH:MM:SS +0000</ts:date>\n</TAG>` in UTC; `INT64_MAX` → element omitted | exactly one `ts:date` child (other element children ignored, so legacy `<ts:ns>` is ignored); text parsed by `GncDateTime`: fast path for exactly `YYYY-MM-DD HH:MM:SS +0000` and `YYYYMMDDHHMMSS`; otherwise regex `YYYY-MM-DD HH:MM:SS[.frac] [±HH[[:]MM]]`; the offset is applied (`10:59:00 -0500` → `15:59:00 +0000`, `+0030` → 30 min earlier, verified, despite the "<1 h offset is reset" comment); fractional seconds truncated; year outside 1400..9999 or unparsable → `INT64_MAX`, which callers turn into 0 with a warning | `xml/sixtp-dom-generators.cpp:134-147`, `engine/gnc-datetime.cpp:381-416, 432-484, 631-636`, `xml/sixtp-dom-parsers.cpp:441-488, 742-750` |
| Date (`GDate`) | `<TAG>\n  <gdate>YYYY-MM-DD</gdate>\n</TAG>` (`g_date_strftime("%Y-%m-%d")`) | exactly one `gdate` child; `sscanf("%d-%d-%d")` then `g_date_valid` | `xml/sixtp-dom-generators.cpp:149-166`, `xml/sixtp-dom-parsers.cpp:491-538` |
| Boolean (element) | `TRUE` / `FALSE` (`boolean_to_dom_tree`; no core writer uses it) | case-insensitive **prefix** `true` (4 chars) / `false` (5) | `xml/sixtp-dom-generators.cpp:39-42`, `xml/sixtp-dom-parsers.cpp:110-130` |
| Commodity reference | `<TAG>\n  <cmdty:space>NS</cmdty:space>\n  <cmdty:id>MNEMONIC</cmdty:id>\n</TAG>` | both children required once each, any order, values `g_ascii_isspace`-stripped; then looked up in the commodity table (miss → NULL) | `xml/sixtp-dom-generators.cpp:109-131`, `xml/sixtp-dom-parsers.cpp:560-641` |
| Plain text | content as stored (§3.4) | concatenation of all text children (`xmlNodeListGetString`), entities decoded, **whitespace kept** (`<act:name>  Cash  </act:name>` keeps the spaces) | `xml/sixtp-dom-parsers.cpp:38-44, 392-426`, `xml/sixtp-dom-parsers.h:79-104` |

KVP booleans are not XML booleans: the engine stores them as KVP strings `"true"`/`"false"` (e.g. `placeholder`,
`hidden`, `tax-related`; see the engine notes).

## 5. KVP slots

### 5.1 Structure

A non-empty frame on an entity is written as `<P:slots>` (`P` = `book`, `act`, `trn`, `split`, `lot`, `cmdty`,
`bgt`, `sx`, …) containing one `<slot>` per key; an empty top-level frame writes nothing
(`5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:328-338`).

```
<slot>
  <slot:key>KEY</slot:key>
  <slot:value type="T">…</slot:value>
</slot>
```

Keys are written in **`strcmp` byte order** (the frame is a `std::map<const char*, …, strcmp>`,
`5.17:libgnucash/engine/kvp-frame.hpp:111-121`), so `Zupper` < `a/b` < `aa-double`, and `0` < `10` < `2`. A key is one
path element: `/` in a key is kept literally (verified `a/b`).

| KVP type | `type=` | Value element | Citation |
|---|---|---|---|
| INT64 | `integer` | text, §4 | `xml/sixtp-dom-generators.cpp:239-241, 246-251` |
| DOUBLE | `double` | text, `%24.18g` | `:253-258` |
| NUMERIC | `numeric` | text, `num/denom` | `:260-265` |
| STRING | `string` | text (`<slot:value type="string"></slot:value>` when empty) | `:221-228, 267-269` |
| GUID | `guid` | text, 32 hex; **no** inner `type` attribute | `:270-275` |
| TIME64 | `timespec` | `<ts:date>` child, §4 | `:229-231, 280-286` |
| GDATE | `gdate` | `<gdate>` child, §4 | `:232-238, 288-290` |
| GLIST | `list` | one `<slot:value type=…>` child per element, in list order, no keys; `<slot:value type="list"/>` when empty | `:291-298` |
| FRAME | `frame` | nested `<slot>` children, sorted; `<slot:value type="frame"/>` when empty | `:299-307` |
| other | none | `<slot:value/>` with no `type` | `:239-241, 309-310` |

Text-valued types set the type attribute **after** content for `integer`/`double`/`numeric`/`guid` (via
`xmlNodeSetContent`, which parses entity references in its argument; harmless because these strings are ASCII
digits/hex) and after creation for `string`; the output always has exactly one attribute.

### 5.2 Excerpt (oracle, from `rich-canonical.gnucash`, Checking account)

```
  <act:slots>
    <slot>
      <slot:key>Zupper</slot:key>
      <slot:value type="string">upper sorts before lower</slot:value>
    </slot>
    <slot>
      <slot:key>aa-double</slot:key>
      <slot:value type="double">0.100000000000000006</slot:value>
    </slot>
    <slot>
      <slot:key>ee-emptyframe</slot:key>
      <slot:value type="frame"/>
    </slot>
    <slot>
      <slot:key>es-emptystring</slot:key>
      <slot:value type="string"></slot:value>
    </slot>
    <slot>
      <slot:key>frame-nested</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>a</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>deep</slot:key>
              <slot:value type="numeric">1/3</slot:value>
            </slot>
          </slot:value>
        </slot>
      </slot:value>
    </slot>
    <slot>
      <slot:key>mm-gdate</slot:key>
      <slot:value type="gdate">
        <gdate>2024-02-29</gdate>
      </slot:value>
    </slot>
    <slot>
      <slot:key>mm-timespec</slot:key>
      <slot:value type="timespec">
        <ts:date>2024-06-02 15:59:00 +0000</ts:date>
      </slot:value>
    </slot>
    <slot>
      <slot:key>nn-guid</slot:key>
      <slot:value type="guid">c080059e1aec4c0b9ad4671e80965686</slot:value>
    </slot>
    <slot>
      <slot:key>zz-list</slot:key>
      <slot:value type="list">
        <slot:value type="integer">3</slot:value>
        <slot:value type="string">two</slot:value>
        <slot:value type="frame">
          <slot>
            <slot:key>k</slot:key>
            <slot:value type="integer">1</slot:value>
          </slot>
        </slot:value>
        <slot:value type="list"/>
      </slot:value>
    </slot>
  </act:slots>
```

(Selected slots; the full block is lines 184-335 of the fixture.)

### 5.3 Read rules

`5.17:libgnucash/backend/xml/sixtp-dom-parsers.cpp:255-388`:

- Only `<slot>` children of a frame are considered; other children (text, unknown elements) are ignored.
- In a `<slot>`, the last `slot:key` and the last `slot:value` win; other children are ignored.
- The value converter is chosen by the `type` attribute: `integer double numeric string guid timespec gdate list
  frame` (`:272-284`). An unknown or missing type (e.g. legacy `binary`) yields no value and **the slot is dropped**
  (verified).
- A slot with a key and a value is stored with `frame->set({key}, value)`; a **duplicate key replaces** the earlier
  value (verified: `dup` → `second`).
- A failed `integer`/`double`/`guid`/`gdate` parse drops the slot; a failed `numeric` stores `0/1` (verified `abc` → `0/1`);
  `string` with no children gives `""`; `timespec` failure stores `INT64_MAX`… which the writer then omits
  (`time64_to_dom_tree` returns NULL, `xmlAddChild(NULL)` is a no-op) leaving `<slot>` with only a key — open
  question 4.
- `list` children are converted in order; children whose conversion fails are skipped.
- `book:slots`, `act:slots`, etc. merge into the entity's existing frame
  (`dom_tree_create_instance_slots`, `:381-386`).

## 6. Core entities

Elements are listed in write order. "Read" gives the handler rule; "req" means `dom_tree_generic_parse` fails the
entity if absent. **All entities parsed with `dom_tree_generic_parse` (account, lot, transaction, split, budget,
recurrence, SX, template block) fail on any unknown child element**; the return value of each element handler is
ignored, so a handler that fails leaves a default value (`5.17:libgnucash/backend/xml/sixtp-dom-parsers.cpp:686-731`,
`:694`). Text nodes are skipped by name (`"text"`, `:722`).

### 6.1 Book

| Element | Written when | Content | Read |
|---|---|---|---|
| `book:id` | always | GUID | sets book GUID (`5.17:libgnucash/backend/xml/gnc-book-xml-v2.cpp:114-122`) |
| `book:slots` | frame non-empty | slots | merged into book frame (`:125-137`) |

Typical book slots (`features`, `options`, `counters`, `counter_formats`, `remove-color-not-set-slots`) are
covered in `storage-and-safety.md` §10 and the business note §10. Excerpt from GnuCash's `sample1.gnucash`:

```
<book:slots>
  <slot>
    <slot:key>counters</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>gncVendor</slot:key>
        <slot:value type="integer">1</slot:value>
      </slot>
    </slot:value>
  </slot>
  <slot>
    <slot:key>options</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>Budgeting</slot:key>
        <slot:value type="frame"/>
      </slot>
    </slot:value>
  </slot>
</book:slots>
```

### 6.2 Commodity `<gnc:commodity version="2.0.0">`

Writer `5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:59-121`. "Currency" here means
`gnc_commodity_is_iso`, i.e. namespace `CURRENCY` (`5.17:libgnucash/engine/gnc-commodity.cpp:1934-1950`).

**Skip rule:** a currency with no quote flag and no slots is not written (`:68-69`). Every non-currency commodity is
written, including `template:template`, which every book has (created with the default currencies,
`5.17:libgnucash/engine/gnc-commodity.cpp:2317-2337`).

| Element | Written when | Content |
|---|---|---|
| `cmdty:space` | always | namespace (`ISO4217` is stored as `CURRENCY`) |
| `cmdty:id` | always | mnemonic |
| `cmdty:name` | non-currency and fullname non-NULL | full name (`<cmdty:name/>` if `""`) |
| `cmdty:xcode` | non-currency and CUSIP non-empty | CUSIP/ISIN |
| `cmdty:fraction` | non-currency | integer SCU |
| `cmdty:get_quotes` | quote flag | empty element `<cmdty:get_quotes/>` |
| `cmdty:quote_source` | quote flag and a source | internal name; a currency with no explicit source writes `currency` (`5.17:libgnucash/engine/gnc-commodity.cpp:1038-1047`) |
| `cmdty:quote_tz` | quote flag and tz non-NULL | tz string; `""` gives `<cmdty:quote_tz/>` (observed for every currency with a quote flag) |
| `cmdty:slots` | frame non-empty | slots (e.g. `auto_quote_control` = `"false"`, `user_symbol`) |

Excerpt (oracle):

```
<gnc:commodity version="2.0.0">
  <cmdty:space>CURRENCY</cmdty:space>
  <cmdty:id>USD</cmdty:id>
  <cmdty:get_quotes/>
  <cmdty:quote_source>currency</cmdty:quote_source>
  <cmdty:quote_tz/>
</gnc:commodity>
<gnc:commodity version="2.0.0">
  <cmdty:space>NASDAQ</cmdty:space>
  <cmdty:id>ACME</cmdty:id>
  <cmdty:name>Acme Corp</cmdty:name>
  <cmdty:xcode>US0000001</cmdty:xcode>
  <cmdty:fraction>10000</cmdty:fraction>
  <cmdty:get_quotes/>
  <cmdty:quote_source>yahoo_json</cmdty:quote_source>
  <cmdty:quote_tz>America/New_York</cmdty:quote_tz>
</gnc:commodity>
<gnc:commodity version="2.0.0">
  <cmdty:space>template</cmdty:space>
  <cmdty:id>template</cmdty:id>
  <cmdty:name>template</cmdty:name>
  <cmdty:xcode>template</cmdty:xcode>
  <cmdty:fraction>1</cmdty:fraction>
</gnc:commodity>
```

**Read** (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:135-299`): not a generic parse; children are
visited in order, unknown ones ignored. If the element names an existing currency (namespace `CURRENCY`/`ISO4217`),
the new commodity starts as a copy of it (`:271-273`). `space`, `id`, `name`, `xcode`, `quote_tz` are
whitespace-stripped (`:174-188`); `fraction` via `string_to_gint64`; `get_quotes` presence sets the flag; an unknown
`quote_source` is registered as a new source (`:155-164`, keeps it for writing). Invalid if namespace or mnemonic
missing or fraction 0 → load fails (`:192-211`, `:280-289`). Insertion merges into an existing table entry by copying
(`5.17:libgnucash/engine/gnc-commodity.cpp:1754-1820`): legacy ISO codes `RUR PLZ UAG NIS MXP TRL` are renamed to
`RUB PLN UAH ILS MXN TRY` (`:131-145`); a non-`template` mnemonic in namespace `template` is moved to `User`.

### 6.3 Price DB `<gnc:pricedb version="1">` and `<price>`

Written only if the DB has ≥ 1 price (`5.17:libgnucash/backend/xml/gnc-pricedb-xml-v2.cpp:473-498`). `<price>` has
**no** namespace prefix and **no** version attribute (`:400-454`):

| Element | Written when | Content |
|---|---|---|
| `price:id` | always | GUID |
| `price:commodity` | always | commodity ref |
| `price:currency` | always | commodity ref |
| `price:time` | always | `ts:date` |
| `price:source` | source string non-empty | e.g. `user:price-editor`, `user:xfer-dialog`, `Finance::Quote` |
| `price:type` | type string non-empty | e.g. `last`, `nav`, `transaction`, `bid`, `ask`, `unknown` |
| `price:value` | always | numeric, **as stored** (`12000/10` is not reduced) |

A price whose commodity or currency is NULL makes the whole pricedb return NULL, i.e. **no prices are written**
(`:418`, `:462`, `:484-489`) — keep that impossible.

Excerpt (oracle; note the 2-space price indent):

```
<gnc:pricedb version="1">
  <price>
    <price:id type="guid">8d673d94edb946588958509aaae77861</price:id>
    <price:commodity>
      <cmdty:space>NASDAQ</cmdty:space>
      <cmdty:id>ACME</cmdty:id>
    </price:commodity>
    <price:currency>
      <cmdty:space>CURRENCY</cmdty:space>
      <cmdty:id>USD</cmdty:id>
    </price:currency>
    <price:time>
      <ts:date>2024-06-05 09:59:00 +0000</ts:date>
    </price:time>
    <price:source>Finance::Quote</price:source>
    <price:type>last</price:type>
    <price:value>12000/10</price:value>
  </price>
  ...
</gnc:pricedb>
```

**Read** (`:87-133`, `:135-206`, `:248-306`, `:343-372`): pricedb character data must be whitespace only; children other than
`price` fail the load. Inside `<price>`, unknown elements are ignored; a commodity/currency reference that does not
resolve fails the load; a bad `price:time` becomes 0. Prices are added in bulk mode: **no same-day de-duplication** on
load (`5.17:libgnucash/engine/gnc-pricedb.cpp:1057-1071`; verified: a duplicate with another GUID is kept and written).
A price `add` failure (e.g. missing commodity element) silently drops that price.

### 6.4 Account `<gnc:account version="2.0.0">`

Writer `5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:73-162`:

| Element | Written when | Content |
|---|---|---|
| `act:name` | always | name (`text_to_dom_tree`: `<act:name/>` if empty) |
| `act:id` | always | GUID |
| `act:type` | always | `NONE BANK CASH CREDIT ASSET LIABILITY STOCK MUTUAL CURRENCY INCOME EXPENSE EQUITY RECEIVABLE PAYABLE ROOT TRADING CHECKING SAVINGS MONEYMRKT CREDITLINE` (`5.17:libgnucash/engine/Account.cpp:4235-4262`) |
| `act:commodity` | commodity non-NULL | commodity ref |
| `act:commodity-scu` | commodity non-NULL | integer, the raw stored SCU (`xaccAccountGetCommoditySCUi`) |
| `act:non-standard-scu` | commodity non-NULL and non-standard flag | `<act:non-standard-scu/>` |
| `act:code` | code non-empty | text |
| `act:description` | description non-empty | text |
| `act:slots` | frame non-empty | slots: `notes`, `placeholder`, `hidden`, `color`, `tax-related`, `tax-US`, `last-num`, `reconcile-info`, `import-map*`, … (engine notes) |
| `act:parent` | account has a parent (root and template root have none) | parent GUID |
| `act:lots` | ≥ 1 lot and not exporting | `<gnc:lot>` children sorted by GUID (memcmp = lowercase-hex string order, `:148-158`) |

`act:hidden` and `act:placeholder` elements are **never written** (`:98-111` commented out) but are read
(§ below). The root account (from the bindings) has no commodity; a GUI-created root normally has one — keep what is
read.

Excerpt (oracle):

```
<gnc:account version="2.0.0">
  <act:name>ACME</act:name>
  <act:id type="guid">6de15cae26084060bfd287540569ffc1</act:id>
  <act:type>STOCK</act:type>
  <act:commodity>
    <cmdty:space>NASDAQ</cmdty:space>
    <cmdty:id>ACME</cmdty:id>
  </act:commodity>
  <act:commodity-scu>10000</act:commodity-scu>
  <act:parent type="guid">85fdb8c115124265ae304f45ccc5cb3b</act:parent>
  <act:lots>
    <gnc:lot version="2.0.0">
      <lot:id type="guid">d2ec6576e51e4aa2972f668b74263a18</lot:id>
      <lot:slots>
        <slot>
          <slot:key>notes</slot:key>
          <slot:value type="string">lot notes</slot:value>
        </slot>
        <slot>
          <slot:key>title</slot:key>
          <slot:value type="string">Lot …</slot:value>
        </slot>
      </lot:slots>
    </gnc:lot>
  </act:lots>
</gnc:account>
<gnc:account version="2.0.0">
  <act:name>NonStd</act:name>
  <act:id type="guid">85b0163c56304d64b42ac8cd59838473</act:id>
  <act:type>BANK</act:type>
  <act:commodity>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>USD</cmdty:id>
  </act:commodity>
  <act:commodity-scu>1000</act:commodity-scu>
  <act:non-standard-scu/>
  <act:parent type="guid">d4e3331aca124864ba674c50ae7ddcf1</act:parent>
</gnc:account>
```

**Read** (generic parse; handlers `5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:414-433`). Required: `act:name`,
`act:id`, `act:type`. Handlers run in **file order**, and some are order-dependent:

- `act:type`: exact string match; anything else (including surrounding whitespace) leaves type `NONE` (verified).
- `act:commodity`: `xaccAccountSetCommodity` sets the commodity, **resets SCU to the commodity fraction and the
  non-standard flag to false**, and increments the commodity's usage count
  (`5.17:libgnucash/engine/Account.cpp:2666-2701`), which can set a currency's quote flag (§10.3).
- `act:commodity-scu`: sets SCU and **sets the non-standard flag if SCU ≠ commodity fraction**
  (`5.17:libgnucash/engine/Account.cpp:2709-2722`); so it must follow `act:commodity`.
- `act:non-standard-scu`: presence sets the flag.
- `act:hidden`, `act:placeholder`: boolean (§4) → stored as `hidden`/`placeholder` slots; written back as slots
  (verified).
- `act:parent`: looks up an already-parsed account; a miss logs an assertion and the account ends up under the root
  (verified, §10.2).
- `act:lots`: each non-text child is parsed as a lot and inserted (any element name is accepted as a lot).
- Legacy `act:currency`, `act:currency-scu`, `act:security`, `act:security-scu`: read with warnings, not written
  (`:275-336`).

After a successful parse: no parent and type ≠ `ROOT` → appended to the root (`:478-492`,
`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:224-238`); the account is left in edit state until the end of the load.

### 6.5 Transaction `<gnc:transaction version="2.0.0">`

Writer `5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:150-192`:

| Element | Written when | Content |
|---|---|---|
| `trn:id` | always | GUID |
| `trn:currency` | always (`xmlAddChild(NULL)` if no currency) | commodity ref |
| `trn:num` | num non-empty | text |
| `trn:date-posted` | always | time |
| `trn:date-entered` | always | time |
| `trn:description` | description non-NULL (always in practice) | text; `<trn:description></trn:description>` if empty |
| `trn:slots` | frame non-empty | slots: `date-posted` (gdate), `notes`, `void-reason`, `void-time`, `trans-read-only`, `reversed-by`, `book_closing`, `assoc_uri`, `gncInvoice`… |
| `trn:splits` | always | `<trn:split>` children in the transaction's split-list order (§6.6) |

Excerpt (oracle, voided transaction):

```
<gnc:transaction version="2.0.0">
  <trn:id type="guid">7e04ac3fbb8d4eff83068196f09164f4</trn:id>
  <trn:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>USD</cmdty:id>
  </trn:currency>
  <trn:date-posted>
    <ts:date>2024-06-06 10:59:00 +0000</ts:date>
  </trn:date-posted>
  <trn:date-entered>
    <ts:date>2024-06-30 12:00:00 +0000</ts:date>
  </trn:date-entered>
  <trn:description>Mistake</trn:description>
  <trn:slots>
    <slot>
      <slot:key>date-posted</slot:key>
      <slot:value type="gdate">
        <gdate>2024-06-06</gdate>
      </slot:value>
    </slot>
    <slot>
      <slot:key>notes</slot:key>
      <slot:value type="string">Voided transaction</slot:value>
    </slot>
    <slot>
      <slot:key>trans-read-only</slot:key>
      <slot:value type="string">Transaction Voided</slot:value>
    </slot>
    <slot>
      <slot:key>void-reason</slot:key>
      <slot:value type="string">entered twice</slot:value>
    </slot>
    <slot>
      <slot:key>void-time</slot:key>
      <slot:value type="string">2026-10-25 13:07:18</slot:value>
    </slot>
  </trn:slots>
  <trn:splits>
    <trn:split>
      <split:id type="guid">aaf247551129420cb95646d72559c157</split:id>
      <split:reconciled-state>v</split:reconciled-state>
      <split:value>0/100</split:value>
      <split:quantity>0/100</split:quantity>
      <split:account type="guid">1feca2abf92347288c268ebf93753ec4</split:account>
      <split:slots>
        <slot>
          <slot:key>void-former-amount</slot:key>
          <slot:value type="numeric">100/100</slot:value>
        </slot>
        <slot>
          <slot:key>void-former-value</slot:key>
          <slot:value type="numeric">100/100</slot:value>
        </slot>
      </split:slots>
    </trn:split>
    …
  </trn:splits>
</gnc:transaction>
```

**Read** (`:330-575`). Required: `trn:id`, `trn:date-posted`, `trn:date-entered`, `trn:splits`. `trn:currency` is
looked up in the table. Bad dates become 0 (`:381-389`). `trn:splits` children must all be `trn:split` (else the
handler stops; its failure is ignored). Each split is appended with `xaccTransAppendSplit`
(`:494`) = `xaccSplitSetParent`, which converts the split value to the transaction currency's fraction **if the
currency is already set**, i.e. if `trn:currency` precedes `trn:splits` (`5.17:libgnucash/engine/Split.cpp:1876-1916`).
The transaction is then committed: splits are sign-sorted (§6.6), quantities converted to the account SCU (§10.4),
`date_entered == 0` replaced by **now** (`5.17:libgnucash/engine/Transaction.cpp:1570-1574`; verified with
`1970-01-01 00:00:00 +0000`). Element order inside the transaction is otherwise free (verified: description before
`trn:id` gives canonical output).

### 6.6 Split `<trn:split>` (no version attribute)

Writer `5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:65-132`:

| Element | Written when | Content |
|---|---|---|
| `split:id` | always | GUID |
| `split:memo` | memo non-empty | text |
| `split:action` | action non-empty | text |
| `split:reconciled-state` | always | one character: `n` `c` `y` `f` `v` |
| `split:reconcile-date` | reconcile date ≠ 0 | time |
| `split:value` | always | numeric, transaction-currency value |
| `split:quantity` | always | numeric, account-commodity amount |
| `split:account` | always | GUID (all-zero GUID if the split has no account) |
| `split:lot` | split in a lot | GUID |
| `split:slots` | frame non-empty | slots (`void-former-*`, `sched-xaction`, `gains-split`, `gains-source`, `lot-split`, `peer_guid`, …) |

**Split order inside `trn:splits`:** the transaction's list order. Every transaction commit runs
`xaccTransSortSplits`, a **stable** sort that puts splits with non-negative value before splits with negative value
(`5.17:libgnucash/engine/Transaction.cpp:540-552, 1508-1510`). Otherwise order is insertion order (file order on
load). Verified: a file with the negative split first is written back with the positive split first.

**Read** (`:332-344`): required `split:id`, `split:reconciled-state`, `split:value`, `split:quantity`,
`split:account`. `reconciled-state` takes the first character of the text. `split:account`/`split:lot` look up
already-parsed objects; a miss leaves no account/lot (orphan handling §10.3).

### 6.7 Lot `<gnc:lot version="2.0.0">` (inside `act:lots`)

Writer `5.17:libgnucash/backend/xml/gnc-lot-xml-v2.cpp:56-72`: `lot:id` (GUID, always), `lot:slots` (if non-empty).
There is no title element: title and notes are the `title` and `notes` slots; closed state is computed, not stored.
Business lots carry `gncInvoice`/`gncOwner` slots (business note). Read: required `lot:id`; generic parse
(`:109-113`).

## 7. Write order of entities and determinism

### 7.1 Commodities

Namespaces from the table's hash, **sorted by `strcmp`**; inside each, commodities sorted by mnemonic `strcmp`
(`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:931-944, 1037-1086`). Uppercase sorts before lowercase:
`CURRENCY` < `FUND` < `NASDAQ` < `template`. Fully determined by data.

### 7.2 Prices

`gnc_pricedb_foreach_price(db, …, stable_order = TRUE)` (`5.17:libgnucash/backend/xml/gnc-pricedb-xml-v2.cpp:484`,
`5.17:libgnucash/engine/gnc-pricedb.cpp:2688-2744`):

```
for commodity in sorted(by (strcmp namespace, strcmp mnemonic)):
    for currency in sorted(by (strcmp namespace, strcmp mnemonic)):
        for price in list:            # kept sorted on insert by compare_prices_by_date
            write(price)
compare_prices_by_date(a, b) = (b.time <=> a.time)  then  guid_compare(a, b)   # newest first, then GUID ascending
```

(`5.17:libgnucash/engine/gnc-pricedb.cpp:674-693, 706-718`.) Fully determined by data (verified: reversed input
order gives canonical output).

### 7.3 Accounts

Root, then `gnc_account_get_descendants(root)`: depth-first **pre-order** over each account's `children` vector
(`5.17:libgnucash/backend/xml/io-utils.cpp:59-81`, `5.17:libgnucash/engine/Account.cpp:2792-2805, 3032-3037`).
The children vector is in **insertion order**: `gnc_account_append_child` does `push_back`
(`5.17:libgnucash/engine/Account.cpp:2870`), and on load accounts are appended in file order. Nothing sorts it (sorted
traversals exist only for the UI). So a 5.17-written file's account order is a fixed point; a new account goes after
its existing siblings' subtrees. Lots inside an account: GUID order (§6.4).

### 7.4 Transactions

`xaccAccountTreeForEachTransaction(root)` (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1166-1176`,
`5.17:libgnucash/engine/Account.cpp:5042-5134`):

```
visited = {}
def visit(acc):                      # post-order: children first, then the account itself
    for child in acc.children: visit(child)
    for split in acc.splits:         # kept sorted by xaccSplitOrder
        t = split.transaction
        if t not in visited: visited.add(t); write(t)
visit(root)
```

`acc.splits` is sorted by `xaccSplitOrder` when the account's edit ends (all accounts are committed at the end of the
load, `5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:836-850`; `5.17:libgnucash/engine/Account.cpp:1925-1948,
1993-2004, 2013`). Between different transactions `xaccSplitOrder` reduces to `xaccTransOrder_num_action`
(`5.17:libgnucash/engine/Split.cpp:1536-1597`, `5.17:libgnucash/engine/Transaction.cpp:1758-1839`):

1. `date_posted` ascending;
2. non-closing before closing (`book_closing` slot);
3. num: `order_by_int64_or_string` — if `strtoull` of both is non-zero, numeric compare then `g_utf8_collate` of the
   remainders; else `g_utf8_collate` of the whole strings (`:1774-1791`). If the book uses the split action as the
   number (`qof_book_use_split_action_for_num_field`), the **split's action** in that account is compared instead;
4. `date_entered` ascending;
5. description by `g_utf8_collate`;
6. GUID (memcmp).

So a transaction is written at its first split in the deepest-first walk. **`g_utf8_collate` uses the process's
`LC_COLLATE`**: verified with four transactions tied on steps 1-4 (descriptions `cherry`, `Banana`, `apple`,
`éclair`): `LC_ALL=C.UTF-8` wrote `Banana apple cherry éclair`, `en_US.UTF-8` wrote `apple Banana cherry éclair`
(`c_C.UTF-8/`, `c_en_US.UTF-8/`). The Python oracle does not call `setlocale(LC_ALL, "")`, so it collates in `C`
unless told to; GnuCash's GUI does. Order is therefore data-determined only up to ties at step 5 (and string nums at
step 3), which need same-second `date_entered`.

Template transactions use the same walk from the template root (§8.1).

### 7.5 Scheduled transactions

The SX list order: file order on load, `g_list_append` on creation (`5.17:libgnucash/engine/SX-book.cpp:212-218`,
`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1202-1227`). Data-determined.

### 7.6 Budgets: not reproducible

`qof_collection_foreach` over the budget collection's `GHashTable` with no sort
(`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1020-1021`, `5.17:libgnucash/engine/qofid.cpp:283-307`). Verified with
9 budgets: three load+saves of the **same** input file produced two different orders (`bx1` vs earlier `b1`), and
successive load+saves kept reordering (`b1`→`b2`→`b3`). Budgets are created with a random GUID and re-keyed during
the parse, so the table's probe history differs per run. GnuCash does not care about budget order on read.

### 7.7 Business objects

Per the business note §1.3, sorted by GUID within each type.

## 8. Out of v0.1 scope, must round-trip

### 8.1 `<gnc:template-transactions>`

Written only if the template root has ≥ 1 descendant (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1179-1199`):
`<gnc:template-transactions>\n` (fprintf), the template root and its descendants as `<gnc:account>`s (same rules as
§6.4/§7.3), the template transactions as `<gnc:transaction>`s (walk §7.4 from the template root), then
`</gnc:template-transactions>\n`. Conventional content (from the fixture and GnuCash's own test file): template root
named `Template Root`, type `ROOT`, commodity `template:template`, SCU 1, `<act:non-standard-scu/>`, no parent (a
template root 5.17 made itself, in a book whose file had none, is written with no `act:commodity`, as the oracle
shows: `sxtg_book_begin` sets none, `5.17:libgnucash/engine/SX-book.cpp:113-125`; the reader gives it
`template:template`, below); one child per SX named by the SX GUID, type `BANK`, same commodity; each template split has value `0/<fraction>`, quantity
`0/1`, the template account, and a `sched-xaction` frame slot with keys `account` (guid), `credit-formula`
(string), `credit-numeric` (numeric), `debit-formula` (string), `debit-numeric` (numeric).

Read: children must be `gnc:account` or `gnc:transaction` (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:847-852`); an account
with no commodity gets `template:template` (`:777-822`); a parentless `ROOT` replaces the book's template root, a
parentless non-root goes under it (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:303-343`); transactions get the
usual `add_transaction_local` treatment.

### 8.2 `<gnc:schedxaction version="2.0.0">`

Writer `5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:80-197`:

| Element | Written when | Content |
|---|---|---|
| `sx:id` | always | GUID |
| `sx:name` | always | text (`xmlNewTextChild`) |
| `sx:enabled` | always | `y`/`n` |
| `sx:autoCreate` | always | `y`/`n` |
| `sx:autoCreateNotify` | always | `y`/`n` |
| `sx:advanceCreateDays` | always | integer |
| `sx:advanceRemindDays` | always | integer |
| `sx:instanceCount` | always | integer |
| `sx:start` | always | gdate |
| `sx:last` | last-occurrence date valid | gdate |
| `sx:num-occur`, `sx:rem-occur` | SX has an occurrence count | integers |
| `sx:end` | else, if SX has an end date | gdate |
| `sx:templ-acct` | always | GUID of the template account |
| `sx:schedule` | always | `<gnc:recurrence>`* (§8.4); `<sx:schedule/>` if none |
| `sx:deferredInstance` | one per deferred instance | `[sx:last gdate]`, `sx:rem-occur`, `sx:instanceCount` |
| `sx:slots` | frame non-empty | slots |

Excerpt (oracle, `rich-canonical.gnucash`):

```
<gnc:schedxaction version="2.0.0">
  <sx:id type="guid">52f96837087e9670c933f9421b0ac626</sx:id>
  <sx:name>Monthly rent</sx:name>
  <sx:enabled>y</sx:enabled>
  <sx:autoCreate>n</sx:autoCreate>
  <sx:autoCreateNotify>n</sx:autoCreateNotify>
  <sx:advanceCreateDays>0</sx:advanceCreateDays>
  <sx:advanceRemindDays>5</sx:advanceRemindDays>
  <sx:instanceCount>2</sx:instanceCount>
  <sx:start>
    <gdate>2024-08-01</gdate>
  </sx:start>
  <sx:last>
    <gdate>2024-09-01</gdate>
  </sx:last>
  <sx:end>
    <gdate>2025-07-01</gdate>
  </sx:end>
  <sx:templ-acct type="guid">22f2905cbcfa0b910c0dbc348cfc1161</sx:templ-acct>
  <sx:schedule>
    <gnc:recurrence version="1.0.0">
      <recurrence:mult>1</recurrence:mult>
      <recurrence:period_type>month</recurrence:period_type>
      <recurrence:start>
        <gdate>2024-08-01</gdate>
      </recurrence:start>
      <recurrence:weekend_adj>back</recurrence:weekend_adj>
    </gnc:recurrence>
  </sx:schedule>
  <sx:deferredInstance>
    <sx:last>
      <gdate>2024-09-01</gdate>
    </sx:last>
    <sx:rem-occur>9</sx:rem-occur>
    <sx:instanceCount>3</sx:instanceCount>
  </sx:deferredInstance>
</gnc:schedxaction>
```

Read: handlers `:617-638` (required `sx:id`, `sx:name`, `sx:autoCreate`, `sx:autoCreateNotify`,
`sx:advanceCreateDays`, `sx:advanceRemindDays`, `sx:start`); version `1.0.0` expects `sx:freqspec`, `2.0.0` expects
`sx:schedule`, mismatches only warn (`:682-718`); an SX without `sx:templ-acct` looks for a template account named by
its GUID (`:722-761`).

### 8.3 `<gnc:budget version="2.0.0">`

Writer `5.17:libgnucash/backend/xml/gnc-budget-xml-v2.cpp:55-86`: `bgt:id` (GUID), `bgt:name` (text, `<bgt:name/>`
if empty), `bgt:description` (text, `<bgt:description/>` if empty), `bgt:num-periods` (uint), `bgt:recurrence
version="1.0.0"` (§8.4), `bgt:slots` if non-empty. Amounts live in slots at `<account-guid>/<period>` (numeric);
notes at `notes/<account-guid>/<period>` (string) (`5.17:libgnucash/engine/gnc-budget.cpp:495-508`). Period keys
sort as strings (`0`, `10`, `2`). Read: required `bgt:id`, `bgt:num-periods`, `bgt:recurrence` (`:143-151`).

```
<gnc:budget version="2.0.0">
  <bgt:id type="guid">0123456789abcdef0123456789abcdef</bgt:id>
  <bgt:name>Budget 2024</bgt:name>
  <bgt:description/>
  <bgt:num-periods>12</bgt:num-periods>
  <bgt:recurrence version="1.0.0">
    <recurrence:mult>1</recurrence:mult>
    <recurrence:period_type>month</recurrence:period_type>
    <recurrence:start>
      <gdate>2024-01-01</gdate>
    </recurrence:start>
  </bgt:recurrence>
  <bgt:slots>
    <slot>
      <slot:key>1feca2abf92347288c268ebf93753ec4</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>0</slot:key>
          <slot:value type="numeric">100/1</slot:value>
        </slot>
        <slot>
          <slot:key>10</slot:key>
          <slot:value type="numeric">1100/1</slot:value>
        </slot>
        …
```

### 8.4 Recurrence

`<TAG version="1.0.0">` with `TAG` = `gnc:recurrence` (SX) or `bgt:recurrence` (budget)
(`5.17:libgnucash/backend/xml/gnc-recurrence-xml-v2.cpp:125-154`): `recurrence:mult` (uint), `recurrence:period_type`
(`once day week month end of month nth weekday last weekday year`,
`5.17:libgnucash/engine/Recurrence.cpp:44-52`), `recurrence:start` (gdate), `recurrence:weekend_adj` (`back` or
`forward`) **omitted when `none`**. Read: required mult, period_type, start; missing weekend_adj = none
(`:95-122`).

### 8.5 Legacy freqspec

`sx:freqspec` > `gnc:freqspec` with `fs:ui_type`, `fs:id`, one of `fs:none fs:once fs:daily fs:weekly fs:monthly
fs:month_relative fs:composite`, using `fs:date fs:interval fs:offset fs:day fs:weekday fs:occurrence fs:weekend_adj`
and nested `gnc:freqspec` (`5.17:libgnucash/backend/xml/gnc-freqspec-xml-v2.cpp:286-296, 428-440`). Read only and
converted to recurrences; 5.17 never writes it, so it cannot appear in a 5.17-written book. If it does, GnuCash
rewrites the SX with `sx:schedule`; red-ale should open such a book read-only.

### 8.6 Pass-through argument

For a 5.17-written book, red-ale can keep the out-of-scope entities (template block, SXes, budgets; also unknown
future entities are impossible since GnuCash rejects unknown tags) as **the exact byte range** from `<gnc:…` to the
end of its closing tag, and write it back followed by `\n`. That is byte-identical because GnuCash produced those bytes
with the same serialiser and none of them depend on other entities' bytes. Alternatively, a DOM pass-through that
re-serialises with §3.2-3.4 is also byte-identical provided it:

1. drops whitespace-only text nodes in element-only content and re-indents (GnuCash's text-bearing elements never
   have element children, so "has an element child" decides);
2. keeps the empty-element spelling as read (`<x/>` vs `<x></x>`): a pull parser that reports empty elements
   separately (e.g. quick-xml `Event::Empty`) preserves it; libxml2-style DOMs do not;
3. keeps whitespace-only *text* content in text-only elements (`<slot:value type="string">   </slot:value>`);
4. re-escapes per §3.4 (`\r` as `&#13;`, `>` as `&gt;`).

Raw byte ranges are simpler and cover all four. What breaks either approach: edits elsewhere that GnuCash would
reflect into these entities (an SX's template splits reference real account GUIDs in `sched-xaction/account`;
deleting such an account changes nothing in the XML of the SX block but leaves a dangling GUID — GnuCash behaves the
same), and the transaction count (template transactions are not counted, so nothing to update). Budget order: keep
file order (§7.6).

## 9. Detection, encoding, compression (read side)

Summary; details in `storage-and-safety.md` §1.3 and §4.3.

- **Gzip**: first two bytes `1f 8b` → read through zlib in a thread (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1659-1683`,
  `:1524-1594`). The write side compresses according to the `file-compression` preference, **not** the input's state;
  a temp name containing `.gz.` forces compression (`:1527`).
- **Type sniff** on the first 255 (decompressed) bytes (`5.17:libgnucash/backend/xml/sixtp.cpp:931-995`): skip
  `isspace`; must start `<?xml` (a UTF-8 BOM therefore fails: verified, "no handler"); skip to `>`; skip
  `isspace`; `<gnc-v2` followed by an `isspace` byte → v2 (so `<gnc-v2>` with no namespace declarations is classified
  "too new"); `<gnc>` → v1; other `<gnc-v…` → too new.
- **Encoding**: v2 also requires the text `encoding=` somewhere before `<gnc-v2` (`:968-980`); without it the load
  fails with `ERR_FILEIO_NO_ENCODING` (verified) and the GUI offers its re-encoding assistant
  (`gnc_xml2_find_ambiguous` / `gnc_xml2_parse_with_subst`, `5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1791-2148`,
  which turns `&#0;`..`&#255;` references into raw bytes with `replace_character_references` (`:1713-1765`) and maps
  non-ASCII words through user-chosen encodings). red-ale should refuse such files. With a declaration, libxml2
  decodes the declared encoding (`xmlCreateIOParserCtxt(…, XML_CHAR_ENCODING_NONE)`,
  `5.17:libgnucash/backend/xml/sixtp.cpp:790-795`): verified, an `ISO-8859-1` file loads and is written as UTF-8 with
  `encoding="utf-8"`.
- **Parser**: SAX1 `startElement`/`endElement`/`characters`/`getEntity` only
  (`5.17:libgnucash/backend/xml/sixtp-stack.cpp:126-129`). Names are matched as literal `prefix:local` strings —
  namespace URIs are ignored, so another prefix for the same URI is an unknown tag. No comment or PI callback:
  comments vanish. CDATA arrives through `characters`.

## 10. Read-side tolerance and load fixups

### 10.1 Load sequence

`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:680-859`: disable the transaction log and data scrubbing (`:760-761`);
parse, calling per entity `add_account_local`, `add_commodity_local`, `add_transaction_local`, … (`:200-350`);
re-enable scrubbing; mark the book saved (`:817`); business scrubs; `xaccAccountTreeScrubQuoteSources`;
`xaccAccountTreeScrubCommodities`; `xaccAccountTreeScrubSplits` (`:819-833`); commit every account of both trees
(`:836-850`), which sorts their splits.

### 10.2 Tolerance (verified variants, `var/<name>/`)

| Input change | Result |
|---|---|
| unknown element in `<gnc:account>` (`<act:foo>`) | "Unhandled tag", load **fails** (parse error), file untouched |
| unknown element in `<gnc:book>` | "Tag <gnc:foo> not allowed", load **fails** |
| XML comment in an account or the book | dropped, canonical output |
| count-data removed / wrong | canonical output |
| transactions in reverse order; negative split first | canonical output (re-sorted) |
| USD `<gnc:commodity>` removed | re-added (usage-count quote flag, §10.3) |
| no `encoding=` in the declaration | `ERR_FILEIO_NO_ENCODING` |
| UTF-8 BOM | not recognised (`ERR_BACKEND_NO_HANDLER`) |
| CRLF line endings | canonical LF output |
| `ISO-8859-1` declared and used | loads; written as UTF-8 |
| account written on one line (no whitespace) | canonical output |
| `<act:name>  Cash  </act:name>` | spaces kept |
| `<act:type>\n    CASH\n  </act:type>` | type becomes `NONE` |
| `<act:id>` with surrounding whitespace | account gets a **new random GUID**; its splits lose their account and are moved to a new `Orphan-USD` account; account count +1 |
| `<split:value> 2500/100 </split:value>` | parsed (regex fallback) |
| `<cmdty:id>  ZETA \n</cmdty:id>` | stripped |
| account notes `"   "`, `placeholder` = `"false"`, `old-currency` slot | all three slots deleted |
| split quantity ≠ value in a same-currency account | quantity set to value |
| value `-25/1`, quantity `-25000/1000` (USD) | written `-2500/100` / `-2500/100` |
| value and quantity `-25005/1000` (USD) | `-2501/100` (half away from zero) |
| ACME quantity `-5/1` / `-500001/100000` (SCU 10000) | `-50000/10000` both |
| EUR quantity `100/1` | `10000/100` |
| prices in reverse order | canonical output |
| `ISO4217` instead of `CURRENCY` in a reference | canonical output |
| child account before its parent | parent lookup fails, account re-parented to the **root** (moved in the tree) |
| `date-entered` = epoch | replaced by the current time |
| `date-posted` with `-0500` offset | converted to UTC |
| `trn:currency` missing | set to the common currency of the splits |
| `trn:description` moved before `trn:id` | canonical output |
| `<trn:description/>` | written `<trn:description></trn:description>` |
| `<act:name></act:name>` | written `<act:name/>` |
| `version="9.9.9" extra="1"` on an account | canonical output |
| duplicate price (same data, other GUID) | both kept |
| `<act:hidden>true</act:hidden><act:placeholder>TRUE</act:placeholder>` | become `hidden`/`placeholder` string slots `"true"` |
| `double` slot `0.1` | written `0.100000000000000006` (then stable) |
| account commodity `RUR` | becomes `RUB`; `RUB` gains a `<gnc:commodity>` with `get_quotes` |

### 10.3 Fixups GnuCash applies on every load

None of these change a well-formed 5.17-written book (verified: identical round trips), but red-ale must know them,
both to judge "GnuCash considers identical" and to avoid writing what GnuCash would rewrite:

| Fixup | Effect | Citation |
|---|---|---|
| Account commodity dedupe | an account's commodity pointer is replaced by the table's; an unknown commodity is inserted into the table | `xml/io-gncxml-v2.cpp:119-160, 200-217` |
| Usage-count quote flag | first account using a currency with `auto_quote_control` ≠ `"false"` and no quote flag sets quote flag and source `currency` → the currency is written | `engine/gnc-commodity.cpp:1402-1430`, `engine/Account.cpp:2682` |
| `xaccAccountScrubKvp` | delete `notes` if whitespace-only, `placeholder` if `"false"`, `hbci` if empty | `engine/Scrub.cpp:1384-1407` |
| Parentless non-root account | appended to the root | `xml/io-gncxml-v2.cpp:224-238` |
| Split value | converted to the transaction currency fraction, `ROUND_HALF_UP`, unless that gives an error or zero from non-zero | `engine/Split.cpp:1280-1310, 1903-1907` |
| Split quantity | converted to `xaccAccountGetCommoditySCU(account)`, `ROUND_HALF_UP`, when the split's account is set at commit | `engine/Split.cpp:1016-1031, 1243-1267` |
| Split order | non-negative values first (stable) | `engine/Transaction.cpp:540-552, 1510` |
| `date_entered == 0` | set to now | `engine/Transaction.cpp:1570-1574` |
| Commodity of a transaction | deduped like accounts | `xml/io-gncxml-v2.cpp:163-197, 271-289` |
| `xaccTransScrubCurrency` (twice: per transaction and over the tree) | orphan splits → `Orphan-<CUR>` account; a non-currency transaction currency → common currency, and same-commodity splits get amount := value | `engine/Scrub.cpp:1121-1235, 179-209` |
| `xaccTransScrubPostedDate` | `INT64_MAX` posted date → from `date-posted` slot (cannot happen from XML: bad dates become 0) | `engine/Scrub.cpp:1543-1556` |
| Quote-source migration | delete account `old-price-source`/`old-quote-tz` slots; migrate them to the commodity if no non-currency commodity has a quote flag | `engine/Scrub.cpp:1317-1380` |
| Old-data cleanup | delete `old-currency`, `old-security`, `old-currency-scu`, `old-security-scu` slots on every account incl. root | `engine/Scrub.cpp:1272-1314` |
| `xaccAccountTreeScrubSplits` | same-commodity split with amount ≠ value at precision `min(SCU, fraction)` → amount := value; the commit then runs `xaccTransScrubImbalance` on that transaction | `engine/Scrub.cpp:213-232, 237-347`, `engine/Transaction.cpp:1549-1567` |
| Price bulk load | no same-day de-duplication | `engine/gnc-pricedb.cpp:1057-1071` |

### 10.4 Denominators

Because of the two conversions above, the denominators GnuCash holds after loading are: split value → the
transaction currency's fraction; split quantity → the account SCU (non-standard SCU if flagged, else the commodity
fraction). A 5.17-written file already satisfies this, except where an account's SCU or a commodity's fraction was
changed after its splits were entered (then GnuCash's next load+save rewrites those splits). Price values, KVP numerics
and business amounts keep their denominators. red-ale keeps denominators as read (CLAUDE.md rule) and, when GnuCash
would convert on load, writes the same converted value only for splits it creates or edits.

## 11. GnuCash's own round-trip test

`5.17:libgnucash/backend/xml/test/gtest-load-save-files.cpp:209-290`: for every `*.gnucash` in
`test-files/load-save/`, load read-only, swap into a new session, save compressed, `inflate` and compare **byte for
byte** with the original (`:146-200`); then load that compressed file, save uncompressed and `memcmp` with the original
(`:76-89`, `:288`). So GnuCash guarantees byte-identical load+save only for its fixtures (no budgets, one SX), in the test
locale. Our oracle runs reproduce it for both fixtures.

## 12. Guidance for red-ale's XML backend

- Write with a dedicated serialiser that implements §3.2-3.4 exactly (two empty spellings per field per §3.3), not a
  general XML library's pretty printer.
- Write count-data, entity order and inner element order from §2 and §6-7. Keep: account children order, SX order,
  budget order, and transaction order **as read** for unedited data; place new or re-dated transactions using §7.4
  with a fixed collation, inserting into the read order rather than re-sorting everything.
- Reject on read what GnuCash rejects (unknown tags, no encoding, bad sniff). Prefer stricter handling where GnuCash
  silently damages data (bad GUID text, unresolved parent, bad numeric): refuse to open for writing and say why.
- Model optional-vs-empty where the writer distinguishes them: `cmdty:quote_tz` (absent vs `""`), `cmdty:name`.
- Keep `double` slot values as their text.
- Always have `template:template` in the commodity table; write currencies per §6.2's skip rule; apply the
  usage-count quote flag when an account starts using a currency.

## 13. Open questions

1. **Other libxml2 versions.** Windows, macOS and Flatpak builds of 5.17 may bundle libxml2 2.10-2.14, whose
   serialiser was reworked (2.13/2.14). Do they still write raw UTF-8, `&#13;`, `&gt;`, and the same empty-element
   forms? Needs a run on those builds; until then treat §3 as Linux-distribution behaviour and accept any escaping on
   read.
2. **Locale for ordering.** Which collation should red-ale use for §7.4 ties when it inserts new transactions? ICU
   root vs the user's locale vs `C`. Only affects same-second ties.
3. **Budget order.** Confirm the mechanism of §7.6 (re-keying after `gnc_budget_new` inside the `GHashTable`); if it
   is just hash-table history, any order is "GnuCash-valid" and file order is the right choice.
4. A `timespec` slot whose date failed to parse is stored as `INT64_MAX`, and the writer then emits a `<slot>` with a
   key and no value; whether GnuCash can re-read that (the slot would be dropped) was not tested.
5. Year formatting below 1000 for `gdate` (`g_date_strftime("%Y")`) and `ts:date` was not tested; the reader accepts
   only 4-digit years in `ts:date`.
6. Whether the GUI-created root account always has a commodity in 5.17 (affects new-book creation, not round trip).
7. Whether `checked_char_cast` on read can ever fire (libxml2 rejects most control characters as invalid XML 1.0
   first); not tested.
