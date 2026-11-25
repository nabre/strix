//! The result of one calculation: its unit among those of its quantity, its own decimal places
//! and rounding step (the user's example: a volume worked out from lengths in cm and m).

use serde_json::{Value, json};
use wordcraft_doc::{Block, Document};

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap()
}

/// What the calc equations show (their linear text, without spaces).
fn shown(s: &Session) -> Vec<String> {
    let mut out = Vec::new();
    for b in &s.doc.body {
        if let Block::Para(p) = &**b {
            for o in &p.objects {
                if let wordcraft_doc::InlineObject::Equation { math, .. } = o
                    && math.calc
                {
                    out.push(wordcraft_doc::math::to_linear(&math.nodes).replace(char::is_whitespace, ""));
                }
            }
        }
    }
    out
}

const VOLUME: &str = "V_e = (260 cm + 20 cm*2)*(1.0 m)*(230 cm + 20 cm) =";

fn volume() -> Session {
    let mut s = Session::new(Document::new());
    run(&mut s, "calc.insert", json!({"text": VOLUME}));
    s
}

#[test]
fn a_results_unit_is_chosen_among_its_quantitys() {
    let mut s = volume();
    assert!(shown(&s)[0].ends_with("=7.5m³"), "{:?}", shown(&s));
    let info = run(&mut s, "calc.resultInfo", json!({}));
    assert_eq!(info["shown"], "m³");
    // A result shown reads back in its unit, as a unit asked for.
    assert_eq!(info["unit"], "m³");
    let units: Vec<&str> = info["units"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    for u in ["cm³", "L", "dm³", "m³"] {
        assert!(units.contains(&u), "{units:?}");
    }
    let r = run(&mut s, "calc.result", json!({"unit": "dm³"}));
    assert_eq!(r["unit"], "dm³");
    assert_eq!(r["shown"], "dm³");
    assert!(shown(&s)[0].ends_with("=7500dm³"), "{:?}", shown(&s));
    run(&mut s, "calc.result", json!({"unit": "L"}));
    assert!(shown(&s)[0].ends_with("=7500L"), "{:?}", shown(&s));
    // Undone in one step, as any edit.
    run(&mut s, "edit.undo", json!({}));
    assert!(shown(&s)[0].ends_with("=7500dm³"), "{:?}", shown(&s));
    // Back to the automatic unit.
    let r = run(&mut s, "calc.result", json!({"unit": "auto"}));
    assert_eq!(r["shown"], "m³");
    assert!(shown(&s)[0].ends_with("=7.5m³"), "{:?}", shown(&s));
    // A unit of another quantity, or no unit, is refused and changes nothing.
    for bad in ["m²", "kN", "3 m", "("] {
        assert!(s.run("calc.result", &json!({"unit": bad})).is_err(), "{bad}");
    }
    assert!(shown(&s)[0].ends_with("=7.5m³"), "{:?}", shown(&s));
}

#[test]
fn a_results_own_decimals_and_step() {
    let mut s = volume();
    // 7.5 m³ to a multiple of 0.2 m³, up (per eccesso) and down (per difetto).
    run(&mut s, "calc.result", json!({"step": 0.2, "stepMode": "up"}));
    assert!(shown(&s)[0].ends_with("=7.6m³"), "{:?}", shown(&s));
    run(&mut s, "calc.result", json!({"stepMode": "down"}));
    assert!(shown(&s)[0].ends_with("=7.4m³"), "{:?}", shown(&s));
    let info = run(&mut s, "calc.resultInfo", json!({}));
    assert_eq!(info["step"], "0.2");
    assert_eq!(info["stepMode"], "down");
    // Decimal places of its own, trailing zeros shown by the step's decimals only.
    run(&mut s, "calc.result", json!({"step": "none", "decimals": 3, "unit": "dm³"}));
    let info = run(&mut s, "calc.resultInfo", json!({}));
    assert_eq!(info["decimals"], 3);
    assert!(shown(&s)[0].ends_with("=7500dm³"), "{:?}", shown(&s));
    // Back to the document's rounding: the unit stays.
    run(&mut s, "calc.result", json!({"reset": true}));
    let info = run(&mut s, "calc.resultInfo", json!({}));
    assert_eq!((info["decimals"].clone(), info["step"].clone(), info["unit"].clone()), (Value::Null, Value::Null, json!("dm³")));
    // Another calculation keeps the document's format.
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "calc.insert", json!({"text": "x = 7.46 m ="}));
    assert!(shown(&s)[1].ends_with("=7.46m"), "{:?}", shown(&s));
}

#[test]
fn only_a_calculation_showing_a_result_has_one_to_manage() {
    let mut s = Session::new(Document::new());
    assert!(s.run("calc.result", &json!({"unit": "m"})).is_err());
    run(&mut s, "calc.insert", json!({"text": "L = 4.12 m"}));
    assert!(s.run("calc.result", &json!({"unit": "cm"})).is_err());
    assert!(s.run("calc.resultInfo", &json!({})).is_err());
    assert!(super::caret_result(&s).is_none());
}
