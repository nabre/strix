# Exercises

> **Status:** done: three kinds (problem, open answer, multiple choice) and question modes ([question-modes.md](question-modes.md)), structure, sandbox, repeated questions, result marks, one results summary, page breaks, parts shown, graph paper per exercise, frontispiece, the series rules every exercise; the contextual Exercise and Series tabs with the Question Settings and Exercise Data panes beside the document; data as equations, variants, inline values, the variant generator; scores table, total score, grading scale, pre-flight check and assessment pack · **Last updated:** 2026-11-17 · **Change:** docs brought up to date with the code (status, the Teaching tab's groups, the commands table, Next); the history of earlier changes is under [Revision history](#revision-history)

An **exercise** (Teaching › Exercise › New Exercise, `exercise.insert`) is a worked problem with a
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
| Data | `strix:exercise.data` | editable (not deletable) | the initial data as equations in the active variant (`luce: L = 6 m`), units in the Unit style; generated from the Exercise Data pane and typed into in place, absent when there are none |
| Questions | `strix:exercise.questions` | can't delete | questions (style Exercise Question), lettered by a list of their own, **starting again at a) in every exercise** (a copy of an exercise pasted into the same document gets lists of its own too, for its questions and their options), and **nested** up to three levels: a), a.1), a.1.1) (Tab / Shift+Tab, or Add Sub-question); between them any free content: **notes** (style Exercise Note, Add Note), tables, figures |
| Solution | `strix:exercise.solution` | can't delete | a heading (Risoluzione), then each question repeated with its free working under it |
| Repeated question | `strix:exercise.prompt` | read-only | `a) …`: one per question, rebuilt automatically; the working after it stays |
| Results summary | `strix:results` | read-only | at the very end of the document, **one for all exercises**; each result is written as an equation (subscripts built up, units upright in the unit colour, as the calculations show them: *a) V<sub>B</sub> = 14.075 kN*), not a calculation; several results of one question side by side with `; `; a warning or *risultato non indicato* stays text |

**Editing keeps the structure.** Backspace and Delete never join a paragraph across the edge of
a part (the statement into the title, a question into the statement, the title into the paragraph
before), and take no letters out of a placeholder shown. A statement, questions or title emptied
of everything (deleting a selection, Cut, Backspace, also over several paragraphs) is one empty
paragraph again and shows its placeholder; typing over it replaces it. A question removed leaves
no blank line behind in the solution.

**Typing stays in the fields.** A caret beside a field (before a question's text, between it and
its points, after the points, before the title, before an option's box) types into the nearest
field, so no text lands outside one; a calculation's result line is generated whole and takes no
typing (`cmd/exercise/guard.rs`).

**Exercises never nest.** A new exercise goes after the exercise at the cursor (before the
results summary); pasting or inserting content that holds an exercise (Paste, Exercise Bank ›
Insert Copy) with the cursor in an exercise puts it on a new paragraph right after that exercise.

### Title line

The first line holds, after the number, either a **title** (bold, the default) or the
**statement itself, with no title** (`exercise.titleLine {"line": "statement"}`, Exercise
tab › General › **Remove Title**, **Show Title** to go back, or `exercise.insert {"titleLine": "statement"}`): the text
written right after *Esercizio 1.* in the body text's weight and size (`Esercizio 1.` Calcola la
velocità media…), its placeholder the statement's in a quiz as in a problem (a quiz's questions keep their own
fields under the line, so the line never repeats *Scrivi qui la domanda.* over the first one). The number keeps the title's look. Kept on the title control
(`<strix:titleLine val="statement"/>`, Word ignores it).

The statement **moves** onto the line, it is never there twice: switching to the statement, the
statement part's first paragraph (its words, their formatting, inline values) goes next to the
number and the part goes; what followed that paragraph (a figure, a table) stays in the part
below. The title is kept aside on the control (`<strix:titleLineTitle>`), not lost. Switching back
to a title, the line's text is the statement part's first paragraph again (a problem gets an empty
statement part when the line was empty, and so does a quiz with more than one question) and the title comes back. One undo
step either way. A statement part left only showing its placeholder beside a line holding the
statement (documents from before, or its rest deleted) goes. The data part follows the title
when there is no statement part.

