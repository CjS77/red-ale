#!/usr/bin/env python3
"""Make one edit to an SQLite book with GnuCash 5.17, as its GUI would, for `tests/oracle_sqlite_as_gnucash.rs`.

Usage (with PYTHONPATH=<5.17 bindings>):
    sql_edits.py <book> <edit> <targets>

`<targets>` is the JSON the test picked from the book's rows (`pair`, `account`, `price`). The book is opened with
SESSION_NORMAL_OPEN and every change is written through by its own commit, as in the GUI: an SQL book has no Save
(`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:580-666`). Prints, as JSON, every WARN, ERROR or CRIT line the
engine wrote, as `opened.py` does.

Each edit sets the same fields to the same values as the test's red-ale side.
"""
import ctypes
import json
import os
import sys
import tempfile

from gnucash import GUID, GncNumeric, GncPrice, Session, SessionOpenMode, Split, Transaction, _sw_core_utils
from gnucash import gnucash_core_c as core

from gnc_engine import engine

NOISE = ("pref_transformations.xml",)

# 2024-07-01 12:00:00 UTC, the red-ale test dispatcher's clock (`red_ale_testkit::TEST_NOW`).
TEST_NOW = 1_719_835_200

dmy_neutral = engine.gnc_dmy2time64_neutral
dmy_neutral.restype = ctypes.c_int64
dmy_neutral.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]


def guid(text):
    g = GUID()
    core.string_to_guid(text, g.instance)
    return g


def numeric(pair, factor=1):
    return GncNumeric(pair[0] * factor, pair[1])


def commodity(book, ident):
    namespace, mnemonic = ident.split("::", 1)
    return book.get_table().lookup(namespace, mnemonic)


def edit_transaction(book, t):
    """A new description, and each split's value and amount doubled, in one edit."""
    tx = guid(t["pair"]["tx"]).TransLookup(book)
    tx.BeginEdit()
    tx.SetDescription("Edited description")
    for s in t["pair"]["splits"]:
        split = guid(s["guid"]).SplitLookup(book)
        split.SetValue(numeric(s["value"], 2))
        split.SetAmount(numeric(s["amount"], 2))
    tx.CommitEdit()


def add_transaction(book, t):
    """A new transaction between the two accounts of the picked one, in its currency."""
    first, second = (guid(s["account"]).AccountLookup(book) for s in t["pair"]["splits"])
    tx = Transaction(book)
    tx.BeginEdit()
    tx.SetCurrency(commodity(book, t["pair"]["currency"]))
    tx.SetDate(20, 6, 2024)
    tx.SetDateEnteredSecs(TEST_NOW)
    tx.SetNum("102")
    tx.SetDescription("Rent")
    for account, value, memo, action, state in ((first, -750, "cheque", "Pay", "n"), (second, 750, "", "", "c")):
        split = Split(book)
        split.SetParent(tx)
        split.SetAccount(account)
        split.SetValue(GncNumeric(value, 100))
        split.SetAmount(GncNumeric(value, 100))
        split.SetMemo(memo)
        split.SetAction(action)
        split.SetReconcile(state)
    tx.CommitEdit()


def delete_transaction(book, t):
    guid(t["pair"]["tx"]).TransLookup(book).Destroy()


def edit_account(book, t):
    """A new name, notes (a slot) and a US tax code (a slot in the frame `tax-US`), in one edit."""
    account = guid(t["account"]).AccountLookup(book)
    account.BeginEdit()
    account.SetName("Renamed account")
    account.SetNotes("Account notes")
    account.SetTaxUSCode("N123")
    account.CommitEdit()


def add_price(book, t):
    """A price as the price editor adds one: the day's neutral time, source `user:price-editor`."""
    price = GncPrice(book)
    price.begin_edit()
    price.set_commodity(commodity(book, t["price"]["commodity"]))
    price.set_currency(commodity(book, t["price"]["currency"]))
    price.set_time64(dmy_neutral(15, 6, 2024))
    price.set_source_string("user:price-editor")
    price.set_typestr("last")
    price.set_value(GncNumeric(12345, 1000))
    price.commit_edit()
    book.get_price_db().add_price(price)


def set_transaction_slots(book, t):
    """Notes and a document link: two slots, in one edit."""
    tx = guid(t["pair"]["tx"]).TransLookup(book)
    tx.BeginEdit()
    tx.SetNotes("Edited notes")
    tx.SetDocLink("https://example.com/receipt.pdf")
    tx.CommitEdit()


EDITS = {
    "transaction": edit_transaction,
    "new-transaction": add_transaction,
    "delete-transaction": delete_transaction,
    "account": edit_account,
    "price": add_price,
    "transaction-slots": set_transaction_slots,
}


def run(path, edit, targets):
    log = tempfile.TemporaryFile()
    saved_stderr = os.dup(2)
    os.dup2(log.fileno(), 2)
    try:
        session = Session("sqlite3://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
        try:
            EDITS[edit](session.book, targets)
        finally:
            session.end()
    finally:
        sys.stderr.flush()
        os.dup2(saved_stderr, 2)
    log.seek(0)
    lines = log.read().decode(errors="replace").splitlines()
    return [l for l in lines if any(k in l for k in (" WARN ", " ERROR ", " CRIT ")) and not any(n in l for n in NOISE)]


def main(argv):
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    print(json.dumps(run(argv[1], argv[2], json.loads(argv[3]))))


if __name__ == "__main__":
    main(sys.argv)
