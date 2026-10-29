//! Graph paper ("foglio a quadretti", docs/graph-paper.md): a square grid drawn behind the
//! text. As the background of a section's pages it is [`SectionProps::graph_paper`]; in the
//! text flow it is a top-level body paragraph whose [`ParaProps::graph`] says how far the grid
//! goes (the rest of the page, whole pages, a height).
//!
//! Sizes in the parameters are millimetres (spacing, height) and points (line widths); every
//! value is clamped by [`GraphPaper::sanitized`].
//!
//! [`SectionProps::graph_paper`]: strix_doc::SectionProps::graph_paper
//! [`ParaProps::graph`]: strix_doc::props::ParaProps::graph

use std::sync::Arc;

use serde_json::{Value, json};
use strix_doc::graph_paper::{GraphPaper, GridLine, GridLineStyle, GridPlacement, MAX_MAJOR_EVERY, MAX_PAGES, PT_PER_MM};
use strix_doc::props::Rgb;
use strix_doc::{Block, Paragraph, Path, Pos, StoryRef, para_block};

use crate::{CmdError, CmdResult, CommandSpec, Selection, Session};

/// The parameters every graph paper command takes (documented once).
macro_rules! grid_params {
    () => {
        r#""placement"?: "background" (behind the text of the section's pages) | "pageEnd" (to the end of the page) | "untilOdd" | "untilEven" (the rest of the page and whole pages until the next odd / even page) | "pages" (for `pages` pages) | "height" (for `height` mm), "pages"?: 1–100, "height"?: mm, "spacing"?: mm (the side of a square, 1–100), "style"?: "solid|dashed|dotted", "color"?: "RRGGBB", "width"?: pt (0.1–6), "majorEvery"?: n (a heavier line every n squares, 0 = none, ≤ 100), "majorStyle"?, "majorColor"?, "majorWidth"?, "followColumns"?: bool (ruled in each text column of the section, the space between them clear; default false: across the whole text width), "frameWidth"?: pt (0.1–6, the frame round the ruled area) | null (the frame weighs what the grid lines do: the default)"#
    };
}

pub fn specs() -> Vec<CommandSpec> {
    let present = |s: &Session| if at_caret(s).is_some() { None } else { Some("no graph paper at the cursor") };
    vec![
        CommandSpec::new("graph.insert", "Graph Paper", "Insert › Pages", insert).params(concat!(
            "{",
            grid_params!(),
            "} → graph paper: the section's background, or a grid block at the caret (an empty paragraph there becomes it; otherwise a new paragraph after the caret's, or before it with the caret at its start); the caret goes in the block"
        )),
        CommandSpec::new("graph.set", "Graph Paper Properties", "Insert › Pages", set)
            .when(present)
            .params(concat!("{", grid_params!(), "} → changes the graph paper at the cursor (its block, else the section's background); a new placement moves it between the background and the text")),
        CommandSpec::new("graph.get", "Graph Paper at Cursor", "Insert › Pages", get).pure().params(
            r#"→ {"grid": {"target": "paragraph"|"section", "placement", "pages"?, "height"?, "spacing", "style", "color", "width", "majorEvery", "majorStyle", "majorColor", "majorWidth", "followColumns", "frameWidth", "frameCustom"} | null}: the graph paper at the cursor, sizes as the parameters take them"#,
        ),
        CommandSpec::new("graph.remove", "Remove Graph Paper", "Insert › Pages", remove)
            .when(present)
            .params("→ removes the graph paper at the cursor: its block (an empty one goes with its paragraph), else the section's background"),
    ]
}

/// Where a grid is: a top-level body paragraph, or the background of the caret's section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Para(usize),
    Section,
}

