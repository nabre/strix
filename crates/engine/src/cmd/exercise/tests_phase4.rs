use serde_json::{Value, json};
use wordcraft_doc::graphic::GraphicKind;
use wordcraft_doc::{Block, Document, InlineObject};

use crate::Session;
use crate::strix_objects::{StrixObjectKind, strix_objects};

fn find_first_graphic(doc: &Document, kind: GraphicKind) -> Option<(f32, f32, wordcraft_doc::graphic::Graphic)> {
    for b in &doc.body {
        if let Block::Para(p) = &**b {
            for o in &p.objects {
                if let InlineObject::Graphic { w, h, graphic, .. } = o
                    && graphic.kind == kind
                {
                    return Some((*w, *h, (**graphic).clone()));
                }
            }
        }
    }
    None
}

#[test]
fn f6_cad_answer_preserves_pagination_and_roundtrips_docx_and_xmath() {
    let mut s = Session::new(Document::new());
    s.run(
        "exercise.insert",
        &json!({"title": "Ottica geometrica", "text": "Completa il tracciamento dei raggi.", "questions": ["Individua l'immagine."]}),
    )
    .unwrap();

    // Place caret in the statement paragraph ("Completa il tracciamento dei raggi.") and insert CAD.
    let stmt_idx = s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| p.plain_text().contains("Completa il tracciamento"))).unwrap();
    let stmt_len = s.doc.body[stmt_idx].as_para().unwrap().text.len().saturating_sub(3);
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::body(stmt_idx, stmt_len));

    let drawing = json!({
        "unit": "mm",
        "scale": 2.0,
        "layers": [
            {"name": "0", "color": [0, 0, 0], "lineweight": 0.35},
            {"name": "Soluzione", "color": [210, 45, 45], "lineweight": 0.5, "answer": true}
        ],
        "entities": [
            {"type": "line", "layer": "0", "a": [0, 0], "b": [80, 0]},
            {"type": "line", "layer": "0", "a": [40, -25], "b": [40, 25]},
            {"type": "line", "layer": "Soluzione", "a": [10, 15], "b": [120, -35]},
            {"type": "circle", "layer": "0", "c": [120, -35], "r": 8, "answer": true}
        ]
    });
    s.run("insert.cad", &json!({"drawing": drawing})).unwrap();

    let (w_teacher, h_teacher, g_teacher) = find_first_graphic(&s.doc, GraphicKind::Cad).unwrap();
    assert!(g_teacher.items.len() >= 3, "teacher CAD must include base + solution entities");
    let first_base_item = g_teacher.items[0].clone();

    // Hide solution -> student copy.
    s.run("exercise.show", &json!({"solution": false})).unwrap();
    let (w_student, h_student, g_student) = find_first_graphic(&s.doc, GraphicKind::Cad).unwrap();
    assert!(
        (w_student - w_teacher).abs() < 1e-4 && (h_student - h_teacher).abs() < 1e-4,
        "student and teacher CAD box must match: ({w_student}, {h_student}) vs ({w_teacher}, {h_teacher})"
    );
    assert!(
        g_student.items.len() < g_teacher.items.len(),
        "student CAD must omit answer layer and answer entities: {} vs {}",
        g_student.items.len(),
        g_teacher.items.len()
    );
    assert_eq!(g_student.items[0], first_base_item, "base geometry coordinates must not shift");

    // Show solution again -> teacher items restored.
    s.run("exercise.show", &json!({"solution": true})).unwrap();
    let (_, _, g_restored) = find_first_graphic(&s.doc, GraphicKind::Cad).unwrap();
    assert_eq!(g_restored.items.len(), g_teacher.items.len());

    // Round-trip through DOCX and XMATH.
    for name in ["f6_test.docx", "f6_test.xmath"] {
        let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
        let loaded = crate::io::open_bytes(name, &bytes).unwrap();
        let (_, _, g_loaded) = find_first_graphic(&loaded, GraphicKind::Cad).unwrap();
        let parsed = strix_cad::Drawing::from_json(g_loaded.cad.as_deref().unwrap()).unwrap();
        assert!(parsed.layers.iter().any(|l| l.name == "Soluzione" && l.answer), "{name}: layer.answer lost");
        assert!(parsed.entities.iter().any(|e| e.answer), "{name}: entity.answer lost");
    }
}

