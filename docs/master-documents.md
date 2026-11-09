# Master documents

> **Status:** first version: insert, create, unlink, reload, save back, open · **Last updated:** 2026-11-09 · **Change:** new

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

Without `path`, the commands act on the subdocument at the caret (Insert and Create ask for a file).

## How the link works

- A subdocument is a block-level rich text content control around its content, tagged
  `strix-subdoc path=<absolute path>` and titled with the file name. It can't be deleted by
  accident (only its content is edited); Unlink removes it.
- **Saving** the master as its own file (`.xmath`, or another editable format; not a static
  export) first writes every subdocument edited in the master back to its file, in that file's
  format, with the master's styles, lists and notes and the file's own document properties. A
  subdocument not edited isn't rewritten. The save's result lists them under `subdocuments`.
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

## Limits

- Not on the web (files are opened and saved through the browser there).
- Section breaks inside a subdocument are kept as in Text from File; headers and footers come
  from the master.

## Revision history

- 2026-11-09: first version.
