# Exercises

> **Status:** stage 1 done: structure, sandbox, repeated questions, result marks, one results summary · **Last updated:** 2026-10-27 · **Change:** working under each question

An **exercise** (Math › Exercise › New Exercise, `exercise.insert`) is a worked problem with a
rigid structure, the core of Strix as a tool for solving problems. (Working name in the brief:
"risolutore esercizi"; the tool is called *Exercise* / *Esercizio*.)

## Structure

Locked block content controls hold the parts, so editing can't break them (a part can't be
deleted; the automatic parts can't be edited):

| Part | Control tag | Lock | Content |
|---|---|---|---|
| Exercise | `strix:exercise` | can't delete | everything below |
| Title | (first paragraph) | | style **Exercise Title** (Titolo esercizio) |
| Statement | `strix:exercise.text` | can't delete | free content: text, figures, tables, equations, drawings |
| Data | `strix:exercise.data` | read-only | the initial data in the active variant (`luce: L = 6 m`), units in the Unit style; generated from the Exercise Properties pane, absent when there are none |
| Questions | `strix:exercise.questions` | can't delete | questions (style Exercise Question), lettered by a list of their own and **nested** up to three levels: a), a.1), a.1.1) (Tab / Shift+Tab, or Add Sub-question); between them any free content: **notes** (style Exercise Note, Add Note), tables, figures |
| Solution | `strix:exercise.solution` | can't delete | a heading (Risoluzione), then each question repeated with its free working under it |
| Repeated question | `strix:exercise.prompt` | read-only | `a) …`: one per question, rebuilt automatically; the working after it stays |
| Results summary | `strix:results` | read-only | at the very end of the document, **one for all exercises** |

Only paragraphs in the Exercise Question style are questions: the repeated questions and the
summary use their labels (`a.1) …`), indented by level; notes and other content between them
are never repeated. `exercise.insert` takes nested questions as `{"text", "level"}`.

The generated words follow the document's language (Italian or English).

## Data and variants (Exercise Properties pane)

Math › Exercise › Properties opens the **Exercise Properties** pane for the exercise at the caret:

- **Data**: rows of symbol, description, value and unit (`L`, luce, 4.5, m). They are the first
  parameters of the exercise's sandbox, so the solution uses `L` without defining it; each row
  is checked as the sandbox reads it and its error is shown under it.
- **Variants**: A, B, C… Each datum has one value per variant; **+** adds a variant (a copy of
  the active one), **−** removes the active one. Choosing a variant recalculates the exercise:
  data, results and summary follow, and the data part and the summary name the variant.

Data and variants are kept on the exercise control, in a `strix:exerciseData` element of its
properties (JSON) that Word ignores; `exercise.data`, `exercise.setData` and `exercise.variant`
read and change them.

## A sandbox

Each exercise is evaluated in a worksheet of its own: parameters, working units and results
defined inside stay inside, and the document's parameters are not seen inside. Two exercises
can both define `L` without interfering, and nothing from an exercise leaks into the text
around it. One step budget still covers the whole document.

## Working under each question

Each question is repeated in the solution as its own read-only line, followed by room to work
it out (text, calculations, tables, drawings). When the questions change the working stays with
its question: an edited question is rewritten in place, a new one comes in after the working of
the one before it with an empty paragraph to write in, a removed one's line goes and its working
joins the question above. (Documents from before keep their working; their repeated questions
are split into one line each.)

## Results

An equation in the solution is marked as the **result** of a question (Math › Exercise ›
Result Of, `equation.answer {"question": n}`; without `question`, the question it is worked
under), saved as `strix:answer="n"` on `m:oMath`). The
summary lists, for every exercise, its title and each question with the value of its result
equations as calculations show it (`a) M = 30.375 kJ`: only the letter and the result, the question is in the exercise), or "risultato non indicato".

The repeated questions and the summary are rebuilt after every command that changes the
questions, titles, marks or results (with the automatic recalculation, `docs/calc.md`), and on
demand with Update Summary (`exercise.update`). Adding an exercise later moves the summary back
to the end.

## Commands

| Id | Does |
|---|---|
| `exercise.insert` | insert an exercise after the paragraph at the caret (`{"title"?, "text"?, "questions"?: [..]}`) |
| `exercise.addQuestion` | add a question to the exercise at the caret (`{"text"?, "level"?: 0–2, "sub"?: bool}`) |
| `exercise.addNote` | add a note (free text, not a question) after the questions (`{"text"?}`) |
| `equation.answer` | the equation at the caret is the result of question `n` (0: none) |
| `exercise.info` | the exercise at the caret: number, title, questions with labels and levels, the equation's mark |
| `exercise.update` | rebuild the repeated questions and the summary |
| `exercise.data` | the data and variants of an exercise, with per-item errors |
| `exercise.setData` | replace the data and variants (`{"exercise"?, "data": {"items", "variants", "active"}}`) |
| `exercise.variant` | make a variant the active one (`{"exercise"?, "active"}`) |
| `view.exercisePane` | show or hide the Exercise Properties pane |

## Next

- A results table per exercise (question, symbol, value, unit) and a check against expected
  values; a summary per variant; statement data recalled into the solution as values.
- Solution steps (substitution, isolation lines) per result.

## Revision history

- 2026-10-27: one repeated question per line with its working under it; Result Of defaults to that question.
- 2026-10-27: Exercise Properties pane: initial data and variants; data part.
- 2026-10-27: nested questions (a), a.1), a.1.1)) and notes between the questions.
- 2026-10-27: new.