/// The graph paper at the caret: the grid block the caret is in, else its section's background.
pub fn at_caret(s: &Session) -> Option<(Target, GraphPaper)> {
    let focus = &s.sel.focus;
    if focus.story == StoryRef::Body
        && let [top] = focus.path.0.as_slice()
        && let Some(Block::Para(p)) = s.doc.body.get(*top as usize).map(|b| &**b)
        && let Some(g) = &p.props.graph
    {
        return Some((Target::Para(*top as usize), g.sanitized()));
    }
    super::page::sect(s).graph_paper.map(|g| (Target::Section, GraphPaper { placement: GridPlacement::Background, ..g.sanitized() }))
}

/// A number parameter: absent or null is `None`, anything but a finite number an error.
fn num(v: &Value, k: &str) -> Result<Option<f32>, CmdError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x.as_f64().filter(|f| f.is_finite()).map(|f| Some(f as f32)).ok_or_else(|| CmdError::Params(format!("`{k}`: a number"))),
    }
}

fn style(v: &Value, k: &str) -> Result<Option<GridLineStyle>, CmdError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => GridLineStyle::parse(t).map(Some).ok_or_else(|| CmdError::Params(format!("`{k}`: solid, dashed or dotted"))),
        Some(_) => Err(CmdError::Params(format!("`{k}`: solid, dashed or dotted"))),
    }
}

fn color(v: &Value, k: &str) -> Result<Option<Rgb>, CmdError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Rgb::parse(t).map(Some).ok_or_else(|| CmdError::Params(format!("`{k}`: a colour \"RRGGBB\""))),
        Some(_) => Err(CmdError::Params(format!("`{k}`: a colour \"RRGGBB\""))),
    }
}

fn line(v: &Value, base: GridLine, keys: [&str; 3]) -> Result<GridLine, CmdError> {
    let [ks, kc, kw] = keys;
    Ok(GridLine { style: style(v, ks)?.unwrap_or(base.style), color: color(v, kc)?.unwrap_or(base.color), width: num(v, kw)?.unwrap_or(base.width) })
}

/// `g` with the parameters in `v` applied (validated, then clamped).
pub fn apply(g: GraphPaper, v: &Value) -> Result<GraphPaper, CmdError> {
    let pages = match num(v, "pages")? {
        Some(n) => n.clamp(1.0, MAX_PAGES as f32) as u32,
        None => match g.placement {
            GridPlacement::Pages { pages } => pages,
            _ => 2,
        },
    };
    let height = match num(v, "height")? {
        Some(mm) => mm * PT_PER_MM,
        None => match g.placement {
            GridPlacement::Height { height } => height,
            _ => 50.0 * PT_PER_MM,
        },
    };
    let placement = match v.get("placement") {
        None | Some(Value::Null) => match g.placement {
            GridPlacement::Pages { .. } => GridPlacement::Pages { pages },
            GridPlacement::Height { .. } => GridPlacement::Height { height },
            other => other,
        },
        Some(Value::String(name)) => GridPlacement::parse(name, pages, height)
            .ok_or_else(|| CmdError::Params("`placement`: background, pageEnd, untilOdd, untilEven, pages or height".into()))?,
        Some(_) => return Err(CmdError::Params("`placement`: a string".into())),
    };
    let major_every = match num(v, "majorEvery")? {
        Some(n) => n.clamp(0.0, MAX_MAJOR_EVERY as f32) as u32,
        None => g.major_every,
    };
    Ok(GraphPaper {
        spacing: num(v, "spacing")?.map_or(g.spacing, |mm| mm * PT_PER_MM),
        line: line(v, g.line, ["style", "color", "width"])?,
        major_every,
        major: line(v, g.major, ["majorStyle", "majorColor", "majorWidth"])?,
        placement,
        follow_columns: match v.get("followColumns") {
            None | Some(Value::Null) => g.follow_columns,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(CmdError::Params("`followColumns`: true or false".into())),
        },
        // Null takes the frame back to the lines' weight; absent keeps it.
        frame: match v.get("frameWidth") {
            None => g.frame,
            Some(Value::Null) => None,
            Some(_) => num(v, "frameWidth")?,
        },
    }
    .sanitized())
}

