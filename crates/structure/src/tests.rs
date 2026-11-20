//! Unit tests for `strix-structure`: textbook cases.

use strix_calc::{Options, Quantity, Value, Worksheet};

use crate::{
    Action, ForceUnit, LengthUnit, Load, LoadAxes, LoadPer, Member, Node, NotableKind, Num, Status, Structure, Support, SupportKind, analyse,
    notables,
};

fn node(id: &str, x: f64, y: f64) -> Node {
    Node { id: id.into(), x: x.into(), y: y.into(), hinge: false }
}

fn member(id: &str, a: &str, b: &str) -> Member {
    Member { id: id.into(), a: a.into(), b: b.into(), ..Member::default() }
}

fn support(node: &str, kind: SupportKind, angle: f64) -> Support {
    Support { node: node.into(), kind, angle: angle.into() }
}

fn force_on(member: &str, at: f64, value: f64) -> Load {
    Load::Force { node: None, member: Some(member.into()), at: at.into(), value: value.into(), angle: Num::new(-90.0) }
}

fn force_at(node: &str, value: f64, angle: f64) -> Load {
    Load::Force { node: Some(node.into()), member: None, at: Num::default(), value: value.into(), angle: angle.into() }
}

fn uniform(member: &str, q: f64) -> Load {
    Load::Distributed {
        member: member.into(),
        q1: q.into(),
        q2: None,
        from: Num::default(),
        to: None,
        angle: Num::new(-90.0),
        axes: LoadAxes::Global,
        per: LoadPer::Length,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * (1.0 + b.abs())
}

macro_rules! assert_close {
    ($a:expr, $b:expr) => {{
        let (a, b) = ($a, $b);
        assert!(close(a, b), "{} = {a}, expected {b}", stringify!($a));
    }};
}

/// Simply supported beam, `L = 6`, `q = 10`.
fn simple_beam() -> Structure {
    Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 6.0, 0.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Pin, 0.0), support("B", SupportKind::Roller, 0.0)],
        loads: vec![uniform("1", 10.0)],
        ..Structure::default()
    }
}

#[test]
fn simply_supported_beam_with_a_uniform_load() {
    let an = analyse(&simple_beam());
    assert_eq!(an.status, Status::Isostatic);
    assert_close!(an.reactions[0].fy, 30.0);
    assert_close!(an.reactions[0].fx, 0.0);
    assert_close!(an.reactions[1].fy, 30.0);
    let m = &an.members[0];
    assert_close!(m.forces(0.0, true).v, 30.0);
    assert_close!(m.forces(6.0, false).v, -30.0);
    assert_close!(m.forces(3.0, true).m, 45.0);
    assert_close!(m.forces(3.0, true).n, 0.0);
    // The maximum moment is notable, where the shear is zero.
    let ms = notables(m, Action::Moment);
    let peak = ms.iter().find(|n| n.kind == NotableKind::Extreme).expect("extreme");
    assert_close!(peak.x, 3.0);
    assert_close!(peak.value, 45.0);
    let vs = notables(m, Action::Shear);
    assert!(vs.iter().any(|n| n.kind == NotableKind::Zero && close(n.x, 3.0)), "{vs:?}");
}

#[test]
fn cantilever_with_a_tip_load() {
    // Wall on the left of A: the fixed support turned −90°.
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 3.0, 0.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Fixed, -90.0)],
        loads: vec![force_at("B", 10.0, -90.0)],
        ..Structure::default()
    };
    let an = analyse(&st);
    assert_eq!(an.status, Status::Isostatic);
    let r = &an.reactions[0];
    assert_close!(r.fy, 10.0);
    assert_close!(r.m, 30.0);
    let m = &an.members[0];
    assert_close!(m.forces(0.0, true).m, -30.0);
    assert_close!(m.forces(3.0, false).m, 0.0);
    assert_close!(m.forces(1.0, true).v, 10.0);
}

#[test]
fn propped_cantilever_is_hyperstatic_of_degree_one() {
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 4.0, 0.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Fixed, -90.0), support("B", SupportKind::Roller, 0.0)],
        loads: vec![uniform("1", 12.0)],
        ..Structure::default()
    };
    let an = analyse(&st);
    assert_eq!(an.status, Status::Hyperstatic { degree: 1 });
    // R_B = 3qL/8, M_A = −qL²/8.
    assert_close!(an.reactions[1].fy, 3.0 * 12.0 * 4.0 / 8.0);
    assert_close!(an.members[0].forces(0.0, true).m, -12.0 * 16.0 / 8.0);
}

/// The force method's released structures: without the roller, a cantilever; without the fixed
/// end's couple, a simply supported beam.
#[test]
fn released_constraints_leave_an_isostatic_structure() {
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 4.0, 0.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Fixed, -90.0), support("B", SupportKind::Roller, 0.0)],
        loads: vec![uniform("1", 12.0)],
        ..Structure::default()
    };
    let an = crate::analyse_released(&st, &[(1, 0)]);
    assert_eq!(an.status, Status::Isostatic);
    assert_eq!(an.reactions.len(), 1);
    assert_close!(an.reactions[0].fy, 48.0);
    assert_close!(an.members[0].forces(0.0, true).m, -96.0);
    let an = crate::analyse_released(&st, &[(0, 2)]);
    assert_eq!(an.status, Status::Isostatic);
    assert_close!(an.reactions[0].m, 0.0);
    assert_close!(an.members[0].forces(2.0, true).m, 24.0);
    // Both: it can move.
    assert!(matches!(crate::analyse_released(&st, &[(0, 2), (1, 0)]).status, Status::Labile { .. }));
}

