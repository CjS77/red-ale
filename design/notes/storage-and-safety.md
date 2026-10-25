# Storage and safety: how GnuCash 5.17 opens, locks, saves, backs up and journals XML books

Scope: the XML backend and the engine/GUI code around it. SQL appears only where it changes how a file is
classified. Every behavioural claim cites `5.17:<path>:<line>`. "Oracle" means the GnuCash 5.17 library and Python
bindings described in `CLAUDE.md`; experiment scripts and outputs were run in a scratch directory and are quoted
verbatim (paths shortened to `…/`).

Terms follow `design/glossary.md`: *Lock*, *Journal* (GnuCash: transaction log, `TransLog`), *Replay*, *Backup*,
*Book feature*, *Read-only trigger*. Two glossary clashes are flagged in §15.

Oracle prefs isolation: the oracle reads GnuCash's GSettings prefs through the system 5.5 schemas, so experiments
ran with `GSETTINGS_BACKEND=keyfile XDG_CONFIG_HOME=<scratch>` (empty keyfile means schema defaults).

---

## 0. Facts that surprise

1. **The lock is a bare `O_CREAT|O_EXCL` file, empty, mode 0600.** No PID, no host, no staleness check, no
   hard-link trick. The `.LNK` hard-link protocol was removed in 4.7 (commit 38cd06e54a, 2021-09-06); only its
   cleanup code and an always-empty `m_linkfile` member remain. (§3)
2. **The save is not a rename.** It writes `<book>.tmp-XXXXXX`, *unlinks the book*, hard-links the temp file to the
   book name, then unlinks the temp file. There is a window in which the book name does not exist. There is no fsync.
   (§4)
3. **A backup is a hard link to the previous book file, made on every save that has a previous file** and named
   with the save's *local* time. It is not a copy unless `link()` fails with EPERM/ENOSYS/ENOTSUP. (§6)
4. **Cleanup deletes the current journal too** under the "never keep" retention policy, while it is still open.
   (§7)
5. **Journal timestamps inside lines are UTC; journal and backup *file names* are local time.** (§8)
6. **The journal does not escape anything.** A tab or newline in a description corrupts the record; the
   oracle wrote a description containing `\t` raw. (§8)
7. **Unknown Book features make the GUI refuse to open the book at all**, not open it read-only. The library and
   Python bindings do not check features. (§10)
8. **Opening any register in the 4.x/5.x GUI sets two Book features**, "Register sort and filter settings stored in
   .gcm file" and "Use a dedicated opening balance account identified by an 'equity-type' slot". Almost every real
   book carries them. (§10)
9. **Auto read-only is GUI-only.** The engine lets you edit a transaction behind the red line. (§11)
10. **`SESSION_READ_ONLY` does not make the book read-only.** It only skips the lock. The GUI marks the book
    read-only separately; through the Python bindings a "read-only" session saved the file over a live lock. (§12)
11. **`SESSION_BREAK_LOCK` does not take the lock.** It ignores the existing `.LCK`, then deletes the other
    process's `.LCK` when the session ends. (§3)
12. **Fatal load errors are lost.** `QofSessionImpl::load` clears the backend's error when it tears the backend down,
    so a too-new or unparsable XML file "opens" as an empty book with error 0. (§2)
13. **A path or `file://` URI for a file that does not exist yet creates an SQLite book**, not XML. (§1)

---

## 1. URIs and file type detection

### 1.1 URI parsing

| Input form | Scheme | Path | Notes |
|---|---|---|---|
| `/abs/path/book.gnucash` (no `://`) | none | the input | `5.17:libgnucash/engine/gnc-uri-utils.c:164-171` |
| `file:///abs/path` | `file` | after `://`, resolved | `…/gnc-uri-utils.c:177-191` |
| `xml:///abs/path` | `xml` | same | file scheme |
| `sqlite3:///abs/path` | `sqlite3` | same | file scheme |
| `postgres://user:pw@host:port/db`, `mysql://…` | as given | text after first `/` of host part | network; `@` searched from the right, `:` from the left (`…/gnc-uri-utils.c:197-248`) |

- File schemes are exactly `file`, `xml`, `sqlite3`, case-insensitive (`5.17:libgnucash/engine/gnc-uri-utils.c:90-96`).
  Unknown schemes count as network schemes since 3.4 (`…:81-89`).
- A Windows `file:///C:/…` form drops the leading `/` (`…:182-187`).
- Relative paths resolve against, in order: the CWD, `$PREFIX/share/gnucash`, the accounts dir, the user data dir
  (`5.17:libgnucash/core-utils/gnc-filepath-utils.cpp:155-200`). red-ale should require absolute paths.
- Normalising a bare path produces `file://<abs>` (`5.17:libgnucash/engine/gnc-uri-utils.c:323-346`). The GUI's
  Save As rewrites `file` to `xml` and appends `.gnucash` if missing (`5.17:gnucash/gnome-utils/gnc-file.c:1584-1600`;
  `5.17:libgnucash/engine/gnc-uri-utils.c:413-429`).
- Constants: `GNC_DATAFILE_EXT ".gnucash"`, `GNC_LOGFILE_EXT ".log"` (`5.17:libgnucash/engine/gnc-uri-utils.h:61-62`).

### 1.2 Choosing a backend

`QofSessionImpl::begin` (`5.17:libgnucash/engine/qofsession.cpp:250-332`):

```
scheme = g_uri_parse_scheme(uri)
if scheme == "file": filename = path            # :277-278
elif no scheme:      filename = uri             # :279-280
if filename is a directory -> ERR_BACKEND_BAD_URL   # :282-290
creating = mode in {NEW_STORE, NEW_OVERWRITE}       # :295
access_method = filename ? "file" : scheme          # :296-299
for provider in registration order:                 # load_backend :166-196
    if provider.access_method != access_method (case-insensitive): continue
    if !creating and !provider.type_check(uri): continue     # :183
    use provider; break
none -> ERR_BACKEND_NO_HANDLER (then BAD_URL if nothing else was set)   # :193-194, :304-311
```

Registration order: the DBI library loads before the XML library (`5.17:libgnucash/engine/gnc-engine.cpp:77-79`).
DBI registers `file` and `sqlite3` (if the sqlite3 driver exists), then `mysql`, `postgres`
(`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:1153-1174`). XML registers `xml` then `file`
(`5.17:libgnucash/backend/xml/gnc-backend-xml.cpp:193-201`). So for `file`/bare paths the SQLite provider is tried
first.

Type checks:

| Provider | `type_check(uri)` returns true when | Cite |
|---|---|---|
| SQLite (`file`, `sqlite3`) | file cannot be `fopen`ed (treated as new) **or** its first 50 bytes start with `SQLite format 3` | `5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:1029-1068` |
| XML (`xml`, `file`) | file cannot be `fopen`ed (new), or size 0, or `gnc_is_xml_data_file_v2` returns XML2, XML1 or POST_XML2 | `5.17:libgnucash/backend/xml/gnc-backend-xml.cpp:114-164` |
| MySQL, PostgreSQL | always | `5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:111` |

Consequences, confirmed by experiment 6 (each file opened `SESSION_READ_ONLY` with four URI forms):

```
sqlite3:// file head: b'SQLite format 3\x00'
create via '/tmp/cl'... -> head b'SQLite format 3\x00'
create via 'file://'... -> head b'SQLite format 3\x00'
(path) ref.gnucash                            OK (top-level accounts=2)
file:// ref.gnucash                           OK (top-level accounts=2)
xml:// ref.gnucash                            OK (top-level accounts=2)
sqlite3:// ref.gnucash                        ERR (1,)
(path) ref.sqlite                             OK (top-level accounts=2)
file:// ref.sqlite                            OK (top-level accounts=2)
xml:// ref.sqlite                             ERR (1,)
sqlite3:// ref.sqlite                         OK (top-level accounts=2)
(path) empty.gnucash                          OK (top-level accounts=0)
(path) no-encoding.gnucash                    ERR (1010,)
(path) gnc-v3.gnucash                         OK (top-level accounts=0)
(path) no-xml-decl.gnucash                    ERR (1,)
(path) leading-ws.gnucash                     OK (top-level accounts=0)
(path) gnc-v2-no-space.gnucash                OK (top-level accounts=0)
(path) random-text.gnucash                    ERR (1,)
(path) gz-random.gnucash                      ERR (1,)
```

