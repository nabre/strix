//! The words Strix writes for exercises (docs/exercise.md › Language): frame titles, placeholders,
//! the solution's heading, the numbering's default prefix, the results summary, the scores table.
//! One set per interface language Strix offers (English, Italian, German, French), looked up by
//! the text's language tag. A language without its own set gets none ([`lookup`]): the caller
//! keeps the words already in the document.

/// Words of the generated parts in one language.
pub(super) struct Words {
    pub exercise: &'static str,
    /// The scores table's total and its default criterion.
    pub total: &'static str,
    pub points: &'static str,
    pub solution: &'static str,
    pub results: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    pub question: &'static str,
    pub no_result: &'static str,
    /// What the results summary says for an open-answer question.
    pub open_answer: &'static str,
    /// Titles of the controls, shown on their frames.
    pub title_part: &'static str,
    pub statement: &'static str,
    pub questions: &'static str,
    pub prompt: &'static str,
    pub data: &'static str,
    pub variant: &'static str,
    /// The archive's document properties (docs/archive.md), as Word shows them.
    pub grade: &'static str,
    pub section: &'static str,
    pub profession: &'static str,
    pub duration: &'static str,
    pub score: &'static str,
    /// Quizzes (docs/exercise.md, Kinds): the answer control's title, the points' abbreviation
    /// after a question (`….. / 2 pt`), the options of a true/false question.
    pub answer: &'static str,
    pub points_short: &'static str,
    pub true_word: &'static str,
    pub false_word: &'static str,
    /// Note shown after an equation solved numerically (`cmd/calc.rs`).
    pub numeric_solution: &'static str,
    /// Grading scale table (`cmd/exercise/grading.rs`) and assessment pack (`cmd/exercise/pack.rs`).
    pub grading_scale: &'static str,
    pub grade_mark: &'static str,
    /// The note under the grading scale table, before the multiple the points are rounded to.
    pub points_rounded: &'static str,
    pub pack_row_prefix: &'static str,
    pub pack_teacher_name: &'static str,
    pub synoptic_heading: &'static str,
    pub question_col: &'static str,
    /// What a question's result format asks, written in brackets after the question.
    pub result_hint: ResultHint,
}

/// The words of a question's result format (`cmd/exercise/result_format.rs`): `{}` stands for the
/// unit, the number of decimal places or the rounding step.
pub(super) struct ResultHint {
    pub unit: &'static str,
    pub decimal: &'static str,
    pub decimals: &'static str,
    pub no_decimals: &'static str,
    pub rounded: &'static str,
    pub rounded_up: &'static str,
    pub rounded_down: &'static str,
    pub to_step: &'static str,
}

/// The words of language tag `lang` (`it-IT`, `fr`, `ita`, `gsw-CH`…), from its primary subtag
/// (two or three letters); `None` for a language without its own words (Spanish, Latin,
/// Romansh…).
pub(super) fn lookup(lang: &str) -> Option<&'static Words> {
    let lang = lang.trim().to_ascii_lowercase();
    let primary = lang.split(['-', '_']).next().unwrap_or_default();
    Some(match primary {
        "it" | "ita" => &IT,
        "en" | "eng" => &EN,
        "fr" | "fra" | "fre" => &FR,
        // Swiss German (`gsw`) is written in Standard German.
        "de" | "deu" | "ger" | "gsw" => &DE,
        _ => return None,
    })
}

/// Every language's words: a generated text is a default (one to translate) when one of them has
/// it. When the same words fit several languages, the first listed wins.
pub(super) const ALL_WORDS: [&Words; 4] = [&IT, &EN, &FR, &DE];

