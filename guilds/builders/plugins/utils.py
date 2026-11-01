"""Backward-compatible alias for :mod:`guilds.core.utils`.

Until B24-H5 this file was a second, hand-maintained copy of
``guilds/core/utils.py``. The two had already drifted in both directions: this
copy carried the hardened ``run_command`` (stdin closed, process tree killed on
timeout) and had lost ``flag_untrusted_content``. A duplicated security helper
is how a fix lands in one place and misses the other, so the implementation now
lives only in ``guilds/core/utils.py`` and this module re-exports it.

Keep it that way: this module must not define functions or classes of its own,
and ``tests/python/test_b24_h5_single_utils_implementation.py`` fails if it
does, or if the names it exposes stop matching the ones in core.

``from guilds.builders.plugins import utils`` and ``from guilds.core import
utils`` therefore hand back the very same objects -- which is what the guilds
importing one or the other already assumed.
"""

from guilds.core.utils import *  # noqa: F401,F403
from guilds.core.utils import __all__ as _core_all

__all__ = list(_core_all)
