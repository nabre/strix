//! Data as equations (docs/exercise.md, Data and variants): a datum is written whole, `L = 4.5 m`,
//! in the Data panel and in the exercise's data part on the page, never as separate symbol,
//! value and unit. The text is split here into the datum's symbol, its value in the active
//! variant and its unit (shared by the variants and kept apart for the variant generator).
//!
//! The data part on the page is editable: a line typed there (`luce: L = 6 m`, a new line, a line
//! deleted) goes into the exercise's data once the caret leaves the part, and the part is then
//! rebuilt from the data. While the caret is in a data part with edits pending, the part is left
//! as typed, so the text never changes under the caret.

use std::collections::BTreeSet;
use std::sync::Arc;

use wordcraft_doc::control::ContentControl;
use wordcraft_doc::{Block, InlineObject, StoryRef};

use super::{DataItem, ExerciseData, Layout, Part, TAG_DATA, TAG_EXERCISE, UNIT_MARK, Words, plain, scan, write_data};
use crate::Session;

/// A datum as written: `description: symbol = value unit` (the description only on the page).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Datum {
    pub description: String,
    pub symbol: String,
    pub value: String,
    pub unit: String,
}

/// Reads a datum written as an equation, `L = 4.5 m` (`with_description`: a page line may start
/// with its description, `luce: L = 4.5 m`). `None` for an empty text. Text without `=` is the
/// symbol alone. The description ends at the last colon with an equation after it, so it may
/// hold an `=` of its own (`velocità = alta: v = 3 m/s`), and a colon in the value (`s = 1:100`)
/// is the value's.
pub fn parse_datum(text: &str, with_description: bool) -> Option<Datum> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let before = text.rfind('=').and_then(|eq| text.get(..eq)).unwrap_or(text);
    let colon = if with_description { before.rfind(':') } else { None };
    let (description, equation) = match colon {
        Some(c) => (text.get(..c).unwrap_or_default().trim(), text.get(c + 1..).unwrap_or_default()),
        None => ("", text),
    };
    let (lhs, rhs) = equation.split_once('=').unwrap_or((equation, ""));
    let symbol = lhs.trim();
    let (value, unit) = split_value_unit(rhs);
    Some(Datum { description: description.to_string(), symbol: symbol.to_string(), value, unit })
}

