//! What an exercise's first line holds after its number (docs/exercise.md, Title line): a
//! **title** (bold, the title's placeholder), or the **statement** itself — no title at all, the
//! instruction or the question written right after `Esercizio 1.` in the body text's weight and
//! size, its placeholder the statement's (`Write the question here.`). Kept on the title
//! control (`strix:titleLine`, an element Word ignores); the number keeps the title's look.
//!
//! The statement **moves** onto the line, so it is never there twice: switching to the statement,
//! the statement part's first paragraph goes next to the number and the part goes (what follows
//! that paragraph — a figure, a table — stays in it); the title is kept aside on the control
//! (`strix:titleLineTitle`). Switching back, the line's text is the statement part's first
//! paragraph again and the title comes back. A statement part left empty while the line holds the
//! statement (a document from before, or the rest of it deleted) goes ([`conform`]).

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::control::{ContentControl, ControlLock};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Block, InlineObject, Paragraph, Pos, StoryRef, para_block};

use super::items::{Kind, read_kind, span};
use super::{MAX_TEXT, TAG_EXERCISE, TAG_TEXT, TAG_TITLE, Words, body_path, control, exercise_words, para, scan, target};
use crate::{CmdError, CmdResult, Selection, Session, p};

const EL: &str = "<strix:titleLine val=\"statement\"/>";
/// The title kept aside while the line holds the statement (plain text, escaped).
const STASH: &str = "strix:titleLineTitle";
/// The body text's size when the document's Normal style names none (Word's default).
const NORMAL_SIZE: f32 = 11.0;

/// What the first line holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line {
    Title,
    Statement,
}

impl Line {
    fn name(self) -> &'static str {
        match self {
            Line::Title => "title",
            Line::Statement => "statement",
        }
    }

    fn parse(t: &str) -> Option<Line> {
        match t {
            "title" => Some(Line::Title),
            "statement" => Some(Line::Statement),
            _ => None,
        }
    }
}

pub(crate) fn read(ctl: &ContentControl) -> Line {
    if ctl.extra.iter().any(|x| x == EL) { Line::Statement } else { Line::Title }
}

/// The title kept aside on the title control.
fn read_stash(ctl: &ContentControl) -> Option<String> {
    let (open, close) = (format!("<{STASH}>"), format!("</{STASH}>"));
    let raw = ctl.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close))?;
    Some(raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&"))
}

/// Keep `title` aside on the title control (nothing: none kept).
fn write_stash(ctl: &mut ContentControl, title: Option<&str>) {
    let open = format!("<{STASH}>");
    ctl.extra.retain(|x| !x.starts_with(&open));
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        let t: String = t.chars().take(MAX_TEXT).collect();
        let escaped = t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        ctl.extra.push(format!("{open}{escaped}</{STASH}>"));
    }
}

/// The placeholder a title control shows for `line`: the title's, or the statement's — in a
/// quiz too, whose questions keep their own fields under the line (the line never shows a second
/// "question" over the first one).
pub(super) fn placeholder(w: &Words, line: Line, _kind: Kind) -> &'static str {
    match line {
        Line::Title => w.title,
        Line::Statement => w.text,
    }
}

/// The exercise's kind and the title control's line, read from the title paragraph `p`.
pub(super) fn of_para(p: &Paragraph) -> Option<(Kind, Line)> {
    let mut kind = Kind::default();
    for o in &p.objects {
        if let InlineObject::ControlStart { control } = o {
            if control.tag == TAG_EXERCISE {
                kind = read_kind(control);
            } else if control.tag == TAG_TITLE {
                return Some((kind, read(control)));
            }
        }
    }
    None
}

/// Whether the exercise whose title is top-level block `start` holds its statement on that line.
pub(super) fn holds_statement(s: &Session, start: usize) -> bool {
    s.doc.body.get(start).and_then(|b| b.as_para()).and_then(of_para).is_some_and(|(_, l)| l == Line::Statement)
}

/// The run properties of a statement written on the title line: the body text's weight and size.
fn statement_props(s: &Session) -> CharProps {
    let size = s.doc.styles.get("Normal").and_then(|st| st.chr.size).unwrap_or(NORMAL_SIZE);
    CharProps { bold: Some(false), size: Some(size), ..CharProps::default() }
}

fn title_showing(p: &Paragraph) -> bool {
    p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if control.tag == TAG_TITLE && control.showing_placeholder))
}

