//! `plot.analysis`: the plot's analysis written after it as live calculations
//! (`docs/plot.md`, Analysis in the document).
//!
//! Each function series gets its definition (`f(x) = x^2 - 4`) and, for what the plot shows,
//! calculations that work it out again in calc: zeros as `0 = f(x_1); x_1 > -3; x_1 < -1`,
//! extrema as `0 = f′(x_2); …` with `f(x_2) =`, the tangent's slope `m = f′(1) =` and line
//! `t(x) = m·(x - 1) + f(1)`, the integral `A = ∫_(-2)^2 f(x) dx =`. The plot only says where to
//! look (the bracket around each point); the values are calc's, so they follow the formula.

use serde_json::{Value, json};
use strix_plot::{NotableKind, NotablePoint, Plot, PlotRendered, SeriesKind};
use wordcraft_doc::math::{Math, parse_linear, to_linear};
use wordcraft_doc::para::Paragraph;
use wordcraft_doc::{Block, InlineObject};

use crate::{CmdError, CmdResult, Session};

/// At most this many lines are written.
const MAX_LINES: usize = 200;

/// The words before each part, from the interface (English by default).
struct Labels {
    zeros: String,
    minimum: String,
    maximum: String,
    tangent: String,
    integral: String,
}

impl Labels {
    fn from(v: &Value) -> Labels {
        let get = |k: &str, d: &str| {
            v.get("labels").and_then(|l| l.get(k)).and_then(Value::as_str).map(|s| s.chars().take(60).collect()).unwrap_or_else(|| d.to_string())
        };
        Labels {
            zeros: get("zeros", "Zeros"),
            minimum: get("minimum", "Minimum"),
            maximum: get("maximum", "Maximum"),
            tangent: get("tangent", "Tangent"),
            integral: get("integral", "Area"),
        }
    }
}

/// One line of the analysis: words, then equations (linear format, calculated).
#[derive(Debug, Clone, PartialEq)]
struct Line {
    label: String,
    equations: Vec<String>,
    /// A definition, shown on its own line (display); the rest stays in the text.
    display: bool,
}

