"""Generate a small synthetic book holding import map entries, with the 5.17 oracle (XML backend).

The entries are made by GnuCash's own import map functions (`5.17:libgnucash/engine/Account.cpp`,
`gnc_account_imap_add_account` and `gnc_account_imap_add_account_bayes`), as its importers make them:

- on Checking, the plain `desc` and `memo` maps (the non-Bayesian matcher) and two Bayesian token sets, each the
  tokens `TransactionGetTokens` makes of a transaction (`5.17:gnucash/import-export/import-backend.cpp:458-496`): the
  description's words as written and the posted day of the week ("Corner Grocer" on Monday 2024-06-03, "Salary ACME"
  on Friday 2024-06-28);
- on Checking, its own CSV account map entry (`csv-account-map`, kept on the account the name means);
- a Bayesian token whose account, Expenses:Old, is then deleted, so the entry is invalid.

Usage: python3 import_map_create.py <path>"""
import ctypes
import os
import sys
from gnucash import _sw_core_utils
assert _sw_core_utils.gnc_version() == '5.17', _sw_core_utils.gnc_version()
from gnucash import Session, SessionOpenMode, Account
import gnucash.gnucash_core_c as c

# The bindings cannot pass a GList of tokens, so the Bayesian call goes through ctypes on libgnc-engine.so.
LIB = os.path.join(os.path.dirname(c.__file__), "..", "..", "..", "libgnc-engine.so")
engine = ctypes.CDLL(os.path.realpath(LIB))
glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_list_append.restype = ctypes.c_void_p
glib.g_list_append.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
glib.g_list_free.argtypes = [ctypes.c_void_p]
engine.gnc_account_imap_add_account_bayes.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
engine.gnc_account_imap_add_account_bayes.restype = None


def add_bayes(account, tokens, target):
    """`gnc_account_imap_add_account_bayes` with a GList of the tokens."""
    strings = [ctypes.c_char_p(t.encode()) for t in tokens]
    glist = None
    for s in strings:
        glist = glib.g_list_append(glist, ctypes.cast(s, ctypes.c_void_p))
    engine.gnc_account_imap_add_account_bayes(int(account.instance), glist, int(target.instance))
    glib.g_list_free(glist)


path = sys.argv[1]
s = Session("xml://" + path, SessionOpenMode.SESSION_NEW_OVERWRITE)
try:
    book = s.book
    root = book.get_root_account()
    USD = book.get_table().lookup("CURRENCY", "USD")

    def acct(parent, name, typ, placeholder=False):
        a = Account(book)
        a.BeginEdit()
        a.SetName(name)
        a.SetType(typ)
        a.SetCommodity(USD)
        a.SetPlaceholder(placeholder)
        parent.append_child(a)
        a.CommitEdit()
        return a

    assets = acct(root, "Assets", c.ACCT_TYPE_ASSET, placeholder=True)
    checking = acct(assets, "Checking", c.ACCT_TYPE_BANK)
    expenses = acct(root, "Expenses", c.ACCT_TYPE_EXPENSE, placeholder=True)
    groceries = acct(expenses, "Groceries", c.ACCT_TYPE_EXPENSE)
    rent = acct(expenses, "Rent", c.ACCT_TYPE_EXPENSE)
    old = acct(expenses, "Old", c.ACCT_TYPE_EXPENSE)
    income = acct(root, "Income", c.ACCT_TYPE_INCOME, placeholder=True)
    salary = acct(income, "Salary", c.ACCT_TYPE_INCOME)

    chk = checking.instance
    c.gnc_account_imap_add_account(chk, "desc", "Corner Grocer", groceries.instance)
    c.gnc_account_imap_add_account(chk, "memo", "March rent", rent.instance)
    c.gnc_account_imap_add_account(chk, "csv-account-map", "CHK-001", chk)
    add_bayes(checking, ["Friday", "Salary", "ACME"], salary)
    add_bayes(checking, ["Monday", "Corner", "Grocer"], groceries)
    add_bayes(checking, ["Coffee"], old)

    old.BeginEdit()
    old.Destroy()
    s.save()
finally:
    s.end()
