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
    /// Titles of the controls, shown on their frames.
    pub title_part: &'static str,
    pub statement: &'static str,
    pub questions: &'static str,
    pub prompt: &'static str,
    pub data: &'static str,
    pub variant: &'static str,
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
    title_part: "Titolo",
    statement: "Testo",
    questions: "Domande",
    prompt: "Domande (automatiche)",
    data: "Dati",
    variant: "variante",
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
    title_part: "Title",
    statement: "Statement",
    questions: "Questions",
    prompt: "Questions (automatic)",
    data: "Data",
    variant: "variant",
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
    title_part: "Titre",
    statement: "Énoncé",
    questions: "Questions",
    prompt: "Questions (automatiques)",
    data: "Données",
    variant: "variante",
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
    title_part: "Titel",
    statement: "Aufgabenstellung",
    questions: "Fragen",
    prompt: "Fragen (automatisch)",
    data: "Daten",
    variant: "Variante",
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
        let fields = |w: &Words| {
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
                w.title_part,
                w.statement,
                w.questions,
                w.prompt,
                w.data,
                w.variant,
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