#[test]
fn fixed_fixed_beam_with_a_central_load() {
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 8.0, 0.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Fixed, -90.0), support("B", SupportKind::Fixed, 90.0)],
        loads: vec![force_on("1", 4.0, 20.0)],
        ..Structure::default()
    };
    let an = analyse(&st);
    assert_eq!(an.status, Status::Hyperstatic { degree: 3 });
    let m = &an.members[0];
    // −PL/8 at the ends, +PL/8 under the load; the shear jumps by P there.
    assert_close!(m.forces(0.0, true).m, -20.0);
    assert_close!(m.forces(8.0, false).m, -20.0);
    assert_close!(m.forces(4.0, true).m, 20.0);
    assert_close!(m.forces(4.0, false).v, 10.0);
    assert_close!(m.forces(4.0, true).v, -10.0);
    assert!(m.jumps_at(4.0));
}

#[test]
fn continuous_beam_over_three_supports() {
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 5.0, 0.0), node("C", 10.0, 0.0)],
        members: vec![member("1", "A", "B"), member("2", "B", "C")],
        supports: vec![support("A", SupportKind::Pin, 0.0), support("B", SupportKind::Roller, 0.0), support("C", SupportKind::Roller, 0.0)],
        loads: vec![uniform("1", 8.0), uniform("2", 8.0)],
        ..Structure::default()
    };
    let an = analyse(&st);
    assert_eq!(an.status, Status::Hyperstatic { degree: 1 });
    // Middle reaction 10qL/8, ends 3qL/8, moment over the middle support −qL²/8.
    assert_close!(an.reactions[1].fy, 10.0 * 8.0 * 5.0 / 8.0);
    assert_close!(an.reactions[0].fy, 3.0 * 8.0 * 5.0 / 8.0);
    assert_close!(an.members[0].forces(5.0, false).m, -8.0 * 25.0 / 8.0);
    assert_close!(an.members[1].forces(0.0, true).m, -8.0 * 25.0 / 8.0);
}

#[test]
fn gerber_beam_with_an_internal_hinge() {
    // Fixed at A, hinge at B, roller at C; load P at the middle of BC.
    let mut st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 2.0, 0.0), node("C", 6.0, 0.0)],
        members: vec![member("1", "A", "B"), member("2", "B", "C")],
        supports: vec![support("A", SupportKind::Fixed, -90.0), support("C", SupportKind::Roller, 0.0)],
        loads: vec![force_on("2", 2.0, 10.0)],
        ..Structure::default()
    };
    st.nodes[1].hinge = true;
    let an = analyse(&st);
    assert_eq!(an.status, Status::Isostatic);
    // BC is simply supported: 5 at B and C; A takes 5 and M_A = 5·2 = 10 (counter-clockwise).
    assert_close!(an.reactions[1].fy, 5.0);
    assert_close!(an.reactions[0].fy, 5.0);
    assert_close!(an.reactions[0].m, 10.0);
    assert_close!(an.members[0].forces(2.0, false).m, 0.0);
    assert_close!(an.members[1].forces(2.0, true).m, 10.0);
    // A member end hinge does the same as the node's.
    st.nodes[1].hinge = false;
    st.members[0].hinge_b = true;
    let an2 = analyse(&st);
    assert_eq!(an2.status, Status::Isostatic);
    assert_close!(an2.reactions[0].m, 10.0);
}

#[test]
fn beam_on_two_rollers_is_labile() {
    let mut st = simple_beam();
    st.supports[0].kind = SupportKind::Roller;
    let an = analyse(&st);
    assert_eq!(an.status, Status::Labile { degree: 1, redundant: 0 });
    assert!(an.reactions.is_empty());
    // Three hinges in a row: labile too, though it counts enough constraints.
    let mut st = simple_beam();
    st.nodes.push(node("C", 12.0, 0.0));
    st.members.push(member("2", "B", "C"));
    st.supports[1].kind = SupportKind::Pin;
    st.supports.push(support("C", SupportKind::Pin, 0.0));
    st.nodes[1].hinge = true;
    st.supports.remove(1);
    st.supports.insert(1, support("B", SupportKind::Pin, 0.0));
    // A pin at the hinge holds it: isostatic. Move it to a roller standing sideways: labile.
    assert!(analyse(&st).status.is_solved(), "{:?}", analyse(&st).status);
    st.supports[1] = support("B", SupportKind::Roller, 90.0);
    assert!(matches!(analyse(&st).status, Status::Labile { .. }), "{:?}", analyse(&st).status);
}

#[test]
fn triangular_truss() {
    // Span 4, height 3; pin at A, roller at C, 12 down at the apex B.
    let mut st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 2.0, 3.0), node("C", 4.0, 0.0)],
        members: vec![member("1", "A", "B"), member("2", "B", "C"), member("3", "A", "C")],
        supports: vec![support("A", SupportKind::Pin, 0.0), support("C", SupportKind::Roller, 0.0)],
        loads: vec![force_at("B", 12.0, -90.0)],
        ..Structure::default()
    };
    for n in &mut st.nodes {
        n.hinge = true;
    }
    let an = analyse(&st);
    assert_eq!(an.status, Status::Isostatic);
    assert!(an.truss);
    let len = 13.0_f64.sqrt();
    // Each rafter: N = −6·ℓ/3 (compression); the tie: +6·2/3 (tension).
    assert_close!(an.members[0].forces(1.0, true).n, -6.0 * len / 3.0);
    assert_close!(an.members[1].forces(1.0, true).n, -6.0 * len / 3.0);
    assert_close!(an.members[2].forces(1.0, true).n, 4.0);
    assert_close!(an.members[2].forces(1.0, true).m, 0.0);
}

