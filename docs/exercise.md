# Exercises

> **Status:** stage 1 done: structure, sandbox, repeated questions, result marks, one results summary, page breaks, parts shown, graph paper per exercise, frontispiece, the series rules every exercise, a grip to delete an exercise · **Last updated:** 2026-11-10 · **Change:** numbering has a separator after the number (`.` `:` `-` `)` or a space); the title's paragraph style can't be changed; placeholders come back whenever a part is emptied, Backspace/Delete never join two parts; exercises in a master document's subdocuments follow the master (styles, one summary, the series); previously: in a series every exercise has the first one's page breaks, parts shown and graph paper, brought back in line automatically; a grip in the margin with Select, Properties and Delete Exercise; previously: Exercise Properties in two tabs, Exercise and Series (one tab for a lone exercise); previously: a lone exercise has no number; parts shown (solution, results) and graph paper per exercise, ruled by the series; a frontispiece; the Exercise group reorganised (Show, Frontispiece; Update Summary and Numbering left to Properties and command search); previously: page breaks

An **exercise** (Math › Exercise › New Exercise, `exercise.insert`) is a worked problem with a
rigid structure, the core of Strix as a tool for solving problems. (Working name in the brief:
"risolutore esercizi"; the tool is called *Exercise* / *Esercizio*.)

## Structure

Locked block content controls hold the parts, so editing can't break them (a part can't be
deleted; the automatic parts can't be edited):