pub(super) fn run(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, src) = super::selected_source(s).ok_or_else(|| CmdError::Disabled("select a plot first".into()))?;
    let plot = Plot::from_json(&src).map_err(|e| CmdError::Failed(format!("the plot can't be read: {e}")))?;
    let rendered = crate::cmd::calc::resolved_plot(s, &src).unwrap_or_else(|| {
        let ws = strix_calc::Worksheet::new(crate::cmd::calc::options_now(s));
        plot.render_for(Some(&ws), true)
    });
    let lines = lines(&plot, &rendered, &Labels::from(v));
    if lines.is_empty() {
        return Err(CmdError::Failed("the plot has no function to analyse".into()));
    }
    let props = s.doc.para_at(&pos).map(|p| p.mark.clone()).unwrap_or_default();
    let mut path = pos.path.clone();
    for line in &lines {
        let mut para = Paragraph::with_text(&line.label, props.clone());
        let alone = line.display;
        for (k, eq) in line.equations.iter().enumerate() {
            if k > 0 || !line.label.is_empty() {
                let at = para.text.len();
                // Wide spaces between equations: a `;` would read as one of their conditions.
                para.insert_text(at, if k > 0 { "\u{2003}\u{2003}" } else { " " }, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
            }
            let math = Math { nodes: parse_linear(eq), calc: true, ..Math::default() };
            let obj = InlineObject::Equation { linear: to_linear(&math.nodes), math, display: alone };
            let at = para.text.len();
            para.insert_object(at, obj, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
        }
        path = path.with_last(path.last().saturating_add(1));
        s.doc.insert_block(pos.story, &path, Block::Para(para)).map_err(|e| CmdError::Failed(e.to_string()))?;
    }
    s.touch();
    let report = crate::cmd::calc::recalculate_now(s);
    Ok(json!({ "lines": lines.len(), "recalculated": report }))
}

/// The analysis of every function series of `p`, from what `r` found.
fn lines(p: &Plot, r: &PlotRendered, labels: &Labels) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut used: Vec<String> = Vec::new();
    let mut point = 0usize;
    let (mut tangents, mut areas) = (0usize, 0usize);
    let unit = p.x_unit.as_deref().filter(|u| !u.trim().is_empty()).map(|u| format!(" {}", u.trim())).unwrap_or_default();
    let number = |v: f64| format!("{}{unit}", num(v));
    for (i, s) in p.series.iter().enumerate() {
        let SeriesKind::Function { expr, var, tangent_at, .. } = &s.kind else { continue };
        if expr.trim().is_empty() {
            continue;
        }
        let f = function_name(&s.label, &mut used);
        let x = var.as_str();
        out.push(Line { label: String::new(), equations: vec![format!("{f}({x}) = {}", expr.replace('*', "·"))], display: true });
        let found: Vec<&NotablePoint> = r.notables.iter().filter(|n| n.series_index == i).collect();
        let of = |kind: NotableKind| -> Vec<f64> {
            let mut xs: Vec<f64> = found.iter().filter(|n| n.kind == kind).map(|n| n.x).filter(|x| x.is_finite()).collect();
            xs.sort_by(f64::total_cmp);
            xs
        };
        let roots = of(NotableKind::Root);
        // One zero a line, the label on the first.
        for (k, &root) in roots.iter().enumerate() {
            point = point.saturating_add(1);
            let xk = format!("{x}_{point}");
            let (a, b) = bracket(&roots, k, root);
            let label = if k == 0 { format!("{}:", labels.zeros) } else { String::new() };
            let zero = format!("0 = {f}({xk}); {xk} > {}; {xk} < {}", number(a), number(b));
            out.push(Line { label, equations: vec![zero], display: false });
        }
        let mut extrema: Vec<(f64, bool)> =
            of(NotableKind::Minimum).into_iter().map(|x| (x, true)).chain(of(NotableKind::Maximum).into_iter().map(|x| (x, false))).collect();
        extrema.sort_by(|a, b| a.0.total_cmp(&b.0));
        let xs: Vec<f64> = extrema.iter().map(|e| e.0).collect();
        for (k, &(xe, min)) in extrema.iter().enumerate() {
            point = point.saturating_add(1);
            let xk = format!("{x}_{point}");
            let (a, b) = bracket(&xs, k, xe);
            let label = if min { &labels.minimum } else { &labels.maximum };
            out.push(Line {
                label: format!("{label}:"),
                equations: vec![format!("0 = {f}′({xk}); {xk} > {}; {xk} < {}", number(a), number(b)), format!("{f}({xk}) =")],
                display: false,
            });
        }
        if let Some(x0) = tangent_at.filter(|t| t.is_finite()) {
            tangents = tangents.saturating_add(1);
            let m = numbered("m", tangents);
            let t = function_name(&numbered("t", tangents), &mut used);
            let x0 = number(x0);
            out.push(Line {
                label: format!("{} ({x} = {x0}):", labels.tangent),
                equations: vec![format!("{m} = {f}′({x0}) ="), format!("{t}({x}) = {m}·({x} - {x0}) + {f}({x0})")],
                display: false,
            });
        }
        for integral in r.integrals.iter().filter(|g| g.series_index == i && g.a.is_finite() && g.b.is_finite()) {
            areas = areas.saturating_add(1);
            let name = numbered("A", areas);
            out.push(Line {
                label: format!("{}:", labels.integral),
                equations: vec![format!("{name} = ∫_({})^({})▒〖{f}({x}) d{x}〗 =", number(integral.a), number(integral.b))],
                display: false,
            });
        }
        if out.len() >= MAX_LINES {
            out.truncate(MAX_LINES);
            break;
        }
    }
    out
}

/// `base` the first time, then `base_2`, `base_3`…
fn numbered(base: &str, n: usize) -> String {
    if n <= 1 { base.to_string() } else { format!("{base}_{n}") }
}

/// The function's name: the label's when it is one (`f(x)`, `g`, `v(t)`), else the next free
/// one of `f g h p q r`.
fn function_name(label: &str, used: &mut Vec<String>) -> String {
    let head = label.split('(').next().unwrap_or("").trim();
    let valid = !head.is_empty()
        && head.chars().count() <= 12
        && head.chars().next().is_some_and(char::is_alphabetic)
        && head.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !label.contains('=')
        && !used.iter().any(|u| u == head);
    let name = if valid {
        head.to_string()
    } else {
        ["f", "g", "h", "p", "q", "r", "u", "w"]
            .iter()
            .map(|s| (*s).to_string())
            .chain((2..).map(|n| format!("f_{n}")))
            .find(|n| !used.contains(n))
            .unwrap_or_else(|| "f".to_string())
    };
    used.push(name.clone());
    name
}

