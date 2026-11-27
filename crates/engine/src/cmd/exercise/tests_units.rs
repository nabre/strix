//! Data given in different units of the same quantity in different variants (docs/exercise.md,
//! Data and variants): `d = 100 m` in variant A, `d = 10000 cm` in variant B.

use serde_json::{Value, json};
use wordcraft_doc::{Block, Document, Pos, StoryRef};

use super::{OBJ, body_path, convert_value, plain_shown, same_quantity};
use crate::Session;

fn session() -> Session {
    let mut s = Session::new(Document::new());
    s.run("document.setText", &json!({"text": "Intro"})).unwrap();
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    s
}

fn texts(s: &Session) -> Vec<String> {
    s.doc
        .body
        .iter()
        .map(|b| match &**b {
            Block::Para(p) => plain_shown(p),
            Block::Table(_) => "[table]".into(),
        })
        .collect()
}

fn item(s: &mut Session, k: usize) -> Value {
    s.run("exercise.data", &json!({"exercise": 1})).unwrap()["data"]["items"][k].clone()
}

/// An exercise `v = d/t` with `d` in metres and `t` in seconds in two variants, the result
/// given by the calculation.
fn motion() -> Session {
    let mut s = session();
    s.run("exercise.insert", &json!({"title": "Moto", "questions": ["Velocità"]})).unwrap();
    let data = json!({
        "items": [
            {"symbol": "d", "description": "spazio", "unit": "m", "values": ["100", "100"]},
            {"symbol": "t", "description": "tempo", "unit": "s", "values": ["10", "20"]}
        ],
        "variants": ["A", "B"],
        "active": 0
    });
    s.run("exercise.setData", &json!({"data": data})).unwrap();
    let free = texts(&s).iter().position(|x| x == "a) Velocità").unwrap() + 1;
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, body_path(free), 0));
    s.run("calc.insert", &json!({"text": "v = d/t"})).unwrap();
    let mut at = s.sel.focus.clone();
    at.off -= OBJ.len_utf8();
    s.sel = crate::Selection::caret(at);
    s.run("equation.answer", &json!({"question": 1})).unwrap();
    s
}

/// The data of variant `active` with datum 0 written as `equation` there.
fn with_equation(s: &mut Session, active: usize, equation: &str) {
    let mut data = s.run("exercise.data", &json!({"exercise": 1})).unwrap()["data"].clone();
    data["active"] = json!(active);
    data["items"][0]["equation"] = json!(equation);
    s.run("exercise.setData", &json!({"exercise": 1, "data": data})).unwrap();
}

#[test]
fn units_of_the_same_quantity() {
    assert!(same_quantity("m", "cm"));
    assert!(same_quantity("kN/m", "N/mm"));
    assert!(same_quantity("km/h", "m/s"));
    assert!(same_quantity("°C", "K"));
    assert!(same_quantity("", ""));
    assert!(!same_quantity("m", "kg"));
    assert!(!same_quantity("m", ""));
    assert!(!same_quantity("m", "nonunit"));
}

#[test]
fn values_are_converted_within_their_quantity() {
    assert_eq!(convert_value("100", "m", "cm").as_deref(), Some("10000"));
    assert_eq!(convert_value("4,5", "m", "cm").as_deref(), Some("450"));
    assert_eq!(convert_value("1500", "mm", "m").as_deref(), Some("1.5"));
    assert_eq!(convert_value("20", "°C", "K").as_deref(), Some("293.15"));
    assert_eq!(convert_value("2", "1/m³", "1/cm³").as_deref(), Some("0.000002"));
    assert_eq!(convert_value("2*a", "m", "cm"), None);
    assert_eq!(convert_value("1", "m", "kg"), None);
}

