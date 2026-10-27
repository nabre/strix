//! Live calculations (docs/calc.md): calc regions are equations marked as calculated
//! (`Math::calc`, the Calculate button on an equation) whose linear text is a `strix-calc`
//! statement (`L := 4.5 m`, `M = kN*m`). They are evaluated in reading order with one shared set
//! of parameters, and evaluation results are written back into the equations. Other equations
//! are never read or changed.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_calc::{Options, RegionKind, Worksheet};
use strix_doc::math::{Math, parse_linear, to_linear};
use strix_doc::{Block, InlineObject, Paragraph};

use crate::{CmdError, CmdResult, CommandSpec, Session, p};

/// Deepest table nesting searched for regions.
const MAX_DEPTH: usize = 16;
/// Most regions evaluated in one document; later equations are left as they are.
const MAX_REGIONS: usize = 10_000;

/// Builds the calculation command table: insert a region, recalculate, list the parameters.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("calc.insert", "Calculation Region", "Calculate › Regions", insert)
            .params(r#"{"text": string (a statement: "L := 4.5 m", "M = kN*m"), "display"?: bool, "digits"?: n (1–15, default 4)}"#),
        CommandSpec::new("calc.recalculate", "Recalculate", "Calculate › Regions", |s, v| {
            let report = recalculate(s, &options(v));
            Ok(report)
        })
        .params(r#"{"digits"?: n (1–15, default 4), "decimalComma"?: bool} → {"regions": n, "changed": n, "errors": [{"region": n, "text", "error"}]}"#),
        CommandSpec::new("equation.calc", "Calculate", "Equation › Tools", toggle_equation)
            .when(|s| if super::equation::equation_at_caret(s).is_some() { None } else { Some("no equation at the caret") })
            .params(r#"{"value"?: bool (default: toggle)} → {"calc": bool, "recalculated"}: the equation at the caret is a calculation region"#),
        CommandSpec::new("calc.newEquations", "Calculate New Equations", "Calculate › Regions", |s, v| {
            s.prefs.calc_new_equations = p::bool(v, "value").unwrap_or(!s.prefs.calc_new_equations);
            Ok(json!({"calcNewEquations": s.prefs.calc_new_equations}))
        })
        .pure()
        .params(r#"{"value"?: bool (default: toggle)}: new equations are calculation regions (a preference)"#),
        CommandSpec::new("calc.variables", "Variables", "Calculate › Regions", variables)
            .pure()
            .params(r#"{"digits"?: n} → {"variables": [{"name", "value" (number in the unit shown), "unit", "si" (SI magnitude), "region" (index of the defining region)}]}"#),
    ]
}

fn options(v: &Value) -> Options {
    let mut o = Options::default();
    if let Some(d) = p::u64(v, "digits") {
        o.sig_digits = u8::try_from(d.clamp(1, 15)).unwrap_or(4);
    }
    if let Some(c) = p::bool(v, "decimalComma") {
        o.decimal_comma = c;
    }
    o
}

/// The linear text of an equation, as the calc engine reads it.
fn equation_text(linear: &str, math: &Math) -> String {
    if math.nodes.is_empty() || math.linear { linear.to_string() } else { to_linear(&math.nodes) }
}

/// Every equation in reading order (body top to bottom, tables cell by cell), with the top-level
/// block it is in and, for a calculated one, its text (`None` for an ordinary equation).
fn regions(blocks: &[Arc<Block>]) -> Vec<(usize, Option<String>)> {
    fn walk(b: &Block, top: usize, depth: usize, out: &mut Vec<(usize, Option<String>)>) {
        if depth > MAX_DEPTH || out.len() >= MAX_REGIONS {
            return;
        }
        match b {
            Block::Para(p) => {
                for o in &p.objects {
                    if out.len() >= MAX_REGIONS {
                        return;
                    }
                    if let InlineObject::Equation { linear, math, .. } = o {
                        out.push((top, math.calc.then(|| equation_text(linear, math))));
                    }
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for cb in &cell.blocks {
                            walk(cb, top, depth + 1, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        walk(b, i, 0, &mut out);
    }
    out
}

/// Evaluates every region and writes changed results back. Returns the report JSON.
pub(crate) fn recalculate(s: &mut Session, opts: &Options) -> Value {
    let found = regions(&s.doc.body);
    let mut ws = Worksheet::new(*opts);
    let mut updates: Vec<Option<String>> = Vec::with_capacity(found.len());
    let mut errors = Vec::new();
    let mut calc_regions = 0usize;
    for (i, (_, text)) in found.iter().enumerate() {
        let Some(text) = text else {
            updates.push(None);
            continue;
        };
        let r = ws.eval_statement(text);
        if r.kind != RegionKind::NotCalc {
            calc_regions += 1;
        }
        if let Some(e) = &r.error {
            errors.push(json!({"region": i, "text": text, "error": e.to_string()}));
        }
        // The linear format of a built-up equation drops spaces: only a real change rewrites it.
        let same = |d: &String| {
            d.split_whitespace().eq(text.split_whitespace()) || d.replace(char::is_whitespace, "") == text.replace(char::is_whitespace, "")
        };
        updates.push(r.display.filter(|d| !same(d)));
    }
    let changed = updates.iter().filter(|u| u.is_some()).count();
    if changed > 0 {
        write_back(s, &found, &updates);
    }
    json!({"regions": calc_regions, "changed": changed, "errors": errors})
}

/// Replaces the text of the regions with an update, touching only the blocks that hold one.
fn write_back(s: &mut Session, found: &[(usize, Option<String>)], updates: &[Option<String>]) {
    let mut index = 0usize;
    let mut tops: Vec<usize> = found.iter().zip(updates).filter(|(_, u)| u.is_some()).map(|((t, _), _)| *t).collect();
    tops.dedup();
    for (top, block) in s.doc.body.iter_mut().enumerate() {
        // Regions in blocks without updates only advance the counter.
        let count = found.iter().filter(|(t, _)| *t == top).count();
        if !tops.contains(&top) {
            index = index.saturating_add(count);
            continue;
        }
        let mut each = |p: &mut Paragraph| {
            let mut touched = false;
            for o in &mut p.objects {
                if let InlineObject::Equation { linear, math, .. } = o {
                    if index >= MAX_REGIONS {
                        return;
                    }
                    if let Some(Some(text)) = updates.get(index) {
                        *math = Math { nodes: parse_linear(text), jc: math.jc, calc: true, ..Math::default() };
                        *linear = to_linear(&math.nodes);
                        touched = true;
                    }
                    index = index.saturating_add(1);
                }
            }
            if touched {
                p.touch();
            }
        };
        super::paste::each_para(Arc::make_mut(block), 0, &mut each);
    }
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::req_str(v, "text")?.trim().to_string();
    if text.is_empty() {
        return Err(CmdError::Params("give the statement as text".into()));
    }
    if text.chars().count() > strix_calc::MAX_INPUT_CHARS {
        return Err(CmdError::Params(format!("a statement is at most {} characters", strix_calc::MAX_INPUT_CHARS)));
    }
    let mut eq = json!({"linear": text, "edit": false, "calc": true});
    if let Some(d) = p::bool(v, "display") {
        eq["display"] = json!(d);
    }
    s.run("insert.equation", &eq).map_err(|e| CmdError::Failed(e.to_string()))?;
    let report = recalculate(s, &options(v));
    Ok(json!({"inserted": true, "recalculated": report}))
}

/// Turns the equation at the caret into a calculation region or back, then recalculates.
fn toggle_equation(s: &mut Session, v: &Value) -> CmdResult {
    let at = super::equation::equation_at_caret(s).ok_or_else(|| CmdError::Disabled("no equation at the caret".into()))?;
    let para = s.doc.para_mut(at.story, &at.path).map_err(|e| CmdError::Failed(e.to_string()))?;
    let Some(InlineObject::Equation { math, .. }) = para.object_at_mut(at.off) else {
        return Err(CmdError::Failed("no equation there".into()));
    };
    let on = p::bool(v, "value").unwrap_or(!math.calc);
    if math.calc != on {
        math.calc = on;
        // The kept source XML can't carry the flag: the writer uses the nodes from now on.
        math.omml.clear();
        para.touch();
    }
    let report = recalculate(s, &options(v));
    Ok(json!({"calc": on, "recalculated": report}))
}

fn variables(s: &mut Session, v: &Value) -> CmdResult {
    let opts = options(v);
    let found = regions(&s.doc.body);
    let mut ws = Worksheet::new(opts);
    let mut defined_at: Vec<(String, usize)> = Vec::new();
    for (i, (_, text)) in found.iter().enumerate() {
        let Some(text) = text else { continue };
        let r = ws.eval_statement(text);
        if let (Some(name), true) = (r.name, r.error.is_none()) {
            defined_at.retain(|(n, _)| *n != name);
            defined_at.push((name, i));
        }
    }
    let list: Vec<Value> = ws
        .scope()
        .iter()
        .map(|(name, q)| {
            let (value, unit) = strix_calc::auto_unit(q.value, q.dim);
            let region = defined_at.iter().find(|(n, _)| n == name).map(|(_, i)| *i);
            json!({"name": name, "value": value, "text": strix_calc::format_number(value, &opts), "unit": unit, "si": q.value, "region": region})
        })
        .collect();
    Ok(json!({"variables": list}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use strix_doc::Document;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": ""})).unwrap();
        s
    }

    /// The text of every equation in the body, in order, without spaces (the linear format
    /// of a built-up equation drops them).
    fn texts(s: &Session) -> Vec<String> {
        regions(&s.doc.body).into_iter().filter_map(|(_, t)| t.map(|t| t.replace(' ', ""))).collect()
    }

    fn add(s: &mut Session, text: &str) -> Value {
        let r = s.run("calc.insert", &json!({"text": text})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        r
    }

    #[test]
    fn a_beam_worksheet_remembers_its_parameters() {
        let mut s = session();
        for t in ["L := 4.5 m", "q := 12 kN/m", "M := q*L^2/8", "M = kN*m"] {
            add(&mut s, t);
        }
        let all = texts(&s);
        assert_eq!(all.len(), 4, "{all:?}");
        assert_eq!(all[3], "M=30.38kN·m");
        let vars = s.run("calc.variables", &json!({})).unwrap();
        let names: Vec<&str> = vars["variables"].as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["L", "q", "M"]);
        assert_eq!(vars["variables"][1]["unit"], "kN/m");
        assert_eq!(vars["variables"][2]["region"], 2);
    }

    #[test]
    fn changing_a_parameter_updates_later_results() {
        let mut s = session();
        add(&mut s, "a := 2 m");
        add(&mut s, "a*3 =");
        assert_eq!(texts(&s)[1], "a*3=6m");
        // Edit the definition through the equation command, then recalculate (F9 does it too).
        s.sel = crate::Selection::caret(strix_doc::Pos { off: 0, ..s.sel.focus.clone() });
        s.run("equation.set", &json!({"linear": "a := 5 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        let r = s.run("references.updateFields", &json!({})).unwrap();
        assert!(r.is_object());
        assert_eq!(texts(&s), ["a:=5m", "a*3=15m"]);
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(r["changed"], 0, "recalculating again changes nothing");
    }

    #[test]
    fn errors_are_reported_and_shown_without_stopping_later_regions() {
        let mut s = session();
        add(&mut s, "x := 2 m + 3 s");
        add(&mut s, "y := 4 kN");
        add(&mut s, "y =");
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(r["errors"].as_array().unwrap().len(), 1, "{r}");
        assert!(r["errors"][0]["error"].as_str().unwrap().contains("cannot add"));
        assert_eq!(texts(&s)[2], "y=4kN");
    }

    #[test]
    fn ordinary_equations_are_left_alone() {
        let mut s = session();
        s.run("insert.equation", &json!({"linear": "a := 2 m", "edit": false, "calc": false})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        s.run("insert.equation", &json!({"linear": "E = mc^2", "edit": false})).unwrap();
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        // The first isn't calculated; the second is, but `mc^2` is no unit, so it stays as typed.
        assert_eq!(r["regions"], 0);
        assert_eq!(texts(&s), ["E=mc^2"]);
    }

    #[test]
    fn the_calculate_button_turns_an_equation_into_a_region_and_back() {
        let mut s = session();
        s.run("calc.newEquations", &json!({"value": false})).unwrap();
        s.run("insert.equation", &json!({"linear": "x := 3 m", "edit": false})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        add(&mut s, "x*2 =");
        assert_eq!(texts(&s), ["x*2=⚠"], "x isn't defined: its equation isn't calculated");
        s.sel = crate::Selection::caret(strix_doc::Pos { off: 0, ..s.sel.focus.clone() });
        let r = s.run("equation.calc", &json!({})).unwrap();
        assert_eq!(r["calc"], true);
        assert_eq!(texts(&s), ["x:=3m", "x*2=6m"]);
        s.run("equation.calc", &json!({"value": false})).unwrap();
        // The unit shown before stays as the requested one.
        assert_eq!(texts(&s), ["x*2=⚠m"]);
        // New equations follow the preference.
        assert_eq!(s.run("calc.newEquations", &json!({})).unwrap()["calcNewEquations"], true);
    }

    #[test]
    fn the_flag_survives_saving_as_xmath() {
        let mut s = session();
        add(&mut s, "L := 2 m");
        s.run("insert.equation", &json!({"linear": "y = 1", "edit": false, "calc": false})).unwrap();
        let bytes = crate::io::save_bytes("w.xmath", &s.doc).unwrap();
        let doc = crate::io::open_bytes("w.xmath", &bytes).unwrap();
        let flags: Vec<bool> = regions(&doc.body).into_iter().map(|(_, t)| t.is_some()).collect();
        assert_eq!(flags, [true, false]);
    }

    #[test]
    fn hostile_insert_params_never_panic() {
        let mut s = session();
        for v in
            [json!({}), json!({"text": ""}), json!({"text": 5}), json!({"text": "(".repeat(100_000)}), json!({"text": "x := 1", "digits": u64::MAX})]
        {
            let _ = s.run("calc.insert", &v);
        }
        let _ = s.run("calc.recalculate", &json!({"digits": -1, "decimalComma": "yes"}));
        let _ = s.run("calc.variables", &json!({"digits": 0}));
    }
}
