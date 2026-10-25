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