fn title_ctl(p: &mut Paragraph) -> Option<&mut ContentControl> {
    p.objects.iter_mut().find_map(|o| match o {
        InlineObject::ControlStart { control } if control.tag == TAG_TITLE => Some(&mut **control),
        _ => None,
    })
}

/// Sets `line` on the title paragraph `p`: the control's element, placeholder and run properties,
/// and the text's weight and size. Whether anything changed.
fn apply(p: &mut Paragraph, line: Line, w: &Words, props: &CharProps) -> bool {
    let Some((kind, old)) = of_para(p) else { return false };
    let Some((a, b)) = span(p, TAG_TITLE) else { return false };
    let shown = placeholder(w, line, kind);
    let mut showing = false;
    if let Some(control) = title_ctl(p) {
        control.extra.retain(|x| x != EL);
        if line == Line::Statement {
            control.extra.push(EL.into());
            control.run_props = Some(props.clone());
        } else {
            control.run_props = None;
        }
        control.placeholder_text = shown.into();
        showing = control.showing_placeholder;
    }
    let from = a + OBJ.len_utf8();
    // A placeholder showing reads the new one.
    if showing && p.text.get(from..b) != Some(shown) {
        let _ = super::items::set_content(p, (a, b), shown);
    }
    let (_, b) = span(p, TAG_TITLE).unwrap_or((a, b));
    // The text's weight and size: the body text's where it sets none (a word made bold in the
    // statement stays bold), or the title style's again.
    let statement = line == Line::Statement;
    let differs = p.run_ranges().any(|(r, pr)| {
        r.start < b && r.end > from && if statement { pr.bold.is_none() || pr.size.is_none() } else { pr.bold.is_some() || pr.size.is_some() }
    });
    if differs && b > from {
        let _ = p.format(from, b, &|pr| {
            if statement {
                pr.bold = pr.bold.or(props.bold);
                pr.size = pr.size.or(props.size);
            } else {
                pr.bold = None;
                pr.size = None;
            }
        });
    }
    let changed = differs || old != line;
    if changed {
        p.touch();
    }
    changed
}

