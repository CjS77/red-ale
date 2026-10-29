<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/help.search
related: method/help.topics, method/help.get
---
Ranks the Manual's Pages for a question in your own words, best first, each with a snippet. Search is offline and
deterministic, and knows GnuCash's words: "quantity" finds *amount*, "bill" finds *document*, a menu path such as
"Stock Split" finds the Page for that menu item. Give `kind` to search one kind of Page and `limit` to cap the hits.

## Examples

A method's own Page ranks first for the words of its name, ahead of an error that shares them:

```example book=none
{"method": "help.search", "params": {"query": "open a book", "limit": 2}}
{"result": {"hits": [{"id": "method/book.open", "kind": "method", "title": "book.open", "summary": "…", "snippet": "…"}, {"id": "error/book-already-open", "kind": "error", "title": "book-already-open", "summary": "…", "snippet": "…"}]}}
```

```example book=none
{"method": "help.search", "params": {"query": "transaction does not balance", "kind": "error", "limit": 2}}
{"result": {"hits": [{"id": "error/unbalanced", "kind": "error", "title": "unbalanced", "summary": "…", "snippet": "…"}, "…"]}}
```