#[test]
fn portal_frame_with_a_horizontal_load() {
    // Columns 4 high, beam 6 long; pin at A, roller at D; 10 to the right at B.
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 0.0, 4.0), node("C", 6.0, 4.0), node("D", 6.0, 0.0)],
        members: vec![member("1", "A", "B"), member("2", "B", "C"), member("3", "C", "D")],
        supports: vec![support("A", SupportKind::Pin, 0.0), support("D", SupportKind::Roller, 0.0)],
        loads: vec![force_at("B", 10.0, 0.0)],
        ..Structure::default()
    };
    let an = analyse(&st);
    assert_eq!(an.status, Status::Isostatic);
    let (a, d) = (&an.reactions[0], &an.reactions[1]);
    assert_close!(a.fx, -10.0);
    // Moments about A: 10·4 = D·6.
    assert_close!(d.fy, 40.0 / 6.0);
    assert_close!(a.fy, -40.0 / 6.0);
    // Column AB: the moment at B is 10·4 = 40, on the outside fibres (local −y of A→B is right):
    // H_A = 10 to the left at A bends it with its right side stretched? Check the magnitude.
    assert_close!(an.members[0].forces(4.0, false).m.abs(), 40.0);
    assert_close!(an.members[1].forces(0.0, true).m.abs(), 40.0);
}

#[test]
fn a_change_of_units_keeps_the_structure() {
    let mut st = simple_beam();
    st.loads.push(force_on("1", 2.0, 5.0));
    st.members[0].e = Some(Num::new(2.1e8)); // kN/m²
    st.members[0].i = Some(Num::new(1e-4)); // m⁴
    st.members[0].area = Some(Num::new(0.07)); // m²
    st.supports[1].angle = Num::new(30.0);
    st.convert_units(LengthUnit::Cm, ForceUnit::N);
    assert_eq!((st.length_unit, st.force_unit), (LengthUnit::Cm, ForceUnit::N));
    assert_eq!(st.nodes[1].x.value, 600.0);
    assert_eq!(st.supports[1].angle.value, 30.0, "angles don't change");
    let Load::Distributed { q1, .. } = &st.loads[0] else { panic!() };
    assert_eq!(q1.value, 100.0, "10 kN/m = 100 N/cm");
    let Load::Force { at, value, .. } = &st.loads[1] else { panic!() };
    assert_eq!((at.value, value.value), (200.0, 5000.0));
    let m = &st.members[0];
    assert_eq!(m.e.as_ref().unwrap().value, 2.1e7, "N/cm²");
    assert_eq!(m.i.as_ref().unwrap().value, 1e4, "cm⁴");
    assert_eq!(m.area.as_ref().unwrap().value, 700.0, "cm², without binary noise");
    // And back: the same numbers as before.
    let mut back = st.clone();
    back.convert_units(LengthUnit::M, ForceUnit::KN);
    let mut orig = simple_beam();
    orig.loads.push(force_on("1", 2.0, 5.0));
    assert_eq!(back.nodes, orig.nodes);
    assert_eq!(back.loads, orig.loads);
    // An expression is kept as written.
    let mut e = simple_beam();
    e.nodes[1].x = Num { value: 6.0, expr: Some("L".into()) };
    e.convert_units(LengthUnit::Mm, ForceUnit::KN);
    assert_eq!(e.nodes[1].x.expr.as_deref(), Some("L"));
}

#[test]
fn triangular_load_on_a_simple_beam() {
    let mut st = simple_beam();
    st.loads = vec![Load::Distributed {
        member: "1".into(),
        q1: 0.0.into(),
        q2: Some(9.0.into()),
        from: Num::default(),
        to: None,
        angle: Num::new(-90.0),
        axes: LoadAxes::Global,
        per: LoadPer::Length,
    }];
    let an = analyse(&st);
    let (l, q) = (6.0, 9.0);
    assert_close!(an.reactions[0].fy, q * l / 6.0);
    assert_close!(an.reactions[1].fy, q * l / 3.0);
    let peak = notables(&an.members[0], Action::Moment).into_iter().find(|n| n.kind == NotableKind::Extreme).expect("extreme");
    assert_close!(peak.x, l / 3.0_f64.sqrt());
    assert_close!(peak.value, q * l * l / (9.0 * 3.0_f64.sqrt()));
}

#[test]
fn couple_at_midspan() {
    let mut st = simple_beam();
    st.loads = vec![Load::Moment { node: None, member: Some("1".into()), at: 3.0.into(), value: 12.0.into() }];
    let an = analyse(&st);
    // Counter-clockwise couple: about A, R_B·L + C = 0, so B pulls down C/L and A pushes up.
    assert_close!(an.reactions[0].fy, 2.0);
    assert_close!(an.reactions[1].fy, -2.0);
    let m = &an.members[0];
    assert_close!(m.forces(3.0, false).m, 6.0);
    assert_close!(m.forces(3.0, true).m, -6.0);
}

