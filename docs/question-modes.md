# Question modes

> **Status:** a mode per question (open, multiple choice, true/false, calculation, CAD drawing,
> worked example, inline completion, matching), mixed in one quiz; figures to complete through
> the CAD drawing's answer layers and entities · **Last updated:** 2026-11-17 · **Change:** docs
> brought up to date with the code (the planned stages that landed moved out of Next; the Show
> Solution path).

Strix's teaching side is built for **assessment moments** (*momenti di valutazione*): tests,
classwork, homework, a worked example handed out before them. The unit is the **exercise**
(docs/exercise.md); this page says **how its questions are answered** and how each mode gives
the students' copy and the corrected copy from the same document.

## The idea: one document, two copies

Every mode answers the same three questions:

| | Students' copy (Solution hidden) | Corrected copy (Solution shown) | What Strix adds up |
|---|---|---|---|
| where the student answers | a space, options, a line | the same place, filled in | points per question → scores table |
| what the teacher prepares | — | the answer, the working, the drawing | results summary |
| what stays identical | the page layout: the space keeps its height | | |

So the teacher prepares **one** file; Exercise › Layout › Show › Solution (the Series tab's in a
series, `exercise.show {"solution"}`) switches between the two copies, and in a master document the series decides for every exercise (docs/exercise.md, In
a master document).

## The modes

| Mode | `type` | Students' copy | Corrected copy | Summary |
|---|---|---|---|---|
| **Open answer** | `open` | a space (cm), squared or blank; the squares' side, line style and colour (`spacing`, `gridStyle`, `gridColor`, `null` for the default; Exercise › Questions › Question Settings › Answer Space) | the answer written in the space | `a) domanda a risposta aperta` (always, also in a quiz of open answers alone: a trace for correcting) |
| **Multiple choice** | `choice` | options A), B)… with empty boxes | the correct boxes ticked (more than one may be) | `b) A, C` |
| **True or false** | `trueFalse` | two empty boxes on the statement's line, in the V and F columns | the right one ticked | `c) V` (the column's initial in the exercise's language: V, T, W) |
| **Calculation** | `calc` | a squared space for the working, then `Answer: A = ______ cm²` | the working (live equations) and `Answer: A = 15 cm²` | `d) A = 15 cm²` |
| **CAD drawing** | `drawing` | a squared space (8 cm) to draw in | the solution's CAD drawing in the space | — |

A **problem** (statement, data, questions, a worked solution after them) stays the exercise kind
for long multi-step problems; the modes above are for **quizzes**, where the exercise *is* its
questions and each answer sits under its question.

### One quiz, mixed modes

Each question has its own mode, kept in its settings (`strix:answerSpace`, `"mode"`), so a
geometry exercise can ask *a) what is a rectangle (open)*, *b) a square is a rectangle
(true/false)*, *c) work out the perimeter (calculation)*, *d) draw it with its dimensions
(drawing)*. New Exercise's kinds only say which mode the first questions start in; Add Question
adds one like the last, or of the mode asked for (`exercise.addQuestion {"type": "calc"}`), with
that mode's space.

### True or false

A true/false question is answered on its statement's line: at the right of the text column two
columns, **V** and **F** (the initials of True and False in the exercise's language: V F in
Italian and French, T F in English, W F in German), each with a check box. The boxes sit at the
same two right-aligned tab stops in every statement, left of the points, so a **battery** of
true/false statements lines up in two columns down the page; above the first statement of each
run of consecutive true/false questions a generated header line names the columns. A statement
that runs over several lines has its boxes on its last line, beside the points: a paragraph can
only put them on its first line by becoming a table, which the quiz structure (one paragraph per
question) doesn't use. The statement has no right indent: past one, the positional tab that sends
the points to the margin would push them to a line of their own.

The boxes behave as a multiple choice's: the right one ticked while the answers show, both empty
in the students' copy; a box ticked by hand while the answers show becomes the answer and clears
the other. `exercise.questions` reads and sets them as before (`"choices": ["Vero", "Falso"]`,
`"correct": "A"`), and `exercise.questionMode` converts to and from the other modes (to multiple
choice: *Vero*, *Falso* and two empty options). In the file the answer control is inline in the
question's paragraph (`strix:exercise.answer`, the two `strix:exercise.box` check boxes after
tabs) and the header is `strix:exercise.trueFalseHead`; true/false options from older files, one
per paragraph, move onto the statement's line when the document is opened.