pub(super) const IT: Words = Words {
    exercise: "Esercizio",
    total: "Totale",
    points: "Punti",
    solution: "Risoluzione",
    results: "Riepilogo dei risultati",
    title: "Titolo dell'esercizio",
    text: "Scrivi qui il testo dell'esercizio: dati, figure, tabelle.",
    question: "Scrivi qui la domanda.",
    no_result: "risultato non indicato",
    open_answer: "domanda a risposta aperta",
    title_part: "Titolo",
    statement: "Testo",
    questions: "Domande",
    prompt: "Domande (automatiche)",
    data: "Dati",
    variant: "variante",
    grade: "Classe",
    section: "Sezione",
    profession: "Professione",
    duration: "Durata (min)",
    score: "Punteggio",
    answer: "Risposta",
    points_short: "pt",
    true_word: "Vero",
    false_word: "Falso",
    numeric_solution: "(soluzione numerica)",
    grading_scale: "Griglia di valutazione",
    grade_mark: "Voto",
    points_rounded: "Punti arrotondati al multiplo di",
    pack_row_prefix: "Fila",
    pack_teacher_name: "Correttore_Docente",
    synoptic_heading: "Tabella sinottica delle varianti",
    question_col: "Domanda",
    result_hint: ResultHint {
        unit: "in {}",
        decimal: "{} decimale",
        decimals: "{} decimali",
        no_decimals: "senza decimali",
        rounded: "arrotondato",
        rounded_up: "arrotondato per eccesso",
        rounded_down: "arrotondato per difetto",
        to_step: "a {}",
    },
};

pub(super) const EN: Words = Words {
    exercise: "Exercise",
    total: "Total",
    points: "Points",
    solution: "Solution",
    results: "Results",
    title: "Exercise title",
    text: "Write the exercise here: data, figures, tables.",
    question: "Write the question here.",
    no_result: "no result marked",
    open_answer: "open-answer question",
    title_part: "Title",
    statement: "Statement",
    questions: "Questions",
    prompt: "Questions (automatic)",
    data: "Data",
    variant: "variant",
    grade: "Grade",
    section: "Section",
    profession: "Profession",
    duration: "Duration (min)",
    score: "Score",
    answer: "Answer",
    points_short: "pt",
    true_word: "True",
    false_word: "False",
    numeric_solution: "(numerical solution)",
    grading_scale: "Grading scale",
    grade_mark: "Grade",
    points_rounded: "Points rounded to the nearest multiple of",
    pack_row_prefix: "Variant",
    pack_teacher_name: "Teacher_Key",
    synoptic_heading: "Multi-variant answer key",
    question_col: "Question",
    result_hint: ResultHint {
        unit: "in {}",
        decimal: "{} decimal place",
        decimals: "{} decimal places",
        no_decimals: "no decimal places",
        rounded: "rounded",
        rounded_up: "rounded up",
        rounded_down: "rounded down",
        to_step: "to {}",
    },
};

const FR: Words = Words {
    exercise: "Exercice",
    total: "Total",
    points: "Points",
    solution: "Solution",
    results: "Récapitulatif des résultats",
    title: "Titre de l'exercice",
    text: "Écrivez ici l'énoncé de l'exercice : données, figures, tableaux.",
    question: "Écrivez ici la question.",
    no_result: "résultat non indiqué",
    open_answer: "question à réponse ouverte",
    title_part: "Titre",
    statement: "Énoncé",
    questions: "Questions",
    prompt: "Questions (automatiques)",
    data: "Données",
    variant: "variante",
    grade: "Classe",
    section: "Section",
    profession: "Profession",
    duration: "Durée (min)",
    score: "Points",
    answer: "Réponse",
    points_short: "pt",
    true_word: "Vrai",
    false_word: "Faux",
    numeric_solution: "(solution numérique)",
    grading_scale: "Barème de notation",
    grade_mark: "Note",
    points_rounded: "Points arrondis au multiple de",
    pack_row_prefix: "Sujet",
    pack_teacher_name: "Corrige_Enseignant",
    synoptic_heading: "Tableau synoptique des variantes",
    question_col: "Question",
    result_hint: ResultHint {
        unit: "en {}",
        decimal: "{} décimale",
        decimals: "{} décimales",
        no_decimals: "sans décimales",
        rounded: "arrondi",
        rounded_up: "arrondi par excès",
        rounded_down: "arrondi par défaut",
        to_step: "à {}",
    },
};