A quiz's **only question goes on the line** (`question_line.rs`): with no title, a quiz with a
single question and nothing written on the line has that question right after the number
(*Esercizio 1.* Scrivi qui la domanda.) instead of the statement's placeholder over a question
showing its own — for every mode (open answer, calculation, drawing, multiple choice,
true/false, text to complete, matching, worked example). The title line and the question's
paragraph become one: the title control stays, empty, and the question (its text, points and a
true/false question's boxes) follows it in the body text's weight and size; the answer (options,
space) stays below. Typing on the line writes the question. A true/false question there names
its boxes (`V ☐  F ☐`), as no header line can go above the number. The question goes back to a
paragraph of its own, lettered again, as soon as that no longer holds: a second question, a title
on the line, a statement on the line, data shown. A problem keeps its statement on the line.

Only paragraphs in the Exercise Question style are questions: the repeated questions and the
summary use their labels (`a.1) …`), indented by level; notes and other content between them
are never repeated. `exercise.insert` takes nested questions as `{"text", "level"}`.

### Language

The generated words are in the language of the text, the one its spelling dictionary checks
with: English, Italian, German (Swiss German `gsw` too) or French, the languages Strix's interface
is offered in (`crates/engine/src/cmd/exercise/words.rs`). Tags are read by their language, two
or three letters (`it-CH`, `ita`, `ger`). A language without its own words (Spanish, Latin,
Romansh…) keeps the words the exercise already has: an Italian exercise whose text is set to
Latin stays Italian, and a new exercise there takes the document's exercises' words (else the
document's language's, else the interface's, else English). A new
exercise takes the caret's language. To change it afterwards, set the language from the status bar or Review › Language
› Set Proofing Language (on a selection, or as the document's default): every exercise whose
title is now in another language rewrites the words Strix wrote for it (the frames' titles, the
placeholders still shown, the *Solution* heading), and the default numbering prefix, the
results summary and the scores table (its header and the default *Points* criterion) follow the
first exercise. Only default words change: your title, statement,
questions, working and a prefix of your own stay as written. One undo step takes it back.

## Kinds of exercise

New Exercise makes one of three kinds (`exercise.insert {"kind"}`); the kind is kept on the
exercise control (a `strix:exerciseKind` element Word ignores). Each question of a quiz has its
own **mode** (open answer, multiple choice, true/false, **calculation**, **CAD drawing**), so one
quiz mixes them: see [question-modes.md](question-modes.md); `"kind": "calc"` and
`"kind": "drawing"` make a quiz whose first questions start in that mode. The New Exercise menu
offers problem, open answer, multiple choice, true or false, inline completion and table to
complete; calculation, worked example, matching and CAD drawing are question types added to an
exercise (Add Question), still available to scripts as `kind`.

| Kind | `kind` | Made of |
|---|---|---|
| **Problem** | `problem` (default) | title, statement, data, questions, solution, results (everything above) |
| **Open answer** | `open` | title, then each question followed by a space to answer in |
| **Multiple choice** | `choice`, `trueFalse` | title, then each question followed by its options A), B)… |

The last two are **quizzes**: the exercise *is* its questions. With one question a statement is
optional and absent by default (an exercise can be just the question); give one with `"text"`, or its
placeholder with `"statement": true`. **With more than one question a quiz has a statement**, like a
problem: the text introducing its questions, under the title (its placeholder until typed over; a
quiz that gets a second question, or a document from before, is given one), or next to the number
when the first line holds it, moving with Show Title / Remove Title. **A lone question has no letter** (like a lone
exercise has no number); a second question letters both a), b). There is no Solution part: the
answers are in the exercise, next to their questions.