/// A variant gives the datum in another unit of its quantity: only that variant changes, and
/// changing only the unit converts the value, so the quantity (and every result) stays; the
/// other variants keep theirs.
#[test]
fn a_variant_gives_a_datum_in_another_unit_of_the_same_quantity() {
    let mut s = motion();
    assert!(texts(&s).contains(&"a) v = 10 m/s".to_string()), "{:?}", texts(&s));
    // Variant B: 100 m written in cm is 10000 cm, the same distance, the same speed.
    with_equation(&mut s, 1, "d = 100 cm");
    let d = item(&mut s, 0);
    assert_eq!(d["unit"], "m");
    assert_eq!(d["units"], json!(["m", "cm"]));
    assert_eq!(d["values"], json!(["100", "10000"]));
    let t = texts(&s);
    assert!(t.contains(&"spazio: d = 10000 cm".to_string()), "{t:?}");
    assert!(t.contains(&"a) v = 5 m/s".to_string()), "{t:?}");
    let info = s.run("exercise.data", &json!({"exercise": 1})).unwrap();
    assert!(info["variantErrors"].as_object().is_none_or(serde_json::Map::is_empty), "{info}");
    assert!(info["errors"].as_array().unwrap().iter().all(Value::is_null), "{info}");
    // A new value in the variant's unit is taken as written.
    with_equation(&mut s, 1, "d = 30000 cm");
    assert_eq!(item(&mut s, 0)["values"], json!(["100", "30000"]));
    assert!(texts(&s).contains(&"a) v = 15 m/s".to_string()), "{:?}", texts(&s));
    // Variant A keeps metres.
    s.run("exercise.variant", &json!({"exercise": 1, "active": 0})).unwrap();
    let t = texts(&s);
    assert!(t.contains(&"spazio: d = 100 m".to_string()), "{t:?}");
    assert!(t.contains(&"a) v = 10 m/s".to_string()), "{t:?}");
    // The units survive saving.
    let bytes = crate::io::save_bytes("e.xmath", &s.doc).unwrap();
    let mut back = Session::new(crate::io::open_bytes("e.xmath", &bytes).unwrap());
    assert_eq!(item(&mut back, 0)["units"], json!(["m", "cm"]));
    // Back to the variants' common unit: one unit again.
    with_equation(&mut s, 1, "d = 300 m");
    let d = item(&mut s, 0);
    assert_eq!(d["unit"], "m");
    assert_eq!(d["values"], json!(["100", "300"]));
    assert!(d.get("units").is_none(), "{d}");
}

/// A unit of another quantity (or a typo) changes only the active variant: the other variants
/// keep their value and unit, and the datum is reported until they agree.
#[test]
fn a_unit_of_another_quantity_never_touches_the_other_variants() {
    let mut s = motion();
    with_equation(&mut s, 1, "d = 100 cm");
    with_equation(&mut s, 0, "d = 3 kg");
    let d = item(&mut s, 0);
    assert_eq!(d["units"], json!(["kg", "cm"]), "{d}");
    assert_eq!(d["values"], json!(["3", "10000"]));
    let info = s.run("exercise.data", &json!({"exercise": 1})).unwrap();
    let error = info["errors"][0].as_str().unwrap_or_default().to_string();
    assert!(error.contains("different quantities") && error.contains("A: kg") && error.contains("B: cm"), "{info}");
    assert!(info["variantErrors"]["B"].to_string().contains("different quantities"), "{info}");
    let check = s.run("exercise.check", &json!({})).unwrap();
    assert!(check.to_string().contains("data_units_conflict"), "{check}");
    // A typo is never taken for another variant's unit either.
    with_equation(&mut s, 0, "d = 100 mx");
    assert_eq!(item(&mut s, 0)["units"], json!(["mx", "cm"]));
    // Fixed: no error left.
    with_equation(&mut s, 0, "d = 100 m");
    let info = s.run("exercise.data", &json!({"exercise": 1})).unwrap();
    assert!(info["errors"].as_array().unwrap().iter().all(Value::is_null), "{info}");
    assert!(!s.run("exercise.check", &json!({})).unwrap().to_string().contains("data_units_conflict"));
}

/// `exercise.setData` given units of different quantities for one datum: reported.
#[test]
fn units_of_different_quantities_given_to_set_data_are_reported() {
    let mut s = motion();
    let mut data = s.run("exercise.data", &json!({"exercise": 1})).unwrap()["data"].clone();
    data["items"][0]["units"] = json!(["m", "kg"]);
    s.run("exercise.setData", &json!({"exercise": 1, "data": data})).unwrap();
    let info = s.run("exercise.data", &json!({"exercise": 1})).unwrap();
    assert!(info["errors"][0].as_str().unwrap_or_default().contains("different quantities"), "{info}");
    assert!(!info["variantErrors"].as_object().unwrap().is_empty(), "{info}");
}

