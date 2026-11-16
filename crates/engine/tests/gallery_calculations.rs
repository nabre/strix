//! Every structure of the Equation tab's galleries, filled in and calculated: the template is
//! built as the ribbon builds it, read back as the linear text a calculation gets, and worked
//! out. A structure with a value must give it; the others are listed, so a new template has to
//! say which it is.

use strix_calc::{Options, evaluate};
use wordcraft_doc::math::{parse_linear, to_linear};
use wordcraft_engine::math_gallery::{template, templates};

/// The names the filled templates read.
const SHEET: &[&str] = &["x = 0.5", "y = 3", "θ = 30°", "a = 3", "b = 4", "n = 4", "k = 2", "A = [2; 3; 4; 5]"];

/// Template id, what goes in its empty slots (in order) and the value shown after `=`.
const CALCULATED: &[(&str, &[&str], &str)] = &[
    ("frac.stacked", &["6", "3"], "2"),
    ("frac.skewed", &["6", "3"], "2"),
    ("frac.linear", &["6", "3"], "2"),
    ("frac.pi2", &[], "1.571"),
    ("frac.half", &[], "0.5"),
    ("script.sup", &["2", "5"], "32"),
    ("script.x2", &[], "0.25"),
    ("rad.sqrt", &["16"], "4"),
    ("rad.nth", &["4", "81"], "3"),
    ("rad.sqrt2", &["49"], "7"),
    ("rad.cbrt", &["27"], "3"),
    ("rad.hyp", &[], "5"),
    ("int.limits", &["0", "3", "y^2 dy"], "9"),
    ("int.stacked", &["0", "3", "y^2 dy"], "9"),
    ("nary.sum", &["A"], "14"),
    ("nary.sumLimits", &["(i=1)", "4", "i^2"], "30"),
    ("nary.sumScripts", &["(i=1)", "4", "i^2"], "30"),
    ("nary.sumLower", &["k", "k^2"], "4"),
    ("nary.sumSub", &["k", "k^2"], "4"),
    ("nary.prod", &["A"], "120"),
    ("nary.prodLimits", &["(i=1)", "4", "i"], "24"),
    ("nary.prodScripts", &["(i=1)", "4", "i"], "24"),
    ("nary.prodLower", &["k", "k+1"], "3"),
    ("nary.binomial", &[], "6"),
    ("nary.sum0n", &["i"], "10"),
    ("nary.prodA", &[], "120"),
    ("brk.paren", &["2+4"], "6"),
    ("brk.square", &["2+4"], "6"),
    ("brk.brace", &["2+4"], "6"),
    ("brk.floor", &["2.7"], "2"),
    ("brk.ceil", &["2.1"], "3"),
    ("brk.abs", &["2−8"], "6"),
    ("brk.norm", &["[3; 4]"], "5"),
    ("brk.binom", &["6", "3"], "20"),
    ("brk.nk", &[], "6"),
    ("fn.sin", &["θ"], "0.5"),
    ("fn.cos", &["(2θ)"], "0.5"),
    ("fn.tan", &["(45°)"], "1"),
    ("fn.csc", &["θ"], "2"),
    ("fn.sec", &["(2θ)"], "2"),
    ("fn.cot", &["(45°)"], "1"),
    ("fn.asin", &["x"], "30°"),
    ("fn.acos", &["x"], "60°"),
    ("fn.atan", &["1"], "45°"),
    ("fn.acsc", &["2"], "30°"),
    ("fn.asec", &["2"], "60°"),
    ("fn.acot", &["1"], "45°"),
    ("fn.sinh", &["0"], "0"),
    ("fn.cosh", &["0"], "1"),
    ("fn.tanh", &["0"], "0"),
    ("fn.csch", &["(ln(2))"], "1.333"),
    ("fn.sech", &["0"], "1"),
    ("fn.coth", &["(ln(2))"], "1.667"),
    ("fn.asinh", &["0"], "0"),
    ("fn.acosh", &["1"], "0"),
    ("fn.atanh", &["0"], "0"),
    ("fn.acsch", &["(4/3)"], "0.693"),
    ("fn.asech", &["1"], "0"),
    ("fn.acoth", &["(5/3)"], "0.693"),
    ("fn.sintheta", &[], "0.5"),
    ("fn.cos2x", &[], "0.54"),
    ("lim.logBase", &["2", "8"], "3"),
    ("lim.log", &["1000"], "3"),
    ("lim.ln", &["e"], "1"),
    ("lim.lim", &["(y→0)", "〖sin(y)/y〗"], "1"),
    ("lim.e", &[], "2.718"),
    ("mat.1x2", &["6", "3"], "[6; 3]"),
    ("mat.2x1", &["6", "3"], "[6; 3]"),
    ("mat.1x3", &["6", "3", "2"], "[6; 3; 2]"),
    ("mat.3x1", &["6", "3", "2"], "[6; 3; 2]"),
    ("mat.det", &["6", "3", "2", "5"], "24"),
];

