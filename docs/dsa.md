# DSA formatting

> **Status:** F1: the command (`design.dsa`), three versions that combine, taken off exactly · **Last updated:** 2026-11-11 · **Change:** first version

A test laid out for students with specific learning difficulties (DSA, *disturbi specifici
dell'apprendimento*), from the same exercises: Design › Document Formatting › DSA
(`design.dsa`). Three versions, one per area of difficulty, that **combine**: a student with
several difficulties gets them together.

## The versions

Any version brings the **common base** (after the British Dyslexia Association's style guide and
WCAG 1.4.12, text spacing):

- body text 13 pt, line spacing 1.5, at least 8 pt after each paragraph;
- left aligned: no justified paragraph style;
- no italics: a style in italics shows its emphasis in bold;
- no automatic hyphenation;
- each exercise on a new page (the series' page breaks).

| | Reading (`reading`) | Writing (`writing`) | Maths and memory (`math`) |
|---|---|---|---|
| Difficulty | dyslexia: slow decoding, crowding | dysgraphia, dysorthography: forming letters, using the page | dyscalculia, working memory: holding data, aligning numbers |
| Text | 14 pt, letters 0.12 em further apart | 13 pt | 13 pt |
| Questions | 12 pt after each | **36 pt after each**: room to write | 8 pt after each |
| Exercise parts | — | the graph paper in **1 cm squares**, heavier lines (0.75 pt), when the exercises have graph paper | the **data framed** |

Where two versions set the same thing, the more generous wins (Reading + Writing: 14 pt text and
36 pt after each question). The page stays white in every version. The typeface is not part of
it: choose one in Design › Fonts.

`design.dsa {"reading": true, "math": true}` turns versions on (and `false` off) leaving the
others as they are; `{"none": true}` takes them all off; without params it says which are on.

## Formatting only

It changes styles, the document's hyphenation and the exercises' series settings, never the
text: in a master document the subdocuments' files keep their own formatting (the master's styles
and series settings stay the master's, [master-documents.md](master-documents.md)). Italics typed
directly on text (not from a style) stay: changing them would change the text.

## Taken off exactly

Before changing a style, its formatting is kept in a hidden style (`StrixDsaBase-<id>`); the
settings it changes (hyphenation, page breaks, graph paper) are recorded in a hidden marker style
(`StrixDsa`). Taking a version off puts everything back and lays the remaining versions over it
again; taking all off removes the hidden styles. Word keeps hidden styles, so a `.docx` saved
with DSA formatting and opened again still goes back to its own. A style changed by hand while
DSA formatting is on gets its own formatting back when it is taken off.

## Next

- F2: the DSA menu in Design › Document Formatting (three check boxes and None).
- F3: fonts embedded in `.docx` (Word's *Embed fonts in the file*).
- F4–F5: Atkinson Hyperlegible and Lexend in Design › Fonts, downloaded on first use.
- Maths and memory: the unit expected next to each answer; tabular figures.
