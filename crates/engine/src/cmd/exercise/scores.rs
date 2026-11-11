//! The scores table of a series of exercises (docs/exercise.md, Scores table): a table that
//! gathers the document's exercises by itself, one row each, and the criteria they are scored on
//! (IMP, MOD, CAL/RIS…), each in two columns: one left empty (for the points given, written on
//! the sheet) and one with the points the exercise is worth, typed in. With more than one
//! criterion a Total column adds them up across; with more than one exercise a Total row adds
//! them up down. It is a table in a content control tagged `strix:scores` that can't be deleted
//! by accident; the exercises' rows, their numbers and the totals are rebuilt with the other
//! automatic parts ([`super::update`]), the criteria's names and the points typed in are kept.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::props::{Align, CharProps};
use wordcraft_doc::table::{Cell, Row, Table};
use wordcraft_doc::{Block, Paragraph, StoryRef, para_block};

use super::{doc_words, numbering, scan};
use crate::{CmdError, CmdResult, Session, p};

pub const TAG_SCORES: &str = "strix:scores";
/// The element on the control's properties holding the number of criteria.
const EL: &str = "strix:scoresTable";
/// Most criteria; longest criterion name, in characters; largest points value.
const MAX_CRITERIA: usize = 12;
const MAX_NAME: usize = 40;
const MAX_POINTS: f64 = 1.0e6;
/// Most exercise rows (the summary's limit).
const MAX_ROWS: usize = super::MAX_EXERCISES;

/// What a scores table holds: its criteria and, for each exercise, the points of each criterion.
#[derive(Debug, Clone, PartialEq)]
struct Model {
    criteria: Vec<String>,
    points: Vec<Vec<Option<f64>>>,
}

fn cell_text(c: &Cell) -> String {
    c.blocks.iter().filter_map(|b| b.as_para()).map(super::plain).collect::<Vec<_>>().join(" ").trim().to_string()
}

/// A number typed in a cell (`2`, `1.5`, `1,5`), within bounds; `None` for anything else.
fn number(t: &str) -> Option<f64> {
    let v: f64 = t.trim().replace(',', ".").parse().ok()?;
    (v.is_finite() && v.abs() <= MAX_POINTS).then_some(v)
}

/// `v` as the table shows it: whole numbers plain, others with up to two decimals, the decimal
/// comma in the languages that write one.
fn shown(v: f64, comma: bool) -> String {
    let t = format!("{:.2}", v);
    let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
    let t = if t == "-0" { "0".to_string() } else { t };
    if comma { t.replace('.', ",") } else { t }
}

fn criteria_count(ctl: &ContentControl) -> Option<usize> {
    let open = format!("<{EL} ");
    let el = ctl.extra.iter().find(|x| x.starts_with(&open))?;
    let (_, rest) = el.split_once("criteria=\"")?;
    let (n, _) = rest.split_once('"')?;
    n.parse::<usize>().ok().map(|n| n.clamp(1, MAX_CRITERIA))
}

fn write_count(ctl: &mut ContentControl, k: usize) {
    let open = format!("<{EL} ");
    ctl.extra.retain(|x| !x.starts_with(&open));
    ctl.extra.push(format!("<{EL} criteria=\"{}\"/>", k.clamp(1, MAX_CRITERIA)));
}

fn control_of(t: &Table) -> Option<&ContentControl> {
    t.controls.open.iter().find(|c| c.tag == TAG_SCORES)
}

/// The top-level blocks that are scores tables.
fn tables(s: &Session) -> Vec<usize> {
    s.doc.body.iter().enumerate().filter(|(_, b)| matches!(&***b, Block::Table(t) if control_of(t).is_some())).map(|(i, _)| i).collect()
}

/// What table `t` holds: the criteria (their number from the control, their names from the
/// header row) and the points typed in each exercise's row.
fn read(t: &Table) -> Model {
    let k = control_of(t).and_then(criteria_count).unwrap_or(1);
    let header = t.rows.first();
    let criteria = (0..k).map(|c| header.and_then(|r| r.cells.get(c + 1)).map(cell_text).unwrap_or_default()).collect();
    let points = t
        .rows
        .iter()
        .skip(1)
        .take(MAX_ROWS)
        .map(|r| (0..k).map(|c| r.cells.get(2 + 2 * c).map(cell_text).and_then(|t| number(&t))).collect())
        .collect();
    Model { criteria, points }
}

