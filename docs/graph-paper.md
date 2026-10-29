# Graph paper

> **Status:** stage 1 done: model, the six placements, rendering (screen, PNG, PDF), commands, Insert-tab menu, Graph Paper pane, `.xmath` round trip; whole squares on the bottom margin, Follow columns, frame weight · **Last updated:** 2026-10-29 · **Change:** the grid sits on the bottom margin in whole squares; Follow columns; a frame of its own weight

**Graph paper** (*foglio a quadretti*, Insert › Pages › Graph Paper, `graph.insert`) is a square
grid drawn behind the text, for working a problem by hand, sketching or writing on squares as
in an exercise book. It is not a picture: it is drawn from a few numbers (the side of a square,
two kinds of line), so it stays sharp at any zoom, in print and in PDF.

## The grid

| Property | Parameter | Range (clamped) | Default |
|---|---|---|---|
| Side of a square (*interasse*) | `spacing` (mm) | 1–100 mm | 5 mm |
| Line style | `style` | `solid`, `dashed`, `dotted` | solid |
| Line colour | `color` (`RRGGBB`) | any | light blue `9FC5E8` |
| Line weight | `width` (pt) | 0.1–6 pt | 0.4 pt |
| A heavier line every N squares | `majorEvery` | 0 (none) – 100 | 0 |
| Heavier line style, colour, weight | `majorStyle`, `majorColor`, `majorWidth` | as above | solid, `6A9ED0`, 0.9 pt |
| Follow columns | `followColumns` | `true` / `false` | false |
| Frame weight | `frameWidth` (pt), `null`: as the grid lines | 0.1–6 pt | as the grid lines |

A grid holds **whole squares** only, never a sliver of one. Across, the squares start at the
left edge of the text column and go as far as whole squares reach. Down:

- a grid that reaches the bottom of the text area (a background, a block to the end of the page,
  the whole pages of a longer block) **sits on the bottom margin**: the part of a square left
  over goes above its first square, as a little more room between the text above and the grid
  (a block's own text starts at its first square);
- a block of a fixed height is ruled from its top down to its last whole square; the text after
  it still starts at the height asked for.

The heavier lines are those on squares 0, N, 2N … from the top-left corner and are drawn over
the ordinary ones. A dimmed body (header/footer editing) fades the grid too.

**Follow columns.** Off (the default), a background is ruled across the whole text width, as if
the section had one column, and the whole pages of a block likewise. On, in a section with more
than one text column, each column is ruled on its own (squares from its left edge, on the bottom
margin) and the space between the columns is left clear. A block already starts in its own
column, and the page's columns after it are ruled one by one either way.

**Frame.** The frame is the border of the ruled area, the outermost lines. By default it weighs
what the ordinary lines do, so the grid looks as it always has (the top and left edges are
heavier lines when there are heavier lines). Given a weight of its own, the edge lines are drawn
as one frame of that weight, in the ordinary lines' style and colour, over the rest; each ruled
area (each page, each column) has its own.

## Placements

| Placement | `placement` | What it covers |
|---|---|---|
| Background | `background` | behind the text of **every page of the section**, over the body (the margin box, below a header taller than its margin); the text flows over it as usual |
| To the end of the page | `pageEnd` (default) | from where it is in the text to the bottom of the page (the column, then the page's other columns); what follows starts on the next page |
| Until the next odd page | `untilOdd` | the rest of the page, then whole pages until the next odd-numbered page starts (on page 3: pages 3 and 4) |
| Until the next even page | `untilEven` | the same up to the next even page (on page 3: page 3 only) |
| For N pages | `pages` + `pages` | the rest of the page and N − 1 whole pages after it (1–100) |
| For a height | `height` + `height` (mm) | a block that tall in the text (at most the page's text height); moved to the next column or page when it doesn't fit; the text after it goes on below it |

Odd and even are the page numbers shown (a restarted numbering counts), like Word's odd and
even section breaks.

A grid in the text flow is a **paragraph** (`ParaProps::graph`): its text, usually none, is
written over the grid, which starts at its first line. Typing in it writes on the squares;
Enter at its end gives a paragraph without the grid after it. It is direct formatting only
(styles never set it) and lives only in top-level body paragraphs; in table cells, text boxes,
headers and footers it is ignored. Web layout leaves graph paper out; the reading view
leaves out the background.

## Commands

| Command | |
|---|---|
| `graph.insert` | a background for the section(s) of the selection, or a block at the caret: an empty paragraph there becomes the block, at the start of a paragraph's text it goes before it, otherwise after the caret's paragraph (or table); the caret goes in the block |
| `graph.set` | changes the graph paper at the cursor: the block the caret is in, else the section's background. A new placement moves it: a block made a background goes from the text (with its paragraph when empty), a background given a text placement becomes a block at the caret |
| `graph.get` | `{"grid": {...} \| null}`: the grid at the cursor with `target` (`paragraph` / `section`), sizes in the parameters' units |
| `graph.remove` | removes it (an empty block goes with its paragraph) |
| `view.graphPaperPane` | shows or hides the Graph Paper pane |

Every parameter is optional and checked: a wrong type or name is an error, numbers out of range
are clamped, non-finite ones fall back to the defaults. `strix-cli run --cmd
'graph.insert={"placement":"pages","pages":3,"majorEvery":5}'`.

## Interface

Insert › Pages › **Graph Paper** opens a menu: *To the End of the Page*, *Page Background*,
*Until the Next Odd/Even Page*, *For Two Pages*, *For a Height of 5 cm*, then *Graph Paper
Properties* (the pane) and *Remove Graph Paper*. The icon (a sheet ruled in squares) is drawn in
code (`icons.rs`, `graphPaper`, `graphPaperProperties`).

The **Graph Paper pane** (docked on the right) shows a preview of the grid on the paper's white
(also in the dark theme), the placement with its pages or height, the side of a square, the
lines (style, colour, weight) Follow columns, the outer frame's weight (with *As the grid lines*), and the heavier lines
(every N squares, style, colour, weight), and *Remove Graph Paper*. Each change is one `graph.set`, one undo step; a number dragged is
applied when the drag ends.

## File format

In `.xmath` the grid is one attribute in Strix's namespace (ignorable by other readers):
`strix:graph` on the block's `w:pPr`, or on the section's `w:sectPr` for a background:

```xml
<w:pPr strix:graph="place=pages:3;spacing=14.173;line=solid:9FC5E8:0.4;every=5;major=solid:6A9ED0:0.9;cols=1;frame=1.5">
```

Sizes are points; `every`/`major`, `cols=1` (Follow columns) and `frame` (its own weight) are
written only when set. Unknown keys are skipped and unreadable values keep their defaults; a value
longer than 1 KiB or with an unknown placement is dropped. Word ignores the attribute, so a
`.docx` export carries no grid.

## Code

- Model: `crates/doc/src/graph_paper.rs` (`GraphPaper`, `GridLine`, `GridPlacement`,
  `sanitized`, `lines`, `to_attr` / `from_attr`); `SectionProps::graph_paper`,
  `ParaProps::graph` (a split leaves it on the first part).
- Layout: `Placed::Grid` (the rect ruled: whole squares down); `crates/layout/src/graph.rs`
  (`background`, the page decoration behind everything; `place_graph_para`; `whole_rows`,
  `page_areas`); tests in `crates/layout/src/tests/graph_paper.rs`. The grid is laid out again
  with the rest of the page on every edit (and undo/redo), so a block follows the text before it.
- Drawing: `display::grid_draws` (two dashed or solid paths, ordinary then heavier, then the
  frame), which the screen, PNG and PDF renderers share.
- Commands: `crates/engine/src/cmd/graph.rs`; pane: `crates/ui-egui/src/graph_pane.rs`.

## Next

- A grid of dots (dot paper) and of millimetres with 1-5-10 hierarchy presets; isometric paper.
- Snap typed text to the squares (one line per square).
- A grid in table cells and text boxes.

## Revision history

- 2026-10-28: new: model, placements, rendering, commands, menu and pane, `.xmath` round trip.