/// The grid as `graph.get` gives it (and the parameters take it).
pub fn to_json(target: Target, g: &GraphPaper) -> Value {
    let mm = |pt: f32| (f64::from(pt / PT_PER_MM) * 100.0).round() / 100.0;
    let pt = |w: f32| (f64::from(w) * 100.0).round() / 100.0;
    let mut out = json!({
        "target": if target == Target::Section { "section" } else { "paragraph" },
        "placement": g.placement.name(),
        "spacing": mm(g.spacing),
        "style": g.line.style.name(),
        "color": g.line.color.hex(),
        "width": pt(g.line.width),
        "majorEvery": g.major_every,
        "majorStyle": g.major.style.name(),
        "majorColor": g.major.color.hex(),
        "majorWidth": pt(g.major.width),
        "followColumns": g.follow_columns,
        "frameWidth": pt(g.frame_width()),
        "frameCustom": g.frame.is_some(),
    });
    match g.placement {
        GridPlacement::Pages { pages } => out["pages"] = json!(pages),
        GridPlacement::Height { height } => out["height"] = json!(mm(height)),
        _ => {}
    }
    out
}

fn body_path(i: usize) -> Path {
    Path(vec![u32::try_from(i).unwrap_or(u32::MAX)])
}

/// Puts `g` (not a background) in the text flow at the caret: an empty top-level paragraph
/// there becomes the block; otherwise a new paragraph goes after the caret's top-level block
/// (before it when the caret is at the start of a paragraph's text). The caret goes in it.
fn put_in_flow(s: &mut Session, g: GraphPaper) -> Result<(), CmdError> {
    if s.sel.focus.story != StoryRef::Body {
        return Err(CmdError::Disabled("graph paper in the text goes in the body; a background one goes on any page".into()));
    }
    let focus = s.sel.focus.clone();
    let top = focus.path.0.first().map_or(0, |&t| t as usize).min(s.doc.body.len().saturating_sub(1));
    let grid = Arc::new(g);
    let at = match s.doc.body.get(top).map(|b| &**b) {
        Some(Block::Para(p)) if focus.path.0.len() == 1 && p.text.is_empty() && p.objects.is_empty() => {
            let para = s.doc.para_mut(StoryRef::Body, &body_path(top))?;
            para.props.graph = Some(grid);
            para.touch();
            top
        }
        Some(Block::Para(p)) if focus.path.0.len() == 1 && focus.off == 0 && p.section.is_none() => {
            insert_block(s, top, grid);
            top
        }
        Some(_) => {
            let at = top.saturating_add(1).min(s.doc.body.len());
            insert_block(s, at, grid);
            at
        }
        None => {
            insert_block(s, 0, grid);
            0
        }
    };
    s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(at), 0));
    Ok(())
}

fn insert_block(s: &mut Session, at: usize, grid: Arc<GraphPaper>) {
    let mut p = Paragraph::new();
    p.props.graph = Some(grid);
    s.doc.body.insert(at.min(s.doc.body.len()), para_block(p));
}

/// Takes the grid off its block: an empty paragraph without a section break goes too (when it
/// isn't the only block). The caret stays on what is left at that place.
fn take_from_para(s: &mut Session, top: usize) -> Result<(), CmdError> {
    let empty = matches!(s.doc.body.get(top).map(|b| &**b), Some(Block::Para(p)) if p.text.is_empty() && p.objects.is_empty() && p.section.is_none());
    if empty && s.doc.body.len() > 1 {
        s.doc.body.remove(top);
        let at = top.min(s.doc.body.len().saturating_sub(1));
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(at), 0));
    } else {
        let para = s.doc.para_mut(StoryRef::Body, &body_path(top))?;
        para.props.graph = None;
        para.touch();
    }
    Ok(())
}

fn set_background(s: &mut Session, g: Option<GraphPaper>) -> Result<(), CmdError> {
    let g = g.map(|g| Arc::new(GraphPaper { placement: GridPlacement::Background, ..g }));
    super::page::with_sect(s, |x| x.graph_paper = g.clone())?;
    Ok(())
}