/// The statement part right after the title (top-level block `at`): its first and last block.
fn statement_range(blocks: &[Arc<Block>], at: usize) -> Option<(usize, usize)> {
    let j = at.checked_add(1)?;
    let Block::Para(first) = &**blocks.get(j)? else { return None };
    match first.objects_with_offsets().next() {
        Some((0, InlineObject::ControlStart { control })) if control.tag == TAG_TEXT => {}
        _ => return None,
    }
    let mut depth = 0usize;
    for (k, b) in blocks.iter().enumerate().skip(j) {
        let Block::Para(p) = &**b else { continue };
        for o in &p.objects {
            match o {
                InlineObject::ControlStart { .. } => depth += 1,
                InlineObject::ControlEnd => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some((j, k));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Whether the statement control opening block `j` shows its placeholder.
fn showing(blocks: &[Arc<Block>], j: usize) -> bool {
    blocks.get(j).and_then(|b| b.as_para()).is_some_and(
        |p| matches!(p.objects.first(), Some(InlineObject::ControlStart { control }) if control.tag == TAG_TEXT && control.showing_placeholder),
    )
}

/// Control markers in `from..to` of `p` that open and close each other (none left open).
fn balanced(p: &Paragraph, from: usize, to: usize) -> bool {
    let mut depth = 0usize;
    for (at, o) in p.objects_with_offsets() {
        if at < from || at >= to {
            continue;
        }
        match o {
            InlineObject::ControlStart { .. } => depth += 1,
            InlineObject::ControlEnd => match depth.checked_sub(1) {
                Some(d) => depth = d,
                None => return false,
            },
            _ => {}
        }
    }
    depth == 0
}

/// Takes `from..to` out of `p` as a paragraph of its own (its runs and objects).
fn take(p: &mut Paragraph, from: usize, to: usize) -> Option<Paragraph> {
    let mut mid = p.split_off(from).ok()?;
    match mid.split_off(to.checked_sub(from)?) {
        Ok(rest) => {
            p.append(rest);
            Some(mid)
        }
        Err(_) => {
            p.append(mid);
            None
        }
    }
}

/// Puts `mid`'s content into `p` at `at`.
fn put(p: &mut Paragraph, at: usize, mid: Paragraph) -> bool {
    let Ok(tail) = p.split_off(at) else { return false };
    p.append(mid);
    p.append(tail);
    true
}

/// The title line's content (from after its control's start to its end) replaced by `mid`, or by
/// `text` (`showing`: the placeholder).
fn set_line(p: &mut Paragraph, mid: Option<Paragraph>, text: &str, showing: bool) {
    let Some((a, b)) = span(p, TAG_TITLE) else { return };
    let from = a + OBJ.len_utf8();
    match mid {
        Some(mid) if b >= from && p.delete(from, b).is_ok() => {
            put(p, from, mid);
        }
        _ => {
            super::items::set_content(p, (a, b), text);
        }
    }
    let shows = showing && span(p, TAG_TITLE).is_some_and(|(a, b)| p.text.get(a + OBJ.len_utf8()..b) == Some(text));
    if let Some(c) = title_ctl(p) {
        c.showing_placeholder = shows;
    }
    p.touch();
}

/// Moves the statement onto the title line of the exercise whose title is `blocks[at]` (see the
/// module docs). Whether anything changed.
fn to_statement(blocks: &mut Vec<Arc<Block>>, at: usize, w: &Words) -> bool {
    let range = statement_range(blocks, at);
    // The statement's first paragraph, when it has text to move.
    let mut moved = None;
    if let Some((j, k)) = range {
        if showing(blocks, j) {
            if j == k {
                blocks.remove(j);
            }
        } else if let Some(Block::Para(first)) = blocks.get(j).map(|b| &**b) {
            let mut first = first.clone();
            let from = OBJ.len_utf8();
            let to = if j == k { span(&first, TAG_TEXT).map_or(first.text.len(), |(_, b)| b) } else { first.text.len() };
            let next_para = matches!(blocks.get(j + 1).map(|b| &**b), Some(Block::Para(_)));
            if to > from
                && balanced(&first, from, to)
                && (j == k || next_para)
                && let Some(mid) = take(&mut first, from, to)
            {
                moved = Some(mid);
                let opener = first.objects.first().cloned();
                blocks.remove(j);
                // The part goes on with what followed that paragraph.
                if j < k
                    && let (Some(opener), Some(next)) = (opener, blocks.get_mut(j))
                    && let Block::Para(next) = Arc::make_mut(next)
                {
                    let props = next.props_of_char(0).clone();
                    let _ = next.insert_object(0, opener, &props);
                }
            }
        }
    }
    let Some(Block::Para(title)) = blocks.get_mut(at).map(Arc::make_mut) else { return false };
    let Some((kind, _)) = of_para(title) else { return false };
    // The title is kept aside.
    let kept = span(title, TAG_TITLE)
        .filter(|_| !title_showing(title))
        .map(|(a, b)| title.text.get(a + OBJ.len_utf8()..b).unwrap_or_default().chars().filter(|&c| c != OBJ).collect::<String>());
    if let Some(c) = title_ctl(title) {
        write_stash(c, kept.as_deref());
    }
    let shown = placeholder(w, Line::Statement, kind);
    let empty = moved.is_none();
    set_line(title, moved, shown, empty);
    true
}

/// Moves the title line's text back into the statement part and the title back onto the line.
fn to_title(blocks: &mut Vec<Arc<Block>>, at: usize, w: &Words, props: &CharProps) -> bool {
    let Some(Block::Para(title)) = blocks.get_mut(at).map(Arc::make_mut) else { return false };
    let Some((kind, _)) = of_para(title) else { return false };
    let Some((a, b)) = span(title, TAG_TITLE) else { return false };
    let from = a + OBJ.len_utf8();
    let shows = title_showing(title);
    let mut mid = if shows || b <= from || !balanced(title, from, b) { None } else { take(title, from, b) };
    let kept = title_ctl(title).and_then(|c| read_stash(c));
    if let Some(c) = title_ctl(title) {
        write_stash(c, None);
    }
    match kept {
        Some(t) => set_line(title, None, &t, false),
        None => set_line(title, None, w.title, true),
    }
    // The body text's weight and size the line gave it are the statement's own anyway.
    if let Some(m) = mid.as_mut()
        && !m.text.is_empty()
    {
        let len = m.text.len();
        let _ = m.format(0, len, &|pr| {
            if pr.bold == props.bold {
                pr.bold = None;
            }
            if pr.size == props.size {
                pr.size = None;
            }
        });
    }
    let mut range = statement_range(blocks, at);
    // A statement part only showing its placeholder gives way to the text.
    let mut reuse: Option<ContentControl> = None;
    if mid.is_some()
        && let Some((j, k)) = range
        && j == k
        && showing(blocks, j)
    {
        if let Some(InlineObject::ControlStart { control }) = blocks.get(j).and_then(|b| b.as_para()).and_then(|p| p.objects.first()) {
            reuse = Some((**control).clone());
        }
        blocks.remove(j);
        range = None;
    }
    let Some(mid) = mid else {
        // Nothing on the line: a problem gets its statement part back, empty; a quiz needs none.
        if range.is_none() && !kind.quiz() {
            let mut c = control(TAG_TEXT, w.statement, ControlLock::SdtLocked);
            c.placeholder_text = w.text.into();
            c.showing_placeholder = true;
            blocks.insert((at + 1).min(blocks.len()), para_block(para("Normal", w.text, vec![c], 1, None)));
        }
        return true;
    };
    // The part's start moves onto the line's text, its first paragraph again.
    let opener = match range {
        Some((j, _)) => match blocks.get_mut(j).map(Arc::make_mut) {
            Some(Block::Para(p)) => {
                let c = match p.objects.first() {
                    Some(InlineObject::ControlStart { control }) => Some((**control).clone()),
                    _ => None,
                };
                if c.is_some() {
                    let _ = p.delete(0, OBJ.len_utf8());
                }
                let empty = p.text.is_empty() && p.objects.is_empty();
                if empty {
                    blocks.remove(j);
                }
                c
            }
            _ => None,
        },
        None => reuse,
    };
    let mut c = opener.unwrap_or_else(|| {
        let mut c = control(TAG_TEXT, w.statement, ControlLock::SdtLocked);
        c.placeholder_text = w.text.into();
        c
    });
    c.showing_placeholder = false;
    let mut np = para("Normal", "", vec![c], 0, None);
    np.append(mid);
    if range.is_none() {
        let len = np.text.len();
        let props = np.props_of_char(len.saturating_sub(1)).clone();
        let _ = np.insert_object(len, InlineObject::ControlEnd, &props);
    }
    blocks.insert((at + 1).min(blocks.len()), para_block(np));
    true
}

/// Sets `line` on the exercise whose title is `blocks[at]`, moving the statement. Whether
/// anything changed.
fn switch(blocks: &mut Vec<Arc<Block>>, at: usize, line: Line, w: &Words, props: &CharProps) -> bool {
    let Some(old) = blocks.get(at).and_then(|b| b.as_para()).and_then(of_para).map(|(_, l)| l) else { return false };
    let mut changed = false;
    if old != line {
        changed |= match line {
            Line::Statement => to_statement(blocks, at, w),
            Line::Title => to_title(blocks, at, w, props),
        };
    }
    if let Some(Block::Para(p)) = blocks.get_mut(at).map(Arc::make_mut) {
        changed |= apply(p, line, w, props);
    }
    changed
}

/// `exercise.titleLine`: what the first line of an exercise holds after its number.
pub(super) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let e = target(s, v)?;
    let layout = scan(s);
    let ex = layout.exercises.get(e).ok_or_else(|| CmdError::Params("no such exercise".into()))?;
    let start = ex.start;
    let want = match p::str(v, "line") {
        None => None,
        Some(t) => Some(Line::parse(t).ok_or_else(|| CmdError::Params("line: \"title\" or \"statement\"".into()))?),
    };
    if !matches!(s.doc.body.get(start).map(|b| &**b), Some(Block::Para(p)) if of_para(p).is_some()) {
        return Err(CmdError::Failed("the exercise has no title line".into()));
    }
    if let Some(line) = want {
        let w = exercise_words(s, &layout, e);
        let props = statement_props(s);
        if switch(&mut s.doc.body, start, line, w, &props) {
            // The caret on the line, where the text now is.
            if let Some((a, _)) = s.doc.body.get(start).and_then(|b| b.as_para()).and_then(|p| span(p, TAG_TITLE)) {
                s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(start), a + OBJ.len_utf8()));
            }
            s.touch();
            super::update(s);
            s.clamp_selection();
        }
    }
    let line = s.doc.body.get(start).and_then(|b| b.as_para()).and_then(of_para).map_or(Line::Title, |(_, l)| l);
    Ok(json!({"exercise": e + 1, "line": line.name()}))
}

/// `exercise.insert`'s `"titleLine"`: the new exercise's first line (`blocks`, its title first).
pub(super) fn on_insert(s: &Session, blocks: &mut Vec<Paragraph>, v: &Value) {
    if p::str(v, "titleLine").and_then(Line::parse) != Some(Line::Statement) || blocks.is_empty() {
        return;
    }
    let props = statement_props(s);
    let mut arcs: Vec<Arc<Block>> = std::mem::take(blocks).into_iter().map(para_block).collect();
    switch(&mut arcs, 0, Line::Statement, super::words(s), &props);
    *blocks = arcs
        .into_iter()
        .filter_map(|b| match Arc::unwrap_or_clone(b) {
            Block::Para(p) => Some(p),
            Block::Table(_) => None,
        })
        .collect();
}

/// An exercise holding its statement on the title line has no statement part left only showing
/// its placeholder (a document from before the statement moved, or its rest deleted): it goes.
/// Whether anything changed.
pub(super) fn conform(s: &mut Session) -> bool {
    let titles: Vec<usize> = s
        .doc
        .body
        .iter()
        .enumerate()
        .filter(|(_, b)| b.as_para().and_then(of_para).is_some_and(|(_, l)| l == Line::Statement))
        .map(|(i, _)| i)
        .collect();
    let mut changed = false;
    for &at in titles.iter().rev() {
        if let Some((j, k)) = statement_range(&s.doc.body, at)
            && j == k
            && showing(&s.doc.body, j)
        {
            s.doc.body.remove(j);
            changed = true;
        }
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn title_para(s: &Session) -> Paragraph {
        s.doc.body.first().and_then(|b| b.as_para()).cloned().unwrap_or_default()
    }

    /// The paragraphs of the statement part (`strix:exercise.text`), as plain text.
    fn statement(s: &Session) -> Vec<String> {
        let layout = scan(s);
        layout
            .parts
            .iter()
            .zip(s.doc.body.iter())
            .filter(|((_, part), _)| *part == super::super::Part::Text)
            .filter_map(|(_, b)| b.as_para().map(super::super::plain))
            .collect()
    }

    fn line_text(s: &Session) -> String {
        super::super::plain(&title_para(s))
    }

    #[test]
    fn the_statement_goes_next_to_the_number() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "open"}));
        assert_eq!(run(&mut s, "exercise.titleLine", json!({}))["line"], "title");
        let st = run(&mut s, "exercise.titleLine", json!({"line": "statement"}));
        assert_eq!(st["line"], "statement");
        let p = title_para(&s);
        // The placeholder is the statement's (the questions keep theirs), in the body text's weight.
        assert!(super::super::plain(&p).contains("Write the exercise here"), "{}", super::super::plain(&p));
        let (a, _) = span(&p, TAG_TITLE).unwrap_or((0, 0));
        assert_eq!(p.props_of_char(a + OBJ.len_utf8()).bold, Some(false));
        // Typed over (the caret is on the line), the text keeps that weight.
        run(&mut s, "text.insert", json!({"text": "Rispondi alle domande"}));
        let p = title_para(&s);
        assert!(super::super::plain(&p).contains("Rispondi alle domande"));
        let (a, _) = span(&p, TAG_TITLE).unwrap_or((0, 0));
        assert_eq!(p.props_of_char(a + OBJ.len_utf8()).bold, Some(false));
        // Back to a title: the text is the statement (a quiz gets one for it), the title its
        // placeholder, bold again (the style's).
        run(&mut s, "exercise.titleLine", json!({"line": "title"}));
        let p = title_para(&s);
        assert_eq!(p.props_of_char(a + OBJ.len_utf8()).bold, None);
        assert!(line_text(&s).contains("Exercise title"), "{}", line_text(&s));
        assert_eq!(statement(&s), ["Rispondi alle domande"]);
    }

    #[test]
    fn inserted_with_the_statement_on_the_first_line() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "problem", "titleLine": "statement"}));
        assert_eq!(run(&mut s, "exercise.titleLine", json!({}))["line"], "statement");
        assert!(line_text(&s).contains("Write the exercise here"));
        // No statement part besides: the line is the statement.
        assert!(statement(&s).is_empty(), "{:?}", statement(&s));
        assert!(s.run("exercise.titleLine", &json!({"line": "nothing"})).is_err());
        // With a text: it is on the line.
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"title": "Moto", "text": "Calcola la velocità.", "titleLine": "statement"}));
        assert!(line_text(&s).contains("Calcola la velocità."), "{}", line_text(&s));
        assert!(!line_text(&s).contains("Moto"));
        assert!(statement(&s).is_empty());
        // Back to a title: the title kept aside comes back, the statement is its own part again.
        run(&mut s, "exercise.titleLine", json!({"line": "title"}));
        assert!(line_text(&s).contains("Moto"), "{}", line_text(&s));
        assert_eq!(statement(&s), ["Calcola la velocità."]);
    }

    /// The owner: "with Text next to the number the text field must not be there: it moves".
    #[test]
    fn the_statement_moves_and_comes_back() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        run(&mut s, "exercise.insert", json!({"title": "Velocità", "questions": ["q"]}));
        // An empty statement: it goes, the line shows its placeholder.
        run(&mut s, "exercise.titleLine", json!({"line": "statement"}));
        assert!(statement(&s).is_empty(), "{:?}", statement(&s));
        assert!(line_text(&s).contains("Scrivi qui il testo"), "{}", line_text(&s));
        assert!(!line_text(&s).contains("Velocità"));
        // Back: the title and an empty statement part.
        run(&mut s, "exercise.titleLine", json!({"line": "title"}));
        assert!(line_text(&s).contains("Velocità"), "{}", line_text(&s));
        assert_eq!(statement(&s).len(), 1);
        // A statement with text and a second paragraph: the first goes on the line, the second
        // stays in the part; a word made bold stays bold.
        run(&mut s, "select.text", json!({"text": "Scrivi qui il testo dell'esercizio: dati, figure, tabelle."}));
        run(&mut s, "text.insert", json!({"text": "Un treno parte."}));
        run(&mut s, "text.newParagraph", json!({}));
        run(&mut s, "text.insert", json!({"text": "Figura sotto."}));
        run(&mut s, "select.text", json!({"text": "treno"}));
        run(&mut s, "format.bold", json!({}));
        assert_eq!(statement(&s), ["Un treno parte.", "Figura sotto."]);
        run(&mut s, "exercise.titleLine", json!({"line": "statement"}));
        assert!(line_text(&s).contains("Un treno parte."), "{}", line_text(&s));
        assert_eq!(statement(&s), ["Figura sotto."]);
        let p = title_para(&s);
        let at = p.text.find("treno").unwrap_or(0);
        assert_eq!(p.props_of_char(at).bold, Some(true));
        let at = p.text.find("Un").unwrap_or(0);
        assert_eq!(p.props_of_char(at).bold, Some(false));
        // One undo step back.
        assert!(s.undo());
        assert_eq!(statement(&s), ["Un treno parte.", "Figura sotto."]);
        assert!(s.redo());
        // Saved and opened again: still on the line, the title kept aside.
        for name in ["e.docx", "e.xmath"] {
            let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
            let mut back = Session::new(crate::io::open_bytes(name, &bytes).unwrap());
            run(&mut back, "exercise.update", json!({}));
            assert_eq!(run(&mut back, "exercise.titleLine", json!({}))["line"], "statement", "{name}");
            assert_eq!(statement(&back), ["Figura sotto."], "{name}");
            run(&mut back, "exercise.titleLine", json!({"line": "title"}));
            assert!(line_text(&back).contains("Velocità"), "{name}: {}", line_text(&back));
            assert_eq!(statement(&back), ["Un treno parte.", "Figura sotto."], "{name}");
        }
        // Back in this session too.
        run(&mut s, "exercise.titleLine", json!({"line": "title"}));
        assert_eq!(statement(&s), ["Un treno parte.", "Figura sotto."]);
        let p = s.doc.body.get(1).and_then(|b| b.as_para()).cloned().unwrap_or_default();
        let at = p.text.find("Un").unwrap_or(0);
        assert_eq!(p.props_of_char(at).bold, None, "the line's weight is not left on the statement");
    }

    /// A document from before the statement moved: an empty statement part beside a line holding
    /// the statement goes; the data go after the title.
    #[test]
    fn an_empty_statement_beside_the_line_goes() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"questions": ["q"]}));
        // The old shape: the line's element set by hand, the statement part left.
        if let Some(Block::Para(p)) = s.doc.body.first_mut().map(Arc::make_mut)
            && let Some(c) = title_ctl(p)
        {
            c.extra.push(EL.into());
        }
        s.touch();
        run(&mut s, "exercise.update", json!({}));
        assert!(statement(&s).is_empty(), "{:?}", statement(&s));
        // Data shown with the statement on the line: right after the title.
        run(
            &mut s,
            "exercise.setData",
            json!({"data": {"items": [{"symbol": "v", "unit": "m/s", "values": ["3"]}], "variants": ["A"], "active": 0}}),
        );
        let layout = scan(&s);
        let parts: Vec<_> = layout.parts.iter().map(|(_, p)| *p).collect();
        assert_eq!(parts.get(1), Some(&super::super::Part::Data), "{parts:?}");
    }
}