#[test]
fn inclined_roller() {
    // Roller at B on a plane at 30°: its reaction leans 30° from the vertical.
    let mut st = simple_beam();
    st.supports[1].angle = 30.0.into();
    let an = analyse(&st);
    assert_eq!(an.status, Status::Isostatic);
    let b = &an.reactions[1];
    assert_close!(b.fy, 30.0);
    assert_close!(b.fx, -30.0 * 30.0_f64.to_radians().tan());
    assert_close!(an.reactions[0].fx, -b.fx);
    // The beam is compressed by the horizontal thrust between A and B.
    assert_close!(an.members[0].forces(1.0, true).n, b.fx);
}

#[test]
fn projected_load_on_a_rafter() {
    // A rafter from (0,0) to (4,3) with 5 per horizontal metre: total 20, as on a 4 m beam.
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 4.0, 3.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Pin, 0.0), support("B", SupportKind::Roller, 0.0)],
        loads: vec![Load::Distributed {
            member: "1".into(),
            q1: 5.0.into(),
            q2: None,
            from: Num::default(),
            to: None,
            angle: Num::new(-90.0),
            axes: LoadAxes::Global,
            per: LoadPer::Projection,
        }],
        ..Structure::default()
    };
    let an = analyse(&st);
    assert_close!(an.reactions[0].fy + an.reactions[1].fy, 20.0);
    // The moment at midspan is q·a²/8 with a the horizontal span.
    assert_close!(an.members[0].forces(2.5, true).m, 5.0 * 16.0 / 8.0);
}

#[test]
fn local_load_across_a_member() {
    // Wind across a column: 2 per metre on a 3 m cantilever column.
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 0.0, 3.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Fixed, 0.0)],
        loads: vec![Load::Distributed {
            member: "1".into(),
            q1: 2.0.into(),
            q2: None,
            from: Num::default(),
            to: None,
            angle: Num::new(-90.0),
            axes: LoadAxes::Local,
            per: LoadPer::Length,
        }],
        ..Structure::default()
    };
    let an = analyse(&st);
    // Local −y of a column going up is +x: the wind blows right, A pushes back left.
    assert_close!(an.reactions[0].fx, -6.0);
    assert_close!(an.reactions[0].m.abs(), 9.0);
}

