//! Header & Footer options for the section being edited (docs/headers-footers.md): the section
//! a header or footer belongs to is the one of the page it is edited on (`Session::page_hint`),
//! so a header inherited from an earlier section is edited for the page's own section. Link to
//! Previous is separate for the header and the footer: linked, the section has none of its own
//! and shows the previous section's (each slot: first page, even pages, the others); unlinked, it
//! gets its own copies of what it showed, to change without touching the other sections.

use serde_json::{Value, json};
use wordcraft_doc::section::HeaderSet;
use wordcraft_doc::{PartKind, StoryRef};

use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

const LOC: &str = "Header & Footer › Navigation";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("hf.state", "Header & Footer Options", LOC, |s, _| Ok(state(s)))
            .params(r#"→ {"editing": "header" | "footer" | null, "section": n (1-based), "sections": n, "headerLinked", "footerLinked": bool (the section shows the previous one's), "firstPage": bool (Different First Page, the section's), "oddEven": bool (Different Odd & Even Pages, the document's)}"#)
            .pure(),
    ]
}

/// Whether the caret is in a header (`Some(true)`), a footer (`Some(false)`) or neither.
fn editing(s: &Session) -> Option<bool> {
    let StoryRef::Part(id) = s.sel.focus.story else { return None };
    match s.doc.parts.get(&id)?.kind {
        PartKind::Header => Some(true),
        PartKind::Footer => Some(false),
        _ => None,
    }
}

/// The section (index into `Document::sections()`) the caret's header or footer is edited for:
/// the one of the page it is edited on (even when a change made that page show another one);
/// in the body, the caret's.
fn focus_section(s: &mut Session) -> usize {
    if editing(s).is_some() {
        let layout = s.layout();
        if let Some(pg) = layout.pages.get(s.page_hint) {
            return pg.section;
        }
    }
    super::page::section_index_of_focus(s)
}

fn set_of(sp: &wordcraft_doc::SectionProps, footer: bool) -> &HeaderSet {
    if footer { &sp.footers } else { &sp.headers }
}

fn linked(set: &HeaderSet) -> bool {
    set.default.is_none() && set.first.is_none() && set.even.is_none()
}

fn state(s: &mut Session) -> Value {
    let k = focus_section(s);
    let sections = s.doc.sections();
    let n = sections.len();
    let (header_linked, footer_linked, first) =
        sections.get(k).map_or((false, false, false), |(_, sp)| (k > 0 && linked(&sp.headers), k > 0 && linked(&sp.footers), sp.title_page));
    json!({
        "editing": editing(s).map(|h| if h { "header" } else { "footer" }),
        "section": k + 1,
        "sections": n,
        "headerLinked": header_linked,
        "footerLinked": footer_linked,
        "firstPage": first,
        "oddEven": s.doc.settings.even_odd_headers,
    })
}

/// The header (or footer) part a slot of section `upto` shows: its own, else the nearest earlier
/// section's in the same slot.
fn shown(s: &Session, upto: usize, footer: bool, slot: fn(&HeaderSet) -> Option<u32>) -> Option<u32> {
    s.doc
        .sections()
        .iter()
        .take(upto.saturating_add(1))
        .filter_map(|(_, sp)| slot(set_of(sp, footer)))
        .filter(|id| s.doc.parts.contains_key(id))
        .last()
}

/// After a link change, the caret goes to what the page it was on shows (when editing that kind).
fn follow(s: &mut Session, footer: bool) {
    if editing(s) != Some(!footer) {
        return;
    }
    let layout = s.layout();
    let shown = layout.pages.get(s.page_hint).and_then(|pg| if footer { pg.footer_story } else { pg.header_story });
    if let Some(id) = shown.filter(|id| s.doc.parts.contains_key(id)) {
        s.sel = Selection::caret(s.doc.end_of(StoryRef::Part(id)));
    }
}

/// `layout.linkToPrevious`: link (or unlink) the header — or the footer, `"footer": true`; by
/// default the one being edited — of the section being edited to the previous section's.
/// `"value"`: true links, false unlinks, left out toggles.
pub(crate) fn link(s: &mut Session, v: &Value) -> CmdResult {
    let footer = p::bool(v, "footer").unwrap_or(editing(s) == Some(false));
    let k = focus_section(s);
    if k == 0 {
        return Err(CmdError::Disabled("the first section has no section before it to link to".into()));
    }
    let Some(end) = s.doc.sections().get(k).map(|(end, _)| *end) else { return Err(CmdError::Failed("no such section".into())) };
    let is_linked = s.doc.sections().get(k).is_some_and(|(_, sp)| linked(set_of(sp, footer)));
    let want = p::bool(v, "value").unwrap_or(!is_linked);
    if want && !is_linked {
        let sect = s.doc.section_mut(end);
        if footer {
            sect.footers = HeaderSet::default();
        } else {
            sect.headers = HeaderSet::default();
        }
    } else if !want && is_linked {
        // Its own copies of what it showed, slot by slot.
        let kind = if footer { PartKind::Footer } else { PartKind::Header };
        let slots: [fn(&HeaderSet) -> Option<u32>; 3] = [|h| h.default, |h| h.first, |h| h.even];
        let mut own = HeaderSet::default();
        for (i, slot) in slots.into_iter().enumerate() {
            let Some(from) = shown(s, k - 1, footer, slot) else { continue };
            let Some(blocks) = s.doc.parts.get(&from).map(|p| p.blocks.clone()) else { continue };
            let id = s.doc.add_part(kind, blocks);
            match i {
                0 => own.default = Some(id),
                1 => own.first = Some(id),
                _ => own.even = Some(id),
            }
        }
        // Nothing to copy: an empty one of its own, so it no longer follows the previous section.
        if linked(&own) {
            let style = if footer { "Footer" } else { "Header" };
            own.default = Some(s.doc.add_part(kind, vec![wordcraft_doc::para_block(wordcraft_doc::Paragraph::new().styled(style))]));
        }
        let sect = s.doc.section_mut(end);
        if footer {
            sect.footers = own;
        } else {
            sect.headers = own;
        }
    }
    s.touch();
    follow(s, footer);
    Ok(state(s))
}

