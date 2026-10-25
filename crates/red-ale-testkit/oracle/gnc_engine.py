"""Direct calls into GnuCash 5.17's engine library for what the Python bindings do not expose (the scrubs, getters with
out-parameters). Objects from the bindings are passed by their SWIG pointer.

Import after the `gnucash` package, so the library is already loaded with the bindings' own search path.
"""
import ctypes
import pathlib

import gnucash

# site-packages/gnucash/__init__.py -> lib/
LIB_DIR = pathlib.Path(gnucash.__file__).resolve().parents[3]
engine = ctypes.CDLL(str(LIB_DIR / "libgnc-engine.so"))


class GncNumeric(ctypes.Structure):
    _fields_ = [("num", ctypes.c_int64), ("denom", ctypes.c_int64)]


PERCENTAGE = ctypes.CFUNCTYPE(None, ctypes.c_char_p, ctypes.c_double)
NO_PROGRESS = PERCENTAGE(lambda message, percent: None)


def ptr(obj):
    """The C pointer behind a bindings object."""
    return ctypes.c_void_p(int(getattr(obj, "instance", obj)))


def frac(n):
    """A gnc_numeric as a reduced "num/denom", as dump.py prints amounts."""
    import math
    g = math.gcd(n.num, n.denom) or 1
    return f"{n.num // g}/{n.denom // g}"


def numeric_fn(name, *argtypes):
    f = getattr(engine, name)
    f.restype = GncNumeric
    f.argtypes = list(argtypes)
    return f


gobject = ctypes.CDLL("libgobject-2.0.so.0")


def set_current_session(session):
    """Make `session` GnuCash's current session, which `gnc_get_num_action` and `gnc_set_num_action` read the book
    option from (`libgnucash/engine/engine-helpers.c:64`, `:119`). The bindings never set it."""
    engine.gnc_set_current_session(ptr(session))


def clear_current_session():
    """Forget the current session without destroying it; the bindings end it themselves."""
    engine.gnc_set_current_session(None)


def set_num_source(book, on):
    """Set "Use Split Action Field for Number" as the book options dialog does: the option through the book's
    `split-action-num-field` property (which drops the engine's cached value), then the `Number Field Source`
    feature when turned on (`gnucash/gnome-utils/gnc-main-window.cpp:4709-4721`)."""
    engine.qof_book_begin_edit(ptr(book))
    gobject.g_object_set(ptr(book), b"split-action-num-field", b"t" if on else b"f", None)
    engine.qof_book_commit_edit(ptr(book))
    if on:
        engine.gnc_features_set_used(ptr(book), b"Number Field Source")


def set_autoreadonly_days(book, days):
    """Set "Day Threshold for Read-Only Transactions (red line)" through the book's `autoreadonly-days` property."""
    engine.qof_book_begin_edit(ptr(book))
    gobject.g_object_set(ptr(book), b"autoreadonly-days", ctypes.c_double(days), None)
    engine.qof_book_commit_edit(ptr(book))


def get_num_action(trans, split):
    """`gnc_get_num_action(trans, split)`: the Number GnuCash shows for a split. Needs the current session."""
    f = engine.gnc_get_num_action
    f.restype = ctypes.c_char_p
    f.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    out = f(ptr(trans), ptr(split))
    return out.decode() if out is not None else None


def is_readonly_by_posted_date(trans):
    """`xaccTransIsReadonlyByPostedDate`."""
    f = engine.xaccTransIsReadonlyByPostedDate
    f.restype = ctypes.c_int
    f.argtypes = [ctypes.c_void_p]
    return bool(f(ptr(trans)))


def unknown_features(book):
    """`gnc_features_test_unknown`: the message GnuCash's GUI shows for features it does not know, or None."""
    f = engine.gnc_features_test_unknown
    f.restype = ctypes.c_char_p
    f.argtypes = [ctypes.c_void_p]
    out = f(ptr(book))
    return out.decode() if out is not None else None