- Creating through a bare path or `file://` (type check skipped, first `file` provider wins) gives an SQLite book.
- Error 1 = `ERR_BACKEND_NO_HANDLER`: every type check refused the file.
- The "OK (0 accounts)" rows are failures whose error was lost (§2.3): the backend logged `Version of Xml file … is
  newer`, `Syntax error in Xml File`, `File not any known type`.

### 1.3 XML byte sniffing

`gnc_is_xml_data_file_v2(path)` (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1685-1709`):

```
if first 2 bytes == 1F 8B:                        # is_gzipped_file :1658-1683
    chunk = gzread(up to 255 bytes); if < 1 byte -> NOT_OURS
else:
    chunk = fread(up to 255 bytes); if 0 bytes -> NOT_OURS     # 5.17:libgnucash/backend/xml/sixtp.cpp:902-928
classify(chunk)                                   # sixtp.cpp:930-997
```

`classify` (`5.17:libgnucash/backend/xml/sixtp.cpp:930-997`):

1. Skip ASCII whitespace (`isspace`). Nothing left → NOT_OURS.
2. Must start with `<?xml` else NOT_OURS.
3. Skip to just after the first `>`, skip whitespace, next char must be `<`.
4. If followed by `gnc-v2` **and then a whitespace char** → `GNC_BOOK_XML2_FILE`. `with_encoding` is true iff the
   text before that `<` contains `encoding=` (searched as `e` + `ncoding=`).
5. Else if followed by `gnc>` → `GNC_BOOK_XML1_FILE`.
6. Else if followed by `gnc-v` → `GNC_BOOK_POST_XML2_0_0_FILE` (too new). Note `<gnc-v2>` with no whitespace lands
   here (experiment row `gnc-v2-no-space`).
7. Else NOT_OURS.

The marker is `GNC_V2_STRING "gnc-v2"` (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:106-108`). Gzip is detected
by magic bytes only; the file name is irrelevant.

`determine_file_type` maps XML2 without encoding to `GNC_BOOK_XML2_FILE_NO_ENCODING`
(`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:199-226`). Load outcome per type
(`…/gnc-xml-backend.cpp:228-300`):

| Type | Load | Error |
|---|---|---|
| XML2 with encoding | parse v2 | `ERR_FILEIO_PARSE_ERROR` (1006) on failure |
| XML2 no encoding | not loaded | `ERR_FILEIO_NO_ENCODING` (1010); the GUI offers a conversion assistant and retries (`5.17:gnucash/gnome-utils/gnc-file.c:1055-1070`) |
| XML1 (`<gnc>`) | parse v1 | 1006 on failure |
| POST_XML2 | not loaded | `ERR_BACKEND_TOO_NEW` (10) |
| NOT_OURS | not loaded | `errno==EACCES` → `ERR_FILEIO_FILE_EACCES` (1011); else `ERR_FILEIO_UNKNOWN_FILE_TYPE` (1005) |

`GNC_BOOK_BIN_FILE` exists in the enum (`5.17:libgnucash/backend/xml/gnc-backend-xml.h:44-52`) but nothing returns
it, so the `-binfmt.bkup` branch in `backup_file` (`…/gnc-xml-backend.cpp:686-695`) is dead.

---

## 2. Open sequence

### 2.1 Backend `session_begin` (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:113-157`)

1. `m_fullpath = gnc_uri_get_path(uri)`; empty → `ERR_FILEIO_FILE_NOT_FOUND` "No path specified".
2. `SESSION_NEW_STORE` and the file exists (`stat` succeeds) → `ERR_BACKEND_STORE_EXISTS` (`:128-133`, `:333-341`).
3. `check_path(path, create)` (`:56-111`): parent must be a directory else `ERR_FILEIO_FILE_NOT_FOUND`; file must
   exist unless creating else `ERR_FILEIO_FILE_NOT_FOUND`; file must not be a directory else
   `ERR_FILEIO_UNKNOWN_FILE_TYPE`.
4. `xaccLogSetBaseName(m_fullpath)` (`:148`): sets the journal base name, and if a journal is already open in this
   process, closes it and opens a new one (§8.2).
5. `SESSION_READ_ONLY` → return, no lock (`:151-152`).
6. Otherwise `m_lockfile = path + ".LCK"` and take the lock (§3).

### 2.2 Load (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:760-852`)

Journal writing is disabled for the whole load (`xaccLogDisable` `:760`, re-enabled `:804`/`:852`), gzip is
decompressed on a thread through a pipe (`:784-785`, `:1523-1594`), then the book is marked clean
(`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:298-299`).

### 2.3 Session-level error handling (`5.17:libgnucash/engine/qofsession.cpp:198-248`)

After `backend->load`, errors other than `FILE_TOO_OLD`, `NO_ENCODING`, `FILE_UPGRADE`, `SQL_DB_TOO_OLD`,
`SQL_DB_TOO_NEW` destroy the backend and replace the book with a new empty one (`:231-245`). `destroy_backend()`
calls `clear_error()` (`:152-162`), which resets `m_last_err`, so the caller sees `ERR_BACKEND_NO_ERR`.
Experiment 6b, calling `qof_session_load` directly:

```
WARN <gnc.backend> [GncXmlBackend::load()] Version of Xml file …/gnc-v3.gnucash is new…
gnc-v3 after load  err= 0 msg=
WARN <gnc.backend> [GncXmlBackend::load()] Syntax error in Xml File …/leading-ws.gnuca…
leading-ws after load  err= 0 msg=
WARN <gnc.backend> [GncXmlBackend::load()] File not any known type
empty after load  err= 0 msg=
```

This is a GnuCash bug. red-ale must surface these as errors, never as an empty book.

### 2.4 GUI open (`gnc_post_file_open`, `5.17:gnucash/gnome-utils/gnc-file.c:795-1186`)

1. Normalise the URI; for network schemes fetch a password from the keyring (`:817-849`).
2. `qof_session_begin(uri, is_readonly ? SESSION_READ_ONLY : SESSION_NORMAL_OPEN)` (`:883-884`).
3. `ERR_BACKEND_BAD_URL` → error dialog, file chooser (`:887-904`).
4. `ERR_BACKEND_LOCKED` or `ERR_BACKEND_READONLY` → dialog "GnuCash could not obtain the lock for %s." with buttons
   *Open Read-Only* (re-begin `SESSION_READ_ONLY`), *Create New File*, *Open Anyway* (re-begin
   `SESSION_BREAK_LOCK`), *Open Folder*, and *Quit* at startup (`:906-999`).
5. `ERR_BACKEND_NO_SUCH_DB` → offer to create (`SESSION_NEW_STORE`) (`:1001-1010`).
6. Load with the journal disabled (`:1039-1043`). If the user chose read-only, `qof_book_mark_readonly(book)`
   (`:1045-1050`).
7. `NO_ENCODING` → conversion assistant, reload (`:1055-1070`). SQL too old → offer `safe_save`; SQL too old/new and
   declined → mark read-only (`:1074-1090`).
8. **Book feature check**: `gnc_features_test_unknown(book)`; any unknown → error dialog and the open is abandoned
   (`:1102-1116`, `:1144-1164`). See §10.4.
9. Post-load scrubs that can dirty the book (`run_post_load_scrubs`, `:753-793`): budget sign scrub (may set or unset
   `Use natural signs in budget amounts`), `xaccAccountScrubColorNotSet`, account-separator warning.

---

## 3. Lock

### 3.1 Protocol (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:634-673`)

```
lockfile = <fullpath> + ".LCK"
fd = open(lockfile, O_RDWR | O_CREAT | O_EXCL, S_IRUSR | S_IWUSR)    # 0600, nothing is written
if fd == -1:
    EACCES -> ERR_BACKEND_READONLY  "Unable to create lockfile, make sure that you have write access to the directory."
    EROFS  -> ERR_BACKEND_READONLY  "Unable to create lockfile, data file is on a read-only filesystem."
    ENOSPC -> ERR_BACKEND_READONLY  "Unable to create lockfile, no space on filesystem."
    EEXIST -> ERR_BACKEND_LOCKED
    other  -> ERR_FILEIO_FILE_LOCKERR "Lockfile creation failed. Please see the tracefile for details."
    unless (mode == SESSION_BREAK_LOCK and err == LOCKED):
        set error; lockfile = ""          # so session_end will not delete someone else's lock
fd stays open for the life of the session (m_lockfd)
```