/// A cell holding `text` aligned `align`, bold when `bold`, spanning `span` grid columns.
fn cell(text: &str, align: Align, bold: bool, span: u32, width: f32) -> Cell {
    let chr = CharProps { bold: bold.then_some(true), ..CharProps::default() };
    let mut para = Paragraph::with_text(text, chr);
    para.props.align = Some(align);
    para.props.space_after = Some(0.0);
    para.props.space_before = Some(0.0);
    let mut c = Cell::empty();
    c.blocks = vec![para_block(para)];
    c.props.span = span;
    c.props.width = Some(width);
    c
}

/// The table for `m` with `rows` exercises labelled `label(n)`, `width` points wide.
fn build(m: &Model, rows: usize, label: &dyn Fn(usize) -> String, total: &str, exercise: &str, width: f32, comma: bool) -> Table {
    let k = m.criteria.len().max(1);
    let across = k > 1;
    let down = rows > 1;
    // Grid: the exercises' column, then two per criterion (and two for the total).
    let pairs = k + usize::from(across);
    let first = (width * 0.16).clamp(36.0, 120.0);
    let pair = ((width - first) / pairs as f32).max(24.0);
    let (blank, value) = (pair * 0.45, pair * 0.55);
    let mut grid = vec![first];
    for _ in 0..pairs {
        grid.push(blank);
        grid.push(value);
    }
    let mut t = Table::new(1, grid.len(), width);
    t.grid = grid;
    let mut header = vec![cell(exercise, Align::Left, true, 1, first)];
    for name in &m.criteria {
        header.push(cell(name, Align::Center, true, 2, pair));
    }
    if across {
        header.push(cell(total, Align::Center, true, 2, pair));
    }
    let mut out_rows = vec![Row { cells: header, ..Row::default() }];
    let get = |r: usize, c: usize| m.points.get(r).and_then(|row| row.get(c)).copied().flatten();
    for r in 0..rows {
        let mut cells = vec![cell(&label(r), Align::Center, true, 1, first)];
        for c in 0..k {
            cells.push(cell("", Align::Right, false, 1, blank));
            cells.push(cell(&get(r, c).map(|v| shown(v, comma)).unwrap_or_default(), Align::Right, false, 1, value));
        }
        if across {
            let any = (0..k).any(|c| get(r, c).is_some());
            let sum: f64 = (0..k).filter_map(|c| get(r, c)).sum();
            cells.push(cell("", Align::Right, false, 1, blank));
            cells.push(cell(&if any { shown(sum, comma) } else { String::new() }, Align::Right, true, 1, value));
        }
        out_rows.push(Row { cells, ..Row::default() });
    }
    if down {
        let mut cells = vec![cell(total, Align::Left, true, 1, first)];
        let col = |c: usize| -> Option<f64> {
            let v: Vec<f64> = (0..rows).filter_map(|r| get(r, c)).collect();
            (!v.is_empty()).then(|| v.iter().sum())
        };
        let mut grand: Option<f64> = None;
        for c in 0..k {
            let v = col(c);
            if let Some(v) = v {
                grand = Some(grand.unwrap_or(0.0) + v);
            }
            cells.push(cell("", Align::Right, false, 1, blank));
            cells.push(cell(&v.map(|v| shown(v, comma)).unwrap_or_default(), Align::Right, true, 1, value));
        }
        if across {
            cells.push(cell("", Align::Right, false, 1, blank));
            cells.push(cell(&grand.map(|v| shown(v, comma)).unwrap_or_default(), Align::Right, true, 1, value));
        }
        out_rows.push(Row { cells, ..Row::default() });
    }
    t.rows = out_rows;
    t
}

/// Each row's cells: their texts and spans (what a rebuild would change).
fn shape(t: &Table) -> Vec<Vec<(String, u32)>> {
    t.rows.iter().map(|r| r.cells.iter().map(|c| (cell_text(c), c.props.span.max(1))).collect()).collect()
}

/// What the scores tables hold (a change there, points typed in, rebuilds their totals).
pub(super) fn fingerprint(s: &Session) -> Vec<Vec<Vec<(String, u32)>>> {
    tables(s)
        .into_iter()
        .filter_map(|i| match s.doc.body.get(i).map(|b| &**b) {
            Some(Block::Table(t)) => Some(shape(t)),
            _ => None,
        })
        .collect()
}

/// Whether the document writes numbers with a decimal comma.
fn comma(s: &Session) -> bool {
    !super::words_lang(s).starts_with("en")
}

