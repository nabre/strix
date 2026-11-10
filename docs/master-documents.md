# Master documents

> **Status:** first version: insert, create, unlink, reload, save back, open · **Last updated:** 2026-11-10 · **Change:** subdocuments marked on the page with Show/Hide ¶; Insert in an exercise goes after it; Unlink/Open act on a lone subdocument; style and numbering changes in the master reach the files; errors after picking a file shown; previously: styles and lists of a subdocument come with it

A **master document** shows other documents — its **subdocuments** — in place, each still linked
to its own file: editing a subdocument's text in the master and saving the master changes that
file. Use it for a book made of chapters, a collection of exercises kept one per file, a report
written by several people.

## Using it

Outlining › Master Document (in Outline view):

| Button | Command | Does |
|---|---|---|
| Insert | `master.insertSubdocument {"path"}` | links a document (any format Strix opens: `.xmath`, `.docx`, `.odt`, `.rtf`…) at the caret, as a block of its own |
| Create | `master.createSubdocument {"path", "overwrite"?}` | saves the selected paragraphs as a new file (`.xmath` when the name has no extension) and links it in their place |
| Unlink | `master.unlinkSubdocument {"path"?}` | the subdocument's text becomes the master's own; the file stays as it is |
| Reload | `master.refresh {"path"?}` | replaces the content with the file's, discarding edits made in the master |
| Open | `master.openSubdocument {"path"?}` | opens the subdocument's file in a window of its own |
| — | `master.saveSubdocuments {"force"?}` | writes the edited subdocuments now |
| — | `master.subdocuments` | lists them: path, file found, edited, conflict |

Without `path`, the commands act on the subdocument at the caret, or on the only one when the
document has one (Insert and Create ask for a file; an error after picking it, such as a file
that can't be read, shows on the status bar). Insert with the caret in an exercise or another
block content control puts the subdocument just after it.

With Show/Hide ¶ on (`view.marks`), every subdocument is marked on the page, wherever the caret
is, like the other formatting marks: a light frame around it,
a bar in the left margin along it, and a tab above it with its file's name (the full path in its
tooltip). A click on the tab puts the caret in it, so Unlink, Reload and Open act on that one.

The **Subdocuments** pane (the Subdocuments button, `view.subdocumentsPane`) lists the linked
files, each on a card with its folder, its first words and its state: *in step*, *edited* (saved
with the master), *conflict* (with Keep this version / Keep the file) or *not found*; a click on
the name goes to it, and Reload, Open and Unlink act on that one. Below, Insert and Create, and
Save or Reload all of them.

## How the link works

- A subdocument is a block-level rich text content control around its content, tagged
  `strix-subdoc path=<absolute path>` and titled with the file name. It can't be deleted by
  accident (only its content is edited); Unlink removes it.
- **Saving** the master as its own file (`.xmath`, or another editable format; not a static
  export) first writes every subdocument edited in the master back to its file, in that file's
  format, with the master's styles, lists and notes and the file's own document properties. A
  subdocument not edited isn't rewritten. Changing a style or a list in the master (the
  exercises' numbering is one) counts as an edit of every subdocument, so their files take it on
  the next save; opening the master or inserting a subdocument (whose new styles come in) doesn't.
  The save's result lists them under `subdocuments`.
- **Opening** the master reloads every subdocument from its file: a chapter edited on its own
  shows up in the book. A missing file leaves the master's copy (the master's file always keeps
  one, so it opens and prints anywhere); the next save writes it back.
- When a file was **changed on disk and in the master** since they last agreed, saving doesn't
  overwrite it: the subdocument is marked as a conflict (kept across reopening), the status bar
  says so, and Reload (take the file's) or `master.saveSubdocuments {"force": true}` (take the
  master's) settles it.
- When the folder holding the master and its subdocuments moves, a link whose path is gone finds
  the file of the same name beside the master.
- A subdocument inside a subdocument is part of its content: it is a link again when that file
  is opened as a master itself. A document can't be its own subdocument, nor link the same file
  twice.

- A subdocument brings the **styles, lists and pictures** its content uses, as Text from File
  does: a style the master has keeps the master's definition (the master rules, as a style
  update), one it lacks comes in, and numbered paragraphs count with the lists brought over.
- It goes in an empty paragraph at the caret; a paragraph after it is added only when nothing
  follows, so two subdocuments inserted one after the other leave no blank line between them.
- **Exercises** in subdocuments are one series of the master (docs/exercise.md, In a master
  document): one results summary at the end of the master, the series' settings on all, written
  back to the files, each file getting its own summary again.

## Limits

- Not on the web: subdocuments are files on disk, which the browser version can't read back or write, so Insert and Create are greyed out there (with that reason); a master made on the desktop still opens and shows its saved copies.
- Section breaks inside a subdocument are kept as in Text from File; headers and footers come
  from the master.

## Revision history

- 2026-11-10: subdocuments marked on the page (frame, margin bar, file tab) with Show/Hide ¶ on; Insert in an exercise goes after it; Unlink and Open act on a lone subdocument; style and numbering changes made in the master are written to the files; an error after the file dialog is shown.

- 2026-11-10: styles and lists of a subdocument come with it; exercises follow the master; no blank paragraph between subdocuments.

- 2026-11-09: first version; the Subdocuments pane.