- The `.LCK` file is empty. Oracle: `LCK bytes: b'' mode: 0o600`.
- There is **no staleness detection**: a leftover `.LCK` from a crash, even with mtime 1970, gives
  `ERR_BACKEND_LOCKED` (experiment 4). Only a human (GUI *Open Anyway*) breaks it.
- No `flock`/`fcntl` lock is taken on the fd; the file's existence is the lock.
- Link-count locking (`<lockfile-dir>/<lockfile>.<hostid>.<pid>.LNK` hard-linked to `.LCK`, `st_nlink == 2`
  check) was removed in 4.7 by commit 38cd06e54a. `m_linkfile` is declared
  (`5.17:libgnucash/backend/xml/gnc-xml-backend.hpp:59`) but never assigned in 5.17; `session_end` and
  `remove_old_files` still reference it (`…/gnc-xml-backend.cpp:168-169`, `:754`).

### 3.2 Release (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:159-197`)

```
session_end():
    if book is marked read-only: set ERR_BACKEND_READONLY; return     # :162-166  (lock NOT released)
    if m_linkfile: unlink(m_linkfile)                                  # never set in 5.17
    close(m_lockfd)
    if m_lockfile: unlink(m_lockfile)    # warns on failure, e.g. ENOENT
    clear paths
```

Called from `QofSessionImpl::end` (`5.17:libgnucash/engine/qofsession.cpp:334-343`) and the backend destructor
(`…/gnc-xml-backend.cpp:51-54`). A crash leaves the `.LCK` behind.

The early return is harmless in normal GUI flows (a book is marked read-only only when no lock was taken), but a
locked book later marked read-only would keep its `.LCK` after `end`.

### 3.3 Modes and the lock, experiment 4 (verbatim, `ls -lai` inode first)

```
[A normal] opened OK
802056 -rw-------  1 code code    0 … book.gnucash.LCK
[B normal while A open] FAILED: … ERR_BACKEND_LOCKED errors=(7,)
[R read-only while A open] opened OK
R.save() done (no exception)
--- after R.save()
802091 -rw-rw-r--  1 code code 1113 … book.gnucash                       <- R overwrote the book A has locked
802090 -rw-rw-r--  1 code code 1097 … book.gnucash.20261025130554.gnucash
802056 -rw-------  1 code code    0 … book.gnucash.LCK
[BL break-lock while A open] opened OK
--- after BL.end() -- A's lock is gone
802091 -rw-rw-r--  1 code code 1113 … book.gnucash
802090 -rw-rw-r--  1 code code 1097 … book.gnucash.20261025130554.gnucash
802089 -rw-rw-r--  1 code code 2132 … book.gnucash.20261025130554.log
WARN … [GncXmlBackend::session_end()] Error on g_unlink(…/book.gnucash.LCK): 2: No such file or directory   <- A.end()
[normal with stale empty .LCK (mtime 1970)] FAILED: … ERR_BACKEND_LOCKED errors=(7,)
[NEW_STORE over existing file] FAILED: … ERR_BACKEND_STORE_EXISTS errors=(8,)
[normal in read-only dir] FAILED: … ERR_BACKEND_READONLY errors=(9,)
[read-only in read-only dir] opened OK
[missing file] FAILED: (1003,)          # ERR_FILEIO_FILE_NOT_FOUND
[directory] FAILED: (1,)                # xml://<dir>: XML type check refuses -> NO_HANDLER
```

---

## 4. Save

### 4.1 Entry points

- `qof_session_save`: returns immediately if the book is clean (`qof_book_session_not_saved` false)
  (`5.17:libgnucash/engine/qofsession.cpp:436-475`, `:439-440`). Experiment 1 "save 3 (clean)" created nothing.
- `GncXmlBackend::sync` (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:302-324`): book read-only →
  `ERR_BACKEND_READONLY`, nothing written. Else `write_to_file(true)` then **`remove_old_files()` unconditionally**,
  even if the write failed (`:322-323`). `safe_sync` is the same (`…/gnc-xml-backend.hpp:42`).
- GUI `gnc_file_save` refuses a read-only book with "The database was opened read-only. Do you want to save it to a
  different place?" (`5.17:gnucash/gnome-utils/gnc-file.c:1469-1480`), and after a successful save rotates the
  journal with `xaccReopenLog()` (`:1506`). Save As does the same (`:1756`).

### 4.2 `write_to_file(make_backup = true)` (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:357-533`)

```
if book read-only: ERR_BACKEND_READONLY; fail                                   # :364-370
tmp = fullpath + ".tmp-XXXXXX"; mktemp(tmp)       # 6 chars replaced; name only  # :377-398
    failure -> ERR_BACKEND_MISC "Failed to make temp file"
backup_file()  (§6)                                # before writing              # :400-408
    failure -> fail (error already set: ERR_FILEIO_BACKUP_ERROR)
ok = gnc_book_write_to_xml_file_v2(book, tmp, compress = pref file-compression)  # :410-411
if ok:
    if stat(fullpath) succeeds:                                                  # :413-457
        chmod(tmp, st_mode of fullpath)     # failure only warned
        chown(tmp, -1, st_gid of fullpath)  # group only; failure only warned
    unlink(fullpath); failure other than ENOENT -> ERR_BACKEND_READONLY; fail    # :458-467
    link_or_make_backup(tmp, fullpath)      # hard link, copy fallback (§6.2)   # :468-476
        failure -> ERR_FILEIO_BACKUP_ERROR "Failed to make backup file <fullpath>"
    unlink(tmp); failure -> ERR_BACKEND_PERM; fail                               # :477-486
    qof_book_mark_session_saved(book)                                            # :491
else:
    unlink(tmp) fails with ENOENT/EACCES/EPERM/ENOSYS/EROFS -> ERR_BACKEND_READONLY
                fails otherwise                              -> ERR_BACKEND_MISC
                succeeds -> ERR_FILEIO_WRITE_ERROR "Unable to write to temp file <tmp>"   # :496-529
```

Points that matter for red-ale:

- **No rename, no fsync.** Between `unlink(fullpath)` and `link(tmp, fullpath)` the book name does not exist; the
  data is still reachable through the backup hard link and `tmp`. If the copy fallback is used for the final step,
  `fullpath` is created with mode 0600 (`copy_file`, `:554-555`), not the chmod-ed mode.
- If the write fails, the old book is untouched (still at `fullpath`) but a backup hard link was already made.
- A crash leaves `<book>.tmp-XXXXXX`. Nothing ever deletes these (`remove_old_files` skips them; experiment 3).
- Mode/group are copied from the old file; owner is never changed. A new book gets `0666 & ~umask` (observed
  `-rw-rw-r--`).
- `mktemp` is used, not `mkstemp`, because the writer must open the file itself for gzip (`:381-387`).
- The book's inode changes on every save (experiment 2: 802046 → 802047 → 802049).

### 4.3 Compression and gzip details

- Pref `general/file-compression`, boolean, schema default `true`
  (`5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:28-32`); library default also `TRUE`
  (`5.17:libgnucash/core-utils/gnc-prefs.cpp:36`); loaded by `file_compression_changed_cb`
  (`5.17:libgnucash/app-utils/gnc-prefs-utils.c:71-79`, `:91`).
