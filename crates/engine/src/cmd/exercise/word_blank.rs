//! Words hidden in a text to complete by hand (docs/question-modes.md, Inline completion).
//!
//! A hidden word is a `strix:blank` inline control (the same as a calculated blank) holding the
//! word itself (`strix:blankWord`) instead of a formula. The corrected copy shows the word; the
//! students' copy a line long enough to write it by hand, never shorter than
//! [`MIN_LINE`] underscores, so its length gives little away. Written as `[[word]]` in a text
//! to complete (an Inline Completion question: elsewhere `[[…]]` stays as typed, so a list
//! `[[1, 2], [3, 4]]` or an integer interval `[[1;n]]` is never taken), or made from a
//! selection inside an exercise with `exercise.blank` and no `expr`.
//!
//! Wherever the word shows (the corrected copy, while writing the exercise) it is in the
//! *Hidden Word* character style ([`STYLE`]: bold, italic, blue, added the first time), so the
//! words that will be hidden stand out; the students' line is in the text's own formatting.

use serde_json::{Value, json};
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{CharProps, Rgb, TextColor};
use wordcraft_doc::styles::{Style, StyleKind};
use wordcraft_doc::{Block, InlineObject, Paragraph, Pos, StoryRef};

use super::items::{Mode, TAG_ITEM};
use super::stem_items::TAG_BLANK;
use crate::{CmdError, CmdResult, Selection, Session};

const EL: &str = "strix:blankWord";
/// The control's title, shown on its frame: never the word, which the students' copy hides.
const TITLE: &str = "Hidden word";
/// The shortest line a hidden word gets in the students' copy.
const MIN_LINE: usize = 14;
/// The longest line (a whole phrase hidden).
const MAX_LINE: usize = 60;
const MAX_WORD_CHARS: usize = 200;
const MAX_PER_PARA: usize = 64;
/// The character style a hidden word shows in (`Hidden Word`).
pub(crate) const STYLE: &str = "HiddenWord";
const STYLE_NAME: &str = "Hidden Word";
const STYLE_COLOR: Rgb = Rgb(0x1F, 0x5F, 0xD0);

/// Adds the Hidden Word style (bold, italic, blue) when the document has none; a style the
/// teacher changed is kept as it is.
pub(crate) fn ensure_style(s: &mut Session) {
    if s.doc.styles.get(STYLE).is_some() {
        return;
    }
    std::sync::Arc::make_mut(&mut s.doc.styles).styles.push(Style {
        id: STYLE.into(),
        name: STYLE_NAME.into(),
        kind: StyleKind::Character,
        chr: CharProps { bold: Some(true), italic: Some(true), color: Some(TextColor::Rgb(STYLE_COLOR)), ..Default::default() },
        priority: Some(31),
        ..Default::default()
    });
}

/// Whether a block (or a cell's paragraph in it) holds a hidden word.
pub(crate) fn block_has_word(b: &Block) -> bool {
    let para = |p: &Paragraph| {
        p.object_offsets().into_iter().any(|o| matches!(p.object_at(o), Some(InlineObject::ControlStart { control }) if read(control).is_some()))
    };
    match b {
        Block::Para(p) => para(p),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|cb| matches!(&**cb, Block::Para(p) if para(p))),
    }
}

/// The formatting of a hidden word's text: the control's own (the text around it), in the
/// Hidden Word style while the word shows.
fn props_for(p: &Paragraph, at: usize, word_shows: bool) -> CharProps {
    let mut props = p.props_of_char(at).clone();
    if word_shows {
        props.style = Some(STYLE.into());
    } else if props.style.as_deref() == Some(STYLE) {
        props.style = None;
    }
    props
}

/// Whether the control `(a, b)` of `p` must be rewritten to show `want`: another text, or the
/// word without its style (or the line with it).
pub(crate) fn needs_write(p: &Paragraph, (a, b): (usize, usize), want: &str, teacher: bool) -> bool {
    let from = a + OBJ.len_utf8();
    if p.text.get(from..b) != Some(want) {
        return true;
    }
    b > from && (p.props_of_char(from).style.as_deref() == Some(STYLE)) != teacher
}

