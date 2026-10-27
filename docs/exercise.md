# Exercises

> **Status:** stage 1 done: structure, sandbox, repeated questions, result marks, one results summary · **Last updated:** 2026-10-27 · **Change:** new

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
| Questions | `strix:exercise.questions` | can't delete | one paragraph per question (style Exercise Question), lettered a), b)… by a list of their own |
| Solution | `strix:exercise.solution` | can't delete | a heading (Risoluzione), the questions repeated, then free content |
| Repeated questions | `strix:exercise.prompt` | read-only | `a) …`, `b) …`: rebuilt automatically from the questions |
| Results summary | `strix:results` | read-only | at the very end of the document, **one for all exercises** |

The generated words follow the document's language (Italian or English).

## A sandbox

Each exercise is evaluated in a worksheet of its own: parameters, working units and results
defined inside stay inside, and the document's parameters are not seen inside. Two exercises
can both define `L` without interfering, and nothing from an exercise leaks into the text
around it. One step budget still covers the whole document.

## Results

An equation in the solution is marked as the **result** of a question (Math › Exercise ›
Result Of, `equation.answer {"question": n}`, saved as `strix:answer="n"` on `m:oMath`). The
summary lists, for every exercise, its title and each question with the value of its result
equations as calculations show it (`a) Momento: M = 30.375 kJ`), or "risultato non indicato".

The repeated questions and the summary are rebuilt after every command that changes the
questions, titles, marks or results (with the automatic recalculation, `docs/calc.md`), and on
demand with Update Summary (`exercise.update`). Adding an exercise later moves the summary back
to the end.

## Commands

| Id | Does |
|---|---|
| `exercise.insert` | insert an exercise after the paragraph at the caret (`{"title"?, "text"?, "questions"?: [..]}`) |
| `exercise.addQuestion` | add a question to the exercise at the caret (`{"text"?}`) |
| `equation.answer` | the equation at the caret is the result of question `n` (0: none) |
| `exercise.info` | the exercise at the caret: number, title, questions, the equation's mark |
| `exercise.update` | rebuild the repeated questions and the summary |

## Next

- A results table per exercise (question, symbol, value, unit) and a check against expected
  values; exercise variants; statement data recalled into the solution as values.
- Solution steps (substitution, isolation lines) per result.

## Revision history

- 2026-10-27: new.
