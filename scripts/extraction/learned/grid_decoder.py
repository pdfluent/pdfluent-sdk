#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Pairwise decisions about words into a grid with spans (#600).

The model of #600 answers four questions about a PAIR of words -- same cell,
same row, same column, and whether the table continues -- and none of those is
a table. This is the deterministic half that turns them into one: cells, their
row and column numbers, and the spans that make a grid rather than a list.

WHY IT IS DETERMINISTIC AND SEPARATE. A decoder that guessed would hide the
model's mistakes behind its own, and then neither could be measured. Every
choice here is arithmetic on the relations it is handed, so a grid that comes
out wrong is either a relation that came in wrong or a rule written down below
-- and `the truth decodes to itself` holds the second case shut.

THE CEILING, MEASURED THREE WAYS
================================
On 394 annotated tables, handed the TRUE relations -- the perfect input a model
can only approach:

    four relations                                     370   93.9 %
    plus a boundary between two words (owner, 23-09)   201   51.0 %
    plus a per-cell span count  (owner, 23-09)         393   99.7 %

No table in any of the three came back with the wrong number of cells. The
middle row is measured and kept because it was asked for; why it fails is under
A FIFTH QUESTION THAT CANNOT WORK below, and the short of it is that the fact
in question is not about a pair of words at all.

AND AGAIN ON 397 TABLES NONE OF THESE RULES EVER SAW, the next of the stream
after those 394, because a rule shaped on a set and then scored on it is
scored on its own reflection:

    four relations                                     374   94.2 %
    plus a boundary between two words                  165   41.6 %
    plus a per-cell span count                         395   99.5 %

The count holds and the boundary falls further. Nothing here was tuned between
the two runs.

WHAT THE FOUR RELATIONS CANNOT SAY
==================================
Handed the TRUE relations of 394 annotated tables -- the perfect input a model
can only approach -- this returns the annotated grid exactly 370 times. No
table came back with the wrong number of cells. All 24 that differ fail the
same way, and the way is worth knowing before anyone trains anything:

    every one of the 24 decoded FEWER rows than the truth, never more;
    every one of the 24 has row-spanning cells, median 5 against 0 elsewhere.

`PMC5664814_table_0` is the shape. Its annotation has six rows, and every cell
of the first band carries `row_nums` `[0, 1]` -- the band is two rows tall and
NOTHING divides it, because no cell occupies row 0 or row 1 alone. Ask the four
relations about those words and they answer "same row" for every pair, which is
the same answer they would give for a band that really is one row. The division
is not in the relations at all.

So this is not a rule to fix here: it is the vocabulary of #600's four
questions, and 93.9 % is the ceiling any model of them can reach on this
sample. Lifting it needs a FIFTH question -- how many rows a cell spans, or
whether a row boundary passes between two words -- which is a change to the
ticket's model and the owner's to make. Written down rather than worked around,
because a decoder that guessed the missing division from the height of the band
would be inventing structure the page never stated, and it would do it silently.

A FIFTH QUESTION THAT CANNOT WORK, AND THE ONE THAT CAN
=======================================================
The owner asked on 23-09-2026 for a fifth relation, "a row boundary runs
between these two words", labelled from `row_nums`, and a column analogue.
Built and measured on the same 394 tables with true relations: 51.0 %, against
93.9 % without it. Three decoder rules were tried -- vetoing `same_row` across
a boundary, partitioning into bands and extending by every same-row partner,
and extending only from partners whose own words occupy ONE band -- and all
three land near 51 %.

THE LABEL IS NOT THE PROBLEM. Measured separately: the bands the boundary
draws ARE the annotated rows -- the band count never exceeds the row count on
any of the 394 tables, 0.2 % of placed words fall outside every row box, and
the reading order interleaves rows on no table at all. And the earlier guess
that the lost rows hold no words was wrong: all 24 of them do.

