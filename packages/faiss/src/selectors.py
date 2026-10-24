# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=faiss
# osprey: module=Selectors
"""ID selectors used by `IndexFlat.remove_ids` and the ID-mapped indexes.

faiss models `IDSelector` as an abstract base with `IDSelectorBatch` (an
explicit label set) and `IDSelectorRange` (a half-open label interval) as the
concrete forms; `is_member(i)` answers membership. The Osprey port spells the
same sum type as one record with a `kind` tag plus every variant's fields, and
`selected` is `is_member`. Implements [PORT-SCOPE]'s one-record spelling of a
faiss sum type (see README divergences).
"""

__all__ = ["IDSelector", "IDSelectorBatch", "IDSelectorRange", "selected"]


class IDSelector:
    """One selector: `kind` is "batch" or "range"; a batch carries `ids`, a
    range carries the half-open interval `[imin, imax)`."""
    kind: str
    ids: list[int]
    imin: int
    imax: int


def IDSelectorBatch(ids):
    """Selects exactly the labels listed in `ids`."""
    return IDSelector(kind="batch", ids=ids, imin=0, imax=0)


def IDSelectorRange(imin, imax):
    """Selects every label in the half-open interval `[imin, imax)`."""
    return IDSelector(kind="range", ids=[], imin=imin, imax=imax)


def selected(selector, id):
    """faiss's `IDSelector.is_member(id)`: whether `id` matches the selector."""
    match selector.kind:
        case "batch":
            return listContains(selector.ids, id)
        case _:
            return id >= selector.imin and id < selector.imax
