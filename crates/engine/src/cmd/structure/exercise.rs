//! `structure.exercise`: a new exercise on a plane structure (`docs/structures.md`, Exercise).
//!
//! The statement holds the structure with its loads (`view: structure`); the questions ask for
//! the reactions, the diagram of N, the diagram of T, where T is zero, the greatest moment there
//! and the diagram of M (a truss: the reactions and the axial forces). In the solution, under
//! each repeated question, its working (the equilibrium equations; the laws stretch by stretch;
//! T's law solved for its zero; M's law there) and its drawing (the reactions; one diagram each).
//! The drawings share a `group`, so a change made to one (in the Structure Editor, by a command)
//! is made to the others, each keeping its view.

use serde_json::{Value, json};
use strix_structure::{Structure, View};
use wordcraft_doc::graphic::GraphicKind;
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, InlineObject, Paragraph, Pos, StoryRef, para_block};

use crate::{CmdError, CmdResult, Session, p};

pub(super) fn run(s: &mut Session, v: &Value) -> CmdResult {
    let given = super::structure_param(v)?;
    let convert = given.is_none();
    let mut st = match given {
        Some(st) => st,
        None => super::template(p::str(v, "template").unwrap_or("beam"))?,
    };
    super::apply_settings(&mut st, v, convert)?;
    st.group = super::free_group(s);
    st.view = View::All;
    super::working::keep_labels(&mut st, v);
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
    // The questions: their words from `questions` (the working's own words are in `labels`).
    let truss = st.render(None).analysis.truss;
    let asked: Vec<(&str, &str)> = if truss {
        vec![("reactions", "Find the support reactions."), ("trussAxial", "Find the axial force in every member and draw it.")]
    } else {
        vec![
            ("reactions", "Find the support reactions."),
            ("axial", "Work out and draw the diagram of the axial force N."),
            ("shear", "Work out and draw the diagram of the shear force {T}."),
            ("zeros", "Find the points where the shear force {T} is zero."),
            ("maxima", "Work out the greatest bending moment at those points ({T}(x) = 0)."),
            ("moments", "Work out and draw the diagram of the bending moment M."),
        ]
    };
    let questions: Vec<String> = asked
        .iter()
        .map(|(k, d)| {
            v.get("questions").and_then(|l| l.get(*k)).and_then(Value::as_str).map(|t| t.chars().take(400).collect()).unwrap_or_else(|| d.to_string())
        })
        .map(|q: String| q.replace("{T}", &shear))
        .collect();
    super::super::exercise::insert_problem(s, &json!({"title": title, "text": text, "questions": questions}))?;
    let caret = s.sel.clone();
    let (text_blocks, prompts) = super::super::exercise::statement_and_prompts(s).ok_or_else(|| CmdError::Failed("the exercise vanished".into()))?;
    let (_, t1) = text_blocks.ok_or_else(|| CmdError::Failed("the exercise has no statement".into()))?;
    if prompts.len() < questions.len() {
        return Err(CmdError::Failed("the exercise has no solution".into()));
    }

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
    let mut parts = super::working::parts(s, &src, v, &props)?;
    // Each part in its bookmark, so a change to the structure writes it again.
    {
        use super::working::{Part, wrap};
        let name = |p: Part| super::working::live_name(&st.group, p);
        wrap(&mut parts.reactions, &name(Part::Reactions))?;
        wrap(&mut parts.axial, &name(Part::Axial))?;
        wrap(&mut parts.shear, &name(Part::Shear))?;
        wrap(&mut parts.zeros, &name(Part::Zeros))?;
        wrap(&mut parts.maxima, &name(Part::Maxima))?;
        wrap(&mut parts.moments, &name(Part::Moments))?;
    }

    // The solution: under each prompt its working, then its drawing.
    let drawing = |s: &Session, view: View| -> Result<Paragraph, CmdError> {
        let mut para = Paragraph::with_text("", props.clone());
        para.insert_object(0, super::graphic(s, &Structure { view, ..st.clone() }), &props).map_err(|e| CmdError::Failed(e.to_string()))?;
        Ok(para)
    };
    let with = |mut blocks: Vec<Paragraph>, view: Option<View>| -> Result<Vec<Paragraph>, CmdError> {
        if let Some(view) = view {
            blocks.push(drawing(s, view)?);
        }
        Ok(blocks)
    };
    let answers = if parts.truss {
        vec![with(parts.reactions, Some(View::Reactions))?, with(parts.axial, Some(View::Axial))?]
    } else {
        vec![
            with(parts.reactions, Some(View::Reactions))?,
            with(parts.axial, Some(View::Axial))?,
            with(parts.shear, Some(View::Shear))?,
            with(parts.zeros, None)?,
            with(parts.maxima, None)?,
            with(parts.moments, Some(View::Moment))?,
        ]
    };
    // The last question first, so the earlier ones' blocks keep their places; the blocks after
    // the statement moved one down.
    for (&(_, after), blocks) in prompts.iter().zip(answers).rev() {
        for (k, para) in blocks.into_iter().enumerate() {
            let i = (after + 2 + k).min(s.doc.body.len());
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
            .run(
                "structure.exercise",
                &json!({"labels": {"title": "Trave", "reactions": "Reazioni"}, "questions": {"reactions": "Calcola le reazioni.", "zeros": "Dove {T} = 0?"}}),
            )
            .unwrap();
        assert_eq!(r["group"], "structure-1", "{r}");
        let found = structures(&s);
        let views: Vec<View> = found.iter().map(|f| f.2.view).collect();
        assert_eq!(views, [View::Structure, View::Equilibrium, View::Reactions, View::Axial, View::Section, View::Shear, View::Moment]);
        assert!(found.iter().all(|f| f.2.group == "structure-1"));
        let t = text(&s);
        assert!(t.contains("Trave") && t.contains("Calcola le reazioni.") && t.contains("Dove T = 0?"), "{t}");
        // The working's heading is its own word, not the question's.
        assert!(t.contains("Reazioni:") && !t.contains("Calcola le reazioni.:"), "{t}");
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
        assert_eq!(found.len(), 7);
        for (_, _, st) in &found {
            assert_eq!(st.node("B").map(|n| n.x.value), Some(8.0), "{:?}", st.view);
        }
        let views: Vec<View> = found.iter().map(|f| f.2.view).collect();
        assert_eq!(views, [View::Structure, View::Equilibrium, View::Reactions, View::Axial, View::Section, View::Shear, View::Moment]);
    }

    /// The equations of the body in reading order: (linear text, result of question).
    fn equations(s: &Session) -> Vec<(String, u32)> {
        s.doc
            .body
            .iter()
            .filter_map(|b| if let Block::Para(p) = &**b { Some(p) } else { None })
            .flat_map(|p| p.objects.iter())
            .filter_map(|o| match o {
                InlineObject::Equation { linear, math, .. } => Some((linear.to_string(), math.answer)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_exercise_finds_where_the_shear_vanishes_and_the_moment_there() {
        // A simply supported beam 6 m long under 10 kN/m: T = 30 kN − 10 kN/m·x, zero at 3 m,
        // M = 45 kN·m there.
        let st = json!({"nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
            "members": [{"id": "1", "a": "A", "b": "B"}],
            "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
            "loads": [{"type": "distributed", "member": "1", "q1": 10, "angle": -90}]});
        let mut s = Session::new(Document::new());
        s.run("structure.exercise", &json!({"structure": st})).unwrap();
        let eqs = equations(&s);
        let zero = eqs.iter().find(|e| e.0.contains("=T_1(x_1)")).unwrap_or_else(|| panic!("{eqs:?}"));
        assert_eq!(zero.1, 4, "{eqs:?}");
        assert!(zero.0.contains("x_1&=3\u{2009}m"), "{zero:?}");
        let max = eqs.iter().find(|e| e.0.starts_with("M_1(x_1)")).unwrap_or_else(|| panic!("{eqs:?}"));
        assert_eq!(max.1, 5);
        assert!(max.0.contains("45\u{2009}kN·m"), "{max:?}");
        // The moment's law comes before its value there.
        let law = eqs.iter().position(|e| e.0.starts_with("M_1(x)=")).unwrap_or_else(|| panic!("{eqs:?}"));
        assert!(law < eqs.iter().position(|e| e.0.starts_with("M_1(x_1)")).unwrap());

        // A triangular load 0 → 30 kN/m: T of degree two, zero at 6 m/√3 = 3.4641 m (its other
        // root, outside the beam, is left out by the stretch's bounds).
        let st = json!({"nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
            "members": [{"id": "1", "a": "A", "b": "B"}],
            "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
            "loads": [{"type": "distributed", "member": "1", "q1": 0, "q2": 30, "angle": -90}]});
        let mut s = Session::new(Document::new());
        s.run("structure.exercise", &json!({"structure": st})).unwrap();
        let eqs = equations(&s);
        let zero = eqs.iter().find(|e| e.0.contains("=T_1(x_1)")).unwrap_or_else(|| panic!("{eqs:?}"));
        assert!(zero.0.contains("x_1&=3.464"), "{zero:?}");
        let max = eqs.iter().find(|e| e.0.starts_with("M_1(x_1)")).unwrap_or_else(|| panic!("{eqs:?}"));
        assert!(max.0.contains("69.282"), "{max:?}");

        // A point load in the middle: T changes sign under it, M is greatest there.
        let st = json!({"nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 4, "y": 0}],
            "members": [{"id": "1", "a": "A", "b": "B"}],
            "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
            "loads": [{"type": "force", "member": "1", "at": 2, "value": 10, "angle": -90}]});
        let mut s = Session::new(Document::new());
        s.run("structure.exercise", &json!({"structure": st})).unwrap();
        let eqs = equations(&s);
        assert!(eqs.iter().any(|e| e.0.starts_with("T_1a(2\u{2009}m)") && e.1 == 4), "{eqs:?}");
        let max = eqs.iter().find(|e| e.0.starts_with("M_1a(2\u{2009}m)") && e.1 == 5).unwrap_or_else(|| panic!("{eqs:?}"));
        assert!(max.0.contains("10\u{2009}kN·m"), "{max:?}");

        // A cantilever under a point load at its tip: T never vanishes; M greatest at the wall.
        let mut s = Session::new(Document::new());
        s.run("structure.exercise", &json!({"template": "cantilever"})).unwrap();
        assert!(text(&s).contains("never vanishes"), "{}", text(&s));
        assert!(equations(&s).iter().any(|e| e.0.starts_with("M_") && e.1 == 5));
    }

    #[test]
    fn a_truss_exercise_asks_for_the_reactions_and_the_axial_forces() {
        let mut s = Session::new(Document::new());
        s.run("structure.exercise", &json!({"template": "truss"})).unwrap();
        let views: Vec<View> = structures(&s).iter().map(|f| f.2.view).collect();
        assert_eq!(views, [View::Structure, View::Equilibrium, View::Reactions, View::Axial]);
        assert!(text(&s).contains("axial force in every member"), "{}", text(&s));
    }
}