/// The equation a datum is written as in the active variant (`L = 4.5 m`); empty for an empty
/// datum.
pub fn equation(item: &DataItem, active: usize) -> String {
    let value = item.values.get(active).map(String::as_str).unwrap_or_default();
    if item.symbol.is_empty() && value.is_empty() && item.unit.is_empty() {
        return String::new();
    }
    format!("{} = {value} {}", item.symbol, item.unit).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A character that may continue a number: digits, separators, signs, exponents, operators.
fn numeric(c: char) -> bool {
    c.is_ascii_digit()
        || "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻".contains(c)
        || matches!(c, '.' | ',' | '+' | '-' | '−' | '·' | '×' | '*' | '/' | '^' | '(' | ')' | ' ' | '\u{a0}' | '\u{2009}' | '\u{202f}')
}

/// Splits the right side of a datum into its value and its unit (`2·10^3 kg` → `2·10^3`, `kg`).
/// A value that isn't a number (`2*a`, `π`) is kept whole, with no unit.
pub fn split_value_unit(rhs: &str) -> (String, String) {
    let rhs = rhs.trim();
    let chars: Vec<(usize, char)> = rhs.char_indices().collect();
    let mut cut = 0usize;
    let mut digit = false;
    for (k, &(i, c)) in chars.iter().enumerate() {
        let prev_digit = k.checked_sub(1).and_then(|p| chars.get(p)).is_some_and(|&(_, p)| p.is_ascii_digit());
        let next_num = chars.get(k + 1).is_some_and(|&(_, n)| n.is_ascii_digit() || matches!(n, '+' | '-' | '−'));
        let ok = if matches!(c, 'e' | 'E') { prev_digit && next_num } else { numeric(c) };
        if !ok {
            break;
        }
        digit |= c.is_ascii_digit();
        cut = i + c.len_utf8();
    }
    let whole = || (rhs.to_string(), String::new());
    if !digit {
        return whole();
    }
    let head = rhs.get(..cut).unwrap_or(rhs);
    // Trailing operators and spaces belong to the unit (`50 /s`), never to the number.
    let value = head.trim_end_matches([' ', '\u{a0}', '\u{2009}', '\u{202f}', '*', '·', '×', '/', '^', '(']);
    let unit = rhs.get(value.len()..).unwrap_or_default().trim();
    let balanced = value.matches('(').count() == value.matches(')').count();
    // A unit starts with a letter (`m`, `µm`, `Ω`) or a sign of its own (`°C`, `%`, `/s`).
    let unit_ok = unit.is_empty() || unit.chars().next().is_some_and(|c| c.is_alphabetic() || matches!(c, '°' | '%' | '‰' | '/' | '\''));
    if value.is_empty() || !balanced || !unit_ok {
        return whole();
    }
    (value.to_string(), unit.to_string())
}

/// The data lines a data part shows (`from..=to`, its heading first), as they read now.
fn page_lines(s: &Session, from: usize, to: usize) -> Vec<String> {
    let placeholder = s.doc.body.get(from).is_some_and(|b| {
        matches!(&**b, Block::Para(p) if p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if control.tag == TAG_DATA && control.showing_placeholder)))
    });
    if placeholder {
        return Vec::new();
    }
    let blocks = s.doc.body.get(from.saturating_add(1)..=to).unwrap_or_default();
    blocks
        .iter()
        .filter_map(|b| match &**b {
            Block::Para(p) => Some(plain(p)),
            _ => None,
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// The data lines the data would show (without the heading).
fn expected_lines(data: &ExerciseData, w: &Words, shown: Option<&BTreeSet<String>>) -> Vec<String> {
    texts(&data.lines(w, shown))
}

/// The text of generated data lines (`lines`, the heading first), as the page reads them.
fn texts(lines: &[(&'static str, String)]) -> Vec<String> {
    lines.iter().skip(1).map(|(_, t)| t.replace(UNIT_MARK, "").trim().to_string()).collect()
}

/// The element of a data part's control keeping the lines it was generated with, so what was
/// typed there is told apart from a datum listed again (no longer shown elsewhere).
const LINES_EL: &str = "strix:dataLines";

/// Marks the data part's control `ctl` with the `lines` it is generated with.
pub(super) fn mark(ctl: &mut ContentControl, lines: &[(&'static str, String)]) {
    let open = format!("<{LINES_EL}>");
    ctl.extra.retain(|x| !x.starts_with(&open));
    let json = serde_json::to_string(&texts(lines)).unwrap_or_default();
    let escaped = json.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    ctl.extra.push(format!("{open}{escaped}</{LINES_EL}>"));
}

/// The lines the data part opening at top-level block `from` was generated with (`None` for a
/// part from before they were kept).
fn generated_lines(s: &Session, from: usize) -> Option<Vec<String>> {
    let open = format!("<{LINES_EL}>");
    let close = format!("</{LINES_EL}>");
    let Some(Block::Para(p)) = s.doc.body.get(from).map(|b| &**b) else { return None };
    let raw = p.objects.iter().find_map(|o| match o {
        InlineObject::ControlStart { control } if control.tag == TAG_DATA => {
            control.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close).map(str::to_string))
        }
        _ => None,
    })?;
    let json = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    let lines: Vec<String> = serde_json::from_str(&json).ok()?;
    Some(lines.into_iter().take(super::MAX_DATA).collect())
}

/// What the data part at `from..=to` showed when generated, else what the data would show.
fn baseline(s: &Session, from: usize, data: &ExerciseData, w: &Words, shown: Option<&BTreeSet<String>>) -> Vec<String> {
    generated_lines(s, from).unwrap_or_else(|| expected_lines(data, w, shown))
}

/// The items `lines` (as generated) list, in order: each line's symbol, the first item with it.
fn listed_by(data: &ExerciseData, lines: &[String]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for l in lines {
        let Some(d) = parse_datum(l, true) else { continue };
        if let Some(k) = (0..data.items.len()).find(|k| !out.contains(k) && data.items.get(*k).is_some_and(|i| i.symbol.trim() == d.symbol.trim())) {
            out.push(k);
        }
    }
    out
}

fn same(a: &[String], b: &[String]) -> bool {
    let norm = |t: &String| t.split_whitespace().collect::<Vec<_>>().join(" ");
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| norm(x) == norm(y))
}

/// Whether the caret is in the blocks `from..=to` of the body.
fn caret_in(s: &Session, from: usize, to: usize) -> bool {
    s.sel.focus.story == StoryRef::Body && s.sel.focus.path.0.first().is_some_and(|&top| (from..=to).contains(&(top as usize)))
}

/// The exercises (in [`scan`] order) whose data part was edited on the page and still holds the
/// caret: their part is left as typed until the caret leaves.
pub(super) fn pending(s: &Session, layout: &Layout, w: &Words, shown: &[BTreeSet<String>]) -> BTreeSet<usize> {
    layout
        .exercises
        .iter()
        .enumerate()
        .filter(|(k, e)| {
            e.data_blocks.is_some_and(|(from, to)| {
                caret_in(s, from, to) && {
                    let base = baseline(s, from, &e.data, w, shown.get(*k));
                    !same(&page_lines(s, from, to), &base)
                }
            })
        })
        .map(|(k, _)| k)
        .collect()
}

/// What the data parts read on the page and whether the caret is in them, so an edit there (and
/// the caret leaving) counts as a change.
pub(super) fn fingerprint(s: &Session, layout: &Layout, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    for e in &layout.exercises {
        if let Some((from, to)) = e.data_blocks {
            (page_lines(s, from, to), caret_in(s, from, to)).hash(h);
        }
    }
}

/// Puts the page's lines into `data`: `shown_items` are the items its data part lists, in order.
/// The same number of lines updates them in order; otherwise lines are matched by symbol, an item
/// whose line is gone is removed and a new line is a new datum (the same value in every variant).
fn merge(data: &mut ExerciseData, shown_items: &[usize], lines: &[Datum]) {
    let active = data.active;
    let n = data.variants.len().max(1);
    let apply = |item: &mut DataItem, d: &Datum| {
        item.symbol = d.symbol.clone();
        item.description = d.description.clone();
        item.unit = d.unit.clone();
        if item.values.len() < n {
            item.values.resize(n, String::new());
        }
        if let Some(v) = item.values.get_mut(active) {
            *v = d.value.clone();
        }
    };
    let mut fresh: Vec<&Datum> = Vec::new();
    let mut gone: Vec<usize> = Vec::new();
    if lines.len() == shown_items.len() {
        for (&i, d) in shown_items.iter().zip(lines) {
            if let Some(item) = data.items.get_mut(i) {
                apply(item, d);
            }
        }
    } else {
        let mut used = vec![false; shown_items.len()];
        for d in lines {
            let found = shown_items
                .iter()
                .enumerate()
                .find(|&(k, &i)| !used.get(k).copied().unwrap_or(true) && data.items.get(i).is_some_and(|it| it.symbol.trim() == d.symbol.trim()));
            match found {
                Some((k, &i)) => {
                    if let Some(u) = used.get_mut(k) {
                        *u = true;
                    }
                    if let Some(item) = data.items.get_mut(i) {
                        apply(item, d);
                    }
                }
                None => fresh.push(d),
            }
        }
        gone = shown_items.iter().zip(&used).filter(|(_, u)| !**u).map(|(&i, _)| i).collect();
    }
    gone.sort_unstable();
    for i in gone.into_iter().rev() {
        if i < data.items.len() {
            data.items.remove(i);
        }
    }
    for d in fresh {
        data.items.push(DataItem {
            symbol: d.symbol.clone(),
            description: d.description.clone(),
            unit: d.unit.clone(),
            values: vec![d.value.clone(); n],
            equation: None,
        });
    }
    if data.variants.is_empty() && !data.items.is_empty() {
        data.variants.push("A".into());
    }
    data.sanitize();
}

/// Stores `data` on the exercise control opening top-level block `start`. Whether it was found.
pub(super) fn store(s: &mut Session, start: usize, data: &ExerciseData) -> bool {
    let Some(block) = s.doc.body.get_mut(start) else { return false };
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    let mut found = false;
    for o in &mut p.objects {
        if let InlineObject::ControlStart { control } = o
            && control.tag == TAG_EXERCISE
        {
            write_data(control, data);
            found = true;
            break;
        }
    }
    if found {
        p.touch();
        s.touch();
    }
    found
}

/// Takes what was typed in the exercises' data parts into their data (`force`: the exercise
/// whose edits are taken even with the caret in its part, before a command changes its data).
/// The parts themselves are rebuilt by [`super::update`]. Whether any data changed.
pub(crate) fn take_page_edits(s: &mut Session, force: Option<usize>) -> bool {
    let layout = scan(s);
    if layout.exercises.iter().all(|e| e.data_blocks.is_none()) {
        return false;
    }
    let w = super::doc_words(s);
    let shown = super::shown::shown(s, &layout);
    let mut changed = false;
    for (k, e) in layout.exercises.iter().enumerate() {
        let Some((from, to)) = e.data_blocks else { continue };
        // Only a part that is all generated data (no user content around it).
        if !(from..=to).all(|i| layout.parts.get(i).is_some_and(|(_, part)| *part == Part::Data)) {
            continue;
        }
        let base = baseline(s, from, &e.data, w, shown.get(k));
        let page = page_lines(s, from, to);
        if same(&page, &base) || (caret_in(s, from, to) && force != Some(k)) {
            continue;
        }
        let lines: Vec<Datum> = page.iter().filter_map(|t| parse_datum(t, true)).collect();
        let mut data = e.data.clone();
        let listed = listed_by(&data, &base);
        merge(&mut data, &listed, &lines);
        if data != e.data && store(s, e.start, &data) {
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datum_is_split_into_symbol_value_and_unit() {
        let d = |t: &str| parse_datum(t, true).unwrap();
        assert_eq!(d("A = 50 m"), Datum { description: String::new(), symbol: "A".into(), value: "50".into(), unit: "m".into() });
        assert_eq!(d("luce: L = 4.5 m"), Datum { description: "luce".into(), symbol: "L".into(), value: "4.5".into(), unit: "m".into() });
        assert_eq!(split_value_unit("2·10^3 kg"), ("2·10^3".into(), "kg".into()));
        assert_eq!(split_value_unit("1.5e3 m/s^2"), ("1.5e3".into(), "m/s^2".into()));
        assert_eq!(split_value_unit("5 eV"), ("5".into(), "eV".into()));
        assert_eq!(split_value_unit("-3 °C"), ("-3".into(), "°C".into()));
        assert_eq!(split_value_unit("50mca"), ("50".into(), "mca".into()));
        assert_eq!(split_value_unit("12"), ("12".into(), String::new()));
        assert_eq!(split_value_unit("2*a"), ("2*a".into(), String::new()));
        assert_eq!(split_value_unit("π"), ("π".into(), String::new()));
        assert_eq!(split_value_unit("(1 + 2"), ("(1 + 2".into(), String::new()));
        assert_eq!(d("h"), Datum { symbol: "h".into(), ..Datum::default() });
        // The panel's equation has no description: a colon stays in the symbol.
        assert_eq!(parse_datum("a:b = 1", false).unwrap().symbol, "a:b");
        assert!(parse_datum("  ", true).is_none());
        let item = DataItem { symbol: "A".into(), unit: "m".into(), values: vec!["50".into()], ..DataItem::default() };
        assert_eq!(equation(&item, 0), "A = 50 m");
        assert_eq!(equation(&item, 3), "A = m");
        assert_eq!(equation(&DataItem::default(), 0), "");
    }

    #[test]
    fn hostile_text_never_panics() {
        for t in ["=", "==", ":=", "é = 1é", "x = 1e", "x = e5", "x = ((((", "x = 1 /", "x = ⁻⁻", "x = 1\u{a0}m", "∑ = 1·", "x=1(m)"] {
            let _ = parse_datum(t, true);
            let _ = parse_datum(t, false);
        }
    }

    /// Found in review: a description with an `=` of its own was cut at it (`velocità` the
    /// symbol, the rest its value). The description ends at the last colon before the equation.
    #[test]
    fn a_description_may_hold_an_equals_sign() {
        let d = |t: &str| parse_datum(t, true).unwrap();
        assert_eq!(
            d("velocità = alta: v = 3 m/s"),
            Datum { description: "velocità = alta".into(), symbol: "v".into(), value: "3".into(), unit: "m/s".into() }
        );
        assert_eq!(d("nota: a = b: L = 6 m"), Datum { description: "nota: a = b".into(), symbol: "L".into(), value: "6".into(), unit: "m".into() });
        // A colon in the value is the value's (a ratio), with or without a description.
        assert_eq!(d("s = 1:100"), Datum { description: String::new(), symbol: "s".into(), value: "1:100".into(), unit: String::new() });
        assert_eq!(d("scala: s = 1:100").description, "scala");
        assert_eq!(d("scala: s = 1:100").symbol, "s");
        // Without descriptions (the Data panel) nothing changes.
        assert_eq!(parse_datum("a = b: c = 3", false).unwrap().symbol, "a");
    }
}