- The choice is made per save from the pref alone; the compression of the file that was opened plays no part.
- `try_gz_open(tmp, "wb", compress, write=TRUE)` (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1523-1594`): names
  containing `.gz.` force compression (`:1527-1528`, legacy; temp names never match). Compressed writes go through a
  pipe to a thread that calls `gzopen(name, "wb")` and `gzwrite` in 4096-byte chunks (`:1376-1403`, `:1405-1446`,
  `:1484-1521`). If the pipe or thread can't be made, it writes **uncompressed** silently (`:1552-1560`, `:1574-1583`).
- `gzopen(…, "wb")` means zlib's default level (`Z_DEFAULT_COMPRESSION`, 6), no file name, mtime 0. The oracle's
  header bytes: `1f8b 0800 0000 0000 0003` (ID, CM=8, FLG=0, MTIME=0, XFL=0, OS=3).
- Success requires `fclose` and the thread join to succeed (`:1596-1622`).
- With `file-compression=false` the oracle wrote plain XML starting `<?xml version="1.0" encoding="utf-8" ?>\n<gnc-v2\n`
  (experiment 2, `nocomp`).

Byte-identical *compressed* output would need zlib's exact deflate at level 6; flate2 with miniz or zlib-ng output
differs. Compare decompressed bytes.

---

## 5. Retention prefs

| GSettings key (`org.gnucash.GnuCash.general`) | Type | Default | Cite |
|---|---|---|---|
| `retain-type-never` | b | false | `5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:73-77` |
| `retain-type-days` | b | **true** | `…:78-82` |
| `retain-type-forever` | b | false | `…:83-87` |
| `retain-days` | d | **30.0** | `…:88-92` |
| `file-compression` | b | true | `…:28-32` |

Mapping (`5.17:libgnucash/app-utils/gnc-prefs-utils.c:43-69`): `never` → `XML_RETAIN_NONE`, else `days` →
`XML_RETAIN_DAYS`, else `XML_RETAIN_ALL` (also the fallback when none is set). Days = `(int)retain-days`.
`days` with 0 days is rewritten to `forever` with 30 days (`:103-112`). Library defaults without a prefs backend:
policy days, 30 days, compression on (`5.17:libgnucash/core-utils/gnc-prefs.cpp:36-38`). Enum order
`XML_RETAIN_NONE, XML_RETAIN_DAYS, XML_RETAIN_ALL` (`5.17:libgnucash/backend/xml/gnc-backend-xml.h:37-43`).

---

## 6. Backups

### 6.1 `backup_file()` (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:675-702`)

```
if stat(fullpath) fails: return errno == ENOENT     # first save of a new book: no backup, success
backup = fullpath + "." + gnc_date_timestamp() + ".gnucash"
return link_or_make_backup(fullpath, backup)
```

- Runs on **every** save that has an existing book file, before the new data is written (§4.2). The backup holds
  the *previous* saved state.
- The name appends to the full file name: `book.gnucash` → `book.gnucash.20261025090504.gnucash`.
- `gnc_date_timestamp()` → `GncDateTime::timestamp()` (`5.17:libgnucash/engine/gnc-date.cpp:1173-1178`):
  `to_iso_string(now.local_time())` cut to `YYYYMMDD` + `HHMMSS` (`5.17:libgnucash/engine/gnc-datetime.cpp:638-644`).
  `now` is whole seconds (`…:311`). **Local time** of the process time zone, 14 digits, no zone marker.
- Two saves in the same second: the second `link()` fails with EEXIST, which is not a copy-fallback errno, so the
  save fails with `ERR_FILEIO_BACKUP_ERROR` (inferred from `:591-632`; not run).

### 6.2 `link_or_make_backup(orig, bkup)` (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:591-632`)

`link(orig, bkup)`; on failure with `EPERM`, `ENOSYS`, `EOPNOTSUPP`, `ENOTSUP`, fall back to `copy_file`
(`:535-589`: `open(bkup, O_WRONLY|O_CREAT|O_TRUNC|O_EXCL, 0600)`, 1024-byte read/write loop, no fsync). Any other
failure → `ERR_FILEIO_BACKUP_ERROR`.

### 6.3 Experiment 2 (TZ=America/New_York, UTC was 13:05; default prefs)

```
TZ = America/New_York  now(local) = 20261025090503  now(utc) = 20261025130503
--- after save 1
802046 -rw-rw-r-- 1 code code 1099 2026-10-25 09:05:03 book.gnucash
802045 -rw-rw-r-- 1 code code  656 2026-10-25 09:05:03 book.gnucash.20261025090503.log
802044 -rw------- 1 code code    0 2026-10-25 09:05:03 book.gnucash.LCK
--- after save 2
802047 -rw-rw-r-- 1 code code 1096 2026-10-25 09:05:04 book.gnucash
802045 -rw-rw-r-- 1 code code 1578 2026-10-25 09:05:04 book.gnucash.20261025090503.log
802046 -rw-rw-r-- 1 code code 1099 2026-10-25 09:05:03 book.gnucash.20261025090504.gnucash
802044 -rw------- 1 code code    0 2026-10-25 09:05:03 book.gnucash.LCK
--- after save 3
802049 -rw-rw-r-- 1 code code 1097 2026-10-25 09:05:05 book.gnucash
802045 -rw-rw-r-- 1 code code 2494 2026-10-25 09:05:05 book.gnucash.20261025090503.log
802046 -rw-rw-r-- 1 code code 1099 2026-10-25 09:05:03 book.gnucash.20261025090504.gnucash
802047 -rw-rw-r-- 1 code code 1096 2026-10-25 09:05:04 book.gnucash.20261025090505.gnucash
802044 -rw------- 1 code code    0 2026-10-25 09:05:03 book.gnucash.LCK
--- after end
802049 -rw-rw-r-- 1 code code 1097 2026-10-25 09:05:05 book.gnucash
802045 -rw-rw-r-- 1 code code 2494 2026-10-25 09:05:05 book.gnucash.20261025090503.log
802046 -rw-rw-r-- 1 code code 1099 2026-10-25 09:05:03 book.gnucash.20261025090504.gnucash
802047 -rw-rw-r-- 1 code code 1096 2026-10-25 09:05:04 book.gnucash.20261025090505.gnucash
```

Each backup has the inode of the previous `book.gnucash` (hard link), keeps its mtime, and is named with the local
time of the save that replaced it. The same run with `retain-type-never=true` and `file-compression=false`:

```
--- after save 1
802053 -rw-rw-r-- 1 code code 4586 2026-10-25 09:05:05 book.gnucash
802051 -rw------- 1 code code    0 2026-10-25 09:05:05 book.gnucash.LCK
--- after save 2
802054 -rw-rw-r-- 1 code code 4583 2026-10-25 09:05:07 book.gnucash
802051 -rw------- 1 code code    0 2026-10-25 09:05:05 book.gnucash.LCK
```

Backups are made, then deleted by the cleanup in the same save; the open journal was deleted after save 1.

---

## 7. Cleanup of old files (`remove_old_files`, `5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:709-832`)

Runs after every `sync` (§4.1). Algorithm:

```
if stat(m_lockfile) fails: return          # read-only sessions (no lockfile) never clean up   :714-715
for dent in readdir(dirname(fullpath)):
    if not dent ends with ".LNK", ".xac", ".gnucash" or ".log" (case-sensitive): skip        :728-732
    name = dirname + "/" + dent
    if not name.startswith(fullpath): skip                                                   :737-741
    if name == fullpath: skip                                                                :744-748
    if name ends with ".LNK":                                                                 :751-765
        delete if name != m_linkfile and mtime(name) < mtime(.LCK)   # leftover from <4.7 locking
        continue
    rest = name[len(fullpath):]
    require rest =~ /^\.[[:digit:]]{14}(\.gnucash|\.log|\.xac)$/  (POSIX ERE, REG_ICASE)      :782-789
    policy NONE: delete                                                                       :804-808
    policy DAYS and days > 0:                                                                 :809-828
        age = (int)(difftime(now, mtime(name)) / 86400)      # truncated whole days
        delete if age >= retain_days
    policy ALL: keep
```

- The regex is case-insensitive but the suffix pre-filter is not, so `.GNUCASH`/`.LOG` files survive.
- The age uses the file's **mtime**, not the timestamp in its name. Backups are hard links, so their mtime is
  the time the *previous* save wrote the book.
- **Nothing else is cleaned**: not `.tmp-XXXXXX`, not `.LCK`, not other books' files, not `*.gz`.
- Under policy NONE the **current journal** matches and is unlinked while still open (experiment 2 `nocomp`).

Experiment 3 (policy days, 30 days; fake files with backdated mtimes, then one edit and save):

