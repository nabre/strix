# Graph paper

> **Status:** stage 1 done: model, the six placements, rendering (screen, PNG, PDF), commands, Insert-tab menu, Graph Paper pane, `.xmath` round trip · **Last updated:** 2026-10-28 · **Change:** new

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

Lines sit on whole squares from the grid's top-left corner (the left edge of the text column
and the top of the block or of the body), as far as whole squares reach inside its area: the
grid never ends in a sliver of a square. The heavier lines are those on squares 0, N, 2N … and
are drawn over the ordinary ones. A dimmed body (header/footer editing) fades the grid too.

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
lines (style, colour, weight) and the heavier lines (every N squares, style, colour, weight),
and *Remove Graph Paper*. Each change is one `graph.set`, one undo step; a number dragged is
applied when the drag ends.

## File format

In `.xmath` the grid is one attribute in Strix's namespace (ignorable by other readers):
`strix:graph` on the block's `w:pPr`, or on the section's `w:sectPr` for a background:

```xml
<w:pPr strix:graph="place=pages:3;spacing=14.173;line=solid:9FC5E8:0.4;every=5;major=solid:6A9ED0:0.9">
```

Sizes are points. Unknown keys are skipped and unreadable values keep their defaults; a value
longer than 1 KiB or with an unknown placement is dropped. Word ignores the attribute, so a
`.docx` export carries no grid.

## Code

- Model: `crates/doc/src/graph_paper.rs` (`GraphPaper`, `GridLine`, `GridPlacement`,
  `sanitized`, `lines`, `to_attr` / `from_attr`); `SectionProps::graph_paper`,
  `ParaProps::graph` (a split leaves it on the first part).
- Layout: `Placed::Grid`; `background_grid` (page decoration, behind everything) and
  `place_graph_para` in `crates/layout/src/lib.rs`; tests in `tests_graph_paper.rs`.
- Drawing: `display::grid_draws` (two dashed or solid paths, ordinary then heavier), which the
  screen, PNG and PDF renderers share.
- Commands: `crates/engine/src/cmd/graph.rs`; pane: `crates/ui-egui/src/graph_pane.rs`.

## Next

- A grid of dots (dot paper) and of millimetres with 1-5-10 hierarchy presets; isometric paper.
- Snap typed text to the squares (one line per square).
- A grid in table cells and text boxes.

## Revision history

- 2026-10-28: new: model, placements, rendering, commands, menu and pane, `.xmath` round trip.