/// Structures that are notation, not a number: operators and arrows, accents and boxes over
/// nothing in particular, set operations, integrals over regions and curves, dots, matrices as
/// such, brackets of one side.
const NOTATION: &[&str] = &[
    "frac.nobar",
    "frac.dydx",
    "frac.DyDx",
    "frac.partial",
    "frac.delta",
    "script.sub",
    "script.subsup",
    "script.pre",
    "script.xy2",
    "script.eiwt",
    "script.pre1n",
    "rad.quadratic",
    "int.plain",
    "int.double",
    "int.triple",
    "int.contour",
    "int.surface",
    "int.volume",
    "int.dx",
    "int.dy",
    "int.dtheta",
    "nary.coprod",
    "nary.union",
    "nary.intersection",
    "nary.or",
    "nary.and",
    "nary.oplus",
    "nary.otimes",
    "nary.odot",
    "nary.uplus",
    "nary.sqcup",
    "nary.double",
    "nary.unionXY",
    "brk.angle",
    "brk.dbl",
    "brk.leftSquare",
    "brk.rightSquare",
    "brk.outward",
    "brk.parenSep",
    "brk.braceSep",
    "brk.angleSep",
    "brk.leftParen",
    "brk.rightParen",
    "brk.leftBrace",
    "brk.rightBrace",
    "brk.leftBar",
    "brk.leftAngle",
    "brk.cases2",
    "brk.cases3",
    "brk.stack2",
    "brk.stack3",
    "brk.binomAngle",
    "brk.casesAbs",
    "brk.nk2",
    "fn.tanIdentity",
    "acc.",
    "lim.min",
    "lim.max",
    "lim.maxExample",
    "op.",
    "mat.1",
    "mat.2x2",
    "mat.2x3",
    "mat.3x2",
    "mat.3x3",
    "mat.cdots",
    "mat.ldots",
    "mat.vdots",
    "mat.ddots",
    "mat.adots",
    "mat.id",
    "mat.paren",
    "mat.square",
    "mat.norm",
    "mat.sparse",
    "mat.identityN",
];

/// The template with its slots filled, as the linear text of the built-up equation.
fn filled(id: &str, fills: &[&str]) -> String {
    let tpl = template(id).unwrap_or_else(|| panic!("no template {id}"));
    let mut fills = fills.iter();
    let mut linear = String::new();
    for c in tpl.linear.chars() {
        match c {
            '⬚' => linear.push_str(fills.next().unwrap_or_else(|| panic!("{id}: a slot without a value"))),
            c => linear.push(c),
        }
    }
    assert!(fills.next().is_none(), "{id}: more values than slots");
    to_linear(&parse_linear(&linear))
}

fn calculate(statement: &str) -> (String, Option<String>) {
    let mut sheet = SHEET.to_vec();
    sheet.push(statement);
    let last = evaluate(&sheet, &Options::default()).pop().unwrap();
    (last.display.unwrap_or_default(), last.error.map(|e| e.to_string()))
}

#[test]
fn every_structure_with_a_value_is_calculated() {
    for (id, fills, value) in CALCULATED {
        let built = filled(id, fills);
        let (shown, error) = calculate(&format!("{built} ="));
        assert_eq!(error, None, "{id}: {built}");
        assert_eq!(shown, format!("{built} = {value}"), "{id}");
    }
}

#[test]
fn every_template_is_calculated_or_listed_as_notation() {
    for tpl in templates() {
        let calculated = CALCULATED.iter().any(|(id, ..)| *id == tpl.id);
        let notation = NOTATION.iter().any(|p| tpl.id == *p || (p.ends_with('.') && tpl.id.starts_with(p)) || tpl.id.starts_with(&format!("{p}x")) || {
            // A family: `int.double`, `int.doubleLimits`, `int.doubleStacked`.
            tpl.id.strip_prefix(p).is_some_and(|rest| ["Limits", "Stacked", "Scripts", "Lower", "zeros", "2", "3"].contains(&rest))
        });
        assert!(calculated != notation, "{}: {}", tpl.id, if calculated { "listed twice" } else { "say whether it is calculated" });
    }
}

/// The ready-made examples that are whole calculations.
#[test]
fn the_galleries_examples_work_out() {
    // Cases: the absolute value, defined and then called.
    let cases = to_linear(&template("brk.casesAbs").unwrap().nodes());
    let mut sheet = vec![cases.as_str(), "f(−3) ="];
    assert_eq!(evaluate(&sheet, &Options::default()).pop().unwrap().display.as_deref(), Some("f(−3) = 3"));
    sheet[1] = "f(2.5) =";
    assert_eq!(evaluate(&sheet, &Options::default()).pop().unwrap().display.as_deref(), Some("f(2.5) = 2.5"));
    // The tangent identity is checked with a value for θ; nothing is reported.
    let identity = to_linear(&template("fn.tanIdentity").unwrap().nodes());
    let (_, error) = calculate(&identity);
    assert_eq!(error, None, "{identity}");
    // A determinant with its unit, and a vector's length.
    let det = filled("mat.det", &["2 m", "1 m", "0 m", "3 m"]);
    assert_eq!(calculate(&format!("{det} =")).0, format!("{det} = 6 m²"));
    let norm = filled("brk.norm", &["A"]);
    assert_eq!(calculate(&format!("{norm} =")).0, format!("{norm} = 7.348"));
    // The inverse functions are angles, never reciprocals.
    let asin = filled("fn.asin", &["(1/2)"]);
    assert_eq!(calculate(&format!("{asin} =")).0, format!("{asin} = 30°"));
}