Each question is a paragraph in the Exercise Question style: its text in an inline control
(`strix:exercise.item`, with a placeholder until typed over), then its **points**, generated
on the right margin (a right-aligned positional tab, whatever the question's indent), with dots
left for the score the student gets: `.......... / 2 pt`, `.......... / 1,5 pt` (`P.` in German;
nothing without points). Documents written with the older ` (2 punti)` form are brought to this
one the next time the exercises are refreshed.
After it comes its **answer** (`strix:exercise.answer`), a block control that can't be deleted
and keeps the question's settings (a `strix:answerSpace` element Word ignores):

- **Open answer**: a space of a given **height in centimetres** (0.5–25, 4 cm by default),
  **squared** (graph paper, 5 mm squares by default, any side from 1 to 100 mm) or **blank**. It is
  a graph paper block of that height (docs/graph-paper.md; blank is its `none` line style), so the
  answer is written in it, on the squares, in the Exercise Answer style (dark blue).
- **Multiple choice**: the options, lettered A), B), C)… by a list of their own, in the Exercise
  Option style, each starting with a **check box** (`strix:exercise.box`, a Word check box
  control). The box is ticked on the **correct** options; more than one can be correct. Click a
  box to tick or clear it, or set them with `exercise.questions {"correct": ["A", "C"]}`. Enter
  at the end of an option adds one (it gets its box). Four empty options by default; **True/False**
  (`trueFalse`) gives two, *Vero* and *Falso* (in the exercise's language).

**The answers show or hide with the Solution** (Layout › Show in the Exercise or Series tab, `exercise.show
{"solution"}`; the series rules it like a problem's solution): hidden, an open answer's text is
hidden text and **its space keeps its height**; every box of a multiple choice is empty (which
options are correct is kept). So one document gives the students' copy and the corrected one.

**Points.** The points of an exercise's questions add up to the exercise's points (`exercise.questionPoints` sets or clears points on questions of both quizzes and problems, inserting `strix:exercise.points` at the end of a problem question when set); a **scores
table** takes them as the exercise's row total, at once, for every exercise whose questions carry points (otherwise every criterion is typed in; see Scores table). Even with a single exercise and a single criterion (`1×1`), the scores table keeps its Total row (`__ / Max`). The results summary lists, for each multiple choice, its letter and
the correct options (`a) A, C`); every open answer gives a line saying so (`b) domanda a risposta
aperta`, in the exercise's language), also when the exercises are all open answers, so that no
part is forgotten when correcting.

`exercise.questions` reads an exercise's questions (text, kind, points, space, calculation line, CAD scale, options and correct
ones, the total) and changes one's settings: `{"question": 2, "points": 3, "height": 6, "grid":
false}`. `exercise.questionMode` converts an existing quiz question's mode in place while preserving its question paragraph, points, space settings and working/options. `exercise.addQuestion` adds a question like the last one (same kind of answer, points
and space), its settings and options given as in `exercise.insert`.

## Numbering

The exercises of a document are numbered by one list, a **document setting** saved with it
(the Series tab's Numbering group; `exercise.numbering`):

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

Two **contextual ribbon tabs** hold every setting (there is no Exercise Properties pane any
more; the Question Settings and Exercise Data panes open from the Exercise tab): **Exercise**, shown
with the cursor in an exercise, for that exercise alone (questions, first line, duration,
variants and data), and **Series**, shown whenever the document has more than one exercise,
wherever the cursor is, for every exercise (the exercises to go to or delete, numbering, parts
shown and graph paper, page breaks before each exercise, each solution and the results summary,
frontispiece). A lone exercise has no Series tab: what applies to it (its page breaks and the
summary's, parts shown, graph paper, frontispiece) is in the Exercise tab's Layout group, and
numbering is left out, since a lone exercise has no number; in a series the Exercise tab's
**Series Settings** brings up the Series tab.

## Parts shown

The **solution** (its heading, the repeated questions and the working) and the exercise's lines in
the **results summary** can be hidden (Layout › Show in the Exercise or Series tab; `exercise.show`), for a
copy without the working, or without the answers. They become hidden text, so nothing is lost:
shown again, they come back as they were (Show/Hide ¶ shows hidden text meanwhile). A hidden
solution still gives its results to the summary. When no exercise shows its results the summary's
heading is hidden too. Hidden parts take no room on the page: a hidden solution that starts a
new page (Page breaks) starts none while it is hidden. The settings are kept on the exercise control (a `strix:exerciseShow`
element Word ignores); other word processors see ordinary hidden text.

**The text goes on after the exercises.** An ordinary paragraph always follows the last exercise,
before the results summary (which stays last), as Word keeps one after a block content control:
with the solution and the results hidden nothing else would follow the questions. Down or a click
reaches it; Ctrl+End lands in the summary, which takes no text, so typing there writes in that
paragraph; Enter makes more. Deleted, it comes back; a new exercise inserted from the end of the
document takes it, so none is left between two exercises (`cmd/exercise/structure.rs`).

## Graph paper per exercise

**Graph paper** can be ruled in each exercise after the statement and the questions, before the
solution (Layout › Show › Graph Paper Before the Solution in the Exercise or Series tab; `exercise.grid {"on": true}`). It is an
ordinary graph paper block (docs/graph-paper.md) with the same settings: placement (to the end of
the page, whole pages, a height…, but not a page background), square size, lines, frame. Set them
in the Exercise or Series tab's Graph Paper panel, with `exercise.grid`, or with the cursor in it in the Graph Paper
pane; in a series a change goes to every exercise's graph paper. Turned off, an empty block goes,
one you wrote on keeps its text.

## Frontispiece

A **frontispiece** (Teaching › Exercise › Frontispiece, or Layout › Frontispiece in the Exercise or Series tab;
`exercise.frontispiece {"on": true}`) is
your own pages before the exercises: title of the series, class, date, instructions. It is ended
by a **Frontispiece Break**: a next-page section break marked as the frontispiece's
(`strix:frontispiece="1"` on its `w:sectPr`), shown with Show/Hide ¶ like any section break, with
its own name. Everything before it is the frontispiece: free text with no frame and no
placeholder, as many pages as you write (page breaks, more paragraphs). Turning it on puts an
empty paragraph ending in the break at the start of the document, the caret in it; what follows
the break starts a new page. Being a section of its own, it can have its own headers and footers
and page numbering. Turning it off removes it with its content, in one undo step. Documents from
before, whose frontispiece was a control with a placeholder, get the break as they are next
updated (the control goes, and the placeholder if still shown). Word sees an ordinary section
break.

## Page breaks

Each exercise can start a new page **when another exercise comes before it** (the first one of
the document never breaks), and its **solution** can start a new page; the **results summary**
can start a new page too. Set them in Layout › Page Breaks (Series tab in a series, Exercise tab
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
  **Exercise Properties** (brings up the Exercise tab) and **Delete Exercise** (that exercise, whole).
- **Delete Exercise** (the Exercise tab, or right-click in the exercise; `exercise.delete`)
  removes the exercise at the cursor, or every exercise a selection covers whole.
- **Select the whole exercise and press Delete or Backspace** (or Cut, which copies it whole so
  Paste brings it back): a selection covering all of an exercise's content (from its title's
  text to the end of its last paragraph, or more: the paragraph before to the one after, Select
  All) takes the exercise with its control markers. Click the exercise's **Esercizio** tab (at
  the right above the title, shown with the cursor anywhere in the exercise) to select all of it.

- **Series tab › Exercises › Delete Exercise** lists every exercise of the series to delete one.

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
- the series' settings (page breaks, parts shown, graph paper), the numbering and the styles are
  **the master's**, on every exercise of every subdocument: kept when the master opens again and
  its files are read back, and when a subdocument comes in before the first exercise;
- the master has its own frontispiece; a subdocument's (a series called up in another master) is
  left out, like its summary;
- the files stay as they were thought out: saving the master writes back only the text edited in
  a subdocument, and the file keeps its own styles, numbering, series' settings and frontispiece,
  with its own summary rebuilt so it still opens complete on its own. Changing a setting, a style
  or the numbering in the master is no edit of the files.

So one file per exercise makes an archive, and a master makes a series out of them to hand out
(with or without the solutions, its own page breaks and frontispiece); another master can call up
a whole series and lay it out its own way, and none of it changes the files.

## Scores table

A **scores table** (Teaching › Scores › Scores Table, `exercise.scores {"on": true}`) lists the
points of each exercise, to hand out with a test. It goes after the caret's paragraph, outside the
exercises (in the frontispiece, say):

| Exercise | IMP | | MOD | | CAL/RIS | | Total | |
|---|---|---|---|---|---|---|---|---|
| 1 | | 2 | | 2 | | 1 | | 5 |
| 2 | | 2 | | 2 | | 1 | | 5 |
| Total | | 4 | | 4 | | 2 | | 10 |

- the **rows are the document's exercises**, gathered by themselves and numbered as they are (a
  new exercise adds its row, a deleted one takes it away). The points stay with their
  exercise: deleting one (Delete Exercise, or Delete, Backspace or Cut over a selection covering
  it) takes its row and the others keep theirs, a new exercise (`exercise.insert`) or one pasted in gets an
  empty row where it stands, and undo brings the points back with the exercise. A worked
  example made an exercise again gets its row by its number;
- each **criterion** (IMP, MOD…) has **two columns**: one left empty, for the points given on the
  sheet, and one for the points the exercise is worth, typed in (`2`, `1,5`). Add Criterion adds
  both (`exercise.scores {"add": "MOD"}`); its name is typed in the header (or `"rename"`);
  Remove Criterion takes one away with its points (`"remove": n`);
- with **more than one criterion** a **Total column** adds each row up across; with **more than
  one exercise** a **Total row** adds each column up down, and the grand total. They follow at
  once as points are typed.

It is a table in a content control (`strix:scores`) that can't be deleted by accident; Scores
Table again (`"on": false`) removes it, in one undo step. Points can be set by script too:
`exercise.scores {"exercise": 1, "criterion": 2, "points": 2}`. Word sees an ordinary table.
Its grand total is the document's **Score** archive property ([archive.md](archive.md)).

**Rows linked to the question points.** When an exercise's questions carry points, their sum is
its row's total, and the criteria are its partials, which always add up to it: the partials
typed in the criteria before the last are kept (each cut down to what is left of the total, never
negative), the **last criterion takes what remains**. With one criterion it holds the whole total;
a criterion added later starts at what is left (0 when the others already make the total);
changing a question's points changes the last partial. The last criterion of a linked row is
computed, so setting it by script is refused. Exercises whose questions carry no points keep
every criterion as typed, as a plain grid.

## Total score

**Total Score** (Teaching › Scores, `exercise.totalScore {"on": true}`) adds, after the caret's
paragraph, a right-aligned line where the student's score is written against the assessment's
maximum: `Punteggio: .......... / 20 pt` (in the exercises' language). It is a locked inline control
(`strix:scoreTotal`) kept up to date with the other automatic parts. The maximum is **computed**
when the document has points — the scores table's grand total, else the exercises' question points
added up — else the one **typed in** (`"points": 30`, kept on the control, `null` clears it), else
dots to fill in by hand (`/ ..... pt`). Total Score again (`"on": false`) removes it.

## The Teaching tab

The **Teaching** tab (Didattica) gathers what is for teaching: the **Exercise** group (New
Exercise with its menu of kinds, **Exercise Tools**, which brings up the Exercise tab or, outside
the exercises, the Series tab, and Frontispiece; everything else about an exercise is on the
contextual tabs below), **Archive** (Exercise Bank, [archive.md](archive.md)), **Scores** (Scores
Table, Add Criterion, Remove Criterion, the **Grading Scale** menu, Total Score), **Assessment**
(Check Assessment, Export Pack: [below](#grading-scale-strixgrading-variant-generator-pre-flight-check--assessment-pack))
and **Graph Paper** (also in Insert › Pages). Inline Expression is not on the ribbon
(`exercise.insertExpr` stays for scripts and `{{expr}}` templates).

## The Exercise and Series tabs

With the cursor in an exercise the ribbon shows the **Exercise** tab (Teaching › Exercise ›
Exercise Tools, the grip's Exercise Properties and `view.exercisePane` bring it up; there is no
pane). First come the groups every type of exercise has (problem, open answer, multiple choice,
true or false, inline completion), then the ones only some types of question use, greyed out
unless the exercise has a question of such a type (the commands stay on for scripts):

- **General**: **Remove Title** / **Show Title** (one button: with a title on the first line it removes it and the text goes next to the number, else it brings the title back), **Duration** in minutes, and
  the exercise's type (Problem; a quiz whose questions are all of one type is named after it,
  True or False, Inline Completion…; else Open Answer or Multiple Choice).
- **Questions**: **Add Question** (in a quiz a split button: the menu adds a question of a type;
  a problem's questions have no type), **Add Sub-question**, **Add Note**, and **Question
  Settings**, which shows or hides the Question Settings pane beside the document
  (`view.questionsPane`; it follows the cursor from exercise to exercise and stays open while
  you write) with one row per question (label, the start of its text) carrying **its own
  settings on the row** — a quiz question's type and every question's and sub-question's
  points, each editable where it is listed; sub-questions are indented under their question
  and the question the cursor is in is highlighted. A quiz question has more under its row,
  shown or hidden with a click on the row and opened for the question the cursor moves into
  (answer space height, squared, square
  size; correct options as check boxes with their text, shuffling, teacher notes; a
  calculation's result, Go to working and tolerance; a drawing's scale and Open in CAD editor;
  matching pairs).
- **Layout** (a lone exercise; in a series **Series Settings** brings up the Series tab):
  **Show** (solution, results, graph paper), **Page Breaks**, **Frontispiece** and **Graph
  Paper**, a panel with the graph paper's settings.
- **Exercise**: **Select Exercise** and **Delete Exercise**.
- **Calculation**: **Result Of** (on for a problem and for calculation and worked example
  questions), then the active **Variant**, **Add Variant**, **Remove Variant**, then **Data**, which shows or hides
  the Exercise Data pane beside the document (`view.dataPane`; it follows the cursor from
  exercise to exercise and stays open while you write: the variants as buttons, with add and
  remove; one row per datum: its equation in the active variant and its description), and
  **Generate Variants**, a panel under the button (on for those and for
  inline completion questions, whose blanks can be calculated, and whenever the exercise
  already has data). While the exercise has several variants, its tab on the page names the
  active one (`Exercise · Variant B`): the printed exercise never names it.
- **Inline Completion**: **Hide Words** (on with an inline completion question).

With more than one exercise the **Series** tab shows, wherever the cursor is: **Exercises** (Go
To Exercise, Previous and Next Exercise, Delete Exercise with the list), **Numbering** (prefix,
number format, separator, first number, an example) and **Layout** (the same as above, for every
exercise of the series, with a page break before each exercise too).

A panel stays open while it is used (its lists and boxes count as inside): a click outside it,
its button again or Esc closes it.

## Data and variants

- **Data**: each datum is an equation, `L = 4.5 m`, with a description (`luce`), written whole
  in the Exercise Data pane and in the exercise's data part on the page: the symbol, the value in the
  active variant and the unit are told apart from it (the unit is shared by the variants, a value
  that isn't a number, `2*a`, stays whole). They are the first parameters of the exercise's
  sandbox, so the solution uses `L` without defining it; each datum is checked as the sandbox
  reads it and its error is shown under it. `exercise.setData` takes `"equation"` on an item.
- **Editing on the page**: the data part (`luce: L = 4.5 m`, one line per datum) can be typed
  into like text. When the caret leaves it, its lines are the data: a changed line changes its
  datum (in the active variant), a new line is a new datum (the same value in every variant), a
  line deleted removes its datum; then the part is rebuilt (the unit in the Unit style). While
  the caret is in it, the part is left as typed. The description is what comes before the last
  colon with an equation after it, so it may hold an `=` of its own (`velocità = alta: v = 3
  m/s`), and a colon in the value (`s = 1:100`) is the value's.
- **What the data part lists** regulates itself: the data the exercise doesn't already show
  (below); there is no switch to hide it.
- **Variants**: A, B, C… Each datum has one value per variant; **+** adds a variant (a copy of
  the active one), **−** removes the active one. Choosing a variant recalculates the exercise:
  data, results and summary follow; the summary names the variant, and the exercise's tab on the
  page shows it while designing.

Data and variants are kept on the exercise control, in a `strix:exerciseData` element of its
properties (JSON) that Word ignores; `exercise.data`, `exercise.setData` and `exercise.variant`
read and change them.

## Inline values

`{{R}}` typed in an exercise's text (or `{{R -> cm}}`, `{{2*R}}`) becomes an **inline value**
(`strix:expr`): the expression evaluated in the exercise's sandbox, its unit in the Unit style,
recalculated with the variant. Its text is never typed into, but it is deleted whole: Backspace
just after it, Delete just before it, either key inside it, or a selection ending inside it
takes all of it (Undo brings it back). Values saved before they could be deleted are unlocked
when the document is recalculated.

An inline value shows what its names are worth **where it stands**: after `L = 2 m` it shows
2 m even if `L := 5 m` comes later. A name the document defines only further on (a
multiple-choice option recalling the result the solution works out) is read where it is defined.
A template typed and turned into a value, or a value rewritten by a recalculation, leaves the
caret where it was in the text, never inside the value.

`exercise.insertExpr` replaces the selection, leaves the caret after the value (so the next
insertion is a new value, not a change of this one), reads names copied in mathematical italic
(`𝑅`) as plain letters, and returns what the value shows and its error at once
(`{"expr", "shown", "error"}`). `calc.insert` likewise returns the statement as calculated
(`shown`).

Right-click on a value: **Inline Value** shows its expression (edit it, Enter applies), how it is
shown — **Value and Unit** (`1.5 m`), **Symbol = Value** (`R = 1.5 m`, the symbol in italics) or
**Number Only** (`1.5`) — and **Delete Value**. Scripts use `exercise.inlineValue`; the way it is
shown is kept on the control (`strix:exprShow`, absent for the value and unit).

**Data already shown are not listed again**: a datum whose value the exercise's title,
statement or questions show — an inline value recalling it (`{{R}}`, `{{R -> cm}}`, in any of
the three ways) or a drawing there whose annotation recalls it (`{{R}}`, `{{R:value}}`,
`{{R:both}}`; not `{{R:symbol}}`) — has no line in the data part, and a data part left empty is
removed. A recall in the solution doesn't count. Deleting the value brings the line back.

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

By default, the last live calculation in each question's working gives its **result**. In a
problem, **Result Of** (Exercise › Calculation, `equation.answer {"question": n}`) lets you
select specific equations. The results summary copies the final value shown in the solution,
including its unit and trailing zeros.

Open the question in **Question Settings** to choose a result unit, decimal places, a rounding
step in that unit and its direction. For example, `79 kN` with two decimal places is shown as
`79.00 kN` in both places. A step of `0.05` in metres with upward rounding shows `2.5333 m` as
`2.55 m`. If the step needs more places, the result keeps those places so the rounded value can
be shown exactly. Empty fields inherit the calculation's format. Enter `none` to disable an
inherited step. The settings follow the question if another calculation becomes its final one.

The same settings can be written with
`exercise.questions {"exercise": 1, "question": 1, "resultFormat": {"unit": "m", "decimals": 2, "step": "0.05", "stepMode": "up"}}`.
Use `resultFormat: null` to restore the inherited format.

The repeated questions and the summary are rebuilt after every command that changes the
questions, titles, marks or results (with the automatic recalculation, `docs/calc.md`), and on
demand with `exercise.update` (Update Summary in command search). Adding an exercise later moves the summary back
to the end.

## Commands

| Id | Does |
|---|---|
| `exercise.insert` | insert an exercise after the paragraph at the caret (`{"kind"?: "problem" \| "open" \| "choice" \| "trueFalse", "title"?, "text"?, "questions"?: [..]}`) |
| `exercise.addQuestion` | add a question to the exercise at the caret (`{"text"?, "level"?: 0–2, "sub"?: bool}`) |
| `exercise.questions` | a quiz's questions, or one question's settings (`{"question"?, "points"?, "height"?: cm, "grid"?, "spacing"?: mm, "correct"?}`) → `{"exercise", "kind", "questions", "total"}` |
| `exercise.addNote` | add a note (free text, not a question) after the questions (`{"text"?}`); disabled until the exercise has a question |
| `equation.answer` | the equation at the caret is the result of question `n` (0: none) |
| `exercise.info` | the exercise at the caret: number, title, questions with labels and levels, the equation's mark |
| `exercise.update` | rebuild the repeated questions and the summary |
| `exercise.numbering` | the numbering of every exercise (`{"prefix"?: text \| null, "format"?: "decimal" \| "upperLetter" \| "lowerLetter" \| "upperRoman" \| "lowerRoman", "start"?: n, "separator"?: "." \| ":" \| "-" \| ")" \| " "}`); without params, the current one → `{"prefix", "format", "start", "separator", "default", "example"}` |
| `exercise.pageBreaks` | page breaks (`{"beforeExercise"?, "beforeSolution"?, "beforeResults"?: bool}`; in a series every exercise); without settings, the current ones |
| `exercise.delete` | remove whole exercises (`{"exercise"?: n \| [n]}`; default: those the selection covers whole, else the one at the caret), one undo step → `{"deleted", "exercises"}` |
| `exercise.show` | the parts shown (`{"exercise"?: n, "solution"?, "results"?: bool}`; in a series every exercise) → `{"series", "solution", "results", "grid", "exercises"}` |
| `exercise.grid` | graph paper after the questions (`{"exercise"?: n, "on"?: bool}` and graph.set's parameters but `background`; in a series every exercise) |
| `exercise.scores` | the scores table: insert or remove it (`{"on"}`), add, remove or rename a criterion (`{"add"}`, `{"remove": n}`, `{"rename": n, "name"}`), set points (`{"exercise": n, "criterion": n, "points"}`) → `{"on", "criteria", "points", "exercises"}` |
| `exercise.frontispiece` | add or remove the frontispiece (`{"on"?: bool}`) → `{"on"}` |
| `exercise.data` | the data and variants of an exercise, with per-item errors |
| `exercise.setData` | replace the data and variants (`{"exercise"?, "data": {"items", "variants", "active"}}`) |
| `exercise.variant` | make a variant the active one (`{"exercise"?, "active"}`) |
| `exercise.inlineValue` | the inline value at the caret (`{"expr", "show", "shown", "blank"}`); change it with `{"expr"?, "show"?: "value" \| "both" \| "number"}`, delete it with `{"remove": true}` |
| `exercise.titleLine` | the first line holds a title or the statement (`{"line": "title" \| "statement"}`; Remove Title / Show Title) |
| `exercise.questionMode` | change a quiz question's mode in place, keeping its text, points and answer settings ([question-modes.md](question-modes.md)) |
| `exercise.questionPoints` | set or clear a question's points, in quizzes and problems |
| `exercise.blank` | Hide Words: an inline completion's hidden word ([question-modes.md](question-modes.md)) |
| `exercise.insertExpr` | insert an inline value (`{{expr}}`) at the caret |
| `exercise.goWorking` | the caret at the end of question n's working space in the solution (`{"question", "exercise"?}`; an empty paragraph is made when there is none) |
| `exercise.generateVariants` | generate clean variants from domains and constraints (below) |
| `exercise.totalScore` | the total score line (`{"on"?, "points"?}`) |
| `exercise.grading` | the grading scale table and its settings (below) |
| `exercise.check` | the pre-flight check of the assessment (below) |
| `file.exportPack` | export the assessment pack (below) |
| `view.gradingPane` | show or hide the Grading Scale Properties pane |
| `view.checkPane` | show or hide the pre-flight check pane (Teaching › Assessment › Check Assessment) |
| `view.questionsPane` | show or hide the Question Settings pane (`{"value"?: bool}`): the questions of the exercise at the cursor, each with its settings on its row |
| `view.dataPane` | show or hide the Exercise Data pane (`{"value"?: bool}`): the data and variants of the exercise at the cursor |
| `view.exercisePane` | bring up the Exercise tab (the Series tab with the cursor outside the exercises) |

## Next

- A results table per exercise (question, symbol, value, unit) and a check against expected
  values; a summary per variant in the document (the assessment pack's Teacher Key already has a
  synoptic table of every variant); statement data recalled into the solution as values.

(Worked steps under a result, listed here before, are calc's Show Steps: [calc.md](calc.md).)

## Grading scale (`strix:grading`), variant generator, pre-flight check & assessment pack

- The grading table has one row per grade of the grid — from the lowest to the highest grade by the grade step (e.g. 1–10 by 0.5) — and one pair of columns: the points that earn the grade, then the grade. Each grade's points are worked out exactly from the curve (decimals included) and rounded to a multiple (`pointsRound`, e.g. 0.25 or 0.5; hundredths when unset); the grade is earned from its points (included) to the next grade's points (excluded), written as a half-open interval `[12; 14[` (the bracket of the excluded end turned outward) (`;` between the ends with decimal commas, `,` otherwise). The lowest grade starts at 0, the highest is earned with full points (shown alone), and a grade no points earn once rounded (the multiple coarser than its share) shows a dash. The numbers line up: each end of the interval and the grade sit in sub-columns split at the decimal separator (whole part right-aligned, fraction left-aligned, the `;` and the brackets in columns of their own) with no borders between them, so nothing shows on the page or in print. The table is as narrow as its content and centred, and a line under it says how the points are counted ("Punti arrotondati al multiplo di 0,25."). Laying it out side by side is done with page columns; its heading row repeats on every page the table runs onto.

- **Grading Scale Properties** (`Teaching › Scores › Grading Scale › Grading Scale Properties`, `view.gradingPane`): a pane with every property — preset, lowest and highest grade, grade step, curve, pass threshold (%) and pass grade, maximum points from the document's points or fixed, the multiple the points are rounded to, who sees it — and a preview of the scale (pass grades green); Insert or Remove the table. Each change is one `exercise.grading`.
- **Grading Scale** (`Teaching › Scores › Grading Scale`, a menu: Show Grading Scale, the presets by country, Grading Scale Properties; `exercise.grading`): inserts or updates a `strix:grading` conversion table bound automatically to the `strix:scores` grand total (`maxPoints: null`) or an explicit cap, with school presets (`it-10`, `ch-6`, `de-6` inverted 6→1, `fr-20`, `percent`, `custom`), `linear` or `piecewiseLinear` curve through `(passRatio, passGrade)`, grade step (`step`: `0.1`, `0.25`, `0.5`, `1.0`…), points rounded to a multiple (`pointsRound`, `null` = hundredths), and `visibility` (`"teacher"` or `"both"`).
- **Continuous validation across all variants** (`exercise.data` `"variantErrors"`): evaluates every variant `A..Z` in memory (sandbox calculations, CAD `Drawing::resolve` and degenerate geometry, and duplicate evaluated multiple-choice options) without mutating the document, showing `⚠` badges and tooltips in the Exercise tab's Variant list.
- **Controlled variant generator** (`exercise.generateVariants`, Exercise tab › Calculation › *Generate Variants*): deterministically generates `count` clean variants within a bounded budget (`5,000` candidate evaluations) from discrete lists or `[min, max, step]` domains and result constraints (`maxDecimals`, `positiveResults`, `distinctFinalResults`, and `extra` predicates such as `L > b`).
- **Inline parametric expressions** (`exercise.insertExpr`, `strix:expr`, `{{expr}}` / `{{expr -> unit}}`): evaluated live in the exercise's sandbox (in statements, questions, and multiple-choice options) with units styled in `Unit`.
- **Assessment pre-flight check** (`Teaching › Assessment › Check Assessment`, which shows or hides the check pane, `view.checkPane`; `exercise.check`): pure diagnostic command inspecting all variants `A..Z`, choice keys and option collisions, linked subdocuments, unused initial data, missing solutions, score totals, laid-out answer heights, durations, and variant page counts, with click-to-jump positions.
- **Assessment Pack batch export** (`Teaching › Assessment › Export Pack`, `file.exportPack`): exports in one step (folder or `.zip` on desktop, `.zip` download on `strix-web`) the Student PDF per variant (`Show = false`), the DSA Student PDF per variant (`design.dsa` active), and the Teacher Key PDF (`Show = true`, worked solutions, grading scale, and final **Multi-Variant Synoptic Table**).

## Templates

File › New offers **Sample test** right after Blank document (`file.new {"template": "test"}`):
a test for a class (title, subtitle, instructions) with a scores table and three exercises with
their solutions — a physics problem (a car accelerating from rest) and a geometry problem (a right
triangle, with a CAD drawing sized by its legs) with data in two variants, and a quiz mixing a
multiple choice, a true/false and a calculation question with points. It is also the document the
desktop app's `--sample` and the web app's `?sample` (the website's "Try it in the browser") open
(`wordcraft_engine::sample_test`). WordCraft's studio handbook stays in the gallery as
"Studio handbook (sample)".

Then **Calculation and exercise** (`file.new {"template": "calc"}`): live
formulas (a rectangular section's area and moment of inertia), then a worked exercise (a simply
supported beam) with data in two variants, a CAD drawing of the beam in its statement sized by the
span `L`, two questions worked in the solution and the results summary. It is built by running the
same commands the ribbon runs (`crates/engine/src/sample_calc.rs`), as the sample test is.

## Revision history

- 2026-11-25: `exercise.goWorking {question}` puts the caret in a question's working space (scripts couldn't find it: the repeated question is read-only and its text appears twice). Inline values read their names where they stand (they read the end of the document, so a name redefined later showed its later value), a name defined only further on still read there; the caret no longer lands inside a value when `{{…}}` is turned into one or a value is rewritten. `exercise.insertExpr` replaces the selection, puts the caret after the value (the next one went into it, changing its formula: "no inline value" after), reads math-italic names and returns `shown`/`error`; a datum given in °C is recalled in °C, units like `1/K` and `1/m³` work in the data.

- 2026-11-17: Docs brought up to date with the code: the status line, the Teaching tab's groups (Exercise with Exercise Tools, Archive, Scores, Assessment, Graph Paper), the commands table completed (`exercise.titleLine`, `questionPoints`, `blank`, `insertExpr`, `generateVariants`, `totalScore`, `grading`, `check`, `file.exportPack`, `view.gradingPane`, `view.checkPane`), the Next list (worked steps are calc's).

- 2026-11-17: fixes found in a review: the scores table's points stay with their exercise when another is deleted or inserted (they followed the row's number); Add Note is disabled until the exercise has a question; a datum's description may hold an `=`; the grading scale's `effectiveMaxPoints` is the maximum the scale is built on (0.5 to 10 000); Hide Words (`exercise.blank` without `expr`) refuses a selection over 200 characters, never hiding a part of it, and one in a question repeated in the solution.
- 2026-11-16: the Question Settings pane has every question's and sub-question's settings on its own row (a quiz question's type, the points), sub-questions indented, instead of one card at the end for the question chosen; what a quiz question's type has opens under its row.
- 2026-11-16: Exercise › Calculation › Data opens the Exercise Data pane beside the document (`view.dataPane`, docked on the right) instead of a drop-down panel under the button; it follows the cursor and stays open while you write.
- 2026-11-16: Exercise › Questions › Question Settings opens a pane beside the document (`view.questionsPane`, docked on the right like the other properties panes) instead of a drop-down panel under the button; it follows the cursor and stays open while you write.
- 2026-11-16: the questions' letters start again at a) in every exercise also in a copy pasted into the same document, which went on with c), d) (and its options with C), D)) by sharing the lists of the exercise it was copied from; documents holding such copies are put right as they open.
- 2026-11-16: every open answer gives a line in the results summary (*domanda a risposta aperta*), also when an exercise or the whole document has open answers alone (no summary before), so that no part is forgotten when correcting.
- 2026-11-15: the grading table lists the grades of the grid (lowest to highest by the step), each with its points as a half-open interval `[from; to[`, the points worked out exactly and rounded to a multiple (`pointsRound`, Grading Scale Properties › Points); the old "a row every n points" (`pointsStep`) is gone.
- 2026-11-15: the grading table's intervals and grades are aligned at the decimal separator in borderless sub-columns; the table is as narrow as its content and centred; a note under it gives the multiple the points are rounded to.
- 2026-11-15: data as equations (`L = 4.5 m`) in the Data panel (equation and description) and editable in the data part on the page; the Show data switch is gone (the part lists what the exercise doesn't already show); the active variant on the exercise's tab instead of the data heading; variants as buttons in the Data panel; the grading table is never split into side-by-side columns and repeats its heading row; New Exercise no longer offers Calculation, Worked Example, Matching and CAD Drawing Question (still question types).
- 2026-11-14: the Exercise Properties pane gives way to two contextual ribbon tabs: **Exercise** (with the cursor in an exercise: questions with a Question Settings panel, first line and duration, variants with Data and Generate Variants panels, a lone exercise's layout, Select and Delete) and **Series** (with more than one exercise, wherever the cursor is: go to, previous, next and delete, numbering, the series' layout); panels stay open while used; `view.exercisePane` brings up the tab; Teaching › Exercise keeps New Exercise, Exercise Tools and Frontispiece.
- 2026-11-15: an ordinary paragraph always follows the last exercise (the text could not go on after an exercise with its solution and results hidden), typing in the summary goes to it; the statement moves onto a title line holding it (no empty statement part besides), the title kept aside and both back when switching to a title; data after the title then.
- 2026-11-14: inline values deleted whole with Backspace or Delete (they were locked against deletion), edited and shown three ways from the context menu (`exercise.inlineValue`, `strix:exprShow`); data the title, statement, questions or a drawing there already show have no line in the data part.
- 2026-11-14: Exercise Properties reorganised (General, compact question list with one question's settings at a time, folded Variants & Data and Layout, Delete and the Series tab's exercise list); Grading Scale Properties pane (`view.gradingPane`); title line holding the statement next to the number (`exercise.titleLine`); typing beside a field goes into it, the result line takes none; pasted exercises never nest; Inline Expression off the ribbon.

- 2026-11-13: Phase 3 — Grading Scale (`exercise.grading`, `strix:grading`), continuous validation across all variants (`variantErrors`), controlled variant generator (`exercise.generateVariants`), inline parametric expressions (`exercise.insertExpr`, `strix:expr`), pre-flight Assessment Check (`exercise.check`, `view.checkPane`), and Assessment Pack batch export (`file.exportPack`).
- 2026-11-13: quizzes expose Variants & Data in Exercise Properties and insert/hide `strix:exercise.data` (`showData` toggle); `exercise.questionMode` converts quiz modes in place; `exercise.questionPoints` sets points on problem questions too, feeding the scores table's first criterion; a 1×1 scores table keeps its Total row.
- 2026-11-13: "Sample test" template (`file.new {"template": "test"}`), second in the File › New gallery, and the document `--sample` and `?sample` open.
- 2026-11-12: question modes: each quiz question has its mode (`"type"`), calculation and CAD drawing questions; a quiz's data part follows its title when it has no statement ([question-modes.md](question-modes.md)).
- 2026-11-12: kinds of exercise: open answer (a space of a height in cm, squared or blank, the answer written in it) and multiple choice (options with a check box ticked on the correct ones, more than one allowed; True/False); a quiz's statement is optional, a lone question has no letter, each question has its points (shown after it, added up in the scores table), the answers hide with the Solution (the space keeps its height); `exercise.questions`; graph paper's `none` line style. The doc's duplicated second half removed.
- 2026-11-11: the generated words in the interface's four languages (English, Italian, German, French; Spanish dropped); three-letter tags and Swiss German recognised; a language without its own words (Spanish, Latin, Romansh…) keeps the exercise's words instead of turning them English; the scores table's default criterion follows the language.
- 2026-11-10: the scores table (`exercise.scores`): the exercises' rows by themselves, two columns per criterion, a Total column with more than one criterion and a Total row with more than one exercise; the Teaching tab gathers exercises, scores and graph paper.
- 2026-11-10: the frontispiece is free text ended by a Frontispiece Break (a marked next-page section break), no control or placeholder; older frontispieces converted. In a master the series' settings, numbering and styles are the master's (kept on reopening), a subdocument's frontispiece is left out, and the files written back keep their own formatting and frontispiece. A hidden solution starts no page.
- 2026-11-10: editing keeps the structure: placeholders come back when a part is emptied over several paragraphs, Backspace/Delete never join two parts nor eat a placeholder, removed questions leave no blank line; exercises in a master document follow the master (styles and lists brought in, one results summary, the series' settings, written back to the files).
- 2026-11-10: a separator after the number in the numbering (period, colon, dash, parenthesis, space; `exercise.numbering {"separator"}`, a drop-down in Exercise Properties › Series); the title's paragraph style is locked to Exercise Title.

- 2026-11-10: the series rules every exercise: page breaks too (no exercise of a series differs any more; the Exercise tab keeps only variants and data), the first exercise's settings forced on every other one however it came in, a new exercise takes the series'; the exercise's grip in the margin (Select, Properties, Delete Exercise).

- 2026-11-09: the Exercise Properties pane in two tabs: Exercise (this exercise's variants, data, page breaks) and Series (numbering, parts shown, page breaks of every exercise and of the summary, frontispiece); a lone exercise has no Series tab and shows what applies to it in Exercise.
- 2026-11-09: a lone exercise has no number or prefix (title and summary); a series rules the parts shown and the graph paper; parts shown (solution, results) as hidden text (`exercise.show`); graph paper per exercise between the questions and the solution (`exercise.grid`, Graph Paper pane follows in a series); a frontispiece of one or more pages (`exercise.frontispiece`). Teaching › Exercise: Show (Solution, Results, Graph Paper) and Frontispiece added; Update Summary (automatic anyway) and Numbering (the same pane as Properties) left out of the ribbon, still in command search.
- 2026-11-09: page breaks before an exercise when another comes before it, before its solution and before the results summary (`exercise.pageBreaks`, Exercise Properties pane).
- 2026-11-08: the generated words follow the proofing language: changing it (status bar, Review › Language) translates an exercise's default words, the default prefix and the summary; French, German and Spanish words.
- 2026-11-08: "Calculation and exercise" template, second in the File › New gallery, after Blank document.
- 2026-10-29: exercise numbering as a document setting (prefix, format, first number; `exercise.numbering`, Numbering in Teaching › Exercise and the Exercise Properties pane); the summary heads each exercise with its label alone, without the title.
- 2026-10-29: delete a whole exercise: Delete Exercise (ribbon, context menu, `exercise.delete`), Delete/Backspace/Cut over a selection covering it, a click on its tab selects it; the summary goes with the last exercise. Before, every way was refused by the locked parts.

- 2026-10-29: the title has its own control (`strix:exercise.title`), so it is typed over, replaced and deleted in place; before, a New Exercise without a title could not have its title typed over.
- 2026-10-28: exercise titles numbered automatically through their style.
- 2026-10-27: one repeated question per line with its working under it; Result Of defaults to that question.
- 2026-10-27: Exercise Properties pane: initial data and variants; data part.
- 2026-10-27: nested questions (a), a.1), a.1.1)) and notes between the questions.
- 2026-10-27: new.