### Calculation

The core of the maths and technical subjects. The question is worked in the space with **live
equations** (docs/calc.md) of the exercise's sandbox, so they use the exercise's **data** and
follow its **variants**: variant A gives `A = 15 cm²`, variant B `A = 24 cm²`, with nothing redone
by hand. The data part of a quiz goes after its statement, or after its title when it has none.

- The **results** are the values found in the space that the working doesn't use afterwards:
  `s = a·t^2/2 =` and `v = a·t =` are two results, while in `d = b + h =`, `P = 2·d =` only `P`
  is (`d` is working). Each is marked as the question's result (like Result Of in a problem);
  with none such, the last equation is the result. The working is shown with its steps
  (substitution, isolation: Math › Show Steps).
- Under the space, **one result line per result** (`strix:exercise.resultLine`, generated,
  can't be edited or deleted), right-aligned on the margin in columns so the lines of one
  question line up: the label (*Risposta:*, *Answer:*, *Réponse:*, in the exercise's language,
  on the first line only), the name with its `=`, a line to write on, the **unit**. In the
  students' copy the line is blank (`Risposta:  s = ________ m`), so the student knows what is
  asked and in which unit (it also serves the DSA *Calcolo e memoria* formatting); in the
  corrected copy the value is written on it, underlined, in the answers' colour
  (`s = 20 ____ m`). The lines are 1 cm apart (exact line spacing), room for a value
  **handwritten 8 mm tall**. With no result yet there is one blank line. Commands report each
  line without label and tabs (`exercise.questions` → `"line": "s = 20 m"`, `"lines": ["s = 20 m",
  "v = 10 m/s"]`).
- The unit is the result's: ask for it after `=` as in any calculation (`A = b·h = cm^2`),
  otherwise the working unit (`m²`) is used.
- Hidden, the working is hidden text and the space keeps its height: the students' copy is laid
  out like the corrected one.
- The results summary lists the results (`c) s = 20 m; v = 10 m/s`), or *risultato non indicato*.

### CAD drawing

For technical drawing, geometry and physics diagrams. The space is squared and 8 cm tall by
default; the teacher draws the solution in it with the CAD editor (docs/cad.md). New drawing
questions start with the starter drawing fitted to the space's height (or the `drawing` source
given), ready to edit. Being parametric, a drawing sized by the exercise's data (`L`, `b`, `h`)
**is redrawn for each variant**. Hidden, the drawing is hidden text: it is left out of the page,
and the student draws on the squares.

A figure **given** to the student (the beam to analyse, the triangle to measure) is not an
answer: it goes in the statement or among the questions as an ordinary CAD drawing, as today.

## Next

- **Ordering** (*ordinamento*): items to put in order, keeping the two-copies rule and giving its
  line to the summary.
- A worked example numbered on its own (*Esempio 1*) instead of in the exercises' numbering.