fn clean(raw: &str) -> String {
    raw.chars().filter(|&c| !c.is_control() && c != OBJ).take(MAX_WORD_CHARS).collect::<String>().trim().to_string()
}

/// The hidden word stored on a blank control (`None`: a calculated blank).
pub(crate) fn read(ctl: &ContentControl) -> Option<String> {
    if ctl.tag != TAG_BLANK {
        return None;
    }
    let (open, close) = (format!("<{EL}>"), format!("</{EL}>"));
    let raw = ctl.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close))?;
    let w = clean(&raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&"));
    (!w.is_empty()).then_some(w)
}

fn make(word: &str) -> ContentControl {
    let mut ctl = ContentControl::new(ControlKind::RichText);
    ctl.tag = TAG_BLANK.into();
    ctl.title = TITLE.into();
    ctl.block = false;
    ctl.lock = ControlLock::ContentLocked;
    let escaped = word.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    ctl.extra.push(format!("<{EL}>{escaped}</{EL}>"));
    ctl
}

/// What a hidden word shows: the word in the corrected copy, a line to write on otherwise.
pub(crate) fn shown(word: &str, teacher: bool) -> String {
    if teacher { word.to_string() } else { "_".repeat(word.chars().count().saturating_mul(2).clamp(MIN_LINE, MAX_LINE)) }
}

/// Wraps `from..to` of `p` (the word, already its text) in a hidden-word control.
fn wrap(p: &mut Paragraph, from: usize, to: usize, word: &str) -> bool {
    let props = props_for(p, from, false);
    if p.delete(from, to).is_err() {
        return false;
    }
    if p.insert_object(from, InlineObject::ControlStart { control: Box::new(make(word)) }, &props).is_err() {
        return false;
    }
    let inner = from + OBJ.len_utf8();
    let mut styled = props.clone();
    styled.style = Some(STYLE.into());
    let _ = p.insert_text(inner, word, &styled);
    let _ = p.insert_object(inner + word.len(), InlineObject::ControlEnd, &props);
    p.touch();
    true
}

/// Whether byte `at` of `p` lies inside an inline control opened in `p` other than the
/// question's text (a calculated blank, an expression, the points, a hidden word…).
fn in_other_control(p: &Paragraph, at: usize) -> bool {
    let mut open: Vec<bool> = Vec::new();
    for off in p.object_offsets() {
        if off >= at {
            break;
        }
        match p.object_at(off) {
            Some(InlineObject::ControlStart { control }) => open.push(!control.block && control.tag != TAG_ITEM),
            Some(InlineObject::ControlEnd) => {
                open.pop();
            }
            _ => {}
        }
    }
    open.last().copied().unwrap_or(false)
}

/// The next `[[word]]` of `p` that makes a hidden word: plain words (no brackets, fields or
/// controls inside), not inside another control. Its byte range and word.
fn next_template(p: &Paragraph) -> Option<(usize, usize, String)> {
    let mut from = 0usize;
    while let Some(rel) = p.text.get(from..)?.find("[[") {
        let open = from + rel;
        let after = open + 2;
        let close = after + p.text.get(after..)?.find("]]")?;
        let raw = p.text.get(after..close).unwrap_or_default();
        let plain = !raw.contains(OBJ) && !raw.contains(['[', ']']) && raw.chars().count() <= MAX_WORD_CHARS && !clean(raw).is_empty();
        if !plain || in_other_control(p, open) {
            from = after;
            continue;
        }
        return Some((open, close + 2, clean(raw)));
    }
    None
}

fn has_template(p: &Paragraph) -> bool {
    p.text.contains("[[") && p.text.contains("]]")
}

/// Whether a block holds a `[[word]]` still to make a hidden word.
pub(crate) fn block_has_template(b: &Block) -> bool {
    match b {
        Block::Para(p) => has_template(p),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|cb| matches!(&**cb, Block::Para(p) if has_template(p))),
    }
}