THE PROBLEM IS THAT THE AMBIGUITY IS SYMMETRIC. `PMC5144649_table_1` shows it
in one line. A cell spanning rows 1 and 2 has all its words in band 1, and a
plain cell sits in row 2. The relations say they share a row, and the bands say
one is in band 1 and the other in band 2 -- which is exactly what they would
say if the first cell were in row 1 alone. Commit either way and the other cell
is wrong: the decoder gives the row-2 cell `[1, 2]`, the truth says `[2]`.
Nothing pairwise separates the two readings, because the fact in question is
not about a pair at all. It is about ONE cell: how many rows it spans.

So the question that can lift the ceiling is a per-cell count -- "this cell
spans N rows", and the column analogue -- and that is not a pairwise relation,
which is why it sits outside the vocabulary #600 was written with. The
annotation carries it already in `row_nums`, so it costs nothing to label.

THE ONE THING THAT IS NOT A PARTITION
=====================================
"Same cell" is an equivalence: two words are in one cell or they are not, and
the classes are the cells. "Same row" is NOT. A cell spanning rows 0 and 1
shares a row with a cell in row 0 and with a cell in row 1, while those two
share no row with each other -- so grouping by connected components would
swallow the whole table into one row, and a clique cover would be a guess.

So rows are built by a SWEEP, not by components: cells are taken in reading
order and a new row opens when a cell shares a row with nothing already in the
open one. A cell that shares a row with several rows belongs to all of them,
which is precisely a row span. Columns are the same rule turned ninety
degrees. Both fall out of the relations alone, and neither needs a threshold.
"""
from __future__ import annotations

Box = list[float]


def cells_of(words: list[dict], same_cell) -> list[dict]:
    """Words grouped into cells, each with its box and its text.

    `same_cell(i, j)` answers for a pair. The grouping is by connected
    component, which IS right here: a word in a cell with another word is in
    that cell, and the relation cannot disagree with itself the way `same_row`
    can.
    """
    parent = list(range(len(words)))

    def root(i: int) -> int:
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for i in range(len(words)):
        for j in range(i + 1, len(words)):
            if same_cell(i, j):
                a, b = root(i), root(j)
                if a != b:
                    parent[a] = b

    groups: dict[int, list[int]] = {}
    for i in range(len(words)):
        groups.setdefault(root(i), []).append(i)

    out = []
    for members in groups.values():
        boxes = [words[i]["bbox"] for i in members]
        # Reading order inside the cell: down the lines, left to right along
        # them. A cell's text is what a reader would say it holds, so the words
        # are not left in whatever order the page drew them.
        ordered = sorted(members,
                         key=lambda i: (-words[i]["bbox"][3], words[i]["bbox"][0]))
        out.append({
            "words": ordered,
            "bbox": [min(b[0] for b in boxes), min(b[1] for b in boxes),
                     max(b[2] for b in boxes), max(b[3] for b in boxes)],
            "text": " ".join(words[i]["text"] for i in ordered),
        })
    return out


def _sweep(cells: list[dict], order, shares) -> list[list[int]]:
    """Index sets along one axis, in two passes, for the reason below.

    FIRST the lines are found, and a cell joins one only if it shares that axis
    with EVERY cell already in it. Sharing with merely one would let a spanning
    cell act as a bridge -- `Total` spanning rows 0 and 1 shares a row with the
    cell in row 0 and with the cell in row 1, so a single `any` pulls the whole
    table into one row. That was the first spelling of this function and its
    test caught it.

    THEN every cell is assigned to every line it shares the axis with
    throughout, which is what puts a spanning cell in each of the lines it
    spans. The two passes are not the same question: the first asks what the
    lines ARE, the second who is ON them.

    A line wholly contained in another is dropped. It can only arise from an
    ordering in which a spanning cell opened a line of its own, and it names no
    row the containing line does not.
    """
    lines: list[list[int]] = []
    for index in order:
        for line in lines:
            if all(shares(index, other) for other in line):
                line.append(index)
                break
        else:
            lines.append([index])

    filled = [[index for index in order
               if all(shares(index, other) for other in line)]
              for line in lines]

    kept: list[list[int]] = []
    for line in filled:
        here = set(line)
        if any(here < set(other) for other in filled):
            continue
        if any(here == set(other) for other in kept):
            continue
        kept.append(line)
    return kept


def _bands(words: list[dict], order: list[int], boundary) -> list[int]:
    """Which band each word is in, by walking the page and cutting at a boundary.

    A sweep and not a grouping, for the reason `_sweep` gives one level up:
    "no boundary between us" is not transitive either, and a run of words with
    no boundary anywhere between neighbours is exactly one band however many
    words it holds.
    """
    band = [0] * len(words)
    current = 0
    for position, index in enumerate(order):
        if position and boundary(order[position - 1], index):
            current += 1
        band[index] = current
    return band


def _with_boundaries(words, cells, same_row, same_column,
                     row_boundary, column_boundary) -> list[dict]:
    """The grid when the fifth and sixth questions are answered too.

    MEASURED, THIS IS WORSE THAN NOT ASKING: 51.0 % against the four-question
    decoder's 93.9 % on the same 394 tables with true relations. It is kept
    because it is the shape the owner asked for on 23-09-2026 and because the
    reason it fails is worth having written down -- but nothing should use it
    until the reason is answered. The finding, in full, is under
    A FIFTH QUESTION THAT CANNOT WORK in the module docstring.

    THE BOUNDARY IS A PARTITION, NOT A VETO, and the difference is the whole of
    this function. The first spelling refined the third question --
    `same_row AND NOT boundary` -- and that was wrong in a way worth recording:
    a cell spanning rows 0 and 1 whose words sit in band 0 genuinely shares row
    1 with the cell below it, and vetoing every cross-band pair threw that
    relation away. Measured on 394 tables with true relations it scored 53.8 %
    against the four-question decoder's 93.9 %.

    So the boundaries say where the LINES are and the third and fourth
    questions say who reaches across them:

      * the bands are the rows, one per run of words with no boundary between
        neighbours -- which recovers a band that is two rows tall, because the
        words of its second row are on the far side of a line;
      * a cell holds every band its OWN words fall in, which is a span it
        straddles;
      * and every band of a cell it shares a row with, which is a span it does
        not -- a header sitting high in a band it spans still shares a row with
        the cell beneath.
    """
    def assign(index_of_word, relation, boundary, order):
        if boundary is None:
            return None
        band = _bands(words, order, boundary)
        mine = [sorted({band[w] for w in cell["words"]}) for cell in cells]
        # ONLY A CELL OF ONE BAND SAYS ANYTHING CERTAIN. "We share a row" means
        # the two row sets meet, not WHERE they meet -- so a cell spread over
        # two bands cannot say which of them the other one is in. A cell whose
        # own words occupy exactly one band can: that band is a row it is in,
        # so a cell sharing a row with it is in that band too.
        #
        # The spelling before this one took the bands of EVERY cell it shared a
        # row with, and a plain cell then inherited its spanning neighbour's
        # extra band. 50.8 % against the four-question decoder's 93.9 %.
        certain = [line[0] if len(line) == 1 else None for line in mine]
        out = []
        for a, cell in enumerate(cells):
            reached = set(mine[a])
            for b, other in enumerate(cells):
                if a == b or certain[b] is None:
                    continue
                if any(relation(i, j) for i in cell["words"] for j in other["words"]):
                    reached.add(certain[b])
            out.append(sorted(reached))
        return out

    down = sorted(range(len(words)),
                  key=lambda i: (-words[i]["bbox"][3], words[i]["bbox"][0]))
    across = sorted(range(len(words)),
                    key=lambda i: (words[i]["bbox"][0], -words[i]["bbox"][3]))
    rows = assign(down, same_row, row_boundary, down)
    columns = assign(across, same_column, column_boundary, across)

    for index, cell in enumerate(cells):
        cell["row_nums"] = rows[index] if rows else []
        cell["column_nums"] = columns[index] if columns else []
    return sorted(cells, key=lambda c: (c["row_nums"][:1], c["column_nums"][:1]))


def _numbered(cells, lines, span_of) -> list[list[int]]:
    """Row (or column) numbers when each cell also says how far it reaches.

    The sweep says which cells share a line and in what order; the count says
    how many rows a cell occupies. Neither alone recovers a band that is two
    rows tall with nothing dividing it -- the sweep cannot see the division and
    the count cannot say where it falls -- and together they do:

      * a line's HEIGHT is the largest count among the cells that sit on that
        line alone, because a cell reaching further is the only evidence that
        the line holds more than one row;
      * a line STARTS where the lines above it end, so a tall line pushes every
        line below it down by the rows it turned out to hold;
      * and a cell occupies its count of rows from the first line it sits on.

    A cell that sits on several lines does not set a height: it is reaching
    across lines that other cells already measured, and letting it set one
    would count its own reach twice.
    """
    on = [set() for _ in cells]
    for number, line in enumerate(lines):
        for index in line:
            on[index].add(number)

    height = [1] * len(lines)
    for index, where in enumerate(on):
        if len(where) == 1:
            only = next(iter(where))
            height[only] = max(height[only], span_of(index))

    start, running = [], 0
    for tall in height:
        start.append(running)
        running += tall

    out = []
    for index, where in enumerate(on):
        first = start[min(where)] if where else 0
        reach = max(1, span_of(index))
        out.append(list(range(first, first + reach)))
    return out


def decode(words: list[dict], same_cell, same_row, same_column,
           row_boundary=None, column_boundary=None,
           row_span=None, column_span=None) -> list[dict]:
    """The grid: every cell with its `row_nums` and `column_nums`.

    The shape the annotation uses, so that a decoded table and an annotated one
    can be compared without a translation in between -- and so that
    `pubtables_cells.html_of` renders either.

    `row_boundary(i, j)` and `column_boundary(i, j)` are the fifth and sixth
    questions (owner, 23-09-2026): does a row or column boundary run BETWEEN
    these two words. They refine the third and fourth rather than replacing
    them -- two words are on one row when they share a row AND no boundary
    divides them -- which is what lets a band whose every cell spans it come
    apart into the rows it really holds. Omitted, the decoder is the four-question
    one and its ceiling is the 93.9 % above.
    """
    cells = cells_of(words, same_cell)
    if row_boundary is not None or column_boundary is not None:
        return _with_boundaries(words, cells, same_row, same_column,
                                row_boundary, column_boundary)

    def cell_shares(relation):
        def shares(a: int, b: int) -> bool:
            return any(relation(i, j)
                       for i in cells[a]["words"] for j in cells[b]["words"])
        return shares

    # Rows top-first and columns left-first, so the numbers a reader expects
    # are the numbers that come out.
    down = sorted(range(len(cells)), key=lambda c: (-cells[c]["bbox"][3],
                                                    cells[c]["bbox"][0]))
    across = sorted(range(len(cells)), key=lambda c: (cells[c]["bbox"][0],
                                                      -cells[c]["bbox"][3]))
    rows = _sweep(cells, down, cell_shares(same_row))
    columns = _sweep(cells, across, cell_shares(same_column))

    if row_span is not None:
        numbered = _numbered(cells, rows, lambda c: row_span(cells[c]["words"][0]))
        for index, line in enumerate(numbered):
            cells[index]["row_nums"] = line
    else:
        for number, line in enumerate(rows):
            for index in line:
                cells[index].setdefault("row_nums", []).append(number)
    if column_span is not None:
        numbered = _numbered(cells, columns,
                             lambda c: column_span(cells[c]["words"][0]))
        for index, line in enumerate(numbered):
            cells[index]["column_nums"] = line
    else:
        for number, line in enumerate(columns):
            for index in line:
                cells[index].setdefault("column_nums", []).append(number)
    for cell in cells:
        cell.setdefault("row_nums", [])
        cell.setdefault("column_nums", [])
    return sorted(cells, key=lambda c: (c["row_nums"][:1], c["column_nums"][:1]))


def _components(count: int, edges) -> list[int]:
    """Connected components as a label per item."""
    parent = list(range(count))

    def root(i: int) -> int:
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for a, b in edges:
        ra, rb = root(a), root(b)
        if ra != rb:
            parent[ra] = rb
    seen: dict[int, int] = {}
    out = []
    for i in range(count):
        r = root(i)
        if r not in seen:
            seen[r] = len(seen)
        out.append(seen[r])
    return out


def _log_odds(p: float) -> float:
    """A probability as evidence that can be averaged. 0.99 is worth four times
    as much as 0.73, which is what a mean of raw probabilities cannot say."""
    p = min(max(p, 1e-6), 1.0 - 1e-6)
    import math
    return math.log(p / (1.0 - p))


def cells_by_average_link(words: list[dict], scored: dict, floor: float = 0.5):
    """Cells as groups that merge on the MEAN evidence between them.

    Union-find merges on a single edge: one confident wrong `same_cell` between
    two otherwise separate groups joins them for good, and nothing later can
    undo it. Average link asks a different question -- is the evidence BETWEEN
    these two groups positive on average -- so one wrong edge against several
    right ones loses (the owner's design, 23-09-2026).

    MEASURED, IT LOSES, and the reason was already on the table before it was
    built. On 374 test tables at 5,000 training tables:

        floor 0.90   union-find 0.6843   average link 0.6692
        floor 0.95   union-find 0.6821   average link 0.6570
        floor 0.98   union-find 0.6808   average link 0.6270

    It is built to survive a confident mistake, and the reliability curve had
    already said there are almost none: an edge at 0.99 or above is really the
    same cell 9,995 times in 10,000. So there is nothing for the averaging to
    protect against, and it pays the full cost -- a true merge carried by one
    strong edge among several weak ones is now refused, because the MEAN is
    what has to clear the floor. That is why it falls further the higher the
    floor goes.

    Kept, unused, with the measurement on it. It is the right shape for a model
    that IS overconfident, and this one is not.

    The score between two groups is the mean log-odds over the neighbour pairs
    that were actually scored. Missing pairs say nothing and are left out
    rather than counted as evidence against: only neighbours were ever asked,
    so a missing pair is a question nobody put, not an answer of no.
    """
    between: dict[tuple[int, int], list[float]] = {}
    for (i, j), probability in scored.items():
        weight = _log_odds(float(probability[3]))
        between[(min(i, j), max(i, j))] = [weight]

    group = {i: {i} for i in range(len(words))}
    scores = {pair: sum(v) / len(v) for pair, v in between.items()}
    bar = _log_odds(floor)

    while True:
        best, best_score = None, bar
        for pair, value in scores.items():
            if value > best_score:
                best, best_score = pair, value
        if best is None:
            break
        a, b = best
        group[a] |= group[b]
        for member in group[b]:
            group[member] = group[a]
        # RECOMPUTE, which is what makes it average link rather than single
        # link: the merged group's score against everyone else is the mean of
        # both halves' evidence, so a group that has earned its members resists
        # a stray edge better the larger it gets.
        fresh: dict[tuple[int, int], float] = {}
        seen: dict[int, int] = {}
        for index in range(len(words)):
            seen[index] = min(group[index])
        pools: dict[tuple[int, int], list[float]] = {}
        for (i, j), values in between.items():
            left, right = seen[i], seen[j]
            if left == right:
                continue
            pools.setdefault((min(left, right), max(left, right)), []).extend(values)
        for pair, values in pools.items():
            fresh[pair] = sum(values) / len(values)
        scores = fresh
        if not scores:
            break

    out: dict[int, list[int]] = {}
    for index in range(len(words)):
        out.setdefault(min(group[index]), []).append(index)
    label = {}
    for number, (_root, members) in enumerate(sorted(out.items())):
        for member in members:
            label[member] = number
    return [label[i] for i in range(len(words))]


def from_edges(words: list[dict], scored: dict, row_span=None, column_span=None,
               floor: float = 0.5, average_link: bool = False) -> list[dict]:
    """A grid from the neighbour pairs a model scored, and nothing else.

    `scored` maps a pair `(i, j)` to four probabilities in the order of
    `pair_features.CLASSES`. Only NEIGHBOURS are scored -- asking about every
    pair on a table is quadratic and mostly trivial -- so the relations the
    decoder needs for distant pairs are not answered anywhere, and that is the
    point of this function.

    IT CLOSES THE RELATIONS TRANSITIVELY BEFORE DECODING, which is the bug this
    fixes. Handed the scored pairs directly, `decode` asked `same_row` of two
    cells at opposite ends of a row, found no answer, and read the silence as
    "no" -- so every table came apart and the grid rate was zero for reasons
    that had nothing to do with the model. Here the neighbour edges are turned
    into components first, and `decode` is then handed relations that are true
    of a PAIR because they are true of the chain between them.

    A CELL THAT SPANS ROWS DOES NOT BRIDGE THEM. A word in a cell reaching over
    two rows is joined to words in both, so a component would swallow the pair
    -- the same trap as `_sweep`'s first spelling, one level down. Such words
    are left out of the row graph and their cells take their rows from the span
    count instead.
    """
    if average_link:
        cell_of = cells_by_average_link(words, scored, floor)
    else:
        cell_edges = [(i, j) for (i, j), p in scored.items() if p[3] > floor]
        cell_of = _components(len(words), cell_edges)

    def bridging(index: int, span) -> bool:
        return span is not None and span(index) > 1

    row_edges = [(i, j) for (i, j), p in scored.items()
                 if p[3] + p[2] > floor
                 and not bridging(i, row_span) and not bridging(j, row_span)]
    column_edges = [(i, j) for (i, j), p in scored.items()
                    if p[3] + p[1] > floor
                    and not bridging(i, column_span) and not bridging(j, column_span)]
    row_of = _components(len(words), row_edges)
    column_of = _components(len(words), column_edges)

    return decode(
        words,
        lambda i, j: cell_of[i] == cell_of[j],
        lambda i, j: cell_of[i] == cell_of[j] or row_of[i] == row_of[j],
        lambda i, j: cell_of[i] == cell_of[j] or column_of[i] == column_of[j],
        row_span=row_span, column_span=column_span,
    )


def fill_empty(cells: list[dict]) -> list[dict]:
    """The cells, plus an empty one at every grid position nothing covers.

    A blank cell holds no words, so nothing built on words can SEE it -- but
    the grid is a rectangle, and once the rows and the columns are known from
    the cells that do hold words, a position no cell and no span covers can
    only be an empty cell. It is inferred rather than read (the owner's
    hypothesis, 23-09-2026).

    WHERE IT CANNOT REACH, and the reason this is not the end of the question:
    an empty cell that should have been part of a SPAN is emitted here as a
    cell of its own, and a row or column made entirely of empty cells is not in
    the rectangle at all, because no word ever put it there. Those two want the
    ruling lines.
    """
    covered = {(row, column) for cell in cells
               for row in cell["row_nums"] for column in cell["column_nums"]}
    if not covered:
        return cells
    rows = max(row for row, _c in covered) + 1
    columns = max(column for _r, column in covered) + 1
    out = list(cells)
    for row in range(rows):
        for column in range(columns):
            if (row, column) not in covered:
                out.append({"row_nums": [row], "column_nums": [column],
                            "text": "", "words": [], "bbox": [0.0, 0.0, 0.0, 0.0]})
    return sorted(out, key=lambda c: (c["row_nums"][:1], c["column_nums"][:1]))


def self_test() -> int:
    failures: list[str] = []

    def check(name: str, got, want, why: str) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r} -- {why}")

    def grid_from(spec: list[tuple[str, list[int], list[int], Box]]):
        """A table written out as cells, turned into words and true relations."""
        words, owner = [], []
        for index, (text, _rows, _cols, box) in enumerate(spec):
            for part, piece in enumerate(text.split()):
                width = (box[2] - box[0]) / max(1, len(text.split()))
                words.append({"text": piece,
                              "bbox": [box[0] + part * width, box[1],
                                       box[0] + (part + 1) * width, box[3]]})
                owner.append(index)
        rows = [set(r) for _t, r, _c, _b in spec]
        cols = [set(c) for _t, _r, c, _b in spec]
        return (words,
                lambda i, j: owner[i] == owner[j],
                lambda i, j: bool(rows[owner[i]] & rows[owner[j]]),
                lambda i, j: bool(cols[owner[i]] & cols[owner[j]]))

    # A plain two by two, y UP so the header is the larger y.
    plain = [
        ("Region", [0], [0], [10.0, 90.0, 60.0, 100.0]),
        ("Revenue", [0], [1], [70.0, 90.0, 130.0, 100.0]),
        ("North", [1], [0], [10.0, 70.0, 60.0, 80.0]),
        ("12,345", [1], [1], [70.0, 70.0, 130.0, 80.0]),
    ]
    out = decode(*grid_from(plain))
    check("a plain grid keeps its cells", len(out), 4, "four cells in, four out")
    check("and its rows", [c["row_nums"] for c in out],
          [[0], [0], [1], [1]], "the header is the top row because y is up here")
    check("and its columns", [c["column_nums"] for c in out],
          [[0], [1], [0], [1]], "")
    check("and the words of a cell are one text",
          sorted(c["text"] for c in out),
          ["12,345", "North", "Region", "Revenue"], "")

    # THE CASE COMPONENTS CANNOT DO. A cell spanning both rows shares a row
    # with the cell in row 0 AND with the one in row 1, which share none with
    # each other.
    spanning = [
        ("Total", [0, 1], [0], [10.0, 70.0, 60.0, 100.0]),
        ("First", [0], [1], [70.0, 90.0, 130.0, 100.0]),
        ("Second", [1], [1], [70.0, 70.0, 130.0, 80.0]),
    ]
    out = decode(*grid_from(spanning))
    by_text = {c["text"]: c for c in out}
    check("a spanning cell is in both rows", by_text["Total"]["row_nums"], [0, 1],
          "components would have made one row of the whole table, which is "
          "why rows are swept and not grouped")
    check("and the cells it spans are not", by_text["First"]["row_nums"], [0], "")
    check("nor the other", by_text["Second"]["row_nums"], [1], "")
    check("and the span is one column wide",
          by_text["Total"]["column_nums"], [0], "")

    # A cell of several words, drawn as a sentence.
    prose = [
        ("Method", [0], [0], [10.0, 90.0, 60.0, 100.0]),
        ("Phenol and SDS in turn", [0], [1], [70.0, 90.0, 200.0, 100.0]),
    ]
    out = decode(*grid_from(prose))
    check("a cell of a sentence is one cell", len(out), 2,
          "the words of a prose cell are same-cell with one another, and a "
          "table whose cells hold sentences is the class #600 nearly lost")
    check("and holds all of it",
          sorted(c["text"] for c in out)[1], "Phenol and SDS in turn", "")

    # THE BAND THAT IS TWO ROWS TALL WITH NOTHING DIVIDING IT, which is the
    # whole reason the fifth question exists. Both header cells span rows 0 and
    # 1; no cell occupies either alone, so the four relations answer "same row"
    # for every pair of their words and cannot tell the band from a single row.
    banded = [
        ("Left", [0, 1], [0], [10.0, 80.0, 60.0, 100.0]),
        ("Right", [0, 1], [1], [70.0, 80.0, 130.0, 100.0]),
        ("Below", [2], [0], [10.0, 60.0, 60.0, 70.0]),
        ("Also", [2], [1], [70.0, 60.0, 130.0, 70.0]),
    ]
    words, cell_of, row_rel, col_rel = grid_from(banded)
    owner_rows = [set(r) for _t, r, _c, _b in banded]
    which = []
    for index, (_text, _rows, _cols, _box) in enumerate(banded):
        which += [index] * len(_text.split())
    four = decode(words, cell_of, row_rel, col_rel)
    check("without the count the band collapses",
          sorted(c["row_nums"] for c in four), [[0], [0], [1], [1]],
          "the four relations cannot see a division that no cell sits on -- "
          "this is the 93.9 % ceiling, held here so it cannot move unnoticed")
    with_count = decode(words, cell_of, row_rel, col_rel,
                        row_span=lambda i: len(owner_rows[which[i]]),
                        column_span=lambda i: 1)
    check("with it the band is two rows",
          sorted(c["row_nums"] for c in with_count),
          [[0, 1], [0, 1], [2], [2]],
          "the count says how far a cell reaches and the sweep says where it "
          "starts; neither alone recovers this band")

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        print(f"\n{len(failures)} problem(s) in the grid decoder.")
        return 1
    print("OK grid decoder: cells are components, rows and columns are swept "
          "so a spanning cell belongs to every line it spans, and a cell of a "
          "sentence stays one cell")
    return 0


if __name__ == "__main__":
    raise SystemExit(self_test())