#[test]
fn expressions_follow_the_calculations() {
    let mut ws = Worksheet::new(Options::default());
    let length = strix_calc::unit("cm").expect("cm");
    ws.define("L", Value::Scalar(Quantity::new(600.0 * length.value, length.dim)));
    let knm = strix_calc::unit("kN").expect("kN").value;
    ws.define("q", Value::Scalar(Quantity::new(10.0 * knm, strix_calc::Dim::new([0, 1, -2, 0, 0, 0, 0]))));
    let mut st = simple_beam();
    st.nodes[1].x = Num::expr("L");
    st.loads = vec![Load::Distributed {
        member: "1".into(),
        q1: Num::expr("q"),
        q2: None,
        from: Num::expr("L/2 - 1"),
        to: None,
        angle: Num::new(-90.0),
        axes: LoadAxes::Global,
        per: LoadPer::Length,
    }];
    let errors = st.resolve(&ws);
    assert!(errors.is_empty(), "{errors:?}");
    assert_close!(st.nodes[1].x.value, 6.0);
    // `L/2 - 1` mixes a length and a number: lengths count in metres.
    match &st.loads[0] {
        Load::Distributed { q1, from, .. } => {
            assert_close!(q1.value, 10.0);
            assert_close!(from.value, 2.0);
        }
        other => panic!("{other:?}"),
    }
    let mut bad = simple_beam();
    bad.nodes[1].x = Num::expr("q");
    let errors = bad.resolve(&ws);
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn json_round_trip_and_defaults() {
    let st = simple_beam();
    let json = st.to_json();
    assert!(!json.contains("\"angle\":-90"), "the default direction is left out: {json}");
    let back = Structure::from_json(&json).expect("parse");
    assert_eq!(back, st.sanitized());
    let small =
        Structure::from_json(r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":"L","y":0}],"members":[{"id":"1","a":"A","b":"B"}]}"#).expect("parse");
    assert_eq!(small.nodes[1].x.expr.as_deref(), Some("L"));
    assert!(small.diagrams.m.positive == crate::Side::Down, "M on the stretched fibres by default");
}

#[test]
fn rendering_student_and_teacher_copies_have_the_same_size() {
    let st = simple_beam();
    let teacher = st.render_for(None, true);
    let student = st.render_for(None, false);
    assert_eq!(teacher.plot.size, student.plot.size);
    assert!(teacher.plot.prims.len() > student.plot.prims.len());
    let text = |r: &crate::StructureRendered| -> Vec<String> { texts(r).into_iter().map(|(t, _)| t).collect() };
    let t = text(&teacher);
    assert!(t.iter().any(|s| s == "45"), "the maximum moment is written: {t:?}");
    assert!(t.iter().any(|s| s == "30 kN"), "the reactions are written: {t:?}");
    assert!(t.iter().any(|s| s == "M [kN·m]"), "{t:?}");
    assert!(!text(&student).iter().any(|s| s == "45"));
    // Not an answer: the student sees everything.
    let mut open = st.clone();
    open.answer = false;
    assert_eq!(open.render_for(None, false).plot.prims.len(), teacher.plot.prims.len());
    let svg = teacher.plot.to_svg();
    assert!(svg.starts_with("<svg"));
}

#[test]
fn a_labile_structure_says_so_and_draws_no_diagrams() {
    let mut st = simple_beam();
    st.supports[0].kind = SupportKind::Roller;
    let r = st.render(None);
    let texts: Vec<String> =
        r.plot.prims.iter().filter_map(|p| if let strix_plot::PlotPrim::Text { text, .. } = p { Some(text.clone()) } else { None }).collect();
    assert!(texts.iter().any(|t| t.starts_with("labile")), "{texts:?}");
}

/// A diagram's name is written over its panel in place of its symbol; an empty one gives the symbol
/// back; a long one is cut.
#[test]
fn the_diagrams_names_title_their_panels() {
    let texts = |st: &Structure| -> Vec<String> {
        st.render(None)
            .plot
            .prims
            .iter()
            .filter_map(|p| if let strix_plot::PlotPrim::Text { text, .. } = p { Some(text.clone()) } else { None })
            .collect()
    };
    let mut st = simple_beam();
    let plain = texts(&st);
    assert!(plain.iter().any(|t| t == "T [kN]"), "{plain:?}");
    st.names.v = "Sforzo di taglio".to_string();
    st.names.m = "Momento flettente".to_string();
    let named = texts(&st);
    assert!(named.iter().any(|t| t == "Sforzo di taglio [kN]"), "{named:?}");
    assert!(named.iter().any(|t| t == "Momento flettente [kN·m]"), "{named:?}");
    assert!(named.iter().any(|t| t == "N [kN]"), "{named:?}");
    assert!(!named.iter().any(|t| t == "T [kN]"), "{named:?}");
    st.names.n = "x".repeat(500);
    assert_eq!(st.clone().sanitized().names.n.chars().count(), crate::Names::MAX);
    // Saved only when given.
    assert!(!simple_beam().to_json().contains("names"));
}

#[test]
fn hostile_input_never_panics() {
    for json in [
        "{}",
        r#"{"members":[{"id":"1","a":"A","b":"A"}]}"#,
        r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"A","x":1,"y":0}],"members":[{"id":"1","a":"A","b":"A"}]}"#,
        r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":1e300,"y":-1e300}],"members":[{"id":"1","a":"A","b":"B"}],"supports":[{"node":"A","kind":"fixed","angle":1e308}],"loads":[{"type":"force","member":"1","at":-5,"value":1e308},{"type":"distributed","member":"1","q1":"1/0","from":9,"to":-9}],"width":-1}"#,
        r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":1,"y":0}],"members":[{"id":"1","a":"A","b":"B","e":0,"i":-1}],"supports":[{"node":"Z","kind":"pin"}],"loads":[{"type":"moment","node":"Q","value":3},{"type":"force","value":2}]}"#,
        r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":1,"y":0},{"id":"C","x":2,"y":0}],"members":[{"id":"1","a":"A","b":"B"}],"supports":[{"node":"C","kind":"pin"}],"loads":[{"type":"force","node":"C","value":3}]}"#,
    ] {
        if let Ok(st) = Structure::from_json(json) {
            let r = st.render(None);
            assert!(r.plot.size.0.is_finite() && r.plot.size.1.is_finite(), "{json}");
            for p in &r.plot.prims {
                if let strix_plot::PlotPrim::Path { pts, .. } = p {
                    assert!(pts.iter().all(|(x, y)| x.is_finite() && y.is_finite()), "{json}");
                }
            }
        }
    }
    assert!(Structure::from_json("not json").is_err());
}

#[test]
fn resultants_and_laws_for_a_written_working() {
    let mut st = simple_beam();
    st.loads.push(Load::Distributed {
        member: "1".into(),
        q1: 2.0.into(),
        q2: Some(8.0.into()),
        from: 0.0.into(),
        to: Some(3.0.into()),
        angle: Num::new(-90.0),
        axes: LoadAxes::Global,
        per: LoadPer::Length,
    });
    let rs = crate::resultants(&st);
    // The uniform load, then the trapezoid as a rectangle (2·3 at 1.5) and a triangle (6·3/2 at 2).
    assert_eq!(rs.len(), 3);
    assert_close!(rs[0].value(), 60.0);
    assert_close!(rs[0].at.0, 3.0);
    assert_close!(rs[1].value(), 6.0);
    assert_close!(rs[1].at.0, 1.5);
    assert_close!(rs[2].value(), 9.0);
    assert_close!(rs[2].at.0, 2.0);
    // The laws of the simple beam: V = 30 − 10x, M = 30x − 5x².
    let an = analyse(&simple_beam());
    let s = crate::stretches(&an.members[0]);
    assert_eq!(s.len(), 1);
    assert_close!(s[0].v[0], 30.0);
    assert_close!(s[0].v[1], -10.0);
    assert_eq!(s[0].v[2], 0.0);
    assert_eq!(s[0].m[0], 0.0);
    assert_close!(s[0].m[1], 30.0);
    assert_close!(s[0].m[2], -5.0);
    assert_eq!(s[0].m[3], 0.0);
    assert_eq!(s[0].n, [0.0; 4]);
}

