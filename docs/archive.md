# Archive properties

> **Status:** stage 1: the properties on a file (`archive.properties`), the score computed from the scores table · **Last updated:** 2026-11-11 · **Change:** first version

A teacher keeps exercises **one per file**, the archive, and assembles them into tests with
[master documents](master-documents.md). Each file says what it is about through **archive
properties**: ordinary document properties, so Word shows them as well (File › Info ›
Properties) and other tools can search a folder by them.

## The properties

| Property | Parameter | Kept as | Values |
|---|---|---|---|
| Grade (*Classe*) | `grade` | custom property, number (`i4`) | 1–20 |
| Section (*Sezione*) | `section` | custom property, text | any text, up to 40 characters (A, B, C…) |
| Profession (*Professione*) | `profession` | custom property, text | any text, up to 40 characters (DIC, DA, GE…) |
| Subject (*Materia*) | `subject` | the document's **Subject** (core property) | any text, up to 255 characters |
| Teacher (*Docente*) | `teacher` | the document's **Author** (core property) | any text, up to 255 characters |
| Duration (*Durata (min)*) | `duration` | custom property, number (`i4`) | 1–600 minutes, typed in |
| Score (*Punteggio*) | — | custom property, number (`i4`, or `r8` with decimals) | **computed** |

`archive.properties {"grade": 3, "section": "B", "profession": "DIC", "subject": "Matematica",
"teacher": "…", "duration": 45}` sets the properties given; `null` or empty text removes one;
without params it reads them. Every value is checked before anything changes: a grade of 0 or
`"3ª"`, a duration of 601, an unknown property refuse the whole command. While the document is
protected (Review › Restrict Editing) the command is refused, like File › Properties.

Text values are free (no fixed lists): searches will compare them ignoring case and spaces.

### Names in Word

The custom properties are named in the language of the document's exercises (the language their
words are written in, [exercise.md](exercise.md#language)):

| | Italian | English | German | French |
|---|---|---|---|---|
| grade | Classe | Grade | Klasse | Classe |
| section | Sezione | Section | Abteilung | Section |
| profession | Professione | Profession | Beruf | Profession |
| duration | Durata (min) | Duration (min) | Dauer (min) | Durée (min) |
| score | Punteggio | Score | Punktzahl | Points |

They are read under any of these names, so a file written in German is found by an Italian
search; setting a property rewrites its name in the document's language. A property edited in
Word (Properties › Custom) is read back as it is.

### The score

The score is the **grand total of the document's scores table** (Teaching › Scores,
[exercise.md](exercise.md#scores-table)): every point typed in for its exercises, across all
criteria. It follows the table as points are typed, with the other automatic parts, and can't be
set (`archive.properties {"score": …}` is refused). A document with exercises but no scores table,
or no points in it, has no score; a document without exercises keeps a property of that name it
already has (a Word file's own). So that an archive file has a score, give it a scores table, even
of one row.

Each document has its own: an archive file's score is its exercise's points, a master
document's is the whole test's. Saving a master writes no property of its into the files.

## Notes

- File › Options › Privacy, *remove personal information on save*, saves no author: the teacher
  is lost in that file.
- `.xmath` keeps the properties too, but only `.docx` (and `.odt`) files are read by Word.

## Next

- Teaching › Archive in the ribbon: a pane to fill the properties in.
- Search a folder (`archive.scan`, `archive.query`): read only the properties part of each file,
  filter by them and by text, insert the chosen files into a master document.