| Part | Control tag | Lock | Content |
|---|---|---|---|
| Exercise | `strix:exercise` | can't delete | everything below |
| Title | `strix:exercise.title` | can't delete | the first paragraph, **your text**: type over its placeholder, select and replace it, delete it (the placeholder comes back); style **Exercise Title** (Titolo esercizio), numbered automatically: *Esercizio 1.*, *Esercizio 2.* … (a list in the style, the word in the document's language; documents made before get it with their next exercise). The number is the list's, never text; Exercise Info follows the title as it is typed (the summary shows only the label; prefix, format, separator and first number: see Numbering). Its paragraph style can't be changed: another style applied to it (Apply Style, Styles pane, Clear Formatting) gives Exercise Title back at once, since the style numbers the title and marks it as the title; to restyle every title, modify the Exercise Title style. Documents made before the title had its own control (the placeholder was on the exercise's, so typing over it was refused) get it as they open |
| Statement | `strix:exercise.text` | can't delete | free content: text, figures, tables, equations, drawings |
| Data | `strix:exercise.data` | read-only | the initial data in the active variant (`luce: L = 6 m`), units in the Unit style; generated from the Exercise Properties pane, absent when there are none |
| Questions | `strix:exercise.questions` | can't delete | questions (style Exercise Question), lettered by a list of their own and **nested** up to three levels: a), a.1), a.1.1) (Tab / Shift+Tab, or Add Sub-question); between them any free content: **notes** (style Exercise Note, Add Note), tables, figures |
| Solution | `strix:exercise.solution` | can't delete | a heading (Risoluzione), then each question repeated with its free working under it |
| Repeated question | `strix:exercise.prompt` | read-only | `a) …`: one per question, rebuilt automatically; the working after it stays |
| Results summary | `strix:results` | read-only | at the very end of the document, **one for all exercises** |

**Editing keeps the structure.** Backspace and Delete never join a paragraph across the edge of
a part (the statement into the title, a question into the statement, the title into the paragraph
before), and take no letters out of a placeholder shown. A statement, questions or title emptied
of everything (deleting a selection, Cut, Backspace, also over several paragraphs) is one empty
paragraph again and shows its placeholder; typing over it replaces it. A question removed leaves
no blank line behind in the solution.

Only paragraphs in the Exercise Question style are questions: the repeated questions and the
summary use their labels (`a.1) …`), indented by level; notes and other content between them
are never repeated. `exercise.insert` takes nested questions as `{"text", "level"}`.

### Language

The generated words are in the language of the text, the one its spelling dictionary checks
with: Italian, English, French, German or Spanish (English for the others). A new exercise takes
the caret's. To change it afterwards, set the language from the status bar or Review › Language
› Set Proofing Language (on a selection, or as the document's default): every exercise whose
title is now in another language rewrites the words Strix wrote for it (the frames' titles, the
placeholders still shown, the *Solution* heading), and the default numbering prefix and the
results summary follow the first exercise. Only default words change: your title, statement,
questions, working and a prefix of your own stay as written. One undo step takes it back.

## Numbering

The exercises of a document are numbered by one list, a **document setting** saved with it
(Math › Exercise › Properties shows it in the Exercise Properties pane; `exercise.numbering`):

- **Prefix**: *Esercizio* / *Exercise* (the document's language) by default, any text of your
  own (*Problema*, *Es.*), or none for the number alone; Default puts the word back.
- **Number format**: 1, 2, 3 · A, B, C · a, b, c · I, II, III · i, ii, iii.
- **Separator**: what follows the number in the titles, from a list: a period (*Esercizio 1.*,
  the default), a colon (*Esercizio 1:*), a dash (*Esercizio 1 -*), a parenthesis (*Esercizio 1)*)
  or a space alone (*Esercizio 1 Title*). The results summary shows the label without it.
- **Start at**: the first exercise's number (1 to 32767).

Every title's label (*Problema C.*) and the summary's lines (*Problema C*) follow at once, in one
undo step; it is the Exercise Title style's list, so it is saved in `.xmath` (and `.docx`) like
any list.

**A lone exercise has no number.** While the document has only one exercise, its title shows no
prefix and no number (it switches the style's list off, Word's "no numbering", `numId 0`) and the
results summary lists its results without a label (only *Variante B* heads them when it has
several variants). Adding a second exercise brings the labels back on both; deleting down to one
takes them away again.

## A series

Two or more exercises in a document are a **series**, and the series rules: every exercise has
the same configuration, the **first exercise's**: numbering (above), page breaks, parts shown and
graph paper (below). No exercise of a series can differ:

- a change made with the cursor in any exercise (or naming one, `"exercise": n`) applies to all
  of them;
- a new exercise takes the series' settings;
- an exercise that comes in different (pasted from another document, from an older file, its
  graph paper changed by hand) is brought back in line with the first one at once (`update`):
  page breaks and parts shown on its control, graph paper ruled, changed or taken away (graph
  paper you wrote on keeps its text);
- deleting the first exercise changes nothing: the next one already has the same settings.

In a series the Exercise Properties pane has two tabs: **Exercise**, the exercise at the caret
alone (its variants and data), and **Series**, the settings of every exercise (numbering, parts
shown and graph paper, page breaks before each exercise, each solution and the results summary,
frontispiece), shown wherever the cursor is. A lone exercise has no Series tab:
what applies to it (its page breaks and the summary's, parts shown, frontispiece) follows in the
Exercise tab, and numbering is left out, since a lone exercise has no number.

## Parts shown

The **solution** (its heading, the repeated questions and the working) and the exercise's lines in
the **results summary** can be hidden (Math › Exercise › Show, or Exercise Properties › Parts
Shown; `exercise.show`), for a
copy without the working, or without the answers. They become hidden text, so nothing is lost:
shown again, they come back as they were (Show/Hide ¶ shows hidden text meanwhile). A hidden
solution still gives its results to the summary. When no exercise shows its results the summary's
heading is hidden too. The settings are kept on the exercise control (a `strix:exerciseShow`
element Word ignores); other word processors see ordinary hidden text.

## Graph paper per exercise

**Graph paper** can be ruled in each exercise after the statement and the questions, before the
solution (Math › Exercise › Show › Graph Paper Before the Solution, or Exercise Properties › Parts
Shown; `exercise.grid {"on": true}`). It is an
ordinary graph paper block (docs/graph-paper.md) with the same settings: placement (to the end of
the page, whole pages, a height…, but not a page background), square size, lines, frame. Set them
in the Exercise Properties pane, with `exercise.grid`, or with the cursor in it in the Graph Paper
pane; in a series a change goes to every exercise's graph paper. Turned off, an empty block goes,
one you wrote on keeps its text.

## Frontispiece

A **frontispiece** (Math › Exercise › Frontispiece, or the Exercise Properties pane;
`exercise.frontispiece {"on": true}`) is
your own first page before the exercises: title of the series, class, date, instructions. It is
free content at the very start of the document, in a control that can't be deleted by accident
(type over its placeholder); what follows it starts a new page. A page break inside it
(Ctrl+Enter) gives it a second page, and so on. Turning it off removes it with its content, in one
undo step.

## Page breaks

Each exercise can start a new page **when another exercise comes before it** (the first one of
the document never breaks), and its **solution** can start a new page; the **results summary**
can start a new page too. Set them in the Exercise Properties pane (Series tab in a series, Exercise tab
for a lone exercise) or with `exercise.pageBreaks`. In a series every exercise has the same
settings (A series, above); a new exercise takes them.

An exercise's two settings are kept on its control (a `strix:exerciseBreaks` element Word
ignores) and shown as Page break before on its title and on its solution's heading, set again
whenever exercises are added, moved or deleted, so the condition always holds. The summary's is
Page break before on its first paragraph, kept when the summary is rebuilt. All three are
ordinary Page break before in the saved file, so other word processors break there too.

## Deleting an exercise

The parts can't be deleted one by one, so an exercise goes **whole**:

- **The grip**: with the cursor in an exercise, a handle of six dots stands in the margin at the
  left of its title. A click opens its menu: **Select Exercise** (all of it, as its tab does),
  **Exercise Properties** (the pane) and **Delete Exercise** (that exercise, whole).
- **Delete Exercise** (Math › Exercise, or right-click in the exercise; `exercise.delete`)
  removes the exercise at the cursor, or every exercise a selection covers whole.
- **Select the whole exercise and press Delete or Backspace** (or Cut, which copies it whole so
  Paste brings it back): a selection covering all of an exercise's content (from its title's
  text to the end of its last paragraph, or more: the paragraph before to the one after, Select
  All) takes the exercise with its control markers. Click the exercise's **Esercizio** tab (at
  the right above the title, shown with the cursor anywhere in the exercise) to select all of it.