/// Every text of a drawing with its angle (level texts: 0).
fn texts(r: &crate::StructureRendered) -> Vec<(String, f64)> {
    r.plot
        .prims
        .iter()
        .filter_map(|p| match p {
            strix_plot::PlotPrim::Text { text, .. } => Some((text.clone(), 0.0)),
            strix_plot::PlotPrim::TextAt { text, angle, .. } => Some((text.clone(), *angle)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_drawing_has_dimensions_stations_and_values_across_the_members() {
    let mut st = simple_beam();
    st.loads.push(Load::Force { node: None, member: Some("1".into()), at: Num::new(2.0), value: Num::new(20.0), angle: Num::new(-60.0) });
    let t = texts(&st.render(None));
    let has = |s: &str| t.iter().any(|(x, _)| x == s);
    // The chain along x: 2 m to the force, 4 m on, 6 m overall.
    assert!(has("2 m") && has("4 m") && has("6 m"), "{t:?}");
    // The inclined force's angle with the beam.
    assert!(has("60°"), "{t:?}");
    // The force's station on the diagrams, written across the axis.
    assert!(t.iter().any(|(x, a)| x == "2 m" && (a.abs() - 90.0).abs() < 1e-9), "{t:?}");
    // M's value at the force's point stands across the beam; N's constant stretch stays level.
    let m_at_force = t.iter().find(|(x, _)| x.parse::<f64>().is_ok_and(|v| v > 50.0)).cloned();
    assert!(m_at_force.is_some_and(|(_, a)| (a.abs() - 90.0).abs() < 1e-9), "{t:?}");
    // Turned off, both go.
    st.diagrams.dimensions = false;
    st.diagrams.stations = false;
    let t = texts(&st.render(None));
    assert!(!t.iter().any(|(x, _)| x.ends_with(" m") || x.ends_with('°')), "{t:?}");
}

#[test]
fn a_zero_close_to_a_load_has_its_station_and_a_nought_diagram_has_none() {
    // T goes from 0.708 to −14.292 over 4.2–5.2 m: nought at 4.247 m, 47 mm past the load's start.
    let mut st = simple_beam();
    let part = |q: f64, from: f64, to: f64| Load::Distributed {
        member: "1".into(),
        q1: q.into(),
        q2: None,
        from: from.into(),
        to: Some(to.into()),
        angle: Num::new(-90.0),
        axes: LoadAxes::Global,
        per: LoadPer::Length,
    };
    st.loads = vec![
        part(10.0, 0.0, 2.5),
        Load::Force { node: None, member: Some("1".into()), at: 3.2.into(), value: 5.0.into(), angle: Num::new(90.0) },
        part(15.0, 4.2, 5.2),
    ];
    let t = texts(&st.render(None));
    let count = |s: &str| t.iter().filter(|(x, a)| x == s && (a.abs() - 90.0).abs() < 1e-9).count();
    // On T (the zero) and on M (its extreme), beside 4.2 m's.
    assert_eq!(count("4.247 m"), 2, "{t:?}");
    assert_eq!(count("4.2 m"), 2, "{t:?}");
    // N is nought all along: no stations on it, T's and M's only.
    assert_eq!(count("2.5 m"), 2, "{t:?}");
    assert_eq!(count("2.071 m"), 2, "{t:?}");
}

#[test]
fn an_inclined_member_has_its_slope_and_a_chain_along_it() {
    let st = Structure {
        nodes: vec![node("A", 0.0, 0.0), node("B", 4.0, 3.0)],
        members: vec![member("1", "A", "B")],
        supports: vec![support("A", SupportKind::Pin, 0.0), support("B", SupportKind::Roller, 0.0)],
        loads: vec![Load::Force { node: None, member: Some("1".into()), at: Num::new(2.0), value: Num::new(10.0), angle: Num::new(-90.0) }],
        ..Structure::default()
    };
    let t = texts(&st.render(None));
    let has = |s: &str| t.iter().any(|(x, _)| x == s);
    // 3-4-5: the slope, the vertical force's angle with the member, distances along it.
    assert!(has("36.87°"), "{t:?}");
    assert!(has("53.13°"), "{t:?}");
    assert!(has("2 m") && has("3 m") && has("4 m"), "{t:?}");
    // Hostile numbers never panic.
    let mut bad = st.clone();
    bad.loads.push(Load::Force {
        node: None,
        member: Some("1".into()),
        at: Num::new(f64::NAN),
        value: Num::new(1e300),
        angle: Num::new(f64::INFINITY),
    });
    bad.nodes[1].x = Num::new(f64::NAN);
    let _ = bad.render(None);
}

#[test]
fn dimensions_and_stations_are_on_in_a_new_structure_and_off_in_one_saved_before_them() {
    // A new structure has both, written out, and reads back the same.
    let st = simple_beam();
    assert!(st.diagrams.dimensions && st.diagrams.stations);
    let json = st.to_json();
    assert!(json.contains("\"dimensions\":true") && json.contains("\"stations\":true"), "{json}");
    let back = Structure::from_json(&json).unwrap();
    assert_eq!(back.diagrams, st.diagrams);
    // One saved before them (its diagrams without the two keys) keeps its drawing and its size.
    let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
    let d = old["diagrams"].as_object_mut().unwrap();
    d.remove("dimensions");
    d.remove("stations");
    let old = Structure::from_json(&old.to_string()).unwrap();
    assert!(!old.diagrams.dimensions && !old.diagrams.stations);
    let mut before = st.clone();
    before.diagrams.dimensions = false;
    before.diagrams.stations = false;
    assert_eq!(old.render(None).plot.size, before.render(None).plot.size);
    // Turned off, they are written as such and read back off.
    let json = before.to_json();
    assert!(!json.contains("dimensions") && !json.contains("stations"), "{json}");
    let back = Structure::from_json(&json).unwrap();
    assert!(!back.diagrams.dimensions && !back.diagrams.stations);
}

#[test]
fn the_editors_schema_has_the_dimensions_and_its_angles_drawn_as_dimensions() {
    let mut st = simple_beam();
    st.loads.push(Load::Force { node: None, member: Some("1".into()), at: Num::new(2.0), value: Num::new(20.0), angle: Num::new(-60.0) });
    let prims = st.schema_prims(10.0, (-1.0, 3.0), strix_calc::Options::default());
    let colour = |s: &str| {
        prims.iter().find_map(|p| match p {
            strix_plot::PlotPrim::TextAt { text, color, .. } if text == s => Some(*color),
            _ => None,
        })
    };
    // The chain along x, as on the page.
    assert!(colour("2 m").is_some() && colour("4 m").is_some() && colour("6 m").is_some(), "{prims:?}");
    // The force's angle in the dimensions' colour.
    assert_eq!(colour("60°"), colour("6 m"));
    // Off, the schema has neither.
    st.diagrams.dimensions = false;
    let prims = st.schema_prims(10.0, (-1.0, 3.0), strix_calc::Options::default());
    assert!(!prims.iter().any(|p| matches!(p, strix_plot::PlotPrim::TextAt { text, .. } if text.ends_with(" m") || text.ends_with('°'))));
}

/// One count runs through all the kinds of load, by place: left to right, then top to bottom,
/// then force, distributed load, couple; the list's order only breaks what's left of a tie.
#[test]
fn loads_are_numbered_together_by_place() {
    let couple = |node: &str| Load::Moment { node: Some(node.into()), member: None, at: Num::default(), value: Num::new(5.0) };
    let mut st = Structure {
        // A portal: column A–C up from the ground, beam C–D, column D–B down to the ground.
        nodes: vec![node("A", 0.0, 0.0), node("C", 0.0, 4.0), node("D", 6.0, 4.0), node("B", 6.0, 0.0)],
        members: vec![member("1", "A", "C"), member("2", "C", "D"), member("3", "D", "B")],
        supports: vec![support("A", SupportKind::Fixed, 0.0), support("B", SupportKind::Fixed, 0.0)],
        loads: vec![
            couple("D"),               // x 6, y 4
            force_on("2", 3.0, 10.0),  // x 3
            force_at("A", 8.0, 0.0),   // x 0, y 0
            uniform("2", 2.0),         // x 0, y 4 (its left end, C)
            force_at("C", 4.0, -90.0), // x 0, y 4, a force: before the distributed load
            force_on("2", 3.0, 0.0),   // does nothing: no number
            force_at("D", 6.0, 0.0),   // x 6, y 4, a force: before the couple
        ],
        ..Structure::default()
    };
    let names = |st: &Structure| -> Vec<String> { crate::load_names(st).iter().map(|n| n.map_or("-".into(), |n| n.linear())).collect() };
    assert_eq!(names(&st), ["C_6", "F_4", "F_3", "q_2", "F_1", "-", "F_5"]);
    // A varying load keeps its number on both ends; the column's load starts at its top end.
    st.loads = vec![
        Load::Distributed {
            member: "3".into(),
            q1: Num::new(1.0),
            q2: Some(Num::new(3.0)),
            from: Num::default(),
            to: None,
            angle: Num::new(0.0),
            axes: LoadAxes::Global,
            per: LoadPer::Length,
        },
        force_at("B", 2.0, 0.0),
    ];
    let n = crate::load_names(&st);
    assert_eq!(n.iter().map(|n| n.map(|n| n.linear_end(2))).collect::<Vec<_>>(), [Some("q_(1,2)".to_string()), Some("F_2".to_string())]);
}

/// The free body of the reactions' working names what its equations use: the reactions as
/// unknowns, a distributed load's resultant, an askew force's horizontal and vertical parts (a
/// letter and its subscript), and puts a station of the dimensions at the resultant's centroid.
#[test]
fn the_free_body_names_the_unknowns_resultants_and_parts() {
    let mut st = simple_beam();
    st.loads.clear();
    st.loads.push(Load::Distributed {
        member: "1".into(),
        q1: 10.0.into(),
        q2: None,
        from: Num::default(),
        to: Some(2.5.into()),
        angle: Num::new(-90.0),
        axes: LoadAxes::Global,
        per: LoadPer::Length,
    });
    st.loads.push(Load::Force { node: None, member: Some("1".into()), at: Num::new(3.2), value: Num::new(10.0), angle: Num::new(30.0) });
    st.diagrams.dimensions = false;
    st.view = crate::View::Equilibrium;
    let r = st.render(None);
    let t: Vec<String> = texts(&r).into_iter().map(|(t, _)| t).collect();
    let pairs: Vec<String> = t.windows(2).map(|w| format!("{}_{}", w[0], w[1])).collect();
    for name in ["H_A", "V_A", "V_B", "F_1", "F_2,h", "F_2,v"] {
        assert!(pairs.iter().any(|p| p == name), "{name}: {t:?}");
    }
    // Names only: no values, no load intensities.
    assert!(!t.iter().any(|x| x.contains("kN")), "{t:?}");
    // The arms' stations, whatever the dimensions' setting: 1.25 m to the resultant.
    assert!(t.iter().filter(|x| x.as_str() == "1.25 m").count() >= 2, "{t:?}");
    let json = st.to_json();
    assert!(json.contains("\"view\":\"equilibrium\""), "{json}");
}

/// The senses counted positive: the free body draws them as a symbol (x, y, a `+` rotation) and
/// its reactions' arrows point so; the reactions' panel crosses out a reaction that came out
/// negative in them; saved only when not the default.
#[test]
fn the_positive_senses_are_drawn_and_followed() {
    use crate::{Positive, SenseTurn, SenseY};
    let mut st = simple_beam();
    st.view = crate::View::Equilibrium;
    let plain = st.render(None);
    let t: Vec<String> = texts(&plain).into_iter().map(|(t, _)| t).collect();
    for sym in ["x", "y", "+"] {
        assert!(t.iter().any(|x| x == sym), "{sym}: {t:?}");
    }
    st.positive = Positive { y: SenseY::Down, rotation: SenseTurn::Cw, ..Positive::default() };
    let down = st.render(None);
    assert_ne!(plain.plot.prims, down.plot.prims, "the arrows and the symbol turn");
    let json = st.to_json();
    assert!(json.contains("\"positive\":{\"x\":\"right\",\"y\":\"down\",\"rotation\":\"cw\"}"), "{json}");
    assert_eq!(Structure::from_json(&json).unwrap().positive, st.positive);
    assert!(!simple_beam().to_json().contains("\"positive\":{"));
    // Upwards reactions are negative downwards: crossed out in red.
    let crossed = |st: &Structure| {
        st.render(None).plot.prims.iter().any(|p| matches!(p, strix_plot::PlotPrim::Path { stroke: Some((c, _)), .. } if *c == [0xC0, 0x1C, 0x1C]))
    };
    st.view = crate::View::Reactions;
    assert!(crossed(&st));
    st.positive = Positive::default();
    assert!(!crossed(&st));
    assert!(Structure::from_json(r#"{"positive": {"y": "sideways"}}"#).is_err());
}

/// The beam of the owner's report: 10 kN/m over its first 2 m, 10 kN upwards at 4.5 m.
fn upward_force_beam(roller: SupportKind) -> Structure {
    let mut st = simple_beam();
    st.supports = vec![support("A", SupportKind::Pin, 0.0), support("B", roller, 0.0)];
    st.loads = vec![
        Load::Distributed {
            member: "1".into(),
            q1: 10.0.into(),
            q2: None,
            from: Num::default(),
            to: Some(2.0.into()),
            angle: Num::new(-90.0),
            axes: LoadAxes::Global,
            per: LoadPer::Length,
        },
        Load::Force { node: None, member: Some("1".into()), at: 4.5.into(), value: 10.0.into(), angle: Num::new(90.0) },
    ];
    st
}

#[test]
fn the_dimensions_keep_clear_of_the_supports_and_the_loads_below_the_beam() {
    use strix_plot::PlotPrim;
    const DIM: [u8; 3] = [0x55, 0x55, 0x55];
    const GREY: [u8; 3] = [0x9A, 0x9A, 0x9A];
    for roller in [SupportKind::Roller, SupportKind::RollerCircle] {
        let st = upward_force_beam(roller);
        let prims = st.schema_prims(10.0, (0.0, 0.0), Options::default());
        // The bottom of everything but the dimensions (their lines are DIM, extension lines GREY).
        let mut bottom = f64::MIN;
        let mut dims_top = f64::MAX;
        for p in &prims {
            match p {
                PlotPrim::Path { pts, stroke, .. } => {
                    let low = pts.iter().map(|q| q.1).fold(f64::MIN, f64::max);
                    match stroke.map(|s| s.0) {
                        Some(DIM) => dims_top = dims_top.min(pts.iter().map(|q| q.1).fold(f64::MAX, f64::min)),
                        Some(GREY) => {}
                        _ => bottom = bottom.max(low),
                    }
                }
                PlotPrim::Dot { at, r, .. } => bottom = bottom.max(at.1 + r),
                PlotPrim::Text { at, text, .. } => {
                    if text.contains("kN") {
                        bottom = bottom.max(at.1);
                    }
                }
                PlotPrim::TextAt { at, text, height, color, .. } if *color == DIM && text.ends_with(" m") => {
                    dims_top = dims_top.min(at.1 - height);
                }
                PlotPrim::TextAt { at, text, height, .. } if text.contains("kN") => bottom = bottom.max(at.1 + height / 2.0),
                _ => {}
            }
        }
        // The upward force's arrow and its name hang below the beam: the chain goes under them.
        assert!(bottom > 0.0 && dims_top > bottom, "{roller:?}: dimensions from {dims_top}, drawing down to {bottom}");
    }
}

#[test]
fn a_roller_drawn_as_a_circle_restrains_like_a_roller() {
    let a = analyse(&upward_force_beam(SupportKind::Roller));
    let b = analyse(&upward_force_beam(SupportKind::RollerCircle));
    assert!(a.status.is_solved() && b.status.is_solved());
    let forces = |an: &crate::Analysis| an.reactions.iter().map(|r| (r.fx, r.fy, r.m)).collect::<Vec<_>>();
    assert_eq!(forces(&a), forces(&b));
    assert_eq!(SupportKind::parse("rollerCircle"), Some(SupportKind::RollerCircle));
    assert_eq!(SupportKind::RollerCircle.name(), "rollerCircle");
    let json = upward_force_beam(SupportKind::RollerCircle).to_json();
    assert!(json.contains("\"rollerCircle\""), "{json}");
}
