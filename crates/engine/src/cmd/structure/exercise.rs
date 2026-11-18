//! `structure.exercise`: a new exercise on a plane structure (`docs/structures.md`, Exercise).
//!
//! The statement holds the structure with its loads (`view: structure`); the questions ask for
//! the reactions and for the internal forces; in the solution, under each repeated question, its
//! working (the equilibrium equations; the laws stretch by stretch) and its drawing (the
//! reactions; the N, T, M diagrams). The three drawings share a `group`, so a change made to one
//! (in the Structure Editor, by a command) is made to the others, each keeping its view.

use serde_json::{Value, json};
use strix_structure::{Structure, View};
use wordcraft_doc::graphic::GraphicKind;
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, InlineObject, Paragraph, Pos, StoryRef, para_block};

use crate::{CmdError, CmdResult, Session, p};

pub(super) fn run(s: &mut Session, v: &Value) -> CmdResult {
    let mut st = match super::structure_param(v)? {
        Some(st) => st,
        None => super::template(p::str(v, "template").unwrap_or("beam"))?,
    };
    super::apply_settings(&mut st, v)?;
    st.group = free_group(s);
    st.view = View::All;
    let st = st.sanitized();
    // A structure with no parameters is checked before anything is written.
    if st.expressions().is_empty() && !st.render(None).analysis.status.is_solved() {
        return Err(CmdError::Failed(format!("the structure can't be solved: {}", st.render(None).analysis.status.describe())));
    }
    let label = |k: &str, d: &str| -> String {
        v.get("labels").and_then(|l| l.get(k)).and_then(Value::as_str).map(|t| t.chars().take(400).collect()).unwrap_or_else(|| d.to_string())
    };
    let shear = st.titles.v.clone();
    let title = label("title", "Structure");
    let text = label("text", "For the structure in the figure, loaded as shown:");
    let q1 = label("reactions", "Classify the structure and find the support reactions.");
    let q2 = label("diagrams", "Write the internal forces N, {T}, M stretch by stretch and draw their diagrams.").replace("{T}", &shear);
    super::super::exercise::insert_problem(s, &json!({"title": title, "text": text, "questions": [q1, q2]}))?;
    let caret = s.sel.clone();
    let (text_blocks, prompts) = super::super::exercise::statement_and_prompts(s).ok_or_else(|| CmdError::Failed("the exercise vanished".into()))?;
    let (_, t1) = text_blocks.ok_or_else(|| CmdError::Failed("the exercise has no statement".into()))?;
    let (&(_, a1), &(_, b1)) = match (prompts.first(), prompts.get(1)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err(CmdError::Failed("the exercise has no solution".into())),
    };

    // The statement: the structure in a paragraph of its own after its words.
    let Some(Block::Para(para)) = s.doc.body.get(t1).map(|b| &**b) else {
        return Err(CmdError::Failed("the statement is not a paragraph".into()));
    };
    let props = para.mark.clone();
    let ends = para.text.chars().rev().take_while(|&c| c == OBJ).count();
    let off = para.text.len().saturating_sub(ends * OBJ.len_utf8());
    let at = Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![t1 as u32]), off);
    let below = s.doc.split_paragraph(&at).map_err(|e| CmdError::Failed(e.to_string()))?;
    let schema = Structure { view: View::Structure, ..st.clone() };
    s.doc.insert_object(&below, super::graphic(s, &schema), &props).map_err(|e| CmdError::Failed(e.to_string()))?;
    super::super::calc::recalculate_now(s);
    // The blocks after the statement moved one down.
    let (a1, b1) = (a1 + 1, b1 + 1);
    let src = s
        .doc
        .body
        .get(t1 + 1)
        .and_then(|b| match &**b {
            Block::Para(p) => p.objects_with_offsets().find_map(|(_, o)| match o {
                InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Structure => graphic.structure.clone(),
                _ => None,
            }),
            _ => None,
        })
        .ok_or_else(|| CmdError::Failed("the structure vanished".into()))?;
    let (reactions, forces) = super::working::sections(s, &src, v, &props)?;

    // The solution, the second question first so the first one's blocks keep their places.
    let drawing = |s: &Session, view: View| -> Result<Paragraph, CmdError> {
        let mut para = Paragraph::with_text("", props.clone());
        para.insert_object(0, super::graphic(s, &Structure { view, ..st.clone() }), &props).map_err(|e| CmdError::Failed(e.to_string()))?;
        Ok(para)
    };
    let mut second = forces;
    second.push(drawing(s, View::Diagrams)?);
    let mut first = reactions;
    first.push(drawing(s, View::Reactions)?);
    for (after, blocks) in [(b1, second), (a1, first)] {
        for (k, para) in blocks.into_iter().enumerate() {
            let i = (after + 1 + k).min(s.doc.body.len());
            s.doc.body.insert(i, para_block(para));
        }
    }
    s.sel = caret;
    s.touch();
    super::super::calc::recalculate_now(s);
    super::super::exercise::update(s);
    s.clamp_selection();
    let exercise = super::super::exercise::exercise_at_caret(s);
    Ok(json!({"exercise": exercise, "group": st.group}))
}