/// The text width of the section block `i` is in.
fn text_width(s: &Session, i: usize) -> f32 {
    s.doc.sections().into_iter().find(|(end, _)| *end >= i).map_or_else(|| s.doc.last_section.text_width(), |(_, sp)| sp.text_width())
}

/// Rebuild table `i` from `m` when it differs; whether it changed. Its control and its
/// table-wide properties stay.
fn put(s: &mut Session, i: usize, m: &Model) -> bool {
    let rows = scan(s).exercises.len().min(MAX_ROWS);
    let labels = numbering::current(s);
    let label = |n: usize| labels.format.format(u32::try_from(n).unwrap_or(u32::MAX).saturating_add(labels.start));
    let w = doc_words(s);
    let width = text_width(s, i);
    let Some(Block::Table(old)) = s.doc.body.get(i).map(|b| &**b) else { return false };
    let mut new = build(m, rows, &label, w.total, w.exercise, width, comma(s));
    if shape(old) == shape(&new) && old.grid.len() == new.grid.len() {
        return false;
    }
    new.controls = old.controls.clone();
    new.props = old.props.clone();
    if let Some(ctl) = new.controls.open.iter_mut().find(|c| c.tag == TAG_SCORES) {
        write_count(ctl, m.criteria.len());
    }
    if let Some(b) = s.doc.body.get_mut(i) {
        *b = Arc::new(Block::Table(new));
    }
    true
}

/// Every scores table brought up to date with the document's exercises (their number) and its
/// own points (the totals). Whether anything changed.
pub(crate) fn update(s: &mut Session) -> bool {
    let mut changed = false;
    for i in tables(s) {
        let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b) else { continue };
        let m = read(t);
        changed |= put(s, i, &m);
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

/// The document's language changed: every criterion `rename` gives a new name (a default one, in
/// another language) takes it, and the tables are rebuilt with the new words. Whether anything
/// changed.
pub(super) fn follow_language(s: &mut Session, rename: impl Fn(&str) -> Option<&'static str>) -> bool {
    let mut changed = false;
    for i in tables(s) {
        let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b) else { continue };
        let mut m = read(t);
        for c in &mut m.criteria {
            if let Some(new) = rename(c) {
                *c = new.to_string();
            }
        }
        changed |= put(s, i, &m);
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

/// The scores table the caret is in, else the document's first.
fn target(s: &Session) -> Option<usize> {
    let all = tables(s);
    let at = (s.sel.focus.story == StoryRef::Body).then(|| s.sel.focus.path.0.first().map(|&i| i as usize)).flatten();
    at.filter(|i| all.contains(i)).or_else(|| all.first().copied())
}

fn state(s: &Session) -> Value {
    let Some(i) = target(s) else { return json!({"on": false}) };
    let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b) else { return json!({"on": false}) };
    let m = read(t);
    let rows = scan(s).exercises.len().min(MAX_ROWS);
    let points: Vec<Vec<Value>> =
        (0..rows).map(|r| (0..m.criteria.len()).map(|c| json!(m.points.get(r).and_then(|x| x.get(c)).copied().flatten())).collect()).collect();
    json!({"on": true, "criteria": m.criteria, "points": points, "exercises": rows})
}

fn name(v: &Value) -> Option<String> {
    v.as_str().map(|t| t.chars().filter(|c| !c.is_control() && *c != wordcraft_doc::para::OBJ).take(MAX_NAME).collect::<String>().trim().to_string())
}

/// `exercise.scores`: insert (`"on": true`) or remove (`false`) the scores table, add a criterion
/// (`"add"`: its name, or true for the default one), remove one (`"remove"`: n, 1-based) or rename
/// one (`"rename": n, "name"`), set an exercise's points (`"exercise": n, "criterion": n,
/// "points": number | null`); without params, the table at the caret (or the first).
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    match p::bool(v, "on") {
        Some(true) if target(s).is_none() => insert(s)?,
        Some(false) => {
            if let Some(i) = target(s) {
                s.doc.body.remove(i);
                s.doc.ensure_nonempty();
                s.touch();
                s.clamp_selection();
            }
            return Ok(state(s));
        }
        _ => {}
    }
    let edits = ["add", "remove", "rename", "points"].iter().any(|k| v.get(*k).is_some());
    if !edits {
        return Ok(state(s));
    }
    let i = target(s).ok_or_else(|| CmdError::Disabled("the document has no scores table".into()))?;
    let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b) else { return Err(CmdError::Failed("no scores table".into())) };
    let mut m = read(t);
    if let Some(a) = v.get("add") {
        if m.criteria.len() >= MAX_CRITERIA {
            return Err(CmdError::Failed(format!("a scores table has at most {MAX_CRITERIA} criteria")));
        }
        let n = match a {
            Value::Bool(true) => doc_words(s).points.to_string(),
            Value::String(_) => name(a).unwrap_or_default(),
            _ => return Err(CmdError::Params("add: a name, or true".into())),
        };
        m.criteria.push(n);
        for row in &mut m.points {
            row.push(None);
        }
    }
    if let Some(r) = v.get("remove") {
        let k = r
            .as_u64()
            .and_then(|k| usize::try_from(k).ok())
            .filter(|k| (1..=m.criteria.len()).contains(k))
            .ok_or_else(|| CmdError::Params("remove: a criterion number, from 1".into()))?;
        if m.criteria.len() == 1 {
            return Err(CmdError::Failed("the scores table keeps at least one criterion".into()));
        }
        m.criteria.remove(k - 1);
        for row in &mut m.points {
            if k - 1 < row.len() {
                row.remove(k - 1);
            }
        }
    }
    if let Some(r) = v.get("rename") {
        let k = r
            .as_u64()
            .and_then(|k| usize::try_from(k).ok())
            .filter(|k| (1..=m.criteria.len()).contains(k))
            .ok_or_else(|| CmdError::Params("rename: a criterion number, from 1".into()))?;
        let n = v.get("name").and_then(name).ok_or_else(|| CmdError::Params("name: the criterion's new name".into()))?;
        if let Some(c) = m.criteria.get_mut(k - 1) {
            *c = n;
        }
    }
    if let Some(pts) = v.get("points") {
        let ex = p::u64(v, "exercise")
            .and_then(|k| usize::try_from(k).ok())
            .filter(|k| *k >= 1)
            .ok_or_else(|| CmdError::Params("exercise: its number, from 1".into()))?;
        let c = p::u64(v, "criterion")
            .and_then(|k| usize::try_from(k).ok())
            .filter(|k| (1..=m.criteria.len()).contains(k))
            .ok_or_else(|| CmdError::Params("criterion: its number, from 1".into()))?;
        let value = match pts {
            Value::Null => None,
            x => Some(
                x.as_f64()
                    .filter(|f| f.is_finite())
                    .map(|f| f.clamp(-MAX_POINTS, MAX_POINTS))
                    .ok_or_else(|| CmdError::Params("points: a number, or null".into()))?,
            ),
        };
        let rows = scan(s).exercises.len().min(MAX_ROWS);
        if ex > rows {
            return Err(CmdError::Params(format!("exercise: the document has {rows}")));
        }
        while m.points.len() < ex {
            m.points.push(vec![None; m.criteria.len()]);
        }
        if let Some(row) = m.points.get_mut(ex - 1) {
            row.resize(m.criteria.len(), None);
            if let Some(cell) = row.get_mut(c - 1) {
                *cell = value;
            }
        }
    }
    put(s, i, &m);
    s.touch();
    s.clamp_selection();
    Ok(state(s))
}