Landed from the stages planned here: the **worked example** (`worked`: its solution always shown,
no points, no row in the scores table), **inline completion** and **matching** (below, Laboratory
and completion questions), and the **figure to complete** (a CAD drawing whose answer layers or
entities are hidden in the students' copy: [cad.md](cad.md), Completion drawings).
**Multiple choice** has parametric options (`{{expr}}` / `strix:expr`), teacher-only distractor
rationales (`strix:rationale`) and a deterministic per-variant option permutation
(`shuffleChoices: true`), via `exercise.questions` and the Question Settings pane.

## Commands

| Id | Mode parameters |
|---|---|
| `exercise.insert` | `"kind": "open" \| "choice" \| "trueFalse" \| "calc" \| "drawing"` (the first questions' mode); per question `"type"` (its own mode) and, for a drawing, `"drawing"` (its CAD source) |
| `exercise.addQuestion` | `"type"` (default: like the last question), `"drawing"` |
| `exercise.questionMode` | `"type": "open" \| "choice" \| "trueFalse" \| "calc" \| "drawing"` — converts an existing quiz question's answer part in place while preserving its question paragraph, points, space settings, working equations and choice options |
| `exercise.questionPoints` | `"points": number \| null` — sets or clears a question's points on both problems and quizzes |
| `exercise.questions` | sets/reports `"type"`, `"points"`, `"tolerance"` (for `calc`, e.g. `"2%"` or `"0.1 cm"`, shown as an interval on the teacher copy), `"shuffleChoices"` and `"rationales"` (for `choice`), plus `"line"`, `"result"`, `"workingPos"`, `"scale"`, `"cadSelect"` |

## Revision history

- 2026-11-17: Docs brought up to date with the code: the status line; Next keeps only what is still planned (ordering, a worked example's own numbering), the worked example, inline completion, matching and figures to complete listed as landed; Show › Solution is on the Exercise and Series tabs.

- 2026-11-16: every open answer gives a line in the results summary (*domanda a risposta aperta*), also when an exercise or the whole document has open answers alone (no summary before), so that no part is forgotten when correcting.
- 2026-11-13: one result line per result (the values the working doesn't use afterwards), right-aligned in columns that line up at the `=`, 1 cm apart for handwriting 8 mm tall; the corrected copy writes the value on the line, underlined.
- 2026-11-13: the calculation's result line is labelled (*Risposta: s = 20 m*) and its value is in the answers' colour once given, so in the corrected copy it no longer looks like a stray repeat of the working's result.
- 2026-11-13: calculation tolerances (`tolerance` in `exercise.questions`), parametric `{{expr}}` options (`strix:expr`), teacher-only option rationales (`strix:rationale`), and deterministic per-variant choice shuffling (`shuffleChoices`).
- 2026-11-13: in-place mode conversion (`exercise.questionMode`), contextual Quiz controls (mode selector, live calculation preview and Go to Working, CAD scale and Edit Drawing), Variants & Data exposed for quizzes (`showData` toggle), and per-question points on problems (`exercise.questionPoints`).
- 2026-11-15: the summary names a true/false answer by its column's initial in the exercise's language (V/F, T/F, W/F), says *risposta aperta* for an open answer beside other results and lists a completion's hidden words; hidden words for texts to complete by hand (Hide Words, `[[word]]`); the answer space's line style and colour; the *table to complete* mode removed (a file's `tableFill` question reads as an open answer).
- 2026-11-12: new: question modes; a mode per question, mixed in one quiz; calculation questions (worked with the exercise's data and variants, result line with blank and unit, result in the summary) and CAD drawing questions (the solution's drawing in a squared space, hidden in the students' copy); a quiz's data part after its title when it has no statement.

## Laboratory and completion questions (Phase 5)

`exercise.insert {"kind": ...}` and a question's `"type"` also accept:

- `worked`: an example whose working stays visible in the student copy. Its
  points are always unset. An exercise containing only worked examples has no
  row in the scores table.
- `cloze`: inline completion — a text with gaps (*testo bucato*) the students fill in by hand.
  - **Hidden words**: select words inside an exercise and click Exercise › Inline Completion › Hide
    Words (`exercise.blank` with no `expr`), or type `[[word]]` in an inline completion
    question. Elsewhere `[[…]]` stays as typed, and so does one holding brackets (a list
    `[[1, 2], [3, 4]]`) or inside another control. The corrected copy shows the words; the
    students' copy a line to write on, twice the word's length and never shorter than 14
    underscores, so its length gives little away. Kept on the `strix:blank` control
    (`<strix:blankWord>`, titled *Hidden word*: the frame never shows the word; the word is
    still in the file, as the solutions are, so hand students a printout or a PDF). The
    results summary lists them (`b) fotosintesi, foglie`). Wherever a hidden word shows (the
    corrected copy, while writing the exercise) it is in the *Hidden Word* character style
    (`HiddenWord`: bold, italic, blue, added the first time and kept as the teacher changes
    it), so the words that will be hidden stand out; the students' line is in the text's own
    formatting.
  - **Calculated gaps**: `{{expression}}` templates in the question become `strix:blank`
    controls; the teacher sees their calculated values, the student a blank retaining the unit.
    `exercise.blank {"expr":"rho -> kg/m³"}` inserts one at the caret in ordinary text too.
    Values use the exercise's sandbox.
- `matching`: two columns with association boxes. Pass `"pairs":[["mass","kg"],
  ["time","s"]]` or edit Pairs in the Exercise tab's Question Settings. The right column uses a
  deterministic permutation per exercise/question/variant. Only teacher copies
  contain the association letters. Pairs and modes survive Word round trips.

Each mode can coexist with existing question modes in one quiz; the Question
Type menu and the per-question selector offer all of them.