/// A group name no structure in the body has yet: `structure-1`, `structure-2`…
fn free_group(s: &Session) -> String {
    let mut used: Vec<String> = Vec::new();
    for b in &s.doc.body {
        let Block::Para(p) = &**b else { continue };
        for (_, o) in p.objects_with_offsets() {
            if let InlineObject::Graphic { graphic, .. } = o
                && graphic.kind == GraphicKind::Structure
                && let Some(st) = graphic.structure.as_deref().and_then(|src| Structure::from_json(src).ok())
                && !st.group.is_empty()
            {
                used.push(st.group);
            }
        }
    }
    let mut n = used.len() + 1;
    for k in 1..=used.len() + 1 {
        let name = format!("structure-{k}");
        if !used.contains(&name) {
            n = k;
            break;
        }
    }
    format!("structure-{n}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use strix_structure::{Structure, View};
    use wordcraft_doc::graphic::GraphicKind;
    use wordcraft_doc::para::OBJ;
    use wordcraft_doc::{Block, Document, InlineObject, Pos, StoryRef};

    use crate::{Selection, Session};

    /// Every structure in the body: its block, offset and source.
    fn structures(s: &Session) -> Vec<(usize, usize, Structure)> {
        let mut out = Vec::new();
        for (i, b) in s.doc.body.iter().enumerate() {
            let Block::Para(p) = &**b else { continue };
            for (off, o) in p.objects_with_offsets() {
                if let InlineObject::Graphic { graphic, .. } = o
                    && graphic.kind == GraphicKind::Structure
                {
                    out.push((i, off, Structure::from_json(graphic.structure.as_deref().unwrap()).unwrap()));
                }
            }
        }
        out
    }

    fn text(s: &Session) -> String {
        s.doc.body.iter().filter_map(|b| if let Block::Para(p) = &**b { Some(p.text.replace(OBJ, "")) } else { None }).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn a_structure_exercise_has_the_structure_in_its_statement_and_the_results_in_its_solution() {
        let mut s = Session::new(Document::new());
        let r = s
            .run("structure.exercise", &json!({"labels": {"title": "Trave", "reactions": "Calcola le reazioni.", "diagrams": "Traccia N, {T}, M."}}))
            .unwrap();
        assert_eq!(r["group"], "structure-1", "{r}");
        let found = structures(&s);
        let views: Vec<View> = found.iter().map(|f| f.2.view).collect();
        assert_eq!(views, [View::Structure, View::Reactions, View::Diagrams]);
        assert!(found.iter().all(|f| f.2.group == "structure-1"));
        let t = text(&s);
        assert!(t.contains("Trave") && t.contains("Calcola le reazioni.") && t.contains("Traccia N, T, M."), "{t}");
        // The statement's drawing comes before the questions, the others after their prompts.
        let q = s.doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if p.text.contains("Calcola le reazioni."))).unwrap();
        assert!(found[0].0 < q && found[1].0 > q && found[2].0 > found[1].0);
        // The reactions are the first question's result.
        let marked = s
            .doc
            .body
            .iter()
            .filter_map(|b| if let Block::Para(p) = &**b { Some(p) } else { None })
            .flat_map(|p| p.objects.iter())
            .filter(|o| matches!(o, InlineObject::Equation { math, .. } if math.answer == 1))
            .count();
        assert!(marked >= 2, "{marked}");
        // A second exercise gets a group of its own.
        let r = s.run("structure.exercise", &json!({"template": "cantilever"})).unwrap();
        assert_eq!(r["group"], "structure-2", "{r}");
    }

    #[test]
    fn a_change_to_one_drawing_of_the_exercise_is_made_to_the_others() {
        let mut s = Session::new(Document::new());
        s.run("structure.exercise", &json!({})).unwrap();
        let (i, off, _) = structures(&s)[0].clone();
        let at = Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![i as u32]), off);
        s.sel = Selection { anchor: at.clone(), focus: Pos { off: off + OBJ.len_utf8(), ..at } };
        s.run("structure.node", &json!({"id": "B", "x": 8})).unwrap();
        let found = structures(&s);
        assert_eq!(found.len(), 3);
        for (_, _, st) in &found {
            assert_eq!(st.node("B").map(|n| n.x.value), Some(8.0), "{:?}", st.view);
        }
        let views: Vec<View> = found.iter().map(|f| f.2.view).collect();
        assert_eq!(views, [View::Structure, View::Reactions, View::Diagrams]);
    }
}