fn convert_para(p: &mut Paragraph) -> bool {
    let mut changed = false;
    for _ in 0..MAX_PER_PARA {
        let Some((a, b, word)) = next_template(p) else { break };
        let props = p.props_of_char(a).clone();
        if p.delete(a, b).is_err() || p.insert_text(a, &word, &props).is_err() {
            break;
        }
        if !wrap(p, a, a + word.len(), &word) {
            break;
        }
        changed = true;
    }
    changed
}

/// The top-level blocks of the Inline Completion questions (question and answer part).
fn completion_blocks(s: &Session) -> Vec<(usize, usize)> {
    let layout = super::scan(s);
    (0..layout.exercises.len())
        .flat_map(|e| super::items::items(s, &layout, e))
        .filter(|it| it.settings.mode == Mode::Cloze)
        .map(|it| (it.question, it.answer.map_or(it.question, |(_, b)| b.max(it.question))))
        .collect()
}

/// Makes every `[[word]]` of an Inline Completion question (`zones`: the exercise of each
/// top-level block, a quick first check) a hidden word. Whether anything changed.
pub(crate) fn convert_all(s: &mut Session, zones: &[Option<usize>]) -> bool {
    let candidate = |top: usize, b: &Block| zones.get(top).copied().flatten().is_some() && block_has_template(b);
    if !s.doc.body.iter().enumerate().any(|(top, b)| candidate(top, b)) {
        return false;
    }
    let ranges = completion_blocks(s);
    let mut changed = false;
    for (top, b) in s.doc.body.iter_mut().enumerate() {
        if !candidate(top, b) || !ranges.iter().any(|&(a, z)| (a..=z).contains(&top)) {
            continue;
        }
        match std::sync::Arc::make_mut(b) {
            Block::Para(p) => changed |= convert_para(p),
            Block::Table(t) => {
                for cb in t.rows.iter_mut().flat_map(|r| &mut r.cells).flat_map(|c| &mut c.blocks) {
                    if let Block::Para(p) = std::sync::Arc::make_mut(cb) {
                        changed |= convert_para(p);
                    }
                }
            }
        }
    }
    if changed {
        ensure_style(s);
        s.touch();
        s.clamp_selection();
    }
    changed
}

/// Writes `text` as the content of the control `(a, b)` of `p` (plain, no unit), in the
/// Hidden Word style when it is the word (`teacher`). Whether it did.
pub(crate) fn write(p: &mut Paragraph, (a, b): (usize, usize), text: &str, teacher: bool) -> bool {
    let from = a + OBJ.len_utf8();
    let props = props_for(p, a, teacher);
    if b > from && p.delete(from, b).is_err() {
        return false;
    }
    if p.insert_text(from, text, &props).is_err() {
        return false;
    }
    p.touch();
    true
}

