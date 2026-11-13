# DSA formatting

> **Status:** F1–F5: the command (`design.dsa`), three versions that combine, taken off exactly; the DSA menu in Design › Document Formatting; Atkinson Hyperlegible and Lexend in Design › Fonts, downloaded on first use; fonts embedded in `.docx` · **Last updated:** 2026-11-12 · **Change:** the DSA menu on the ribbon; the DSA typefaces, downloaded and embedded

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

On the ribbon: Design › Document Formatting › **DSA**, a menu with the three versions as check
boxes (Reading, Writing, Maths and Memory: tick as many as the student needs) and **No DSA
Formatting**, which takes them all off.

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

## Typefaces

Design › Fonts ends with two typefaces made for readers who struggle, both under the SIL Open
Font License: **Atkinson Hyperlegible** (letters told apart at a glance: low vision, dyslexia)
and **Lexend** (wide letters and spacing: reading fluency). Choosing one sets it as the theme's
heading and body font, like the gallery's other entries; the DSA versions never fix the typeface.

Neither is bundled. The first click downloads it (`font.download {"family"}`) from the suite's
font store (craft-libs `craft-font-store`: Google Fonts at a pinned commit, each file checked by
its sha256), the same store and the same consent as Language packs: the click is the person's
consent to downloading, and agents (MCP, the CLI) can download only once it is on. The text shows
the font as soon as it arrives; it stays installed for the next start.

## Fonts in the Word document

Choosing one of them also turns on **Embed Fonts in the File** (the check box under the fonts,
`file.embedFonts {"on"}`; Word's *Embed fonts in the file*, `w:embedTrueTypeFonts`): saving as a
Word document puts the fonts you added or downloaded that the document uses into the package, so
a colleague's or a student's Word shows them without installing anything. The files are
obfuscated as ECMA-376 Part 1 §17.8.1 requires (`word/fonts/*.odttf`, related from the font
table); only TrueType fonts whose licence allows embedding go in (OS/2 `fsType`), a variable
font once, and the system's fonts never (Word's *Do not embed common system fonts*). Fonts the
file already embedded stay.

## Next

- Maths and memory: the unit expected next to each answer; tabular figures.

## Direct emphasis and mathematical digits (Phase 5)

DSA formatting also converts directly italicised prose to bold, with a hidden
character style retaining the original italic, bold, character style and digit
settings. These markers follow runs through editing and splitting, and are
backed up with the DSA styles in `customXml/strixObjects.xml`. Disabling DSA
restores those direct properties after a Word round trip. Equation nodes keep
their own mathematical italic formatting.

Maths and memory requests OpenType tabular digits (`tnum`, with proportional
digits disabled) through resolved character properties and font shaping. Word
packages carry `w14:numSpacing`. When a problem's solution is hidden, a result
marked for a question also supplies a guided `symbol = __________ unit` line
before the solution, beneath its answer space. Removing Maths and memory removes
these generated guides.