fn result(s: &Session) -> CmdResult {
    Ok(json!({"grid": at_caret(s).map(|(t, g)| to_json(t, &g))}))
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let g = apply(GraphPaper::default(), v)?;
    if g.placement == GridPlacement::Background {
        set_background(s, Some(g))?;
    } else {
        put_in_flow(s, g)?;
    }
    s.touch();
    s.clamp_selection();
    result(s)
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let (target, old) = at_caret(s).ok_or_else(|| CmdError::Disabled("no graph paper at the cursor".into()))?;
    let g = apply(old, v)?;
    match (target, g.placement == GridPlacement::Background) {
        (Target::Section, true) => set_background(s, Some(g))?,
        (Target::Section, false) => {
            // Checked first: the background stays when the block can't go in.
            if s.sel.focus.story != StoryRef::Body {
                return Err(CmdError::Disabled("graph paper in the text goes in the body".into()));
            }
            set_background(s, None)?;
            put_in_flow(s, g)?;
        }
        (Target::Para(top), false) => {
            let para = s.doc.para_mut(StoryRef::Body, &body_path(top))?;
            para.props.graph = Some(Arc::new(g));
            para.touch();
        }
        (Target::Para(top), true) => {
            take_from_para(s, top)?;
            set_background(s, Some(g))?;
        }
    }
    s.touch();
    s.clamp_selection();
    result(s)
}

fn get(s: &mut Session, _: &Value) -> CmdResult {
    result(s)
}