/// `exercise.blank` without `expr`: hides the selected words (in one paragraph of the body).
pub(crate) fn hide_selection(s: &mut Session, _v: &Value) -> CmdResult {
    let (x, y) = s.sel.ordered();
    if s.sel.is_collapsed() {
        return Err(CmdError::Params("select the words to hide, or give expr".into()));
    }
    if x.story != StoryRef::Body || y.story != StoryRef::Body || x.path != y.path {
        return Err(CmdError::Disabled("select words within one paragraph of the body".into()));
    }
    // Only inside an exercise: elsewhere nothing would ever turn the words into a line.
    let top = x.path.0.first().map(|&t| t as usize);
    let in_exercise = top.is_some_and(|t| super::scan(s).parts.get(t).is_some_and(|(ex, _)| ex.is_some()));
    if !in_exercise {
        return Err(CmdError::Disabled("select words inside an exercise".into()));
    }
    let para = s.doc.para_mut(x.story, &x.path).map_err(|e| CmdError::Failed(e.to_string()))?;
    let (from, to) = (x.off.min(para.text.len()), y.off.min(para.text.len()));
    let raw = para.text.get(from..to).ok_or_else(|| CmdError::Params("the selection is not on whole characters".into()))?;
    if raw.contains(OBJ) || in_other_control(para, from) {
        return Err(CmdError::Disabled("the selection holds or is inside a field or a control".into()));
    }
    // Spaces selected around the words stay outside the blank.
    let lead = raw.len() - raw.trim_start().len();
    let word = clean(raw);
    if word.is_empty() {
        return Err(CmdError::Params("the selection has no words".into()));
    }
    let a = from + lead;
    let b = a + word.len();
    if para.text.get(a..b) != Some(word.as_str()) {
        return Err(CmdError::Params("select plain words to hide".into()));
    }
    if !wrap(para, a, b, &word) {
        return Err(CmdError::Failed("could not hide the words".into()));
    }
    ensure_style(s);
    let end = b + 2 * OBJ.len_utf8();
    s.sel = Selection::caret(Pos::new(x.story, x.path.clone(), end));
    s.touch();
    super::super::calc::recalculate_now(s);
    super::update(s);
    s.clamp_selection();
    Ok(json!({"blank": true, "word": word}))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use std::sync::Arc;

    use crate::Session;

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn texts(s: &Session) -> Vec<String> {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(super::super::plain).collect()
    }

    #[test]
    fn words_in_double_brackets_hide_in_the_students_copy() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "cloze", "questions": [{"text": "La [[fotosintesi]] avviene nelle [[foglie]]."}]}));
        run(&mut s, "exercise.show", json!({"solution": true}));
        assert!(texts(&s).iter().any(|t| t.contains("La fotosintesi avviene nelle foglie.")), "{:?}", texts(&s));
        run(&mut s, "exercise.show", json!({"solution": false}));
        let line = "_".repeat(22);
        let short = "_".repeat(14);
        assert!(texts(&s).iter().any(|t| t.contains(&format!("La {line} avviene nelle {short}."))), "{:?}", texts(&s));
        // Back: the words again, nothing lost.
        run(&mut s, "exercise.show", json!({"solution": true}));
        assert!(texts(&s).iter().any(|t| t.contains("La fotosintesi avviene nelle foglie.")), "{:?}", texts(&s));
    }

    #[test]
    fn the_results_summary_lists_hidden_words_and_names_open_answers() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        run(
            &mut s,
            "exercise.insert",
            json!({"kind": "open", "questions": [
                {"text": "Che cos'è un rettangolo?"},
                {"text": "La [[fotosintesi]] avviene nelle [[foglie]].", "type": "cloze"}
            ]}),
        );
        run(&mut s, "exercise.show", json!({"solution": true, "results": true}));
        let t = texts(&s);
        assert!(t.contains(&"a) domanda a risposta aperta".to_string()), "{t:?}");
        assert!(t.contains(&"b) fotosintesi, foglie".to_string()), "{t:?}");
    }

    /// The owner: open answers leave a trace in the summary even when the exercise has nothing
    /// else, so that no part is forgotten when correcting.
    #[test]
    fn open_answers_alone_are_named_in_the_results_summary() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        run(&mut s, "exercise.insert", json!({"kind": "open", "questions": [{"text": "Che cos'è un rettangolo?"}, {"text": "E un rombo?"}]}));
        // A second exercise with a lone question: the line has no letter.
        run(&mut s, "exercise.insert", json!({"kind": "open", "questions": [{"text": "Che cos'è un cerchio?"}]}));
        run(&mut s, "exercise.show", json!({"solution": true, "results": true}));
        let t = texts(&s);
        assert!(t.contains(&"a) domanda a risposta aperta".to_string()), "{t:?}");
        assert!(t.contains(&"b) domanda a risposta aperta".to_string()), "{t:?}");
        assert!(t.contains(&"domanda a risposta aperta".to_string()), "{t:?}");
    }

    #[test]
    fn a_selection_becomes_a_hidden_word() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "cloze", "questions": [{"text": "Il cuore pompa il sangue."}]}));
        run(&mut s, "select.text", json!({"text": " sangue"}));
        let r = run(&mut s, "exercise.blank", json!({}));
        assert_eq!(r["word"], "sangue");
        run(&mut s, "exercise.show", json!({"solution": false}));
        assert!(texts(&s).iter().any(|t| t.contains(&format!("Il cuore pompa il {}.", "_".repeat(14)))), "{:?}", texts(&s));
        // Nothing selected: an error, never a panic.
        assert!(s.run("exercise.blank", &json!({})).is_err());
    }

    /// `[[…]]` makes hidden words only in a text to complete, and never from a list or an
    /// interval: elsewhere the text stays exactly as typed.
    #[test]
    fn double_brackets_elsewhere_stay_as_typed() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(
            &mut s,
            "exercise.insert",
            json!({"kind": "open", "questions": [
                {"text": "Che cosa stampa a = [[1, 2], [3, 4]]?"},
                {"text": "Scrivi [[1;n]] in estensione."},
                {"text": "Il [[cuore]] pompa; a = [[1, 2], [3, 4]].", "type": "cloze"}
            ]}),
        );
        run(&mut s, "calc.recalculate", json!({}));
        run(&mut s, "exercise.show", json!({"solution": false}));
        let t = texts(&s);
        assert!(t.iter().any(|x| x.contains("a = [[1, 2], [3, 4]]?")), "{t:?}");
        assert!(t.iter().any(|x| x.contains("Scrivi [[1;n]] in estensione.")), "{t:?}");
        // In the completion the word hides, the list stays.
        assert!(t.iter().any(|x| x.contains(&format!("Il {} pompa; a = [[1, 2], [3, 4]].", "_".repeat(14)))), "{t:?}");
    }

    /// The hidden word's control never carries the word as its title (the frame shows it).
    #[test]
    fn the_control_title_never_gives_the_word_away() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "cloze", "questions": [{"text": "La [[fotosintesi]] avviene."}]}));
        run(&mut s, "exercise.show", json!({"solution": false}));
        let titles: Vec<String> = s
            .doc
            .control_ranges(wordcraft_doc::StoryRef::Body)
            .into_iter()
            .filter(|r| super::read(&r.control).is_some())
            .map(|r| r.control.title.clone())
            .collect();
        assert_eq!(titles, vec![super::TITLE.to_string()]);
    }

    /// A hidden word shows in the Hidden Word style (bold, italic, blue) so it stands out; the
    /// students' line is in the text's own formatting, and the style comes back with the word.
    #[test]
    fn hidden_words_show_in_their_own_style() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "cloze", "questions": [{"text": "La [[fotosintesi]] avviene."}]}));
        let st = s.doc.styles.get(super::STYLE).cloned().unwrap();
        assert_eq!(st.kind, wordcraft_doc::styles::StyleKind::Character);
        assert_eq!((st.chr.bold, st.chr.italic), (Some(true), Some(true)));
        assert_eq!(st.chr.color, Some(wordcraft_doc::props::TextColor::Rgb(super::STYLE_COLOR)));
        let style_of = |s: &Session, needle: &str| {
            s.doc.body.iter().filter_map(|b| b.as_para()).find_map(|p| p.text.find(needle).map(|at| p.props_of_char(at).style.clone()))
        };
        run(&mut s, "exercise.show", json!({"solution": true}));
        assert_eq!(style_of(&s, "fotosintesi"), Some(Some(super::STYLE.to_string())));
        assert_ne!(style_of(&s, "avviene"), Some(Some(super::STYLE.to_string())));
        run(&mut s, "exercise.show", json!({"solution": false}));
        let line = style_of(&s, "_____").expect("the students' line");
        assert_ne!(line.as_deref(), Some(super::STYLE));
        run(&mut s, "exercise.show", json!({"solution": true}));
        assert_eq!(style_of(&s, "fotosintesi"), Some(Some(super::STYLE.to_string())));
        // Hide Words styles the selected words too.
        run(&mut s, "select.text", json!({"text": "avviene"}));
        run(&mut s, "exercise.blank", json!({}));
        assert_eq!(style_of(&s, "avviene"), Some(Some(super::STYLE.to_string())));
    }

    /// Hide Words outside an exercise: refused, the text untouched.
    #[test]
    fn hide_words_outside_an_exercise_is_refused() {
        let mut s = Session::new(wordcraft_doc::Document::from_text("Il cuore pompa il sangue."));
        run(&mut s, "select.text", json!({"text": "sangue"}));
        assert!(s.run("exercise.blank", &json!({})).is_err());
        assert_eq!(texts(&s), vec!["Il cuore pompa il sangue.".to_string()]);
    }
}
