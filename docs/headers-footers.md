# Headers and footers: the Header & Footer tab

> **Status:** first version: the Header & Footer tab, Link to Previous for the header and the footer apart, Different First Page for the section edited · **Last updated:** 2026-11-10 · **Change:** new

While a header or footer is being edited (double-click the top or bottom margin, Insert ›
Header › Edit Header), the **Header & Footer** tab comes up and is shown; leaving the header goes
back to the tab shown before.

| Group | Button | Command |
|---|---|---|
| Navigation | Go to Header, Go to Footer | `insert.editHeader` / `insert.editFooter` `{"page"}`: the page's own |
| | Previous, Next | `hf.previous` / `hf.next`: another section's |
| | Link Header to Previous, Link Footer to Previous | `layout.linkToPrevious {"footer"?, "value"?}` |
| Options | Different First Page | `layout.differentFirstPage {"value"?}` (the section's) |
| | Different Odd & Even Pages | `layout.differentOddEven {"value"?}` (the document's) |
| Close | Close Header and Footer | `insert.closeHeader` |

The tab also shows which section the header belongs to (*Section 2 / 3*). `hf.state` returns it
all: what is edited, the section, whether its header and its footer are linked, Different First
Page and Odd & Even.

## The section being edited

A header or footer is edited **for the section of the page it is edited on**, even when that
section shows a header inherited from an earlier one, as in Word. Different First Page and Link
to Previous act on that section, not on the first section that owns the header.

## Link to Previous, header and footer apart

Each section's header and footer are linked to the previous section's separately:

- **linked** (the default for a new section): the section has no header of its own and shows the
  previous section's, slot by slot (first page, even pages, the others);
- **unlinked**: the section gets its own copies of what it showed, which can then be changed
  without touching the other sections. With nothing to copy it gets an empty one, so it no
  longer follows the previous section.

The header's link and the footer's are independent: a frontispiece (docs/exercise.md) can have
the same footer as the exercises and a header of its own. The first section has nothing before
it, so its links are greyed out. After a change the caret goes to the header the page now shows.

## A header taller than the top margin

The top margin is where the body starts at the least: a header that grows past it (more lines, a
logo in the text) pushes the body down below it, with no gap, as Word does. A picture *floating*
in the header (in front of or behind the text) doesn't count, also as in Word: put it in line
with the text, or in a table cell, for the body to follow it.

## Code

`crates/engine/src/cmd/hf_options.rs` (`hf.state`, Link to Previous, Different First Page);
the tab: `crates/ui-egui/src/ribbon/strix.rs` (`header_footer`, `follow_header_footer`).

## Revision history

- 2026-11-10: new: the Header & Footer tab; Link to Previous for the header and the footer apart, both ways; Different First Page and Link to Previous act on the section of the page edited.