```
--- before reopen+save (fake old files created)
… book.gnucash.20200101000000.gnucash      40 days old
… book.gnucash.20200101000001.log          40
… book.gnucash.20200101000002.xac          40
… book.gnucash.20200101000003.gnucash      29.9
… book.gnucash.20200101000004.gnucash      30.1
… book.gnucash.20200101000005.GNUCASH      40
… book.gnucash.20200101000006.LOG          40
… book.gnucash.20200101000007.gnucash.gz   40
… book.gnucash.2020010100000.gnucash       40   (13 digits)
… book.gnucash.LCK.old                     40
… book.gnucash.new.LNK                     -1 (newer than .LCK)
… book.gnucash.old.LNK                     40
… book.gnucash.tmp-AbC123                  40
… book.gnucash.x20200101000000.gnucash     40
… other.gnucash.20200101000000.gnucash     40
--- after save (retain-type=days, retain-days=30)
802085 -rw-rw-r-- 1 code code 1095 2026-10-25 13:05:37 book.gnucash
802072 -rw-rw-r-- 1 code code    1 2026-09-25 16:29:37 book.gnucash.20200101000003.gnucash
802074 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 book.gnucash.20200101000005.GNUCASH
802077 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 book.gnucash.20200101000006.LOG
802083 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 book.gnucash.20200101000007.gnucash.gz
802075 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 book.gnucash.2020010100000.gnucash
802069 -rw-rw-r-- 1 code code 1101 2026-10-25 13:05:37 book.gnucash.20261025130537.gnucash
802068 -rw-rw-r-- 1 code code 1738 2026-10-25 13:05:37 book.gnucash.20261025130537.log
802084 -rw------- 1 code code    0 2026-10-25 13:05:37 book.gnucash.LCK
802082 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 book.gnucash.LCK.old
802079 -rw-rw-r-- 1 code code    1 2026-10-26 13:05:37 book.gnucash.new.LNK
802081 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 book.gnucash.tmp-AbC123
802076 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 book.gnucash.x20200101000000.gnucash
802080 -rw-rw-r-- 1 code code    1 2026-09-15 14:05:37 other.gnucash.20200101000000.gnucash
```

Deleted: `…000000.gnucash`, `…000001.log`, `…000002.xac`, `…000004.gnucash` (30.1 days), `old.LNK`.

---

## 8. Journal (`.log`)

### 8.1 File

- Name: `<log_base_name> + "." + gnc_date_timestamp() + ".log"` (`5.17:libgnucash/engine/TransLog.cpp:173-176`),
  where the base is the book's full path (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:148`). Example
  `book.gnucash.20261025090503.log`. Timestamp: **local time**, as for backups (§6.1). With no base set:
  `translog.<ts>.log` in the CWD (`TransLog.cpp:170-171`).
- Opened with `std::ofstream(…, std::ios::app)` (`:178`): appends if the name already exists (same second).
  Mode `0666 & ~umask` (observed `-rw-rw-r--`). Open failure prints to stdout and journaling silently stops (`:179-188`).
- One process-wide stream (`:88-91`), not per session or per book.

### 8.2 Lifecycle

| Event | Effect | Cite |
|---|---|---|
| `xaccTransBeginEdit` (outermost) on a book not marked read-only | `xaccOpenLog()`: opens a journal if none is open, then writes a `B` record | `5.17:libgnucash/engine/Transaction.cpp:1340-1357` |
| Backend `session_begin` | `xaccLogSetBaseName(path)`: if a journal is open, close it and open a new one under the new base | `5.17:libgnucash/engine/TransLog.cpp:119-131` |
| GUI successful Save / Save As | `xaccReopenLog()`: if open, close and open a new timestamped file | `5.17:gnucash/gnome-utils/gnc-file.c:1506`, `:1756`; `TransLog.cpp:108-116` |
| Session end | nothing; the journal stays open | (no call in `gnc-xml-backend.cpp`) |
| Load, replay, export, GUI open/close | journaling disabled (`xaccLogDisable`) around them | `5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:760`, `:852`; `5.17:gnucash/gnome-utils/gnc-file.c:1039-1043` |

- A journal file therefore appears only after the first transaction edit, not at open. In experiment 1, creating
  accounts made no journal; the first `Transaction.BeginEdit` did.
- In the GUI each save starts a new journal, so a journal holds the changes since the last save, plus the
  transaction edits begun before it. The library alone (Python) keeps one journal for the process and only starts a
  new file on the next `session_begin`; each reopen writes a fresh header, so a file can contain several headers
  (experiment 4 log).
- Only **transactions** are journaled. Accounts, prices, commodities, business objects and book slots are not.

### 8.3 Format (`5.17:libgnucash/engine/TransLog.cpp:198-288`)

Header, written on every open:

```
mod\ttrans_guid\tsplit_guid\ttime_now\tdate_entered\tdate_posted\tacc_guid\tacc_name\tnum\tdescription\tnotes\tmemo\taction\treconciled\tamount\tvalue\tdate_reconciled\n
-----------------\n                      (17 '-')
```

Record, one per `xaccTransWriteLog(trans, flag)`:

```
===== START\n
<one line per split, in trans->splits order>
===== END\n            (written with std::endl: flushed after every record)
```

Split line: 17 fields joined by `\t`, ending `\n`:

| # | Field | Source | Format |
|---|---|---|---|
| 1 | mod | flag | one char `B`/`C`/`R`/`D` |
| 2 | trans_guid | transaction GUID | 32 lowercase hex |
| 3 | split_guid | split GUID | 32 hex |
| 4 | time_now | `gnc_time(NULL)` | `YYYY-MM-DD HH:MM:SS`, **UTC** |
| 5 | date_entered | `trans->date_entered` | same |
| 6 | date_posted | `trans->date_posted` | same (GnuCash's neutral time 10:59:00 UTC for dates) |
| 7 | acc_guid | split's account GUID | 32 hex, empty if no account |
| 8 | acc_name | account *name* (not full name) | raw |
| 9 | num | `trans->num` | raw |
| 10 | description | `trans->description` | raw |
| 11 | notes | `xaccTransGetNotes` (KVP `notes`) | raw |
| 12 | memo | `split->memo` | raw |
| 13 | action | `split->action` | raw |
| 14 | reconciled | `split->reconciled` | one char (`n`,`c`,`y`,`f`,`v`) |
| 15 | amount | `xaccSplitGetAmount` | `num/denom` decimal int64s, as stored (not reduced) |
| 16 | value | `xaccSplitGetValue` | `num/denom` |
| 17 | date_reconciled | `split->date_reconciled` | UTC; `1970-01-01 00:00:00` when never reconciled |

- Dates come from `gnc_time64_to_iso8601_buff` → `GncDateTime::format_iso8601()`: UTC, `T` replaced by space,
  19 chars (`5.17:libgnucash/engine/gnc-date.cpp:1213-1235`; `5.17:libgnucash/engine/gnc-datetime.cpp:630-636`).
- **No quoting or escaping.** Tabs and newlines in strings are written raw (`TransLog.cpp:60-65` admits it).
- The transaction currency, split lots, KVP other than notes, and the account's commodity are not recorded.

Which events write which flag:

| Flag | Written by | State recorded | Cite |
|---|---|---|---|
| `B` | `xaccTransBeginEdit`, outermost level only, book not read-only, not shutting down | before the edit (an empty record for a brand-new transaction) | `Transaction.cpp:1344-1352` |
| `C` | `trans_cleanup_commit` after a successful commit, book not read-only | after the commit | `Transaction.cpp:1499-1500` |
| `R` | `xaccTransRollbackEdit`, book not read-only | after restoring the original | `Transaction.cpp:1731-1732` |
| `D` | `do_destroy` (commit of a destroyed or emptied transaction), not shutting down, book not read-only | the transaction as it is destroyed | `Transaction.cpp:1396-1410`; `5.17:libgnucash/engine/qofinstance.cpp:1021-1029` |

A destroy writes `B` … `D`, never `C` (`qof_commit_edit_part2` calls `on_free` *instead of* `on_done`). A commit
that empties a transaction is a destroy (`Transaction.cpp:1537-1538`).

### 8.4 Oracle example (experiment 1, `cat -A`: `^I` = tab, `$` = end of line)

Script: create Bank and Food, post one transaction (description `"Groceries\twith tab"`, num `101`, notes
`a note`, memo `memo1`, action `Buy`, ±12.34 USD), save, edit the description, begin and roll back an edit, delete
the transaction, save.

```
mod^Itrans_guid^Isplit_guid^Itime_now^Idate_entered^Idate_posted^Iacc_guid^Iacc_name^Inum^Idescription^Inotes^Imemo^Iaction^Ireconciled^Iamount^Ivalue^Idate_reconciled$
-----------------$
===== START$
===== END$
===== START$
C^I65e21d8a290f4355b9555dfca9bfe888^I4b9a929657604caaa5a5f57b2a4b1917^I2026-10-25 13:04:17^I2026-10-25 13:04:17^I2024-03-15 10:59:00^Id031a27171554d279107453116bb1dee^IBank^I101^IGroceries^Iwith tab^Ia note^Imemo1^I^In^I-1234/100^I-1234/100^I1970-01-01 00:00:00$
C^I65e21d8a290f4355b9555dfca9bfe888^Ib7fa927032c94a4ea19da098987e9b42^I2026-10-25 13:04:17^I2026-10-25 13:04:17^I2024-03-15 10:59:00^I48e84d8dc4e24c538e3db22e8d33a5c1^IFood^I101^IGroceries^Iwith tab^Ia note^I^IBuy^In^I1234/100^I1234/100^I1970-01-01 00:00:00$
===== END$
===== START$
B^I65e21d8a…^Ib7fa9270…^I2026-10-25 13:04:18^I2026-10-25 13:04:17^I2024-03-15 10:59:00^I48e84d8d…^IFood^I101^IGroceries^Iwith tab^Ia note^I^IBuy^In^I1234/100^I1234/100^I1970-01-01 00:00:00$
B^I65e21d8a…^I4b9a9296…^I2026-10-25 13:04:18^I2026-10-25 13:04:17^I2024-03-15 10:59:00^Id031a271…^IBank^I101^IGroceries^Iwith tab^Ia note^Imemo1^I^In^I-1234/100^I-1234/100^I1970-01-01 00:00:00$
===== END$
===== START$
C^I…^IFood^I101^IGroceries edited^Ia note^I^IBuy^In^I1234/100^I1234/100^I1970-01-01 00:00:00$
C^I…^IBank^I101^IGroceries edited^Ia note^Imemo1^I^In^I-1234/100^I-1234/100^I1970-01-01 00:00:00$
===== END$
===== START$
B^I… (2 lines, "Groceries edited") $
===== END$
===== START$
R^I… (2 lines, "Groceries edited") $
===== END$
===== START$
B^I… (2 lines) $
===== END$
===== START$
D^I65e21d8a290f4355b9555dfca9bfe888^Ib7fa927032c94a4ea19da098987e9b42^I2026-10-25 13:04:18^I2026-10-25 13:04:17^I2024-03-15 10:59:00^I48e84d8dc4e24c538e3db22e8d33a5c1^IFood^I101^IGroceries edited^Ia note^I^IBuy^In^I1234/100^I1234/100^I1970-01-01 00:00:00$
D^I65e21d8a290f4355b9555dfca9bfe888^I4b9a929657604caaa5a5f57b2a4b1917^I2026-10-25 13:04:18^I2026-10-25 13:04:17^I2024-03-15 10:59:00^Id031a27171554d279107453116bb1dee^IBank^I101^IGroceries edited^Ia note^Imemo1^I^In^I-1234/100^I-1234/100^I1970-01-01 00:00:00$
===== END$
```

(Elided lines marked `…`; the full file is reproducible with the script described.) Note the first record is empty
(begin edit before splits were added), the raw tab inside `Groceries^Iwith tab` gives 18 fields, and split order
can change between records. The rollback attempt's `B` reflects state before the edit, the `R` the restored state.
The directory after this run:

```
-rw-rw-r-- 1 code code  758 13:04:18 book.gnucash
-rw-rw-r-- 1 code code 3728 13:04:18 book.gnucash.20261025130417.log
-rw-rw-r-- 1 code code 1163 13:04:17 book.gnucash.20261025130418.gnucash
```

---

## 9. Replay (`5.17:gnucash/import-export/log-replay/gnc-log-replay.cpp`)

GUI-only (*File → Import → Replay GnuCash .log file*); not callable from the oracle. Behaviour from source:

### 9.1 File handling (`gnc_file_log_replay`, `:542-648`)

1. Journaling is disabled for the whole replay (`:560`, `:645`).
2. Refuses the journal currently being written (basename equal to the open journal's name) (`:583-590`;
   `5.17:libgnucash/engine/TransLog.cpp:140-152`).
3. First line (read with a 256-byte buffer) must **start with** the header field list (no trailing `\n` compared);
   else "The log file you selected cannot be read. The file header was not recognized." (`:552-554`, `:617-624`).
   Empty file → "The log file you selected was empty." Repeated headers later in the file are ignored as noise.
4. Scans lines (256-byte buffer) for ones starting `===== START`; each starts `process_trans_record` (`:627-636`).
   Lines longer than 255 bytes are read in pieces; a piece starting `===== START` would be misread (edge case).

### 9.2 Record parsing (`interpret_split_record`, `:142-254`)

- Fields split on single `\t` by a custom `strtok` that keeps empty fields (`:105-140`). Line read with a 2048-byte
  buffer and right-trimmed by `g_strchomp` (`:376-382`), which strips trailing whitespace including tabs (harmless:
  the last field, `date_reconciled`, is never empty). Lines over 2047 bytes split into two corrupt records.
- Each non-empty field sets a `*_present` flag; empty fields are "absent".
- GUIDs: `string_to_guid`; dates: `gnc_iso8601_to_time64_gmt` (UTC; parse failure gives `INT64_MAX`)
  (`5.17:libgnucash/engine/gnc-date.cpp:1189-1201`); numerics: `gnc_numeric_from_string`; strings copied to 255
  bytes max (`STRING_FIELD_SIZE 256`, `:62`), silently truncated.
- An 18th field logs "Expected number of fields exceeded!" but the record is still used (`:248-251`), with
  every field after the embedded tab shifted by one.
- `mod` other than B/C/R/D: `log_action` stays 0 (= `LOG_BEGIN_EDIT`) but `present` is set, so it is ignored like `B`.

### 9.3 Applying a record (`process_trans_record`, `:357-540`)

A record is all lines up to `===== END` (or EOF). State: `trans = NULL`, `first_record = TRUE`.

| Line flag | Action |
|---|---|
| `B` | ignored (`:388-390`) |
| `R` | ignored (`:391-393`) |
| `D`, first line | look up the transaction by GUID; if found: clear its read-only flag if set, `BeginEdit`, `Destroy`. Not found: "The transaction to delete was not found!" (`:394-411`) |
| `D`, later lines | `xaccTransDestroy(trans)` again (`:412-413`; `trans` may be NULL) |
| `C`, first line with a trans GUID | look up; found → `BeginEdit`, save and clear its read-only reason; not found → create a new transaction, `BeginEdit`. Force its GUID; set date entered, date posted, num, description, notes for each field present (`:415-462`) |
| `C`, every line with a split GUID | look up or create the split; force its GUID; if `acc_guid` present, look up the account and `xaccAccountInsertSplit` (a missing account means NULL); if the transaction has no currency, set the account's currency or the default currency; append a new split to the transaction; set memo, action, reconcile date, reconcile flag, amount, value for each field present (`:463-517`) |
| `===== END` / EOF | if `trans`: `xaccTransScrubCurrency`, restore the read-only reason, `CommitEdit` (`:527-538`) |

Consequences:

- Replay is **upsert by GUID**, so replaying a journal onto a book that already has the changes is idempotent for
  fields present. It is not a faithful undo log: splits removed during an edit stay in the book (only splits listed
  are touched), empty fields never clear a value, and the currency is guessed.
- `R` records do not undo; a `B` with no following `C` changes nothing.
- The engine's commit-time scrub (imbalance, trading) runs on each replayed transaction.
- Empty `===== START`/`===== END` records do nothing.

---

## 10. Book features

### 10.1 Storage

Book KVP frame `features`, one slot per Book feature: key = feature name, value = string description
(`5.17:libgnucash/engine/qofbook.cpp:1245`, `:1271-1289`). XML form (experiment 5, written by GnuCash 5.17 after a
load/edit/save, unchanged including the unknown entry):

```xml
<book:slots>
  <slot>
    <slot:key>features</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>Credit Notes</slot:key>
        <slot:value type="string">Customer and vendor credit notes (requires at least GnuCash 2.5.0)</slot:value>
      </slot>
      <slot>
        <slot:key>Frobnicate</slot:key>
        <slot:value type="string">Frobnication (requires at least GnuCash 9.0)</slot:value>
      </slot>
    </slot:value>
  </slot>
  …
```

Setting re-writes the slot only if absent or the description differs, and dirties the book
(`qofbook.cpp:1282-1288`). Unsetting deletes the slot (`:1317-1331`).

### 10.2 Known features in 5.17

Names: `5.17:libgnucash/engine/gnc-features.h:54-64`. Table: `5.17:libgnucash/engine/gnc-features.cpp:36-48`.

| Key (slot name) | Description written (exact) | Set when |
|---|---|---|
| `Credit Notes` | `Customer and vendor credit notes (requires at least GnuCash 2.5.0)` | `gncInvoiceSetIsCreditNote(inv, TRUE)` (`5.17:libgnucash/engine/gncInvoice.c:576-593`) |
| `Number Field Source` | `User specifies source of 'num' field'; either transaction number or split action (requires at least GnuCash 2.5.0)` | GUI turns on the book option "Use Split Action Field for Number" (`5.17:gnucash/gnome-utils/gnc-main-window.cpp:4708-4721`). Never unset. |
| `Extra data in addresses, jobs or invoice entries` | `Extra data for addresses, jobs or invoice entries (requires at least GnuCash 2.6.4)` | commit of an address, job, entry or tax table that has any KVP (`gncAddress.c:493-497`, `gncJob.c:427-431`, `gncEntry.c:1635-1640`, `gncTaxTable.c:651-656`, all `5.17:libgnucash/engine/`) |
| `Account GUID based Bayesian data` | `Use account GUID as key for Bayesian data (requires at least GnuCash 2.6.12)` | not set by 5.17 code; superseded by the flat form |
| `Account GUID based bayesian with flat KVP` | `Use account GUID as key for bayesian data and store KVP flat (requires at least Gnucash 2.6.19)` | first Bayesian import-map read/write converts old data; also every Bayesian map update (`5.17:libgnucash/engine/Account.cpp:5462-5507`, `:5604`) |
| `ISO-8601 formatted date strings in SQLite3 databases.` | `Use ISO formatted date-time strings in SQLite3 databases (requires at least GnuCash 2.6.20)` | SQLite backend on save (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:880`) |
| `Register sort and filter settings stored in .gcm file` | `Store the register sort and filter settings in .gcm metadata file (requires at least GnuCash 3.3)` | **opening any register page in the GUI** (`5.17:gnucash/gnome/gnc-plugin-page-register.cpp:404-406`) |
| `Use natural signs in budget amounts` | `Store budget amounts unreversed (i.e. natural) signs (requires at least Gnucash 3.8)` | GUI open: set if budgets exist and it's missing (after flipping signs), **unset** if no budgets (`5.17:libgnucash/engine/ScrubBudget.c:193-216`, called from `5.17:gnucash/gnome-utils/gnc-file.c:768`); also new budget plugin (`5.17:gnucash/gnome/gnc-plugin-budget.c:181-183`) |
| `Show extra account columns in the Budget View` | `Show extra account columns in the Budget View (requires at least Gnucash 3.8)` | GUI budget view option (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:903-906`) |
| `Use a dedicated opening balance account identified by an 'equity-type' slot` | `Use a dedicated opening balance account identified by an 'equity-type' slot (requires at least Gnucash 4.3)` | **opening any register page** (`gnc-plugin-page-register.cpp:408-410`) or finishing the new-account-hierarchy assistant (`5.17:gnucash/gnome/assistant-hierarchy.cpp:1432-1433`) |

Obsolete (`gnc-features.cpp:61-63`): `Use a Book-Currency`, description `User-specified book currency stored in KVP.
Never implemented but some user managed to get it set anyway. (requires at least GnuCash 2.7.0)`. When found it is
**removed** from the book (dirtying it) and not reported (`gnc-features.cpp:88-96`).

`gnc_features_set_used` refuses names not in the table (`gnc-features.cpp:107-121`), so GnuCash never adds an
unknown feature.

### 10.3 Unknown-feature check (`5.17:libgnucash/engine/gnc-features.cpp:77-105`)

```
unknowns = [(k, descr) for (k, descr) in book.features if k not in features_table]   # qofbook.cpp:1291-1308
remove obsolete ones from the book and from unknowns
if unknowns empty: OK
message = _("This Dataset contains features not supported by this version of GnuCash. You must use a newer version
           of GnuCash in order to support the following features:") + "\n* " + descr for each unknown
```

The message lists **descriptions**, not keys, in KVP map order (sorted by key).

### 10.4 What GnuCash does

Only the GUI calls it, after load (`5.17:gnucash/gnome-utils/gnc-file.c:1102-1116`): show the message as an error
and abandon the open (destroy the session, release the lock). No read-only fallback. The engine and the Python
bindings load such books without complaint (experiment 5 loaded `Frobnicate` and saved it back).

---

## 11. Book options that act as read-only triggers

All live under the book KVP frame `options` (`KVP_OPTION_PATH`, `5.17:libgnucash/engine/qofbookslots.h:55`), section
`Accounts` (`…:64`). The GUI writes booleans as strings `"t"`/`"f"` and number ranges as `double`
(`5.17:libgnucash/engine/gnc-optiondb.cpp:404-410`, `:488-503`; option registration `:1206-1219`). XML (experiment 5,
round-tripped by GnuCash):

```xml
  <slot>
    <slot:key>options</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>Accounts</slot:key>
        <slot:value type="frame">
          <slot>
            <slot:key>Day Threshold for Read-Only Transactions (red line)</slot:key>
            <slot:value type="double">30</slot:value>
          </slot>
          <slot>
            <slot:key>Use Split Action Field for Number</slot:key>
            <slot:value type="string">t</slot:value>
          </slot>
          <slot>
            <slot:key>Use Trading Accounts</slot:key>
            <slot:value type="string">t</slot:value>
          </slot>
        </slot:value>
      </slot>
    </slot:value>
  </slot>
```

Doubles are written `%24.18g`, trimmed (`5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:189-203`). Oracle
readings for that book: `use_trading_accounts: True`, `use_split_action_for_num_field: True`,
`num_days_autoreadonly: 30`, and `xaccTransIsReadonlyByPostedDate` True for a 2024 transaction.

| Option (KVP path) | Type | Engine test | Effect | Cite |
|---|---|---|---|---|
| `options/Accounts/Use Trading Accounts` | string | true iff value is exactly `"t"` | commit-time scrub creates/clears trading splits under a top-level `Trading` account, by commodity namespace | `5.17:libgnucash/engine/qofbook.cpp:919-928`; `5.17:libgnucash/engine/Scrub.cpp:864-871`, `:530-552` |
| `options/Accounts/Use Split Action Field for Number` | string | exactly `"t"` (cached) | "Num" means split action, transaction num shows as T-Num; business code writes numbers there | `qofbook.cpp:930-959`; `5.17:libgnucash/engine/engine-helpers.c:56-125` |
| `options/Accounts/Day Threshold for Read-Only Transactions (red line)` | double, 0–3650, default 0 | `(int)value != 0` | transactions posted strictly before `today − N days` (local date) are read-only **in the GUI**; SX template transactions exempt | `qofbook.cpp:974-1019`; `5.17:libgnucash/engine/Transaction.cpp:2373-2400` |

Other paths in the same frame: `options/Budgeting/Default Budget` (`qofbookslots.h:69-70`), business options, and
the `counters`/`counter_formats` trees that live at the top level, not under `options` (`qofbook.cpp:1361-1376`).

What GnuCash does with each:

- **Auto read-only:** enforced only in the register and dialogs (`split-register-model.c:1989-1999`,
  `gnc-split-reg.c:1128`, `datecell-gnome.c:99-135`, `dialog-utils.c:407-415`, all under `5.17:gnucash/`). The engine
  does not check it: experiment 5 printed `engine allowed edit; description now: edited despite red line`.
- **Trading accounts:** the engine applies it on every transaction commit through `xaccTransScrubImbalance`
  (`Transaction.cpp:1547-1565`, scrub call at `:1558`). An editor that does not implement trading-account balancing would write
  transactions GnuCash would rebalance differently.
- **Num source:** the engine stores both fields either way; only meaning and UI placement change. Turning it on in
  the GUI also sets the `Number Field Source` Book feature (§10.2).

---

## 12. Session modes

`5.17:libgnucash/engine/qofsession.h:120-132`:

| Mode | Value | Type check | Must exist | Lock | Book marked read-only | Notes |
|---|---|---|---|---|---|---|
| `SESSION_NORMAL_OPEN` | 0 | yes | yes | `O_EXCL`; `EEXIST` → `ERR_BACKEND_LOCKED` | no | |
| `SESSION_NEW_STORE` | 2 | no | no | yes | no | existing file → `ERR_BACKEND_STORE_EXISTS` |
| `SESSION_NEW_OVERWRITE` | 3 | no | no | yes | no | |
| `SESSION_READ_ONLY` | 4 | yes | yes | **none** | **no** (GUI does it: `gnc-file.c:1045-1050`) | no cleanup either (§7) |
| `SESSION_BREAK_LOCK` | 5 | yes | yes | tries `O_EXCL`; on `EEXIST` keeps going **without** a lock fd, and deletes the existing `.LCK` at `end` | no | §3.3 |

Effects of `qof_book_mark_readonly` (`5.17:libgnucash/engine/qofbook.cpp:497-508`, `qof_book_mark_readonly` at `:504`): no journal records
(`Transaction.cpp:1348`, `:1409`, `:1499`, `:1731`), `sync`/`write_to_file` refuse with `ERR_BACKEND_READONLY`
(`gnc-xml-backend.cpp:315-320`, `:364-370`), `session_end` returns early without releasing a lock (`:162-166`).

---

## 13. Other files beside a book

| File | Made by | Cleaned by | Cite |
|---|---|---|---|
| `<book>.LCK` | `session_begin` | `session_end` | §3 |
| `<book>.<14 digits>.gnucash` | `backup_file` on each save | retention | §6, §7 |
| `<book>.<14 digits>.log` | first transaction edit; GUI save; next `session_begin` | retention | §8 |
| `<book>.<14 digits>.xac` | pre-1.8 GnuCash only | retention | `gnc-xml-backend.cpp:729` |
| `<book>.tmp-XXXXXX` | `write_to_file`; left behind by a crash mid-save | never | §4.2 |
| `*.LNK` | GnuCash < 4.7 lock protocol | cleanup removes those older than the current `.LCK` | §3.1, §7 |
| `<book>-binfmt.bkup` | dead code | never | §1.3 |
| `<userdata>/books/<book basename>.gcm` (or `_2`, `_3`… when another book with that name exists; matched by `[Top] BookGuid`) | GUI state: open tabs, register sort/filter | never | `5.17:libgnucash/app-utils/gnc-state.c:69-200`; `gnc-state.h:62-64`; `5.17:libgnucash/core-utils/gnc-filepath-utils.cpp:1126-1129` |

GnuCash 5.17 creates no `~` or `.bak` files (no such code in the XML backend, `gnc-file.c` or `TransLog.cpp`).
Save As into GnuCash's own user-data directory is refused with `ERR_FILEIO_RESERVED_WRITE`
(`5.17:gnucash/gnome-utils/gnc-file.c:1285-1309`, `:1605-1612`).

---

## 14. Error codes used here

`5.17:libgnucash/engine/qofbackend.h:57-127`.

| Code | Name | Raised here when |
|---|---|---|
| 1 | `ERR_BACKEND_NO_HANDLER` | no provider accepts the URI/file type |
| 3 | `ERR_BACKEND_BAD_URL` | URI is a directory (`file`/bare), or no backend |
| 7 | `ERR_BACKEND_LOCKED` | `.LCK` exists |
| 8 | `ERR_BACKEND_STORE_EXISTS` | `NEW_STORE` over an existing file |
| 9 | `ERR_BACKEND_READONLY` | can't create `.LCK` (EACCES/EROFS/ENOSPC); save of a read-only book; can't unlink the old book |
| 10 | `ERR_BACKEND_TOO_NEW` | `<gnc-v…` other than `gnc-v2` (lost, §2.3) |
| 16 | `ERR_BACKEND_PERM` | can't unlink the temp file after a save |
| 17 | `ERR_BACKEND_MISC` | `mktemp` failed; temp write failed and unlink failed oddly |
| 1002 | `ERR_FILEIO_FILE_LOCKERR` | other lock-creation errno |
| 1003 | `ERR_FILEIO_FILE_NOT_FOUND` | missing parent dir or file |
| 1005 | `ERR_FILEIO_UNKNOWN_FILE_TYPE` | path is a directory; NOT_OURS at load (lost, §2.3) |
| 1006 | `ERR_FILEIO_PARSE_ERROR` | XML parse failure (lost, §2.3) |
| 1007 | `ERR_FILEIO_BACKUP_ERROR` | backup link/copy failed; final link of the temp file failed |
| 1008 | `ERR_FILEIO_WRITE_ERROR` | writing the temp file failed |
| 1010 | `ERR_FILEIO_NO_ENCODING` | v2 file without `encoding=` |
| 1011 | `ERR_FILEIO_FILE_EACCES` | NOT_OURS at load with `errno == EACCES` |

---

## 15. Implications for red-ale

These are proposals for the design docs, not GnuCash behaviour.

1. **Lock:** create `<book>.LCK` with `O_CREAT|O_EXCL`, mode 0600, empty, and hold it for the session, so GnuCash
   and red-ale exclude each other. Never treat a `.LCK` as stale automatically. Never delete a `.LCK` red-ale did not
   create (do not copy `BREAK_LOCK`'s behaviour). Release on every exit path, including when the session became
   read-only.
2. **Read-only sessions never write**, unlike `SESSION_READ_ONLY` through the bindings.
3. **Save** in GnuCash's order so the files on disk look the same: backup hard link (copy fallback with the same
   errnos), write `<book>.tmp-XXXXXX`, copy mode and group, replace the book, remove the temp file. red-ale may use an
   atomic `rename` for the replace step; the resulting files match GnuCash's, with no window where the book name is
   missing. Add fsync of the temp file and directory. Decide whether to keep the input's compression (proposed:
   keep it, so a plain-XML book stays plain) rather than a global pref.
