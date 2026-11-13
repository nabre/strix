# Strix objects through Word

> **Status:** done: Word documents (`.docx`, `.docm`, `.dotx`, `.dotm`) keep Strix objects live through Word; objects changed in Word wait for a decision · **Last updated:** 2026-11-11 · **Change:** first version

A Strix document is an OOXML package: `.xmath` is what Strix saves by default, and a `.docx`
holds the same package. Strix's own objects carry their live behaviour in markup Word doesn't
know (`strix:calc` on an equation or a table, a CAD drawing's JSON source attached to its
picture, an exercise's data on its content control, graph paper on a section or a paragraph).
Word shows such a file without trouble, but **when Word saves it, that markup is gone**: the
calculations become plain equations, the drawing a plain picture.

So a Word document Strix writes carries its Strix objects a second time, in two things Word
keeps when it saves:

- **Content controls.** Every calculation, calculated table, CAD drawing and graph-paper
  paragraph is wrapped in a content control (`w:sdt`) tagged `strix:obj:N`, titled *Strix* and
  locked against deletion (not against editing). Exercises already are content controls.
- **A custom XML part** (`customXml/strixObjects.xml`, namespace
  `urn:io.github.nabre.strix:objects:2026`; Word may rename it `customXml/itemN.xml`). For each
  object N: its source (the equation's flags and per-equation `calcOpts`, the table's calc settings, the drawing's JSON, the
  graph paper's settings) and a fingerprint of what it showed; then the document's calculation options (`<strix:calcOptions>`), each section's graph paper
  and frontispiece, and each exercise's data, by their place in the document.

`.xmath` files are written without any of this: they are Strix's own and nobody else saves them.

## Opening a Word document

Every Word package Strix opens (`.docx`, `.docm`, `.dotx`, `.dotm`, and `.xmath` too) goes
through the same step (`crates/engine/src/word_envelope.rs`, `unwrap`):

1. the Strix part is taken out of the document;
2. for each `strix:obj:N` wrapper, what the object shows now is compared with its fingerprint:
   - **unchanged**: the markup Word dropped is put back (the equation is a calculation again, the
     picture a CAD drawing from its JSON, the table calculated, the graph paper back) and the
     wrapper goes. Calculations recompute and drawings redraw as in any document;
   - **changed in the other program** (a result typed over, an equation rewritten, the table's
     text edited, the drawing cropped, replaced or reshaped): the object stays as Word left it,
     static, inside its wrapper, and is listed as pending;
3. the sections' graph paper and frontispiece and the exercises' data are put back where Word
   dropped them.

A file with neither the part nor wrappers (any Word document not written by Strix) is left
untouched.

## Objects changed in another program

When objects are pending, `file.open` says how many (`"externalEdits": n` in its result), and in the app a bar under the ribbon offers:

- **Recalculate from Strix**: the objects are live again from their Strix source. The change
  made in Word is replaced (a typed-over result is recomputed, a picture redrawn from the CAD
  source); a rewritten formula stays as written and is calculated as such.
- **Keep Changes**: the objects keep what Word shows and stay static; their Strix source is
  dropped.
- **Decide Later** (the close button): the bar goes for this document; the objects stay static
  and pending. Saving before deciding counts as Keep Changes.

Scripts, the CLI and MCP use `file.externalEdits`: no `action` (or `"list"`) lists the pending
objects (`{"pending": [{"id", "kind"}]}`); `{"action": "recalculate" | "keep", "ids"?: [n]}`
resolves them (all by default).

## Saving

Because Word documents keep them, a document with Strix objects saves to `.docx`, `.docm`,
`.dotx` and `.dotm` like to `.xmath`: Save writes a Word document in place, Save As offers them,
File › Export enables them. OpenDocument, RTF, plain text and Word XML still lose the objects, so
those stay refused for such a document, with the reason and `.xmath` proposed (docs/file-format-parity.md).
New documents still save as `.xmath` by default.

## What is checked, and how

The fingerprint is taken from what the object shows, not from its XML, so Word writing the same
content differently doesn't count as a change:

| Object | Unchanged when |
|---|---|
| Calculation (also an answer or a stepped calculation) | the equation's linear text, without spaces, is the same |
| Calculated table | the cells' text (equations as linear text), without spaces, is the same |
| CAD drawing | Word kept the drawing as Strix wrote it; or the picture is Strix's SVG, byte for byte; or it is a picture Word made from it (its own rendering), not cropped, in the same proportions |
| Scientific plot (`strix-plot`) | Word kept `word/strix/plotN.json` as Strix wrote it; or the picture is Strix's SVG, byte for byte; or it is Word's uncropped rasterised picture in the same proportions |
| Graph paper | always (there is nothing to edit) |

Limits: a picture Word made from the drawing and then replaced with *Change Picture* at the same
proportions isn't told from the original, so the drawing comes back from its source. An object
moved out of its wrapper in Word comes back static.

## Tests

`crates/engine/src/tests/word_round_trip.rs` simulates what Word does when it saves (every
`strix:` attribute and element, the CAD source parts and their picture extension removed;
content controls and custom XML parts kept) and checks that every kind of object comes back live,
that a result typed over waits for a decision, that both decisions work (also through
`file.externalEdits`), that saving with pending objects starts clean, that `.xmath` carries no
envelope and that a hostile Strix part never panics. `crates/engine/src/tests/strix_objects.rs`
and `crates/ui-egui/src/file_dialogs/tests.rs` cover the save rules.

Word itself isn't run by the tests. To check by hand on a machine with Word: save a document with
a calculation, a calculated table and a CAD drawing as `.docx` in Strix, open it in Word, type
something elsewhere, save, and open it again in Strix: everything is live. Then change a result
in Word, save, and open it in Strix: the bar asks.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-13 | minor | Live scientific and experimental plots (`strix-plot`, `word/strix/plotN.json` + SVG envelope) preserved across Word `.docx` round trips |
| 2026-11-13 | minor | Inline parametric expressions (`strix:expr` formula), grading scale config (`strix:grading`), choice indices (`strix:choiceIdx`), `shuffleChoices` and option `strix:rationale` preserved across Word `.docx` round trips |
| 2026-11-13 | minor | Document calculation options (`<strix:calcOptions>`) and per-equation overrides (`calcOpts`) preserved across Word round trips |
| 2026-11-11 | major | First version: content-control wrappers and the Strix custom XML part in Word documents, live objects after Word saves, pending objects changed elsewhere (`file.externalEdits`, the bar under the ribbon), Word documents allowed for documents with Strix objects |