/// `layout.differentFirstPage`: the section being edited (or those of the selection in the body)
/// has a different first page header and footer; `"value"` sets it, left out toggles.
pub(crate) fn different_first(s: &mut Session, v: &Value) -> CmdResult {
    if editing(s).is_none() {
        let on = p::bool(v, "value").unwrap_or(!super::page::sect(s).title_page);
        return super::page::with_sect(s, |x| x.title_page = on);
    }
    let k = focus_section(s);
    let Some((end, now)) = s.doc.sections().get(k).map(|(end, sp)| (*end, sp.title_page)) else {
        return Err(CmdError::Failed("no such section".into()));
    };
    let on = p::bool(v, "value").unwrap_or(!now);
    s.doc.section_mut(end).title_page = on;
    s.touch();
    Ok(state(s))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::Document;

    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// Two sections (a frontispiece, then the rest), a header and a footer on the first.
    fn two_sections() -> Session {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": "Cover\nBody"}));
        run(&mut s, "select.text", json!({"text": "Body"}));
        run(&mut s, "layout.break", json!({"kind": "nextPage"}));
        run(&mut s, "caret.docStart", json!({}));
        run(&mut s, "insert.header", json!({"text": "Head"}));
        run(&mut s, "insert.closeHeader", json!({}));
        run(&mut s, "insert.footer", json!({"text": "Foot"}));
        run(&mut s, "insert.closeHeader", json!({}));
        s
    }

    fn part_text(s: &Session, id: u32) -> String {
        s.doc.plain_text(wordcraft_doc::StoryRef::Part(id))
    }

    /// The owner: "the link can be made separately for the header or the footer", and the first
    /// page and odd/even pages can differ.
    #[test]
    fn header_and_footer_link_to_the_previous_section_separately() {
        let mut s = two_sections();
        assert!(s.doc.sections().len() >= 2, "{:?}", s.doc.sections().len());
        // Edit page 2's header: section 2, linked to section 1 (it shows "Head").
        run(&mut s, "insert.editHeader", json!({"page": 1}));
        let st = run(&mut s, "hf.state", json!({}));
        assert_eq!(
            (st["editing"].as_str(), st["section"].as_u64(), st["headerLinked"].as_bool(), st["footerLinked"].as_bool()),
            (Some("header"), Some(2), Some(true), Some(true)),
            "{st}"
        );
        // Unlink the header only: section 2 gets its own copy, the footer stays linked.
        let st = run(&mut s, "layout.linkToPrevious", json!({}));
        assert_eq!((st["headerLinked"].as_bool(), st["footerLinked"].as_bool()), (Some(false), Some(true)), "{st}");
        let wordcraft_doc::StoryRef::Part(own) = s.sel.focus.story else { panic!("in a header") };
        assert_eq!(part_text(&s, own), "Head");
        run(&mut s, "select.all", json!({}));
        run(&mut s, "text.insert", json!({"text": "Exercises"}));
        // Section 1 keeps its own.
        let first = s.doc.sections()[0].1.headers.default.unwrap();
        assert_eq!(part_text(&s, first), "Head");
        assert_ne!(first, own);
        // The footer, unlinked by name, from the header.
        let st = run(&mut s, "layout.linkToPrevious", json!({"footer": true, "value": false}));
        assert_eq!((st["headerLinked"].as_bool(), st["footerLinked"].as_bool()), (Some(false), Some(false)), "{st}");
        // Linked again: section 2 shows section 1's header.
        let st = run(&mut s, "layout.linkToPrevious", json!({"value": true}));
        assert_eq!(st["headerLinked"].as_bool(), Some(true));
        assert_eq!(s.sel.focus.story, wordcraft_doc::StoryRef::Part(first), "the caret goes to what the page shows");
        // Different First Page for the section edited (section 2), not the first one.
        let st = run(&mut s, "layout.differentFirstPage", json!({"value": true}));
        assert_eq!(st["firstPage"].as_bool(), Some(true));
        assert!(!s.doc.sections()[0].1.title_page && s.doc.sections()[1].1.title_page);
        // The first section has nothing to link to.
        run(&mut s, "insert.closeHeader", json!({}));
        run(&mut s, "insert.editHeader", json!({"page": 0}));
        assert!(s.run("layout.linkToPrevious", &json!({})).is_err());
    }
}