The exercise goes in one undo step; an empty paragraph stays where it was (with Delete
Exercise), text sharing its first or last paragraph stays, and a deletion starting at the
title's paragraph leaves the next paragraph as it was (no exercise style left behind). The
other exercises renumber, the summary drops its lines, and it goes with the last exercise. A
selection over only part of an exercise is still refused, with a message saying how to delete
the whole one.

## In a master document

Exercises kept one per file and gathered in a master document (docs/master-documents.md) are one
series of the master, which rules as when a style is updated:

- a subdocument comes in with the styles and lists its content uses (a style the master already
  has keeps the master's look), so titles are in Exercise Title and numbered by the master's
  numbering, and questions keep their letters;
- the results summary is the master's, one for every exercise at the end of the master: the
  subdocuments' own summaries are left out (a subdocument inserted at the end goes before it);
- the series' settings (page breaks, parts shown, graph paper) are the master's first exercise's,
  on every exercise of every subdocument, also when the master opens;
- saving the master writes them back to the subdocument files, each with its own summary rebuilt
  so it still opens complete on its own; a style or the numbering (prefix, format, separator)
  changed in the master reaches every file too.

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
summary lists, for every exercise, its label alone (*Esercizio 1*, as the exercises are numbered, never the title; the variant when there are several) and each question with the value of its result
equations as calculations show it (`a) M = 30.375 kJ`: only the letter and the result, the question is in the exercise), or "risultato non indicato".

The repeated questions and the summary are rebuilt after every command that changes the
questions, titles, marks or results (with the automatic recalculation, `docs/calc.md`), and on
demand with `exercise.update` (Update Summary in command search). Adding an exercise later moves the summary back
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
| `exercise.numbering` | the numbering of every exercise (`{"prefix"?: text \| null, "format"?: "decimal" \| "upperLetter" \| "lowerLetter" \| "upperRoman" \| "lowerRoman", "start"?: n, "separator"?: "." \| ":" \| "-" \| ")" \| " "}`); without params, the current one → `{"prefix", "format", "start", "separator", "default", "example"}` |
| `exercise.pageBreaks` | page breaks (`{"beforeExercise"?, "beforeSolution"?, "beforeResults"?: bool}`; in a series every exercise); without settings, the current ones |
| `exercise.delete` | remove whole exercises (`{"exercise"?: n \| [n]}`; default: those the selection covers whole, else the one at the caret), one undo step → `{"deleted", "exercises"}` |
| `exercise.show` | the parts shown (`{"exercise"?: n, "solution"?, "results"?: bool}`; in a series every exercise) → `{"series", "solution", "results", "grid", "exercises"}` |
| `exercise.grid` | graph paper after the questions (`{"exercise"?: n, "on"?: bool}` and graph.set's parameters but `background`; in a series every exercise) |
| `exercise.frontispiece` | add or remove the frontispiece (`{"on"?: bool}`) → `{"on"}` |
| `exercise.data` | the data and variants of an exercise, with per-item errors |
| `exercise.setData` | replace the data and variants (`{"exercise"?, "data": {"items", "variants", "active"}}`) |
| `exercise.variant` | make a variant the active one (`{"exercise"?, "active"}`) |
| `view.exercisePane` | show or hide the Exercise Properties pane |

## Next

- A results table per exercise (question, symbol, value, unit) and a check against expected
  values; a summary per variant; statement data recalled into the solution as values.
- Solution steps (substitution, isolation lines) per result.

## Template

File › New offers **Calculation and exercise** right after Blank document (`file.new {"template": "calc"}`): live
formulas (a rectangular section's area and moment of inertia), then a worked exercise (a simply
supported beam) with data in two variants, a CAD drawing of the beam in its statement sized by the
span `L`, two questions worked in the solution and the results summary. It is built by running the
same commands the ribbon runs (`crates/engine/src/sample_calc.rs`).

## Revision history

- 2026-11-10: editing keeps the structure: placeholders come back when a part is emptied over several paragraphs, Backspace/Delete never join two parts nor eat a placeholder, removed questions leave no blank line; exercises in a master document follow the master (styles and lists brought in, one results summary, the series' settings, written back to the files).
- 2026-11-10: a separator after the number in the numbering (period, colon, dash, parenthesis, space; `exercise.numbering {"separator"}`, a drop-down in Exercise Properties › Series); the title's paragraph style is locked to Exercise Title.

- 2026-11-10: the series rules every exercise: page breaks too (no exercise of a series differs any more; the Exercise tab keeps only variants and data), the first exercise's settings forced on every other one however it came in, a new exercise takes the series'; the exercise's grip in the margin (Select, Properties, Delete Exercise).

- 2026-11-09: the Exercise Properties pane in two tabs: Exercise (this exercise's variants, data, page breaks) and Series (numbering, parts shown, page breaks of every exercise and of the summary, frontispiece); a lone exercise has no Series tab and shows what applies to it in Exercise.
- 2026-11-09: a lone exercise has no number or prefix (title and summary); a series rules the parts shown and the graph paper; parts shown (solution, results) as hidden text (`exercise.show`); graph paper per exercise between the questions and the solution (`exercise.grid`, Graph Paper pane follows in a series); a frontispiece of one or more pages (`exercise.frontispiece`). Math › Exercise: Show (Solution, Results, Graph Paper) and Frontispiece added; Update Summary (automatic anyway) and Numbering (the same pane as Properties) left out of the ribbon, still in command search.
- 2026-11-09: page breaks before an exercise when another comes before it, before its solution and before the results summary (`exercise.pageBreaks`, Exercise Properties pane).
- 2026-11-08: the generated words follow the proofing language: changing it (status bar, Review › Language) translates an exercise's default words, the default prefix and the summary; French, German and Spanish words.
- 2026-11-08: "Calculation and exercise" template, second in the File › New gallery, after Blank document.
- 2026-10-29: exercise numbering as a document setting (prefix, format, first number; `exercise.numbering`, Numbering in Math › Exercise and the Exercise Properties pane); the summary heads each exercise with its label alone, without the title.
- 2026-10-29: delete a whole exercise: Delete Exercise (ribbon, context menu, `exercise.delete`), Delete/Backspace/Cut over a selection covering it, a click on its tab selects it; the summary goes with the last exercise. Before, every way was refused by the locked parts.

- 2026-10-29: the title has its own control (`strix:exercise.title`), so it is typed over, replaced and deleted in place; before, a New Exercise without a title could not have its title typed over.
- 2026-10-28: exercise titles numbered automatically through their style.
- 2026-10-27: one repeated question per line with its working under it; Result Of defaults to that question.
- 2026-10-27: Exercise Properties pane: initial data and variants; data part.
- 2026-10-27: nested questions (a), a.1), a.1.1)) and notes between the questions.
- 2026-10-27: new.
