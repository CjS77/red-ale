# This program is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License as
# published by the Free Software Foundation; either version 2 of
# the License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program; if not, contact:
#
# Free Software Foundation           Voice:  +1-617-542-5942
# 51 Franklin Street, Fifth Floor    Fax:    +1-617-542-2652
# Boston, MA  02110-1301,  USA       gnu@gnu.org
"""The book-building helpers of GnuCash's report tests, replayed through GnuCash 5.17's Python bindings.

Ported from `5.17:bindings/guile/test/test-engine-extras.scm` (licence notice above, kept from that file). Each
helper makes the same engine calls, in the same order, as the Scheme function it names, and cites its lines. Report
fixture scripts in this directory use them to rebuild the books GnuCash's report tests build in code; see
`design/reports.md` section 13.

Differences from a GnuCash test run, all for reproducible output and none visible to a report:

- GUIDs are fixed: `<kind><sequence>` in hex, where kind is `b` for the book, `a` for accounts (the root account is
  `a000...0`), `c` for transactions, `d` for splits and `e` for prices, numbered in creation order.
- A transaction's entered date and a voided transaction's `void-time` are the pinned now (`NOW`) plus one second per
  transaction made so far, instead of the wall clock (`Transaction.cpp:2517-2524`).
- The book is written uncompressed.

Run with `TZ=UTC`: price times and dates from `gnc-dmy2time64` are local midnight, and posted dates are 10:59 UTC
(`5.17:libgnucash/engine/gnc-date.cpp:1262-1265`, `:1323-1328`).
"""
import calendar
import ctypes
import gzip
import os
import pathlib
import shutil
import sys
import tempfile
import time
from fractions import Fraction

if os.environ.get("TZ") != "UTC":
    sys.exit("run the report fixture scripts with TZ=UTC")

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "crates/red-ale-testkit/oracle"))

from gnucash import _sw_core_utils  # noqa: E402

assert _sw_core_utils.gnc_version() == "5.17", "the oracle must be GnuCash 5.17"

import gnucash.gnucash_core_c as c  # noqa: E402
from gnucash import Account, GncNumeric, GncPrice, Session, SessionOpenMode, Split, Transaction  # noqa: E402

from gnc_engine import engine, gobject, ptr  # noqa: E402

FIXTURES = REPO / "fixtures/reports"

# The pinned now of design/reports.md section 13: 2024-03-15 12:00:00 UTC.
NOW = calendar.timegm((2024, 3, 15, 12, 0, 0))
DAY = 86400

ASSET = c.ACCT_TYPE_ASSET
CREDIT = c.ACCT_TYPE_CREDIT
EQUITY = c.ACCT_TYPE_EQUITY
EXPENSE = c.ACCT_TYPE_EXPENSE
INCOME = c.ACCT_TYPE_INCOME
LIABILITY = c.ACCT_TYPE_LIABILITY
STOCK = c.ACCT_TYPE_STOCK


class GncGUID(ctypes.Structure):
    _fields_ = [("data", ctypes.c_ubyte * 16)]


class GValue(ctypes.Structure):
    _fields_ = [("g_type", ctypes.c_size_t), ("data", ctypes.c_uint64 * 2)]


G_TYPE_STRING = 16 << 2
engine.gnc_dmy2time64.restype = ctypes.c_int64
engine.gnc_dmy2time64.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
engine.gnc_time64_to_iso8601_buff.restype = ctypes.c_char_p
engine.gnc_time64_to_iso8601_buff.argtypes = [ctypes.c_int64, ctypes.c_char_p]
engine.qof_instance_set_guid.argtypes = [ctypes.c_void_p, ctypes.POINTER(GncGUID)]


class FixtureBook:
    """The current book (`gnc-get-current-book`): a new XML book in a temporary directory, saved to `out` by
    `save`."""

    def __init__(self, out):
        self.out = pathlib.Path(out)
        self.work = tempfile.mkdtemp()
        self.path = os.path.join(self.work, "book.gnucash")
        self.session = Session("xml://" + self.path, SessionOpenMode.SESSION_NEW_OVERWRITE)
        self.book = self.session.book
        self.counts = {}
        self.entered = NOW
        set_guid(self.book, self.guid("b"))
        self.root = self.book.get_root_account()
        set_guid(self.root, self.guid("a"))
        self.table = self.book.get_table()
        self.pricedb = self.book.get_price_db()

    def guid(self, kind):
        n = self.counts.get(kind, 0)
        self.counts[kind] = n + 1
        return f"{kind}{n:0{32 - len(kind)}x}"

    def next_entered(self):
        self.entered += 1
        return self.entered

    def save(self):
        """Save, end the session (releasing the lock), and write the book uncompressed to `out`."""
        self.session.save()
        self.session.end()
        data = pathlib.Path(self.path).read_bytes()
        if data[:2] == b"\x1f\x8b":
            data = gzip.decompress(data)
        self.out.parent.mkdir(parents=True, exist_ok=True)
        self.out.write_bytes(data)
        shutil.rmtree(self.work)