#[test]
fn f5_plot_calculus_data_regression_student_mode_and_roundtrips() {
    let mut s = Session::new(Document::new());
    s.run("document.setText", &json!({"text": ""})).unwrap();
    s.run("calc.insert", &json!({"text": "k := 2"})).unwrap();
    s.run("text.insert", &json!({"text": "\n"})).unwrap();

    // Calculated table with measurement uncertainty for experimental series.
    s.run("calc.insertTable", &json!({"rows": 5, "cols": 2})).unwrap();
    let cells = [
        (0, 0, "t [s]"),
        (0, 1, "x_m [m]"),
        (1, 0, "1"),
        (1, 1, "2.0 ± 0.2"),
        (2, 0, "2"),
        (2, 1, "4.0 ± 0.2"),
        (3, 0, "3"),
        (3, 1, "6.0 ± 0.2"),
        (4, 0, "4"),
        (4, 1, "8.0 ± 0.2"),
    ];
    for (r, c, txt) in cells {
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![1, r, c, 0]), 0));
        s.run("text.insert", &json!({"text": txt})).unwrap();
    }
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("calc.recalculate", &json!({})).unwrap();

    let plot_spec = json!({
        "width": 130.0,
        "height": 85.0,
        "xMin": -3.0,
        "xMax": 5.0,
        "yMin": -4.0,
        "yMax": 10.0,
        "title": "Cinematica e analisi",
        "xLabel": "x",
        "yLabel": "y",
        "grid": true,
        "minorGrid": true,
        "legend": true,
        "series": [
            {
                "type": "function",
                "label": "f(x)",
                "expr": "k*x^2 - 2",
                "roots": true,
                "extrema": true,
                "tangentAt": 1.0,
                "integral": [0.0, 2.0]
            },
            {
                "type": "function",
                "label": "1/x",
                "expr": "1/x",
                "dashed": true
            },
            {
                "type": "data",
                "label": "Misure",
                "xCol": "t",
                "yCol": "x_m",
                "xUnit": "s",
                "yUnit": "m",
                "regression": "linear"
            }
        ]
    });
    s.run("plot.insert", &json!({"plot": plot_spec})).unwrap();

    let info = s.run("plot.get", &json!({})).unwrap();
    assert_eq!(info["errors"].as_array().unwrap().len(), 0, "plot errors: {info}");
    let notables = info["notables"].as_array().unwrap();
    assert!(notables.iter().any(|n| n["kind"] == "root" && (n["x"].as_f64().unwrap() - 1.0).abs() < 1e-2), "expected root near x=1: {notables:?}");
    assert!(notables.iter().any(|n| n["kind"] == "minimum" && n["x"].as_f64().unwrap().abs() < 1e-2), "expected minimum near x=0: {notables:?}");
    let area = info["integrals"][0]["value"].as_f64().unwrap();
    assert!((area - 4.0 / 3.0).abs() < 2e-2, "expected integral 4/3, got {area}");
    let r2 = info["regressions"][0]["r2"].as_f64().unwrap();
    assert!(r2 > 0.999, "expected R^2 ~ 1, got {r2}");

    // Strix object detection + DOCX and XMATH round-trips.
    let objs = strix_objects(&s.doc);
    assert!(objs.iter().any(|o| o.kind == StrixObjectKind::Plot && o.count == 1), "{objs:?}");
    for name in ["f5_test.docx", "f5_test.xmath"] {
        let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
        let loaded = crate::io::open_bytes(name, &bytes).unwrap();
        let mut s2 = Session::new(loaded);
        s2.run("calc.recalculate", &json!({})).unwrap();
        let (_, _, g) = find_first_graphic(&s2.doc, GraphicKind::Plot).unwrap();
        assert!(g.plot.is_some() && !g.items.is_empty(), "{name}: plot graphic must stay live");
    }
}

#[test]
fn f5_student_blank_grid_and_d4_variant_plot_validation() {
    let mut s = Session::new(Document::new());
    s.run("exercise.insert", &json!({"title": "Studio di funzione", "text": "Traccia il grafico.", "questions": ["Disegna f(x)."]})).unwrap();
    s.run(
        "exercise.setData",
        &json!({
            "exercise": 1,
            "data": {
                "variants": ["A", "B"],
                "active": 0,
                "items": [{"symbol": "a", "description": "parametro", "unit": "", "values": ["4", "-4"]}]
            }
        }),
    )
    .unwrap();

    let stmt_idx = s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| p.plain_text().contains("Traccia il grafico"))).unwrap();
    let stmt_len = s.doc.body[stmt_idx].as_para().unwrap().text.len().saturating_sub(3);
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::body(stmt_idx, stmt_len));

    s.run(
        "plot.insert",
        &json!({
            "xMin": 0.0,
            "xMax": 4.0,
            "yMin": 0.0,
            "yMax": 8.0,
            "answer": true,
            "expr": "sqrt(a) * x"
        }),
    )
    .unwrap();

    // Variant B has a = -4 -> sqrt(-4) must be caught by continuous multi-variant validation (D4)!
    let data_res: Value = s.run("exercise.data", &json!({"exercise": 1})).unwrap();
    assert!(data_res["variantErrors"].get("B").is_some(), "expected variant B plot domain error in variantErrors: {data_res}");
    assert!(data_res["variantErrors"].get("A").is_none(), "variant A must be clean: {data_res}");

    // Student vs Teacher dual mode: identical bounding box, fewer items on student copy.
    let (w_t, h_t, g_t) = find_first_graphic(&s.doc, GraphicKind::Plot).unwrap();
    s.run("exercise.show", &json!({"solution": false})).unwrap();
    let (w_s, h_s, g_s) = find_first_graphic(&s.doc, GraphicKind::Plot).unwrap();
    assert!((w_t - w_s).abs() < 1e-4 && (h_t - h_s).abs() < 1e-4);
    assert!(g_s.items.len() < g_t.items.len(), "student blank grid must omit answer curve: {} vs {}", g_s.items.len(), g_t.items.len());
}