/// A new scores table after the caret's paragraph (outside any exercise or other control), with
/// one criterion; the caret goes in its first criterion's name.
fn insert(s: &mut Session) -> Result<(), CmdError> {
    if scan(s).exercises.is_empty() {
        return Err(CmdError::Disabled("the document has no exercise".into()));
    }
    let at = s.sel.focus.clone();
    if at.story != StoryRef::Body || at.path.depth() != 0 || s.doc.control_at(&at).is_some() {
        return Err(CmdError::Failed(
            "put the caret in the main text outside the exercises (in the frontispiece, say) to insert the scores table there".into(),
        ));
    }
    let i = at.path.0.first().map_or(0, |&i| i as usize);
    let mut ctl = ContentControl::new(ControlKind::RichText);
    ctl.tag = TAG_SCORES.into();
    ctl.title = "Scores".into();
    ctl.block = true;
    ctl.lock = ControlLock::SdtLocked;
    write_count(&mut ctl, 1);
    let mut t = Table::new(1, 3, text_width(s, i));
    t.controls.open.push(ctl);
    t.controls.close = 1;
    let at_i = (i + 1).min(s.doc.body.len());
    s.doc.body.insert(at_i, Arc::new(Block::Table(t)));
    // A paragraph after it, so the text can go on after the table.
    if !matches!(s.doc.body.get(at_i + 1).map(|b| &**b), Some(Block::Para(_))) {
        s.doc.body.insert(at_i + 1, para_block(Paragraph::new()));
    }
    let m = Model { criteria: vec![doc_words(s).points.to_string()], points: Vec::new() };
    put(s, at_i, &m);
    s.touch();
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::{Block, Document};

    use super::cell_text;
    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn grid(s: &Session) -> Vec<Vec<String>> {
        let t = s.doc.body.iter().find_map(|b| match &**b {
            Block::Table(t) if super::control_of(t).is_some() => Some(t.clone()),
            _ => None,
        });
        t.map(|t| t.rows.iter().map(|r| r.cells.iter().map(cell_text).collect()).collect()).unwrap_or_default()
    }

    /// The owner's example: exercises 1–4 scored on IMP, MOD and CAL/RIS (2, 2, 1 each), a Total
    /// column across (5 each) and a Total row down (8, 8, 4, 20).
    #[test]
    fn a_scores_table_gathers_the_exercises_and_adds_up() {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": "Verifica"}));
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"title": "Uno", "questions": ["q"]}));
        // One exercise, one criterion: no totals.
        run(&mut s, "caret.docStart", json!({}));
        let st = run(&mut s, "exercise.scores", json!({"on": true}));
        assert_eq!(st["criteria"], json!(["Punti"]), "{st}");
        assert_eq!(grid(&s), vec![vec!["Esercizio", "Punti"], vec!["1", "", ""]]);
        // Three more exercises: their rows come by themselves, with the Total row.
        for t in ["Due", "Tre", "Quattro"] {
            run(&mut s, "caret.docEnd", json!({}));
            run(&mut s, "exercise.insert", json!({"title": t, "questions": ["q"]}));
        }
        assert_eq!(grid(&s).len(), 1 + 4 + 1, "{:?}", grid(&s));
        // The criteria: renamed and added (two columns each), the Total column comes with the second.
        run(&mut s, "exercise.scores", json!({"rename": 1, "name": "IMP"}));
        run(&mut s, "exercise.scores", json!({"add": "MOD"}));
        run(&mut s, "exercise.scores", json!({"add": "CAL/RIS"}));
        for ex in 1..=4 {
            for (c, pts) in [(1, 2), (2, 2), (3, 1)] {
                run(&mut s, "exercise.scores", json!({"exercise": ex, "criterion": c, "points": pts}));
            }
        }
        let g = grid(&s);
        assert_eq!(g[0], ["Esercizio", "IMP", "MOD", "CAL/RIS", "Totale"]);
        assert_eq!(g[1], ["1", "", "2", "", "2", "", "1", "", "5"]);
        assert_eq!(g[5], ["Totale", "", "8", "", "8", "", "4", "", "20"]);
        // Points typed in a cell count too, with a decimal comma: the totals follow at once.
        let i = s.doc.body.iter().position(|b| matches!(&**b, Block::Table(t) if super::control_of(t).is_some())).unwrap();
        let path = wordcraft_doc::Path(vec![i as u32, 2, 6, 0]);
        let len = s.doc.para(wordcraft_doc::StoryRef::Body, &path).map_or(0, |p| p.len());
        let pos = |off| wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, path.clone(), off);
        s.sel = crate::Selection { anchor: pos(0), focus: pos(len) };
        run(&mut s, "text.insert", json!({"text": "1,5"}));
        let g = grid(&s);
        assert_eq!(g[2][8], "5,5");
        assert_eq!(g[5][6], "4,5");
        assert_eq!(g[5][8], "20,5");
        // An exercise deleted: its row goes, the totals follow.
        run(&mut s, "exercise.delete", json!({"exercise": 4}));
        assert_eq!(grid(&s).len(), 1 + 3 + 1);
        assert_eq!(grid(&s)[4][8], "15,5");
        // Saved and opened again, it is still the scores table.
        let bytes = crate::io::save_bytes("s.xmath", &s.doc).unwrap();
        let mut back = Session::new(crate::io::open_bytes("s.xmath", &bytes).unwrap());
        assert_eq!(run(&mut back, "exercise.scores", json!({}))["criteria"], json!(["IMP", "MOD", "CAL/RIS"]));
        // A criterion removed: back to two, still with the Total column.
        run(&mut s, "exercise.scores", json!({"remove": 3}));
        assert_eq!(grid(&s)[0], ["Esercizio", "IMP", "MOD", "Totale"]);
        // Removed: one undo brings it back.
        run(&mut s, "exercise.scores", json!({"on": false}));
        assert!(grid(&s).is_empty());
        assert!(s.undo());
        assert!(!grid(&s).is_empty());
    }
}