BOOK = None


def new_book(out):
    """Start a new current book that will be saved to `out`."""
    global BOOK
    BOOK = FixtureBook(out)
    return BOOK


def set_guid(instance, hex_guid):
    guid = GncGUID()
    ctypes.memmove(guid.data, bytes.fromhex(hex_guid), 16)
    engine.qof_instance_set_guid(ptr(instance), ctypes.byref(guid))


def numeric(x):
    """A Scheme exact number as `gnc_scm_to_numeric` passes it on: the reduced fraction
    (`5.17:bindings/guile/gnc-engine-guile.cpp:1709-1727`)."""
    f = Fraction(x)
    return GncNumeric(f.numerator, f.denominator)


def dmy2time64(day, month, year):
    """`gnc-dmy2time64`: local midnight."""
    return engine.gnc_dmy2time64(day, month, year)


def localtime(t):
    """`gnc-localtime` under TZ=UTC."""
    return time.gmtime(t)


def incdate_days(t, days):
    """`incdate` with a day or week delta (`5.17:bindings/guile/date-utilities.scm:276-290`): add to the day of the
    month and normalise, which under UTC is a whole number of days."""
    return t + days * DAY


def make_date_list(start, end, days):
    """`gnc:make-date-list` for day and week steps (`date-utilities.scm:356-379`): the dates from `start` by
    `days` while before `end`, then `end`."""
    out = []
    date = start
    while date < end:
        out.append(date)
        date = incdate_days(date, days)
    return out + [end]


def start_this_month(now=NOW):
    """`gnc:get-start-this-month` (`date-utilities.scm:589-596`)."""
    tm = localtime(now)
    return calendar.timegm((tm.tm_year, tm.tm_mon, 1, 0, 0, 0))


def end_this_month(now=NOW):
    """`gnc:get-end-this-month` (`date-utilities.scm:598-606`)."""
    tm = localtime(now)
    return calendar.timegm((tm.tm_year, tm.tm_mon, calendar.monthrange(tm.tm_year, tm.tm_mon)[1], 23, 59, 59))


def mnemonic_to_commodity(sym):
    """`mnemonic->commodity` (`test-engine-extras.scm:383-387`): the default report currency is USD."""
    return BOOK.table.lookup("CURRENCY", sym)


def commodity_equiv(a, b):
    return a.get_namespace() == b.get_namespace() and a.get_mnemonic() == b.get_mnemonic()


class Env:
    """`create-test-env` (`test-engine-extras.scm:97-99`): only the counter is used by the helpers ported here."""

    def __init__(self):
        self.counter = 0


def create_test_env():
    return Env()


def env_counter_next(env):
    """`:107-108`."""
    env.counter += 1
    return env.counter


def env_string(env, prefix):
    """`:110-111`."""
    return f"{prefix}-{env_counter_next(env)}"


def env_create_transaction(env, date, credit, debit, aaa):
    """`env-create-transaction` (`:118-142`). `credit` gets `aaa`, `debit` its negation; currency USD."""
    txn = Transaction(BOOK.book)
    set_guid(txn, BOOK.guid("c"))
    split_1 = Split(BOOK.book)
    set_guid(split_1, BOOK.guid("d"))
    split_2 = Split(BOOK.book)
    set_guid(split_2, BOOK.guid("d"))
    tm = localtime(date)
    aaa = Fraction(aaa)
    txn.BeginEdit()
    txn.SetDescription(env_string(env, "ponies"))
    txn.SetCurrency(mnemonic_to_commodity("USD"))
    txn.SetDate(tm.tm_mday, tm.tm_mon, tm.tm_year)
    split_1.SetParent(txn)
    split_2.SetParent(txn)
    split_1.SetAccount(credit)
    split_2.SetAccount(debit)
    split_1.SetAmount(numeric(aaa))
    split_2.SetAmount(numeric(-aaa))
    split_1.SetValue(numeric(aaa))
    split_2.SetValue(numeric(-aaa))
    txn.SetDateEnteredSecs(BOOK.next_entered())
    txn.CommitEdit()
    return txn