4. **Backups and journal file names use local time**, `YYYYMMDDHHMMSS`. Two saves in one second must not fail;
   GnuCash fails there, red-ale should wait or refuse with a clear error.
5. **Retention** uses `mtime`, truncated days, the exact regex and suffix filter in §7, defaults "days"/30. Don't
   delete the open journal.
6. **Journal**: same header and record format; fields UTC; escaping impossible without breaking GnuCash's replay,
   so red-ale's own replay must cope with raw tabs (match the 17-field layout from both ends, or refuse the record).
7. **Known Book features for v0.1** must include the two the GUI sets on almost every book (`Register sort and
   filter settings stored in .gcm file`, `Use a dedicated opening balance account identified by an 'equity-type'
   slot`). Otherwise nearly every real book would open read-only. Their data (a `.gcm` file outside the book; an
   `equity-type` account slot) is preserved by round-tripping.
8. **Fatal load errors are errors**, never an empty book (§2.3).
9. **New books** are created through `xml://` semantics; never infer SQLite from a missing file.
10. Glossary clashes to fix in `design/glossary.md`: *Backup* is a hard link, not a copy; *Journal* holds changes
    since the last *GUI* save (the library does not rotate on save), only for transactions.

---

## Open questions

1. Two saves within the same second: inferred from `link()` returning EEXIST that the second save fails with
   `ERR_FILEIO_BACKUP_ERROR`. Not run on the oracle (needs sub-second edit/save timing).
2. `gnc_iso8601_to_time64_gmt` parsing of `YYYY-MM-DD HH:MM:SS` as UTC is from the function name and
   `GncDateTime(const char*)`; not traced into the string constructor.
3. Log replay could not be exercised (GUI-only); §9 is from source alone.
4. The user-data directory holding `books/*.gcm` on Linux (`~/.local/share/gnucash` per XDG) was not traced through
   `gnc_userdata_dir`; docs-only.
5. GUI autosave interval and its interaction with backups/journals (`gnucash/gnome-utils/gnc-autosave.c`) were not
   examined; each autosave is a normal `gnc_file_save` (`gnc-autosave.c:216`), so it makes a backup and a new journal.
6. Whether a `Day Threshold` stored as `integer` (`int64`) by other tools is read correctly through the GObject
   `double` property (`qofbook.cpp:168-170`, `:259-268`) was not tested.
