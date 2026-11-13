# Question modes

> **Status:** stage 1 done: a mode per question (open, multiple choice, true/false, calculation,
> CAD drawing), mixed in one quiz; stages 2–4 planned · **Last updated:** 2026-11-12 · **Change:**
> new: the teaching side structured as question modes; calculation (worked in a squared space,
> result line `A = ______ cm²` from the exercise's data and variants) and CAD drawing (the
> solution's drawing hidden in the students' copy) questions.

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

So the teacher prepares **one** file; Teaching › Exercise › Show › Solution switches between the
two copies, and in a master document the series decides for every exercise (docs/exercise.md, In
a master document).

## The modes

| Mode | `type` | Students' copy | Corrected copy | Summary |
|---|---|---|---|---|
| **Open answer** | `open` | a space (cm), squared or blank | the answer written in the space | — |
| **Multiple choice** | `choice` | options A), B)… with empty boxes | the correct boxes ticked (more than one may be) | `b) A, C` |
| **True or false** | `trueFalse` | *Vero* / *Falso* with empty boxes | the right one ticked | `c) A` |
| **Calculation** | `calc` | a squared space for the working, then `A = ______ cm²` | the working (live equations) and `A = 15 cm²` | `d) A = 15 cm²` |
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

### Calculation

The core of the maths and technical subjects. The question is worked in the space with **live
equations** (docs/calc.md) of the exercise's sandbox, so they use the exercise's **data** and
follow its **variants**: variant A gives `A = 15 cm²`, variant B `A = 24 cm²`, with nothing redone
by hand. The data part of a quiz goes after its statement, or after its title when it has none.

- The **last equation in the space is the result** (marked as the question's result, like
  Result Of in a problem); the ones before it are working, shown with their steps
  (substitution, isolation: Math › Show Steps).
- Under the space a **result line** (`strix:exercise.resultLine`, generated, can't be edited or
  deleted): while the answers are hidden, the result's name, a blank and its **unit**
  (`A = ______ cm²`), so the student knows what is asked and in which unit (it also serves the
  DSA *Calcolo e memoria* formatting); when they show, the result (`A = 15 cm²`). With no
  result yet it is a blank.
- The unit is the result's: ask for it after `=` as in any calculation (`A = b·h = cm^2`),
  otherwise the working unit (`m²`) is used.
- Hidden, the working is hidden text and the space keeps its height: the students' copy is laid
  out like the corrected one.
- The results summary lists the result (`c) A = 15 cm²`), or *risultato non indicato*.

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

- **Stage 2 — Worked example** (*esempio svolto*): an exercise shown as a model before the
  others: numbered on its own (*Esempio 1*), its solution always shown, also in the students'
  copy, outside the scores table and the results summary.
- **Stage 3 — Figure to complete**: a CAD drawing given in the question with some parts
  (dimensions, sides, labels) marked as the answer: hidden in the students' copy, shown in the
  corrected one. Needs a per-entity "answer" flag in the drawing (crates/cad).
- **Stage 4 — More closed modes**: fill in the blanks (*completamento*, blanks inline in the
  question's text), matching (*abbinamento*, two columns), ordering. Each keeps the two-copies
  rule and gives its line to the summary.
- **Calculation**: a tolerance per result (± absolute or %) for the grading grid; changing an
  existing question's mode (today: delete it and add one of the new mode); a result line per
  result when the space holds several.
- The **grading grid** (points → mark) closes the chain: docs/exercise.md, Next.

## Commands

| Id | Mode parameters |
|---|---|
| `exercise.insert` | `"kind": "open" \| "choice" \| "trueFalse" \| "calc" \| "drawing"` (the first questions' mode); per question `"type"` (its own mode) and, for a drawing, `"drawing"` (its CAD source) |
| `exercise.addQuestion` | `"type"` (default: like the last question), `"drawing"` |
| `exercise.questions` | reports each question's `"type"` and, for a calculation, its `"line"` as shown |

## Revision history

- 2026-11-12: new: question modes; a mode per question, mixed in one quiz; calculation questions (worked with the exercise's data and variants, result line with blank and unit, result in the summary) and CAD drawing questions (the solution's drawing in a squared space, hidden in the students' copy); a quiz's data part after its title when it has no statement.