fn remove(s: &mut Session, _: &Value) -> CmdResult {
    match at_caret(s).ok_or_else(|| CmdError::Disabled("no graph paper at the cursor".into()))?.0 {
        Target::Para(top) => take_from_para(s, top)?,
        Target::Section => set_background(s, None)?,
    }
    s.touch();
    s.clamp_selection();
    result(s)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// One paragraph, "Before", with the caret at its end.
    fn session() -> Session {
        let mut s = Session::new(strix_doc::Document::from_text("Before"));
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(0), 6));
        s
    }

    #[test]
    fn grids_survive_an_xmath_round_trip() {
        let mut s = session();
        s.run("graph.insert", &json!({"placement": "pages", "pages": 3, "spacing": 4, "majorEvery": 5, "style": "dotted", "majorColor": "AA0000"}))
            .unwrap();
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(0), 0));
        s.run("graph.insert", &json!({"placement": "background", "spacing": 10, "color": "00FF00"})).unwrap();
        let bytes = crate::io::save_bytes("g.xmath", &s.doc).unwrap();
        let back = crate::io::open_bytes("g.xmath", &bytes).unwrap();
        let s2 = Session::new(back);
        let (_, g) = grid_para(&s2).unwrap();
        let (_, want) = grid_para(&s).unwrap();
        assert_eq!(g.placement, GridPlacement::Pages { pages: 3 });
        assert_eq!(g.line.style, GridLineStyle::Dotted);
        assert_eq!(g.major_every, 5);
        assert_eq!(g.major.color, Rgb(0xAA, 0, 0));
        assert!((g.spacing - want.spacing).abs() < 0.01);
        let bg = s2.doc.last_section.graph_paper.as_deref().copied().unwrap();
        assert_eq!(bg.placement, GridPlacement::Background);
        assert_eq!(bg.line.color, Rgb(0, 0xFF, 0));
        assert!((bg.spacing - 10.0 * PT_PER_MM).abs() < 0.01);
    }

    fn grid_para(s: &Session) -> Option<(usize, GraphPaper)> {
        s.doc.body.iter().enumerate().find_map(|(i, b)| match &**b {
            Block::Para(p) => p.props.graph.as_deref().map(|g| (i, *g)),
            _ => None,
        })
    }

    #[test]
    fn insert_puts_a_block_after_the_caret_paragraph() {
        let mut s = session();
        let out = s.run("graph.insert", &json!({"spacing": 4, "majorEvery": 5, "color": "112233"})).unwrap();
        assert_eq!(out["grid"]["placement"], "pageEnd");
        assert_eq!(out["grid"]["target"], "paragraph");
        assert_eq!(out["grid"]["spacing"], 4.0);
        let (i, g) = grid_para(&s).unwrap();
        assert_eq!(i, 1);
        assert_eq!(g.major_every, 5);
        assert_eq!(g.line.color, Rgb(0x11, 0x22, 0x33));
        assert_eq!(s.sel.focus.path, body_path(1));
        // Undo takes it away.
        s.run("edit.undo", &json!({})).unwrap();
        assert!(grid_para(&s).is_none());
    }

    #[test]
    fn an_empty_paragraph_becomes_the_block() {
        let mut s = Session::new(strix_doc::Document::from_text("Before\n\nAfter"));
        assert_eq!(s.doc.body.len(), 3);
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(1), 0));
        s.run("graph.insert", &json!({"placement": "pages", "pages": 3})).unwrap();
        assert_eq!(s.doc.body.len(), 3);
        assert_eq!(grid_para(&s).map(|(i, g)| (i, g.placement)), Some((1, GridPlacement::Pages { pages: 3 })));
        // At the start of a paragraph's text, the block goes before it.
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(2), 0));
        s.run("graph.remove", &json!({})).unwrap_err();
        s.run("graph.insert", &json!({"placement": "height"})).unwrap();
        assert_eq!(s.doc.body.len(), 4);
        assert_eq!(s.sel.focus.path, body_path(2));
        assert_eq!(s.doc.para_at(&Pos::new(StoryRef::Body, body_path(3), 0)).map(|p| p.plain_text()), Some("After".to_string()));
    }

    #[test]
    fn set_changes_and_moves_the_grid() {
        let mut s = session();
        s.run("graph.insert", &json!({"placement": "height", "height": 40})).unwrap();
        let out = s.run("graph.set", &json!({"height": 60, "style": "dashed"})).unwrap();
        assert_eq!(out["grid"]["placement"], "height");
        assert_eq!(out["grid"]["height"], 60.0);
        assert_eq!(out["grid"]["style"], "dashed");
        // To the background: the (empty) block goes, the section takes the grid.
        let blocks = s.doc.body.len();
        let out = s.run("graph.set", &json!({"placement": "background"})).unwrap();
        assert_eq!(out["grid"]["target"], "section");
        assert_eq!(s.doc.body.len(), blocks - 1);
        assert!(grid_para(&s).is_none());
        assert!(s.doc.last_section.graph_paper.is_some());
        assert_eq!(s.doc.last_section.graph_paper.as_ref().unwrap().line.style, GridLineStyle::Dashed);
        // And back into the text.
        let out = s.run("graph.set", &json!({"placement": "untilOdd"})).unwrap();
        assert_eq!(out["grid"]["target"], "paragraph");
        assert!(s.doc.last_section.graph_paper.is_none());
        assert_eq!(grid_para(&s).map(|(_, g)| g.placement), Some(GridPlacement::UntilOdd));
    }

    #[test]
    fn remove_and_disabled_without_a_grid() {
        let mut s = session();
        assert!(s.run("graph.set", &json!({"spacing": 3})).is_err());
        assert!(s.run("graph.remove", &json!({})).is_err());
        assert_eq!(s.run("graph.get", &json!({})).unwrap()["grid"], Value::Null);
        s.run("graph.insert", &json!({"placement": "background"})).unwrap();
        assert_eq!(s.run("graph.get", &json!({})).unwrap()["grid"]["target"], "section");
        s.run("graph.remove", &json!({})).unwrap();
        assert!(s.doc.last_section.graph_paper.is_none());
        s.run("graph.insert", &json!({})).unwrap();
        let n = s.doc.body.len();
        s.run("graph.remove", &json!({})).unwrap();
        assert_eq!(s.doc.body.len(), n - 1);
    }

    #[test]
    fn bad_params_are_errors_and_values_are_clamped() {
        let mut s = session();
        for bad in
            [json!({"placement": "sideways"}), json!({"style": "wavy"}), json!({"color": "nope"}), json!({"spacing": "x"}), json!({"placement": 4})]
        {
            assert!(s.run("graph.insert", &bad).is_err(), "{bad}");
        }
        let out = s.run("graph.insert", &json!({"spacing": 1e9, "width": -5, "pages": 1e12, "placement": "pages", "majorEvery": -3})).unwrap();
        assert_eq!(out["grid"]["spacing"], 100.0);
        assert_eq!(out["grid"]["pages"], 100);
        assert_eq!(out["grid"]["majorEvery"], 0);
        assert_eq!(out["grid"]["width"], 0.1);
    }

    /// The grid rects of the session's layout, by page, checked against a layout made from
    /// scratch (as reopening the document would): the grid is never left where an earlier edit
    /// put it.
    fn grids(s: &mut Session) -> Vec<(usize, [i32; 4])> {
        let rects = |l: &strix_layout::DocLayout| -> Vec<(usize, [i32; 4])> {
            let r = |v: f32| (v * 10.0).round() as i32;
            l.pages
                .iter()
                .enumerate()
                .flat_map(|(i, pg)| {
                    pg.items.iter().filter_map(move |it| match it {
                        strix_layout::Placed::Grid { rect, .. } => Some((i, [r(rect.x), r(rect.y), r(rect.w), r(rect.h)])),
                        _ => None,
                    })
                })
                .collect()
        };
        let live = rects(&s.layout());
        assert_eq!(live, rects(&s.export_layout()), "the live layout's grid is the one a fresh layout gives");
        live
    }

    fn top(g: &[(usize, [i32; 4])]) -> i32 {
        g.first().map_or(0, |(_, r)| r[1])
    }

    #[test]
    fn the_grid_follows_every_edit() {
        let mut s = session();
        s.run("graph.insert", &json!({"placement": "pageEnd"})).unwrap();
        let start = grids(&mut s);
        assert_eq!(start.len(), 1);
        // Paragraphs typed above it move it down, still on the bottom margin.
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(0), 6));
        for _ in 0..5 {
            s.run("text.newParagraph", &json!({})).unwrap();
            s.run("text.insert", &json!({"text": "more"})).unwrap();
        }
        let moved = grids(&mut s);
        assert!(top(&moved) > top(&start) + 50, "{start:?} then {moved:?}");
        assert_eq!(moved[0].1[1] + moved[0].1[3], start[0].1[1] + start[0].1[3]);
        // Undo and redo take it back and forth.
        let mut steps = 0;
        while grid_para(&s).map(|(i, _)| i) != Some(1) && steps < 20 {
            s.run("edit.undo", &json!({})).unwrap();
            steps += 1;
        }
        assert_eq!(grids(&mut s), start);
        for _ in 0..steps {
            s.run("edit.redo", &json!({})).unwrap();
        }
        assert_eq!(grids(&mut s), moved);
        // Text written on it, then Enter at its start: the grid goes down with the text, which
        // stays on the squares (on the same page).
        let (block, _) = grid_para(&s).unwrap();
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(block), 0));
        s.run("text.insert", &json!({"text": "on the squares"})).unwrap();
        let written = grids(&mut s);
        s.sel = Selection::caret(Pos::new(StoryRef::Body, body_path(block), 0));
        s.run("text.newParagraph", &json!({})).unwrap();
        s.run("text.newParagraph", &json!({})).unwrap();
        let (now, _) = grid_para(&s).unwrap();
        assert_eq!(now, block + 2, "the grid is on the paragraph with the text");
        assert_eq!(s.doc.body.get(now).and_then(|b| b.as_para()).map(|p| p.plain_text()), Some("on the squares".to_string()));
        let pushed = grids(&mut s);
        assert!(top(&pushed) > top(&written), "{written:?} then {pushed:?}");
        assert!(pushed.iter().all(|(page, _)| *page == 0));
        // Backspace at its start joins the empty paragraphs back: the grid stays on the text.
        s.run("text.backspace", &json!({})).unwrap();
        s.run("text.backspace", &json!({})).unwrap();
        assert_eq!(grid_para(&s).map(|(i, _)| i), Some(block));
        assert_eq!(grids(&mut s), written);
        // A background follows the edits too.
        s.run("graph.set", &json!({"placement": "background"})).unwrap();
        let bg = grids(&mut s);
        s.run("text.insert", &json!({"text": "x"})).unwrap();
        assert_eq!(grids(&mut s), bg);
        s.run("edit.undo", &json!({})).unwrap();
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(grid_para(&s).map(|(i, _)| i), Some(block));
        assert_eq!(grids(&mut s), written);
    }

    #[test]
    fn follow_columns_and_frame_are_parameters() {
        let mut s = session();
        let out = s.run("graph.insert", &json!({"placement": "background"})).unwrap();
        assert_eq!(out["grid"]["followColumns"], false);
        assert_eq!(out["grid"]["frameCustom"], false);
        assert_eq!(out["grid"]["frameWidth"], out["grid"]["width"]);
        let out = s.run("graph.set", &json!({"followColumns": true, "frameWidth": 2.5})).unwrap();
        assert_eq!(out["grid"]["followColumns"], true);
        assert_eq!(out["grid"]["frameWidth"], 2.5);
        assert_eq!(out["grid"]["frameCustom"], true);
        // Kept when other things change, clamped, and reset by null.
        let out = s.run("graph.set", &json!({"spacing": 4, "frameWidth": 99})).unwrap();
        assert_eq!((out["grid"]["followColumns"].clone(), out["grid"]["frameWidth"].clone()), (json!(true), json!(6.0)));
        let out = s.run("graph.set", &json!({"frameWidth": null, "width": 0.8})).unwrap();
        assert_eq!((out["grid"]["frameCustom"].clone(), out["grid"]["frameWidth"].clone()), (json!(false), json!(0.8)));
        for bad in [json!({"followColumns": "yes"}), json!({"frameWidth": "thick"}), json!({"followColumns": 1})] {
            assert!(s.run("graph.set", &bad).is_err(), "{bad}");
        }
        // Saved and read back.
        s.run("graph.set", &json!({"frameWidth": 1.75})).unwrap();
        let bytes = crate::io::save_bytes("g.xmath", &s.doc).unwrap();
        let back = crate::io::open_bytes("g.xmath", &bytes).unwrap();
        let g = back.last_section.graph_paper.as_deref().copied().unwrap();
        assert!(g.follow_columns);
        assert_eq!(g.frame, Some(1.75));
    }

    #[test]
    fn hostile_params_never_panic_and_lay_out() {
        let junk = [
            json!({"placement": "pages", "pages": -1e308, "height": 1e308, "spacing": -0.0}),
            json!({"placement": "height", "height": -1e308, "majorEvery": 1e308, "majorWidth": 1e308}),
            json!({"placement": "untilEven", "spacing": 1e-30, "width": 1e-30, "color": "#zzzzzz"}),
            json!({"placement": "background", "pages": "many", "majorStyle": 7}),
            json!({"placement": "", "style": ""}),
            json!({"followColumns": null, "frameWidth": -1e308}),
            json!({"followColumns": [1], "frameWidth": {"a": 1}}),
            json!({"placement": "background", "followColumns": true, "frameWidth": 1e-30}),
        ];
        for j in &junk {
            for id in ["graph.insert", "graph.set", "graph.remove", "graph.get"] {
                let mut s = Session::new(crate::sample::sample_document());
                let _ = s.run("graph.insert", &json!({}));
                let _ = s.run(id, j);
                s.clamp_selection();
                let _ = s.layout();
            }
        }
    }
}