def gnc_pricedb_create(currency, commodity, time64, value):
    """`gnc-pricedb-create` (`:144-157`): source "user:xfer-dialog", type "test"."""
    if commodity_equiv(currency, commodity):
        return
    price = GncPrice(BOOK.book)
    set_guid(price, BOOK.guid("e"))
    price.begin_edit()
    price.set_commodity(commodity)
    price.set_currency(currency)
    price.set_time64(time64)
    price.set_source(c.PRICE_SOURCE_XFER_DLG_VAL)
    price.set_typestr("test")
    price.set_value(numeric(value))
    price.commit_edit()
    BOOK.pricedb.add_price(price)


def env_transfer_foreign(env, dd, mm, yy, debit, credit, amount1, amount2, description=None, void_reason=None,
                         reconcile=None, num=None, notes=None, memo=None, currency=None):
    """`env-transfer-foreign` (`:170-197`)."""
    return env_create_multisplit_transaction(
        env, dd, mm, yy, [(debit, -Fraction(amount1), -Fraction(amount1)), (credit, amount1, amount2)],
        description=description, void_reason=void_reason, reconcile=reconcile, num=num, memo=memo,
        currency=currency, notes=notes)


def env_transfer(env, dd, mm, yy, debit, credit, amount, description=None, void_reason=None, reconcile=None,
                 num=None, notes=None, memo=None):
    """`env-transfer` (`:199-223`): `debit` gets -amount and `credit` +amount, despite the names."""
    return env_create_multisplit_transaction(
        env, dd, mm, yy, [(debit, -Fraction(amount), -Fraction(amount)), (credit, amount, amount)],
        description=description, void_reason=void_reason, reconcile=reconcile, num=num, memo=memo, notes=notes)


def env_create_multisplit_transaction(env, dd, mm, yy, list_of_splits, description=None, pricedb=True,
                                      void_reason=None, reconcile=None, currency=None, num=None, notes=None,
                                      memo=None):
    """`env-create-multisplit-transaction` (`:239-292`). Splits are `(account, value, amount[, action])`. The
    transaction's currency is the first split's account commodity unless given. Each split whose account commodity
    differs from that currency, with a non-zero amount, adds a price of value/amount at local midnight of the posted
    day (`:279-286`)."""
    if not list_of_splits:
        return None
    txn = Transaction(BOOK.book)
    set_guid(txn, BOOK.guid("c"))
    txn_curr = currency or list_of_splits[0][0].GetCommodity()
    txn.BeginEdit()
    txn.SetDescription(description if description is not None else env_string(env, "ponies"))
    txn.SetCurrency(txn_curr)
    txn.SetDate(dd, mm, yy)
    for split in list_of_splits:
        add_split(txn, txn_curr, split, memo, reconcile, pricedb, (dd, mm, yy))
    if num is not None:
        txn.SetNum(num)
    entered = BOOK.next_entered()
    if void_reason is not None:
        txn.Void(void_reason)
        set_void_time(txn, entered)
    if notes is not None:
        txn.SetNotes(notes)
    txn.SetDateEnteredSecs(entered)
    txn.CommitEdit()
    return txn


def add_split(txn, txn_curr, split, memo, reconcile, pricedb, dmy):
    """The body of the split loop of `env-create-multisplit-transaction` (`:259-287`)."""
    acc, val, amt = split[0], Fraction(split[1]), Fraction(split[2])
    action = split[3] if len(split) > 3 else None
    if val * amt < 0:
        raise ValueError(f"env-create-multisplit-transaction error for {dmy}: opposing signs {val} {amt}")
    newsplit = Split(BOOK.book)
    set_guid(newsplit, BOOK.guid("d"))
    newsplit.SetParent(txn)
    newsplit.SetAccount(acc)
    newsplit.SetValue(numeric(val))
    newsplit.SetAmount(numeric(amt))
    if memo is not None:
        newsplit.SetMemo(memo)
    if action is not None:
        newsplit.SetAction(action)
    if reconcile is not None:
        newsplit.SetReconcile(reconcile[0])
        newsplit.SetDateReconciledSecs(reconcile[1])
    if pricedb and amt != 0 and not commodity_equiv(txn_curr, acc.GetCommodity()):
        gnc_pricedb_create(txn_curr, acc.GetCommodity(), dmy2time64(*dmy), val / amt)


def set_void_time(txn, t):
    """Replace the `void-time` slot `xaccTransVoid` stamps with the wall clock (`Transaction.cpp:2517-2524`)."""
    buf = ctypes.create_string_buffer(64)
    engine.gnc_time64_to_iso8601_buff(t, buf)
    value = GValue()
    gobject.g_value_init(ctypes.byref(value), ctypes.c_size_t(G_TYPE_STRING))
    gobject.g_value_set_string(ctypes.byref(value), buf.value)
    engine.qof_instance_set_kvp(ptr(txn), ctypes.byref(value), ctypes.c_uint(1), b"void-time")
    gobject.g_value_unset(ctypes.byref(value))