/// Limits that hold the point `xs[k]` and none of its neighbours: halfway to each neighbour, or
/// one unit (scaled to the point) where there is none; rounded outwards to short numbers.
fn bracket(xs: &[f64], k: usize, x: f64) -> (f64, f64) {
    let reach = (x.abs() * 0.5).max(1.0);
    let lo = k.checked_sub(1).and_then(|j| xs.get(j)).map_or(reach, |&p| (x - p) / 2.0);
    let hi = xs.get(k.saturating_add(1)).map_or(reach, |&n| (n - x) / 2.0);
    let gap = lo.min(hi).max(f64::MIN_POSITIVE);
    // Enough digits to stay within the gap.
    let digits = (-gap.log10()).ceil().clamp(0.0, 10.0) + 1.0;
    let scale = 10f64.powf(digits);
    // Outwards by less than a tenth of the gap: still short of the neighbours.
    let a = ((x - lo) * scale).floor() / scale;
    let b = ((x + hi) * scale).ceil() / scale;
    let (a, b) = if a < x && x < b { (a, b) } else { (x - lo, x + hi) };
    (a, b)
}

/// A short number for the linear format: up to 4 decimals, no trailing zeros.
fn num(v: f64) -> String {
    let v = if v.abs() < 1e-12 { 0.0 } else { v };
    let mut text = format!("{v:.4}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if text == "-0" { "0".to_string() } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_hold_only_their_point() {
        let xs = [-2.0, 2.0];
        assert_eq!(bracket(&xs, 0, -2.0), (-3.0, 0.0));
        assert_eq!(bracket(&xs, 1, 2.0), (0.0, 3.0));
        let (a, b) = bracket(&[1.0, 1.001], 0, 1.0);
        assert!(a < 1.0 && b > 1.0 && b < 1.001, "{a} {b}");
        assert_eq!(num(1.234_567_89), "1.2346");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(3.0), "3");
    }

    #[test]
    fn function_names_follow_labels() {
        let mut used = Vec::new();
        assert_eq!(function_name("f(x)", &mut used), "f");
        assert_eq!(function_name("f(x)", &mut used), "g");
        assert_eq!(function_name("v(t)", &mut used), "v");
        assert_eq!(function_name("Parabola 2", &mut used), "h");
        assert_eq!(function_name("y = x²", &mut used), "p");
    }

    /// The equations of the paragraphs after the first, as calc shows them.
    fn shown(s: &Session) -> Vec<String> {
        let mut out = Vec::new();
        for b in s.doc.body.iter().skip(1) {
            if let Block::Para(p) = &**b {
                for o in &p.objects {
                    if let InlineObject::Equation { linear, math, .. } = o {
                        out.push(crate::cmd::calc::equation_text(linear, math));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn analysis_is_written_after_the_plot_as_live_calculations() {
        let mut s = Session::new(wordcraft_doc::Document::default());
        let plot = json!({
            "xMin": -4.0, "xMax": 4.0, "yMin": -6.0, "yMax": 6.0,
            "series": [{"type": "function", "label": "f(x)", "expr": "x^2 - 4", "roots": true, "extrema": true, "tangentAt": 1.0, "integral": [-2.0, 2.0]}]
        });
        s.run("plot.insert", &json!({"plot": plot})).unwrap();
        let r = s.run("plot.analysis", &json!({"labels": {"zeros": "Zeri"}})).unwrap();
        assert!(r["lines"].as_u64().unwrap() >= 5, "{r}");
        let text = shown(&s).join("\n").replace(' ', "");
        for want in ["f(x) = x^2 - 4", "⇒ x_1 = -2", "⇒ x_2 = 2", "⇒ x_3 = 0", "f(0) = -4", "m = f′(1) = 2", "A = ∫", "= -10.667"] {
            let want = want.replace(' ', "");
            assert!(text.contains(&want), "missing {want} in\n{text}");
        }
        let labels: Vec<String> = s.doc.body.iter().filter_map(|b| if let Block::Para(p) = &**b { Some(p.text.clone()) } else { None }).collect();
        assert_eq!(labels.iter().filter(|t| t.starts_with("Zeri:")).count(), 1, "{labels:?}");
    }

    #[test]
    fn analysis_needs_a_plot_with_a_function() {
        let mut s = Session::new(wordcraft_doc::Document::default());
        assert!(s.run("plot.analysis", &json!({})).is_err());
        let plot = json!({"series": [{"type": "data", "points": [{"x": 1.0, "y": 2.0}]}]});
        s.run("plot.insert", &json!({"plot": plot})).unwrap();
        assert!(s.run("plot.analysis", &json!({})).is_err());
    }
}
