//! Forms docs/calc.md gives for calculations, built from text (`calc.insert {"text": …}`): the
//! linear text of what is built is what a calculation reads (`crates/calc/src/tests.rs`,
//! `forms_built_from_text_are_calculated`).

use wordcraft_doc::math::{MNode, ScriptKind, parse_linear, to_linear};

fn built(text: &str) -> String {
    to_linear(&parse_linear(text))
}

/// A power written on a function's name in superscript characters is the name's, as `^` writes
/// it: the function applies to what follows, not to the power.
#[test]
fn a_superscript_on_a_function_name_is_its_power() {
    assert_eq!(built("β = sin⁻¹((R − r)/(R + r))"), "β=sin^(−1)\u{2061}((R−r)/(R+r))");
    assert_eq!(built("sin²(30°)"), "sin^2\u{2061}(30°)");
    assert_eq!(built("cos²α + sin²α ="), "cos^2\u{2061}α+sin^2\u{2061}α=");
    assert_eq!(built("tanh⁻¹ x"), "tanh^(−1)\u{2061}x");
    assert_eq!(parse_linear("sin²(x)"), parse_linear("sin^2(x)"));
    let nodes = parse_linear("sin⁻¹(x)");
    let [MNode::Func { name, e }] = nodes.as_slice() else { panic!("{nodes:?}") };
    assert!(matches!(name.as_slice(), [MNode::Script { kind: ScriptKind::Sup, .. }]), "{name:?}");
    assert_eq!(to_linear(e), "x");
    // The forms written with `^` are as they were, and so is a superscript anywhere else.
    assert_eq!(built("sin^-1(x)"), "sin^(-1)\u{2061}x");
    assert_eq!(built("sin^(-1)(x)"), "sin^(-1)\u{2061}x");
    assert_eq!(built("x⁻¹ + 2"), "x⁻¹+2");
    assert_eq!(built("sin x²"), "sin\u{2061}x²");
    assert_eq!(built("lim┬(x→0⁺) x"), "lim┬(x→0⁺)\u{2061}x");
    // A sign alone is no power.
    assert_eq!(built("sin⁻(x)"), "sin\u{2061}(⁻(x))");
}

/// An operand after a space is not the operator's (as Word builds it): the linear text has an
/// empty operand, which a calculation reads past.
#[test]
fn an_operand_after_a_space_follows_an_empty_one() {
    assert_eq!(built("∫_0^1 x^2 + 1 dx ="), "∫_0^1▒()x^2+1dx=");
    assert_eq!(built("∑_(i=1)^3 i² ="), "∑_(i=1)^3▒()i²=");
}

#[test]
fn hostile_superscripts_never_panic() {
    for s in ["sin⁻", "sin⁻⁻¹(x)", "sin⁺", "sin¹", "cos⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹⁹", "sin⁻¹", "sin⁻¹^2_3(x)", "sin⁴⁵⁶⁷⁸⁹⁰x"]
    {
        let nodes = parse_linear(s);
        let _ = parse_linear(&to_linear(&nodes));
    }
    let long = format!("sin{}", "²".repeat(30_000));
    let _ = to_linear(&parse_linear(&long));
    assert_eq!(built("sin⁴⁵⁶⁷⁸⁹⁰x"), "sin^4567890\u{2061}x");
}