const DE: Words = Words {
    exercise: "Aufgabe",
    total: "Summe",
    points: "Punkte",
    solution: "Lösung",
    results: "Ergebnisübersicht",
    title: "Titel der Aufgabe",
    text: "Schreiben Sie hier die Aufgabe: Daten, Abbildungen, Tabellen.",
    question: "Schreiben Sie hier die Frage.",
    no_result: "kein Ergebnis angegeben",
    open_answer: "Frage mit offener Antwort",
    title_part: "Titel",
    statement: "Aufgabenstellung",
    questions: "Fragen",
    prompt: "Fragen (automatisch)",
    data: "Daten",
    variant: "Variante",
    grade: "Klasse",
    section: "Abteilung",
    profession: "Beruf",
    duration: "Dauer (min)",
    score: "Punktzahl",
    answer: "Antwort",
    points_short: "P.",
    true_word: "Wahr",
    false_word: "Falsch",
    numeric_solution: "(numerische Lösung)",
    grading_scale: "Notenschlüssel",
    grade_mark: "Note",
    points_rounded: "Punkte gerundet auf ein Vielfaches von",
    pack_row_prefix: "Variante",
    pack_teacher_name: "Korrekturschluessel",
    synoptic_heading: "Variantenübersicht",
    question_col: "Frage",
    result_hint: ResultHint {
        unit: "in {}",
        decimal: "{} Nachkommastelle",
        decimals: "{} Nachkommastellen",
        no_decimals: "ohne Nachkommastellen",
        rounded: "gerundet",
        rounded_up: "aufgerundet",
        rounded_down: "abgerundet",
        to_step: "auf {}",
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: Option<&Words>, b: &Words) -> bool {
        a.is_some_and(|a| (a.title, a.solution, a.text) == (b.title, b.solution, b.text))
    }

    #[test]
    fn tags_find_their_words() {
        for (tag, want) in [
            ("it-IT", &IT),
            ("it-CH", &IT),
            ("ita", &IT),
            ("de-CH", &DE),
            ("gsw-CH", &DE),
            ("ger", &DE),
            ("fr-CH", &FR),
            ("fre", &FR),
            ("en-GB", &EN),
        ] {
            assert!(same(lookup(tag), want), "{tag}");
        }
        for tag in ["es-ES", "pt-BR", "pl-PL", "la", "rm-CH", "el-GR", "", "x-none", "und"] {
            assert!(lookup(tag).is_none(), "{tag}");
        }
    }

    /// Every set is complete, and an exercise's frame titles and placeholders tell its language
    /// apart from every other (what keeps an exercise's words when its text turns to a language
    /// without its own).
    #[test]
    fn every_set_is_complete_and_distinct() {
        let fields = |w: &Words| -> [&str; 41] {
            [
                w.exercise,
                w.total,
                w.points,
                w.solution,
                w.results,
                w.title,
                w.text,
                w.question,
                w.no_result,
                w.open_answer,
                w.title_part,
                w.statement,
                w.questions,
                w.prompt,
                w.data,
                w.variant,
                w.grade,
                w.section,
                w.profession,
                w.duration,
                w.score,
                w.answer,
                w.points_short,
                w.true_word,
                w.false_word,
                w.numeric_solution,
                w.grading_scale,
                w.grade_mark,
                w.points_rounded,
                w.pack_row_prefix,
                w.pack_teacher_name,
                w.synoptic_heading,
                w.question_col,
                w.result_hint.unit,
                w.result_hint.decimal,
                w.result_hint.decimals,
                w.result_hint.no_decimals,
                w.result_hint.rounded,
                w.result_hint.rounded_up,
                w.result_hint.rounded_down,
                w.result_hint.to_step,
            ]
        };
        let shown = |w: &Words| [w.exercise, w.title_part, w.statement, w.questions, w.solution, w.prompt, w.title, w.text, w.question];
        let sets = ALL_WORDS;
        for (i, a) in sets.iter().enumerate() {
            assert!(fields(a).iter().all(|f| !f.trim().is_empty()), "set {i} has an empty word");
            for (j, b) in sets.iter().enumerate() {
                if i != j {
                    assert_ne!(shown(a), shown(b), "sets {i} and {j} can't be told apart");
                }
            }
        }
    }
}