/// Typed on the page, a unit of the same quantity is the active variant's alone, its value
/// converted when only the unit changed.
#[test]
fn a_unit_typed_on_the_page_changes_only_the_active_variant() {
    let mut s = motion();
    s.run("exercise.variant", &json!({"exercise": 1, "active": 1})).unwrap();
    let at = s.doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if plain_shown(p).starts_with("tempo:"))).unwrap();
    let len = s.doc.para(StoryRef::Body, &body_path(at)).unwrap().text.len();
    let end = s.doc.clamp(&Pos::body(at, len));
    s.sel = crate::Selection { anchor: s.doc.clamp(&Pos::body(at, 0)), focus: end };
    s.run("text.insert", &json!({"text": "tempo: t = 20 ms"})).unwrap();
    s.sel = crate::Selection::caret(Pos::body(0, 0));
    s.run("exercise.info", &json!({})).unwrap();
    let t_item = item(&mut s, 1);
    assert_eq!(t_item["units"], json!(["s", "ms"]), "{t_item}");
    assert_eq!(t_item["values"], json!(["10", "20000"]));
    let t = texts(&s);
    assert!(t.contains(&"tempo: t = 20000 ms".to_string()), "{t:?}");
    assert!(t.contains(&"a) v = 5 m/s".to_string()), "{t:?}");
    s.run("exercise.variant", &json!({"exercise": 1, "active": 0})).unwrap();
    assert!(texts(&s).contains(&"tempo: t = 10 s".to_string()), "{:?}", texts(&s));
}

/// The variant generator draws a datum given in several units with its unit, kept with its
/// value exactly as validated (reciprocal units too).
#[test]
fn generated_variants_keep_each_value_with_its_unit() {
    let mut s = motion();
    let mut data = s.run("exercise.data", &json!({"exercise": 1})).unwrap()["data"].clone();
    data["items"][0]["units"] = json!(["m", "cm"]);
    data["items"][0]["values"] = json!(["100", "20000"]);
    data["items"].as_array_mut().unwrap().push(json!({"symbol": "n", "unit": "1/m³", "units": ["1/m³", "1/cm³"], "values": ["2", "3"]}));
    s.run("exercise.setData", &json!({"exercise": 1, "data": data})).unwrap();
    s.run("exercise.generateVariants", &json!({"exercise": 1, "count": 4, "domains": {"t": [5, 10, 20, 25]}})).unwrap();
    let pairs = |d: &Value| -> Vec<(String, String)> {
        let values = d["values"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string());
        let units: Vec<String> = match d["units"].as_array() {
            Some(u) => u.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            None => vec![d["unit"].as_str().unwrap().to_string(); 4],
        };
        values.zip(units).collect()
    };
    let d = item(&mut s, 0);
    assert_eq!(d["values"].as_array().map(Vec::len), Some(4));
    for (v, u) in pairs(&d) {
        assert!(matches!((v.as_str(), u.as_str()), ("100", "m") | ("20000", "cm")), "{d}");
    }
    let n = item(&mut s, 2);
    for (v, u) in pairs(&n) {
        assert!(matches!((v.as_str(), u.as_str()), ("2", "1/m³") | ("3", "1/cm³")), "{n}");
    }
}

/// Data saved before units per variant read as before; hostile units never break them.
#[test]
fn units_per_variant_are_sanitized() {
    let mut s = session();
    s.run("exercise.insert", &json!({"title": "Moto", "questions": ["Velocità"]})).unwrap();
    let data = json!({
        "items": [{"symbol": "d", "unit": "m", "units": ["", "cm", "km", "\u{1F}x"], "values": ["1", "2"]}],
        "variants": ["A", "B"],
        "active": 1
    });
    s.run("exercise.setData", &json!({"data": data})).unwrap();
    let d = item(&mut s, 0);
    assert_eq!(d["units"], json!(["m", "cm"]), "{d}");
    assert!(texts(&s).contains(&"d = 2 cm".to_string()), "{:?}", texts(&s));
}