def env_create_root_account(env, type_, commodity):
    """`:294-295`."""
    return env_create_account(env, type_, commodity, BOOK.root)


def env_create_account(env, type_, commodity, parent):
    """`env-create-account` (`:297-305`): named "account-N" from the counter."""
    acct = Account(BOOK.book)
    set_guid(acct, BOOK.guid("a"))
    acct.BeginEdit()
    parent.BeginEdit()
    acct.SetCommodity(commodity)
    acct.SetName(env_string(env, "account"))
    acct.SetType(type_)
    parent.append_child(acct)
    acct.CommitEdit()
    parent.CommitEdit()
    return acct


def env_create_daily_transactions(env, start_date, end_date, to_account, from_account):
    """`env-create-daily-transactions` (`:308-318`): for each day from `start_date`, latest first and leaving out
    the list's last date (`end_date` itself), move the day of the month from `from_account` to `to_account`."""
    dates = make_date_list(start_date, end_date, 1)
    for date in list(reversed(dates))[1:]:
        env_create_transaction(env, date, to_account, from_account, localtime(date).tm_mday)


def env_create_account_structure(env, account_structure):
    """`env-create-account-structure` (`:320-353`). A structure is `[name, {options}?, child...]`, options being
    `type` and `commodity`, inherited by children; the top inherits commodity USD. Returns `[account, child...]`."""
    def create_substructure(parent, options, structure):
        name, rest = structure[0], structure[1:]
        own = rest[0] if rest and isinstance(rest[0], dict) else {}
        children = rest[1:] if own else rest
        merged = {**options, **own}
        new_account = env_create_account(env, merged["type"], merged["commodity"], parent)
        new_account.BeginEdit()
        new_account.SetName(name)
        new_account.CommitEdit()
        return [new_account] + [create_substructure(new_account, merged, child) for child in children]

    return create_substructure(BOOK.root, {"commodity": mnemonic_to_commodity("USD"), "type": None},
                               account_structure)


def list_flatten(tree):
    """`gnc:list-flatten` (`5.17:bindings/guile/utilities.scm:143-148`), in order."""
    return [x for item in tree for x in (list_flatten(item) if isinstance(item, list) else [item])]


def env_create_account_structure_alist(env, account_structure):
    """`:355-358`, as a dict keeping the first account of each name, as `assoc` finds it."""
    out = {}
    for acct in list_flatten(env_create_account_structure(env, account_structure)):
        out.setdefault(acct.GetName(), acct)
    return out


def env_expense_account_structure(env):
    """`env-expense-account-structure` (`:360-370`)."""
    return env_create_account_structure(
        env, ["Expenses", {"type": EXPENSE}, ["Groceries"], ["Rent"], ["Auto", ["Tax"], ["Parking"], ["Petrol"]]])


# Checking a saved book with GnuCash.

def check_book(path, balances):
    """Open `path` read-only in GnuCash and compare account balances. `balances` lists `(account, expected, date)`:
    the account by GUID or by full name (`Root.Asset.Bank`; the first of that name), the expected balance as a
    number in the account's commodity, and a time for the balance as of that time, or None for the balance over all
    splits. Exits with the differences."""
    work = tempfile.mkdtemp()
    copy = os.path.join(work, "book.gnucash")
    shutil.copy(path, copy)
    session = Session("xml://" + copy, SessionOpenMode.SESSION_READ_ONLY)
    try:
        root = session.book.get_root_account()
        accounts = {}
        for acct in root.get_descendants():
            accounts[acct.GetGUID().to_string()] = acct
            accounts.setdefault(acct.get_full_name(), acct)
        failures = [f for f in (check_balance(accounts, *b) for b in balances) if f]
    finally:
        session.end()
        shutil.rmtree(work)
    if failures:
        sys.exit(f"{path}:\n" + "\n".join(failures))


def check_balance(accounts, name, expected, date):
    acct = accounts.get(name)
    if acct is None:
        return f"no account {name}"
    n = acct.GetBalance() if date is None else acct.GetBalanceAsOfDate(date)
    got = Fraction(n.num(), n.denom())
    return None if got == Fraction(expected) else f"{name}: balance {got} != expected {Fraction(expected)}"


def out_dir():
    """The output directory: the first argument, or `fixtures/reports`."""
    return pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else FIXTURES
