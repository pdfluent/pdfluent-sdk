# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Engine-mode flag behind the public mode switch.

A fresh process starts in layout mode; the switch flips to the legacy
converter and back. Lanes read the flag through :func:`is_layout_mode`
instead of keeping their own copy, so every entry agrees on the mode.

The switch also clears the batch poison flag: any batch run poisons later
layout conversions in the same process, and selecting a mode -- even the
one already selected -- lifts it again.
"""

_LAYOUT = True
_BATCH_POISONED = False


def is_layout_mode():
    # type: () -> bool
    """Return True while the layout engine is selected."""
    return _LAYOUT


def set_layout_mode(yes):
    # type: (object) -> None
    """Select layout for truthy values and legacy for falsy ones."""
    global _LAYOUT
    global _BATCH_POISONED
    _LAYOUT = bool(yes)
    _BATCH_POISONED = False


def is_batch_poisoned():
    # type: () -> bool
    """Return True after a batch run poisoned layout conversions."""
    return _BATCH_POISONED


def note_batch_run():
    # type: () -> None
    """Record a batch run, poisoning later layout conversions."""
    global _BATCH_POISONED
    _BATCH_POISONED = True
