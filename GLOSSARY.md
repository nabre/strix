# WordCraft

A clean-room, Rust-native word processor. These are the words its interface, docs and code use for
the same things.

## Languages and proofing

**Interface language**:
The language of WordCraft's own menus, buttons and messages.
_Avoid_: Display language, UI locale

**Proofing language**:
The language a piece of text is marked as, which decides how its spelling, grammar and hyphenation
are checked. A language is told apart from another by its tag (language, script, region, variant),
never by the name a list shows for it.
_Avoid_: Text language, editing language, document language

**Language pack**:
A downloadable bundle of a language's spelling dictionary and hyphenation patterns.
_Avoid_: Dictionary download, language data (that is the embedded data)

**Language list**:
The proofing languages offered in Set Proofing Language, Review › Language and the status bar's
menu: the languages WordCraft names, plus those of every installed (and, in Set Proofing Language,
every available) Language pack, each once.
_Avoid_: Language menu (that is only one of the places it is shown)

**Your languages**:
The proofing languages Options › Language lists: each English variant in use (the default proofing
language and any in the open document), every language with an installed
Language pack, and every proofing language used in the open document.
_Avoid_: Authoring languages, preferred languages

**Default proofing language**:
The proofing language new blank documents get, set with Set as default; without one, the interface
language.
_Avoid_: Preferred language

**Proofing status**:
Where a language's proofing comes from: built in, an installed Language pack, your own dictionary,
a system dictionary, available to install, or not available.

**Custom dictionary**:
The words you added with Add to Dictionary, accepted in every language.
_Avoid_: User dictionary (in the interface)

**Dictionaries folder**:
A folder of Hunspell dictionaries you supply, used before the system's (an installed Language pack
comes after it).
_Avoid_: Custom dictionaries

## Export

**Strict export**:
An export that writes nothing, and reports why, when a character or picture the output is meant
to show would be missing from it. Text the output doesn't show (hidden text, deletions without
markup) isn't checked.
_Avoid_: Safe mode, validated export

**Missing glyph**:
A character the output draws that no face in the font fallback chain can draw, so it would appear
as an empty box.
_Avoid_: Tofu, box, notdef character

**Missing picture**:
A picture the document refers to that the output cannot include: its part is absent, its data
cannot be decoded, its format is unsupported, or it is linked from outside the file.
_Avoid_: Broken image, dropped image

## Strix: calculations and teaching

These are Strix's own words, on top of WordCraft's; the feature documents in `docs/` describe each.

**Calculation**:
An equation Strix works out: one `=` and one unknown it finds, isolates (or solves numerically) and
assigns, recalculated as the document changes ([calc.md](docs/calc.md)).

**Working**:
The steps Strix writes under or after a calculation, from the formula to the value with its unit
(`calc.steps`); for a structure, the equilibrium equations and internal-force laws written as
calculations by Write Working (`structure.working`).

**Calculated table**:
A table whose columns are named values or formulas computed row by row, with an optional total row;
its commands are on the contextual Calculated Table tab.

**Exercise**:
A locked block holding a title, a statement, questions, an optional solution and its lines in the
results summary; each one is a calculation sandbox ([exercise.md](docs/exercise.md)).
_Avoid_: Problem (that is one kind of exercise), item

**Series**:
Two or more exercises in one document: they are numbered, and the parts shown and the graph paper
are set once for all of them (the Series tab).

**Quiz**:
An exercise made of questions (open answer, multiple choice, true or false, calculation, CAD
drawing; see [question-modes.md](docs/question-modes.md)), as opposed to a problem with data and
a solution.
_Avoid_: Test (that is the whole document handed out)

**Question mode**:
How a question is answered: open answer, multiple choice, true or false, calculation, CAD drawing,
and the worked, cloze, table-fill and matching modes; the Question Settings pane calls it the
question's type.

**Exercise data**:
The given values of an exercise, one equation per datum (`L = 4.5 m`) with a description, shown in
the Exercise Data pane.

**Variant**:
One set of values for an exercise's data (A, B, C…), so the same exercise can be handed out with
different numbers; Generate Variants makes them from ranges and constraints.
_Avoid_: Version (that is a DSA version), copy

**Results summary**:
The list at the end of the document with each question's result, for correcting.
_Avoid_: Answer key (that is the Assessment pack's teacher PDF)

**Students' copy / Corrected copy**:
The document as handed out (answer spaces empty, solutions hidden) and as the teacher keeps it
(solutions and answers shown).

**Scores table**:
The table of each exercise's points by criterion, with the total score (Teaching › Scores).

**Grading scale**:
The table that turns points into grades: a preset or custom scale, the pass threshold and the
curve, each grade with its interval of points (Teaching › Scores › Grading Scale).

**Assessment pack**:
The files exported for a test in one go: a students' PDF per variant, their DSA versions and the
teacher's key with the grading scale and the synoptic table of the variants (`file.exportPack`).

**Archive**:
A teacher's exercises kept one per file, each file carrying archive properties (grade, section,
subject, teacher, duration…) and searched from the Exercise Bank pane ([archive.md](docs/archive.md)).

**DSA formatting**:
The versions of a document for students with specific learning disorders (Reading, Writing, Maths
and Memory), which combine over a common base (Design › DSA; [dsa.md](docs/dsa.md)).

**Graph paper**:
A square grid ruled behind the text, as a page background or a block of a given extent
([graph-paper.md](docs/graph-paper.md)).
_Avoid_: Grid (alone), squared paper

**CAD drawing**:
A technical drawing in the document whose coordinates and dimensions can be calculation
parameters, redrawn with the calculations ([cad.md](docs/cad.md)).

**Plot**:
A scientific graph of functions or data against axes with units, its curves driven by the
calculations ([plot.md](docs/plot.md)).
_Avoid_: Chart (that is Word's chart object)

**Structure**:
A plane beam, frame or truss (nodes, members, supports, loads) that Strix classifies, solves and
draws with its reactions and N, V (T), M diagrams ([structures.md](docs/structures.md)).

**Strix object**:
What Strix adds to a document that other formats would lose: calculations, calculated tables,
exercises, CAD drawings, plots, structures and graph paper. A document with any is saved as
`.xmath`, or as a Word document that keeps them through content controls and their sources
([word-round-trip.md](docs/word-round-trip.md)).
_Avoid_: Custom object, embedded object
