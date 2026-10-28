use crate::*;

fn run(statements: &[&str]) -> Vec<RegionResult> {
    evaluate(statements, &Options::default())
}

/// The display text of the last statement.
fn last(statements: &[&str]) -> String {
    run(statements).pop().and_then(|r| r.display).unwrap_or_default()
}

fn last_error(statements: &[&str]) -> CalcError {
    run(statements).pop().and_then(|r| r.error).expect("an error")
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

const BEAM: [&str; 3] = ["L := 4.5 m", "q := 12 kN/m", "M := q*L^2/8"];

fn beam(last_statement: &str) -> String {
    let mut s = BEAM.to_vec();
    s.push(last_statement);
    last(&s)
}

#[test]
fn beam_example_from_the_design() {
    let results = run(&["L := 4.5 m", "q := 12 kN/m", "M := q*L^2/8", "M = kN*m"]);
    assert_eq!(results.len(), 4);
    for r in &results[..3] {
        assert_eq!(r.kind, RegionKind::Definition);
        assert_eq!(r.display, None);
        assert_eq!(r.error, None);
    }
    assert_eq!(results[0].name.as_deref(), Some("L"));
    assert!(approx(results[2].value.clone().unwrap().scalar().unwrap().value, 30375.0));
    assert_eq!(results[3].kind, RegionKind::Evaluation);
    assert_eq!(results[3].display.as_deref(), Some("M = 30.375 kN·m"));
}

#[test]
fn requested_unit_variants() {
    assert_eq!(beam("M = 30.4 kN*m"), "M = 30.375 kN·m");
    assert_eq!(beam("M = kN·m"), "M = 30.375 kN·m");
    assert_eq!(beam("M = kN m"), "M = 30.375 kN·m");
    assert_eq!(beam("M = 99 kN*m"), "M = 30.375 kN·m");
    assert_eq!(beam("q = kN/m"), "q = 12 kN/m");
    assert_eq!(beam("L = mm"), "L = 4500 mm");
    assert_eq!(beam("L^2 = m^2"), "L^2 = 20.25 m²");
}

#[test]
fn automatic_units_prefer_coherent_si() {
    assert_eq!(last(&["0.004 N ="]), "0.004 N = 4 mN");
    assert_eq!(last(&["0 Pa ="]), "0 Pa = 0 Pa");
    assert_eq!(last(&["1e15 W ="]), "1e15 W = 1000000 GW");
    // Torque has the dimension of energy: J unless N·m is asked for. Named units get an
    // engineering prefix (kN, MPa, kJ); lengths and base units stay unprefixed.
    // A force times a length reads as written: a moment in kN·m, not kJ.
    assert_eq!(beam("M ="), "M = 30.375 kN·m");
    assert_eq!(beam("q ="), "q = 12 kN/m");
    assert_eq!(beam("L*q ="), "L*q = 54 kN");
    assert_eq!(last(&["1/2 m ="]), "1/2 m = 0.5 m^-1");
    assert_eq!(last(&["5 kg/m ="]), "5 kg/m = 5 kg·m^-1");
    assert_eq!(last(&["3 m/s / 2 s ="]), "3 m/s / 2 s = 1.5 m/s²");
    assert_eq!(last(&["2 MPa ="]), "2 MPa = 2 MPa");
    assert_eq!(last(&["3 =",]), "3 = 3");
}

#[test]
fn unit_conversions() {
    assert_eq!(last(&["1 km + 3 m ="]), "1 km + 3 m = 1003 m");
    assert_eq!(last(&["1 h = s"]), "1 h = 3600 s");
    assert_eq!(last(&["100 % ="]), "100 % = 100 %");
    assert_eq!(last(&["100 % = 1"]), "100 % = 100 %");
    assert_eq!(last(&["2 kN/(4 m^2) ="]), "2 kN/(4 m^2) = 0.5 kN/m²");
    assert_eq!(last(&["2 kN/(4 m^2) = Pa"]), "2 kN/(4 m^2) = 500 Pa");
    assert_eq!(last(&["1 kWh = J"]), "1 kWh = 3600000 J");
    assert_eq!(last(&["1 bar = kPa"]), "1 bar = 100 kPa");
    assert_eq!(last(&["1 t = kg"]), "1 t = 1000 kg");
    assert_eq!(last(&["90 min = h"]), "90 min = 1.5 h");
    assert_eq!(last(&["2 L = cm³"]), "2 L = 2000 cm³");
    assert_eq!(last(&["180° ="]), "180° = 180°");
    assert_eq!(last(&["180° = rad"]), "180° = 3.142 rad");
    assert_eq!(last(&["π rad = °"]), "π rad = 180°");
    assert_eq!(last(&["0.25 = %"]), "0.25 = 25 %");
    assert_eq!(last(&["1 µm = nm"]), "1 µm = 1000 nm");
    assert_eq!(last(&["1 GPa = MPa"]), "1 GPa = 1000 MPa");
    assert_eq!(last(&["5 V * 2 A = W"]), "5 V * 2 A = 10 W");
    assert_eq!(last(&["10 V / 2 A = Ω"]), "10 V / 2 A = 5 Ω");
    assert_eq!(last(&["10 kohm = Ω"]), "10 kohm = 10000 Ω");
    assert_eq!(last(&["1 kHz = Hz"]), "1 kHz = 1000 Hz");
    assert_eq!(last(&["1 mg = kg"]), "1 mg = 1·10^-6 kg");
    assert_eq!(last(&["2 m * 3 m * 4 m ="]), "2 m * 3 m * 4 m = 24 m³");
}

#[test]
fn unit_table() {
    assert_eq!(unit("kN"), Some(Quantity::new(1000.0, unit("N").unwrap().dim)));
    assert!(unit("kg").is_some());
    assert!(unit("min").is_some_and(|q| q.value == 60.0));
    assert!(unit("mol").is_some_and(|q| q.value == 1.0));
    assert!(unit("cd").is_some_and(|q| q.value == 1.0));
    // Prefixes only on units that take them.
    assert_eq!(unit("kkg"), None);
    assert_eq!(unit("mkg"), None);
    assert_eq!(unit("kmin"), None);
    assert_eq!(unit("kh"), None);
    assert_eq!(unit("k°"), None);
    assert_eq!(unit("xyz"), None);
    assert!(unit("μs").is_some()); // Greek mu
    assert!(unit("us").is_some());
}

#[test]
fn dimensions_and_powers() {
    assert_eq!(last(&["sqrt(9 m^2) ="]), "sqrt(9 m^2) = 3 m");
    assert_eq!(last(&["√16 ="]), "√16 = 4");
    assert_eq!(last(&["√(16 m²) ="]), "√(16 m²) = 4 m");
    assert!(matches!(last_error(&["sqrt(4 m) ="]), CalcError::FractionalDimension { .. }));
    assert_eq!(last_error(&["sqrt(4 m) ="]).to_string(), "cannot take this power of m: the unit would get a fractional exponent");
    assert!(matches!(last_error(&["2^(3 m) ="]), CalcError::ExponentNotDimensionless { .. }));
    assert_eq!(last_error(&["m^100*m^100 ="]), CalcError::DimensionOverflow);
    assert_eq!(last_error(&["m^200 ="]), CalcError::DimensionOverflow);
    assert_eq!(last_error(&["sqrt(-1) ="]), CalcError::NotReal);
    assert_eq!(Dim::NONE.try_pow(0.5), Ok(Dim::NONE));
    let m = Dim::new([1, 0, 0, 0, 0, 0, 0]);
    assert_eq!(m.try_mul(m).unwrap().exponents()[0], 2);
    assert_eq!(Dim::new([127, 0, 0, 0, 0, 0, 0]).try_mul(m), Err(CalcError::DimensionOverflow));
    assert_eq!(Dim::new([-128, 0, 0, 0, 0, 0, 0]).try_div(m), Err(CalcError::DimensionOverflow));
}

#[test]
fn errors() {
    let r = run(&["2 m + 3 s ="]).pop().unwrap();
    assert_eq!(r.kind, RegionKind::Evaluation);
    assert_eq!(r.display.as_deref(), Some("2 m + 3 s = ⚠"));
    assert_eq!(r.error.unwrap().to_string(), "cannot add m and s");
    assert_eq!(last_error(&["2 m - 3 ="]).to_string(), "cannot subtract m and a number");

    assert_eq!(last_error(&["x_unknown + 1 ="]), CalcError::UnknownName("x_unknown".into()));
    assert_eq!(last_error(&["x_unknown + 1 ="]).to_string(), "unknown name x_unknown");

    // Use before definition.
    let results = run(&["b =", "b := 2"]);
    assert_eq!(results[0].error, Some(CalcError::UnknownName("b".into())));
    assert_eq!(results[0].display.as_deref(), Some("b = ⚠"));
    assert_eq!(results[1].error, None);

    // Wrong requested unit: the unit is kept next to the warning.
    let r = run(&["L := 4.5 m", "L = N"]).pop().unwrap();
    assert_eq!(r.error.as_ref().unwrap().to_string(), "result is in m, not N");
    assert_eq!(r.display.as_deref(), Some("L = ⚠ N"));

    assert_eq!(last_error(&["1/0 ="]), CalcError::DivisionByZero);
    assert_eq!(last_error(&["0^-1 ="]), CalcError::DivisionByZero);
    assert_eq!(last_error(&["1 m/(2 m - 2 m) ="]), CalcError::DivisionByZero);
    assert_eq!(last_error(&["1e308*1e308 ="]), CalcError::Overflow);
    assert_eq!(last_error(&["1e999 ="]), CalcError::Overflow);
    assert_eq!(last_error(&["10^10^10 ="]), CalcError::Overflow);
    assert_eq!(last_error(&["ln(0) ="]), CalcError::Domain { function: "ln".into() });
    assert_eq!(last_error(&["asin(2) ="]), CalcError::Domain { function: "asin".into() });
    assert!(matches!(last_error(&["sin(2 m) ="]), CalcError::NeedsDimensionless { .. }));
    assert!(matches!(last_error(&["sin(1, 2) ="]), CalcError::Arity { .. }));
    assert!(matches!(last_error(&["max() ="]), CalcError::Arity { .. }));
    assert_eq!(last_error(&["max(1 m, 2 s) ="]).to_string(), "cannot compare m and s");
    assert_eq!(last_error(&["sin x ="]), CalcError::FunctionNeedsArguments("sin".into()));
    assert_eq!(last_error(&["2 3 ="]), CalcError::Syntax { pos: 3, message: "unexpected number".into() });
    assert!(matches!(last_error(&["(2 ="]), CalcError::Syntax { .. }));
    assert!(matches!(last_error(&["2 + ="]), CalcError::Syntax { .. }));
    assert_eq!(last_error(&["x := 2 = = "]), CalcError::Syntax { pos: 8, message: "a statement takes one := and at most one = after it".into() });
    assert!(matches!(last_error(&["x := := 2"]), CalcError::Syntax { .. }));
    assert!(matches!(last_error(&["2 $ 3 ="]), CalcError::Syntax { pos: 3, .. }));
    assert_eq!(last_error(&["1 = kN"]).to_string(), "result is in a number, not kN");
    assert_eq!(last_error(&["sin := 3"]), CalcError::BadDefinition("sin is a function and cannot be redefined".into()));
    assert!(matches!(last_error(&["2 x := 3"]), CalcError::Syntax { .. }));
    // An error never stops later regions.
    let results = run(&["a := 2 m + 1 s", "c := 5", "c ="]);
    assert!(results[0].error.is_some());
    assert_eq!(results[2].display.as_deref(), Some("c = 5"));
}

#[test]
fn define_and_show() {
    let mut s = BEAM.to_vec();
    s.extend(["W := 243 cm^3", "σ := M/W =", "σ = MPa", "τ := M/W = MPa"]);
    let results = run(&s);
    let show = &results[4];
    assert_eq!(show.kind, RegionKind::DefineAndShow);
    assert_eq!(show.name.as_deref(), Some("σ"));
    assert_eq!(show.display.as_deref(), Some("σ := M/W = 125 MPa"));
    assert_eq!(results[5].display.as_deref(), Some("σ = 125 MPa"));
    assert_eq!(results[6].display.as_deref(), Some("τ := M/W = 125 MPa"));
    // A wrong display unit still defines the name.
    let results = run(&["a := 3 m = N", "a = m"]);
    assert!(results[0].error.is_some());
    assert_eq!(results[0].display.as_deref(), Some("a := 3 m = ⚠ N"));
    assert_eq!(results[1].display.as_deref(), Some("a = 3 m"));
    // ≔ is the same as :=.
    assert_eq!(last(&["x ≔ 2 m =", "x ="]), "x = 2 m");
}

#[test]
fn redefinition_semantics() {
    let results = run(&["a := 2", "b := a*3", "a := 5", "a =", "b ="]);
    assert_eq!(results[3].display.as_deref(), Some("a = 5"));
    assert_eq!(results[4].display.as_deref(), Some("b = 6"));
    // A failed redefinition undefines the name, and so do names depending on it.
    let results = run(&["a := 2", "a := 2 m + 1 s", "a =", "b := a + 1", "b ="]);
    assert_eq!(results[2].error, Some(CalcError::UnknownName("a".into())));
    assert!(results[3].error.is_some());
    assert_eq!(results[4].error, Some(CalcError::UnknownName("b".into())));
    // A parameter wins over a unit and a constant, except right after a number, where a unit
    // name is the unit (`2 m` is two metres; `2·m` multiplies by the parameter).
    assert_eq!(last(&["m := 3", "2·m ="]), "2·m = 6");
    assert_eq!(last(&["m := 3", "2 m ="]), "2 m = 2 m");
    assert_eq!(last(&["m := 3", "m*2 ="]), "m*2 = 6");
    assert_eq!(last(&["g := 2", "g ="]), "g = 2");
    assert_eq!(last(&["m := 3", "1 km = m"]), "1 km = 1000 m"); // display units are units only
    let mut sheet = Worksheet::new(Options::default());
    sheet.eval_statement("x := 1");
    sheet.eval_statement("y := 2");
    sheet.eval_statement("x := 3");
    let names: Vec<_> = sheet.scope().iter().map(|(n, v)| (n.to_string(), v.scalar().unwrap().value)).collect();
    assert_eq!(names, vec![("x".to_string(), 3.0), ("y".to_string(), 2.0)]);
}

#[test]
fn decimal_comma() {
    let opts = Options { decimal_comma: true, ..Options::default() };
    let results = evaluate(&["x := 1,5 m", "y := max(x; 2 m)", "y =", "x*3 =", "2,5e3 ="], &opts);
    assert!(results.iter().all(|r| r.error.is_none()), "{results:?}");
    assert_eq!(results[2].display.as_deref(), Some("y = 2 m"));
    assert_eq!(results[3].display.as_deref(), Some("x*3 = 4,5 m"));
    assert_eq!(results[4].display.as_deref(), Some("2,5e3 = 2500"));
    let results = evaluate(&["z := 1234567890,5", "z ="], &opts);
    assert_eq!(results[1].display.as_deref(), Some("z = 1,235·10^9"));
    // Re-evaluating the comma output is stable.
    let again = evaluate(&["z := 1234567890,5", "z = 1,235·10^9"], &opts);
    assert_eq!(again[1].display, results[1].display);
    // Without decimal comma, `,` separates arguments.
    assert_eq!(last(&["max(1, 5) ="]), "max(1, 5) = 5");
}

#[test]
fn greek_names_and_subscripts() {
    let results = run(&["σ_max := 250 MPa", "f_ck := 30 MPa", "σ_max/f_ck =", "α := 30°", "sin(α) =", "2π =", "γ_1 := 2", "γ_1^2 ="]);
    assert!(results.iter().all(|r| r.error.is_none()), "{results:?}");
    assert_eq!(results[2].display.as_deref(), Some("σ_max/f_ck = 8.333"));
    assert_eq!(results[4].display.as_deref(), Some("sin(α) = 0.5"));
    assert_eq!(results[5].display.as_deref(), Some("2π = 6.283"));
    assert_eq!(results[7].display.as_deref(), Some("γ_1^2 = 4"));
    assert_eq!(last(&["g = m/s²"]), "g = 9.807 m/s²");
    assert_eq!(last(&["pi*e ="]), "pi*e = 8.54");
}

#[test]
fn implicit_multiplication_precedence() {
    let q = |s: &str| run(&[s]).pop().unwrap().value.unwrap().scalar().unwrap();
    // Implicit multiplication binds tighter than `/`.
    assert!(approx(q("12 kN/m =").value, 12000.0));
    assert!(approx(q("2 kN/4 m^2 =").value, 500.0));
    assert!(approx(q("1/2 m =").value, 0.5));
    assert_eq!(q("1/2 m =").dim, Dim::new([-1, 0, 0, 0, 0, 0, 0]));
    assert!(approx(q("1/2*m =").value, 0.5));
    assert_eq!(q("1/2*m =").dim, Dim::new([1, 0, 0, 0, 0, 0, 0]));
    // Explicit operators are left to right.
    assert!(approx(q("6/2*3 =").value, 9.0));
    assert!(approx(q("6/2·3 =").value, 9.0));
    // Powers bind tightest and are right-associative; unary minus is looser than ^.
    assert!(approx(q("2^3^2 =").value, 512.0));
    assert!(approx(q("-2^2 =").value, -4.0));
    assert!(approx(q("2 m^2 =").value, 2.0));
    assert!(approx(q("10^-3 =").value, 1e-3));
    assert!(approx(q("2(3 + 4) =").value, 14.0));
    assert!(approx(q("3² =").value, 9.0));
    assert!(approx(q("2e-3 =").value, 0.002));
    assert!(approx(q("2 e =").value, 2.0 * std::f64::consts::E));
}

#[test]
fn not_calc_statements_are_left_alone() {
    for text in ["E = mc^2", "a + b", "∫ f dx = F", "", "x = y", "= 5", "a = b = c", "y = 2 x"] {
        let r = run(&[text]).pop().unwrap();
        assert_eq!(r.kind, RegionKind::NotCalc, "{text}");
        assert_eq!(r.display, None, "{text}");
        // An equation with several unknowns is left as written, and the report says why.
        assert!(matches!(r.error, None | Some(CalcError::Underdetermined(_) | CalcError::Syntax { .. })), "{text}: {:?}", r.error);
    }
    assert_eq!(run(&["E = mc^2"]).pop().unwrap().error, Some(CalcError::Underdetermined(vec!["E".into(), "mc".into()])));
}

#[test]
fn number_formatting() {
    let o = Options::default();
    let f = |v: f64| format_number(v, &o);
    // Auto: 3 decimals from 1 up, 3 significant digits below 1.
    assert_eq!(f(30.375), "30.375");
    assert_eq!(f(30.3754), "30.375");
    assert_eq!(f(1003.0), "1003");
    assert_eq!(f(123456.0), "123456");
    assert_eq!(f(0.5), "0.5");
    assert_eq!(f(0.0035), "0.0035");
    assert_eq!(f(0.123456), "0.123");
    // Half away from zero on the shortest form: 2.675 is 2.68, not 2.67.
    assert_eq!(format_number(2.675, &Options { digits: 2, ..o }), "2.68");
    assert_eq!(format_number(-2.675, &Options { digits: 2, ..o }), "-2.68");
    assert_eq!(f(0.0000123456), "1.235·10^-5");
    assert_eq!(f(0.000123456), "1.235·10^-4");
    assert_eq!(f(1.23e9), "1.23·10^9");
    assert_eq!(f(12345678.0), "1.235·10^7");
    assert_eq!(f(9999999.9), "9999999.9");
    assert_eq!(f(9999999.9996), "1·10^7");
    assert_eq!(f(-2.5), "-2.5");
    assert_eq!(f(-0.0), "0");
    assert_eq!(f(-0.0001), "-1·10^-4");
    assert_eq!(f(9.99996), "10");
    assert_eq!(f(f64::NAN), "⚠");
    // Trailing zeros, decimals, significant digits.
    assert_eq!(format_number(2.5, &Options { zeros: true, ..o }), "2.500");
    assert_eq!(format_number(1234.5, &Options { digits: 2, count: Count::Significant, ..o }), "1200");
    assert_eq!(format_number(9.99, &Options { digits: 1, count: Count::Decimals, zeros: true, ..o }), "10.0");
    assert_eq!(format_number(0.0001, &Options { digits: 2, count: Count::Decimals, ..o }), "0");
    assert_eq!(format_number(2.0 / 3.0, &Options { digits: 0, ..o }), "0.7");
    assert_eq!(format_number(2.0 / 3.0, &Options { digits: 0, count: Count::Decimals, ..o }), "1");
    assert_eq!(format_number(2.0 / 3.0, &Options { digits: 200, ..o }), "0.666666666666667");
    assert_eq!(format_number(1.5, &Options { decimal_comma: true, ..o }), "1,5");
}

#[test]
fn idempotent_round_trip() {
    let cases: &[&[&str]] = &[
        &["L := 4.5 m", "q := 12 kN/m", "M := q*L^2/8", "M = kN*m"],
        &["L := 4.5 m", "q := 12 kN/m", "M := q*L^2/8", "M ="],
        &["x := 3e12 m", "x ="],
        &["x := 3e-7 m", "x = mm"],
        &["x := -2.5 kN", "x ="],
        &["x := 1 m", "x = ⚠"],
        &["x := 1 m", "x = ⚠ mm"],
        &["x := 2 m", "1/x ="],
        &["x := 2 m", "x^4 ="],
        &["x := 5 kg/m", "x ="],
        &["x := 2 Hz", "x = 1/s"],
        &["x := 90°", "x = °"],
        &["x := 0.25", "x = %"],
        &["x := 3 m", "x = (m^2)^0.5"],
        &["x := 3 m^2", "x = m^2"],
        &["x := 1 N/m^2", "x = kN/(m·m)"],
        &["x := 2 m =", "x ="],
        &["2 m + 3 s ="],
        &["L := 4.5 m", "L = N"],
    ];
    for case in cases {
        let first = run(case);
        let shown = first.last().and_then(|r| r.display.clone()).expect("display");
        let mut again = case.to_vec();
        if let Some(last) = again.last_mut() {
            *last = &shown;
        }
        let second = run(&again);
        assert_eq!(second.last().and_then(|r| r.display.clone()), Some(shown.clone()), "case {case:?}");
        assert_eq!(second.last().map(|r| r.kind), first.last().map(|r| r.kind), "case {case:?}");
    }
}

#[test]
fn limits() {
    let long = format!("{} =", "1+".repeat(3000) + "1");
    assert_eq!(last_error(&[&long]), CalcError::TooLong { limit: MAX_INPUT_CHARS });
    let deep = format!("{}1{} =", "(".repeat(200), ")".repeat(200));
    assert_eq!(last_error(&[&deep]), CalcError::TooComplex);
    let ok = format!("{}1{}", "(".repeat(30), ")".repeat(30));
    assert_eq!(last(&[&format!("{ok} =")]), format!("{ok} = 1"));
    assert_eq!(last(&["5 min (2) ="]), "5 min (2) = 10"); // `min(` is the function
    assert_eq!(last(&["min(2 h, 90 min) = min"]), "min(2 h, 90 min) = 90 min");
    let many = format!("{} =", "1+".repeat(1000) + "1");
    assert_eq!(last_error(&[&many]), CalcError::TooComplex);
    let minus = format!("{}1 =", "-".repeat(1000));
    assert_eq!(last_error(&[&minus]), CalcError::TooComplex);
    let roots = format!("{}16 =", "√".repeat(1000));
    assert_eq!(last_error(&[&roots]), CalcError::TooComplex);
    let powers = format!("{}2 =", "2^".repeat(1000));
    assert_eq!(last_error(&[&powers]), CalcError::TooComplex);
    // Long non-calc text stays untouched without an error.
    let prose = "x".repeat(10_000);
    assert_eq!(run(&[&prose])[0], RegionResult { kind: RegionKind::NotCalc, name: None, value: None, display: None, error: None, steps: None });
    // A too-long definition undefines its name.
    let long_def = format!("a := {}", "1+".repeat(3000) + "1");
    let results = run(&["a := 1", &long_def, "a ="]);
    assert_eq!(results[1].name.as_deref(), Some("a"));
    assert_eq!(results[2].error, Some(CalcError::UnknownName("a".into())));
}

/// xorshift64*: deterministic, dependency-free.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn hostile_input_never_panics() {
    const PIECES: &[&str] = &[
        "1",
        "2.5",
        "0",
        "1e308",
        "1e-308",
        "10",
        "9e9",
        ",",
        ";",
        ".",
        "e",
        "E",
        "π",
        "pi",
        "g",
        "m",
        "kN",
        "MPa",
        "µm",
        "°",
        "%",
        "/",
        "*",
        "·",
        "×",
        "^",
        "²",
        "³",
        "¹",
        "(",
        ")",
        " ",
        "=",
        ":=",
        "≔",
        "⚠",
        "-",
        "−",
        "+",
        "sqrt(",
        "√",
        "ln(",
        "asin(",
        "max(",
        "round(",
        "x",
        "y",
        "σ_max",
        "_",
        "€",
        "\u{0}",
        "\u{FE0F}",
        "é",
        "∫",
        "10^10^10",
        "m^127",
        "m^-128",
        ":",
        "==",
        "kWh",
        "Ω",
        "deg",
        "1,5",
        "x :=",
        "y :=",
        "= kN*m",
        "= 1.2·10^9 m",
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..4000 {
        let opts = Options {
            digits: (round % 17) as u8,
            decimal_comma: round % 3 == 0,
            zeros: round % 2 == 0,
            count: [Count::Auto, Count::Decimals, Count::Significant][round % 3],
            steps: round % 2 == 1,
        };
        let mut statements = Vec::new();
        for _ in 0..4 {
            let n = rng.below(24);
            let s: String = (0..n).map(|_| PIECES[rng.below(PIECES.len())]).collect();
            statements.push(s);
        }
        let refs: Vec<&str> = statements.iter().map(String::as_str).collect();
        for r in evaluate(&refs, &opts) {
            if let Some(e) = &r.error {
                assert!(!e.to_string().is_empty());
            }
        }
    }
    // Random bytes decoded lossily.
    for _ in 0..2000 {
        let n = rng.below(64);
        let bytes: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        let s = String::from_utf8_lossy(&bytes).into_owned();
        let _ = run(&[&s, &format!("{s} ="), &format!("x := {s}")]);
    }
    // Pathological shapes.
    let hostile = [
        "(".repeat(5000),
        format!("{}1{} =", "(".repeat(2000), ")".repeat(2000)),
        format!("{}1 =", "-".repeat(4000)),
        format!("{}2 =", "2^".repeat(2000)),
        format!("{}x =", "√".repeat(4000)),
        format!("{} =", "m²".repeat(2000)),
        format!("{} =", "m^127*".repeat(500) + "1"),
        format!("{} =", "9".repeat(4000)),
        format!("x := {}", "x+".repeat(2000) + "1"),
        "10^10^10^10 =".to_string(),
        "(1e308)^(1e308) =".to_string(),
        "1e-308/1e308 =".to_string(),
        "(-8)^(1/3) =".to_string(),
        "m^(1/3) =".to_string(),
        "m^1e300 =".to_string(),
        "round(1; 1e300) =".to_string(),
        "x".repeat(1_000_000) + " =",
        "= = = := := :=".to_string(),
        ":=".to_string(),
        "=".to_string(),
        "x := =".to_string(),
        "x :=".to_string(),
        "⚠ =".to_string(),
        "1 = ⚠ ⚠".to_string(),
        "1 = 2·10^".to_string(),
        "1 = -".to_string(),
        "1 = 1/".to_string(),
    ];
    for s in &hostile {
        let _ = run(&[s.as_str()]);
        let _ = evaluate(&[s.as_str()], &Options { digits: 255, decimal_comma: true, ..Options::default() });
    }
}

#[test]
fn every_statement_can_use_a_plain_equals_sign() {
    let r = run(&["L = 4.5 m", "q = 12 kN/m", "M = q*L^2/8", "M = kN*m", "W = 243 cm^3", "σ = M/W = MPa"]);
    for d in &r[..3] {
        assert_eq!(d.kind, RegionKind::Definition, "{d:?}");
        assert_eq!(d.display, None);
        assert_eq!(d.error, None);
    }
    assert_eq!(r[2].name.as_deref(), Some("M"));
    assert_eq!(r[3].display.as_deref(), Some("M = 30.375 kN·m"));
    assert_eq!(r[5].kind, RegionKind::DefineAndShow);
    assert_eq!(r[5].display.as_deref(), Some("σ = M/W = 125 MPa"));
    // `:=` still defines (and redefines).
    assert_eq!(last(&["x := 2 m", "x := 3 m", "x ="]), "x = 3 m");
    // A known name followed by a number is shown, not redefined: the number is refreshed.
    assert_eq!(last(&["L = 4.5 m", "L = 9 m"]), "L = 4.5 m");
}

#[test]
fn the_one_unknown_is_isolated_and_assigned() {
    let r = run(&["F = 10 N", "m = 4 kg", "F = m*a", "a = m/s^2"]);
    assert_eq!(r[2].kind, RegionKind::Solved);
    assert_eq!(r[2].name.as_deref(), Some("a"));
    assert_eq!(r[2].display.as_deref(), Some("F = m*a ⇒ a = 2.5 m/s²"));
    assert_eq!(r[3].display.as_deref(), Some("a = 2.5 m/s²"));
    // The unit after `⇒` is kept on the next calculation, the number refreshed.
    assert_eq!(last(&["F = 10 N", "m = 4 kg", "F = m*a ⇒ a = 7 mm/s^2"]), "F = m*a ⇒ a = 2500 mm/s²");
    // The unknown may sit on either side, under powers, roots and functions.
    assert_eq!(last(&["V = 8 m^3", "V = d^3"]), "V = d^3 ⇒ d = 2 m");
    assert_eq!(last(&["A = 50 cm^2", "A = π*r^2/4 ⇒ r = cm"]), "A = π*r^2/4 ⇒ r = 7.979 cm");
    assert_eq!(last(&["y = 2", "exp(k) = y"]), "exp(k) = y ⇒ k = 0.693");
    assert_eq!(last(&["12 kN = q*3 m"]), "12 kN = q*3 m ⇒ q = 4 kN/m");
    assert_eq!(last(&["h = 2 m", "v = sqrt(2*g*h) = m/s"]), "v = sqrt(2*g*h) = 6.263 m/s");
    // Several occurrences: solved numerically, with the unit found from the equation.
    assert_eq!(last(&["x^2 + x = 6"]), "x^2 + x = 6 ⇒ x = 2");
    assert_eq!(last(&["A = 6 m^2", "x^2 + x*1 m = A"]), "x^2 + x*1 m = A ⇒ x = 2 m");
    assert!(matches!(last_error(&["x^2 + 1 = 0"]), CalcError::NoSolution(_)));
    // A name that is also a unit is unknown when it stands alone: `m = 3 kg` defines m.
    assert_eq!(last(&["m = 3 kg", "m ="]), "m = 3 kg");
}

#[test]
fn equations_without_unknowns_are_checked() {
    let r = run(&["a = 2", "b = 3", "c = 5", "a + b = c", "a*b = c", "a + b = 6"]);
    assert_eq!(r[3].kind, RegionKind::Check);
    assert_eq!(r[3].error, None);
    assert_eq!(r[4].error.as_ref().map(ToString::to_string).as_deref(), Some("the two sides differ: 6 ≠ 5"));
    // A plain number after `=` with every name known is a result to refresh.
    assert_eq!(r[5].kind, RegionKind::Evaluation);
    assert_eq!(r[5].display.as_deref(), Some("a + b = 5"));
}

#[test]
fn sums_and_products_over_ranges() {
    assert_eq!(last(&["n = 4", "∑_(i=1)^n▒i^2 ="]), "∑_(i=1)^n▒i^2 = 30");
    assert_eq!(last(&["∏_(k=1)^5▒k ="]), "∏_(k=1)^5▒k = 120");
    assert_eq!(last(&["Σ_(i=1)^3▒(2 i m) ="]), "Σ_(i=1)^3▒(2 i m) = 12 m");
    assert_eq!(last(&["i = 1..5", "∑_i▒i ="]), "∑_i▒i = 15");
    assert_eq!(last(&["i = 1..5", "i ="]), "i = [1; 2; 3; 4; 5]");
    assert_eq!(last(&["∑_(i=3)^2▒i ="]), "∑_(i=3)^2▒i = 0");
    assert_eq!(last(&["F = [1; 2; 3] kN", "x = [1; 2; 3] m", "∑_(i=1)^3▒F[i]*x[i] = kN*m"]), "∑_(i=1)^3▒F[i]*x[i] = 14 kN·m");
    assert_eq!(last(&["F = [1; 2; 3] kN", "∑▒F ="]), "∑▒F = 6 kN");
    assert!(matches!(last_error(&["∑_(i=1)^1e9▒i ="]), CalcError::TooManyValues { .. }));
    assert!(matches!(last_error(&["∑_(i=1)^9999▒∑_(j=1)^9999▒1 ="]), CalcError::TooMuchWork));
    assert!(matches!(last_error(&["∑_(i=1)^2.5▒i ="]), CalcError::NotWhole { .. }));
    assert!(matches!(last_error(&["∑_(i=1)^(2 m)▒i ="]), CalcError::NeedsDimensionless { .. }));
}

#[test]
fn vectors_compute_element_by_element() {
    let f = |s: &str| last(&["F = [1; 2; 3] kN", s]);
    assert_eq!(f("F ="), "F = [1; 2; 3] kN");
    assert_eq!(f("F = N"), "F = [1000; 2000; 3000] N");
    assert_eq!(f("F = [9; 9] kN"), "F = [1; 2; 3] kN");
    assert_eq!(f("2 F ="), "2 F = [2; 4; 6] kN");
    assert_eq!(f("F[2] ="), "F[2] = 2 kN");
    assert_eq!(f("sum(F) = kN"), "sum(F) = 6 kN");
    assert_eq!(f("mean(F) ="), "mean(F) = 2 kN");
    assert_eq!(f("len(F) ="), "len(F) = 3");
    assert_eq!(f("max(F) ="), "max(F) = 3 kN");
    assert_eq!(last(&["v = 1..20", "v ="]), "v = [1; 2; 3; 4; 5; 6; 7; 8; …; 20]");
    assert_eq!(last(&["v = 1..20", "v = [1; 2; …; 20]"]), "v = [1; 2; 3; 4; 5; 6; 7; 8; …; 20]");
    assert_eq!(f("F[4] ="), "F[4] = ⚠");
    assert!(matches!(last_error(&["F = [1; 2; 3] kN", "F[4] ="]), CalcError::IndexOutOfRange { index: 4, len: 3 }));
    assert!(matches!(last_error(&["[1; 2] + [1; 2; 3] ="]), CalcError::LengthMismatch { left: 2, right: 3 }));
    assert!(matches!(last_error(&["[1 m; 2 s] ="]), CalcError::MixedVector { .. }));
    assert!(matches!(last_error(&["1..1e9 ="]), CalcError::TooManyValues { .. }));
}

fn table(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect()
}

#[test]
fn a_calculated_table_fills_formula_columns_and_totals() {
    let mut ws = Worksheet::new(Options::default());
    ws.eval_statement("γ = 1.35");
    let rows = table(&[
        &["Beam", "L [m]", "q [kN/m]", "M [kN·m] = γ*q*L^2/8"],
        &["B1", "4.5", "12", ""],
        &["B2", "6000 mm", "10", "999"],
        &["", "", "", "stale"],
        &["Total", "", "", "Σ ="],
        &["", "max =", "", "mean = 1"],
    ]);
    let r = ws.eval_table(&rows);
    assert_eq!(r.errors, vec![]);
    assert_eq!(
        r.updates,
        vec![
            (1, 3, "41.006".to_string()),
            (2, 3, "60.75".to_string()),
            (3, 3, String::new()),
            (4, 3, "Σ = 101.756".to_string()),
            (5, 1, "max = 6".to_string()),
            (5, 3, "mean = 50.878".to_string()),
        ]
    );
    assert_eq!(r.columns, vec!["L", "q", "M"]);
    assert_eq!(ws.eval_statement("sum(M) = kN*m").display.as_deref(), Some("sum(M) = 101.756 kN·m"));
    assert_eq!(ws.eval_statement("L =").display.as_deref(), Some("L = [4.5; 6] m"));
}

#[test]
fn table_errors_stay_in_their_cells() {
    let mut ws = Worksheet::new(Options::default());
    let rows = table(&[&["a [m]", "b [s]", "c = a + b", "d [kg] = a*2"], &["1", "2", "", ""], &["", "3", "", ""], &["Σ =", "", "", ""]]);
    let r = ws.eval_table(&rows);
    let at: Vec<(usize, usize)> = r.errors.iter().map(|(r, c, _)| (*r, *c)).collect();
    assert_eq!(at, vec![(1, 2), (1, 3), (2, 2), (2, 3)]);
    assert!(matches!(r.errors[0].2, CalcError::DimensionMismatch { .. }));
    assert!(matches!(r.errors[2].2, CalcError::MissingValue(ref n) if n == "a"));
    assert!(r.updates.contains(&(1, 2, "⚠".to_string())));
    assert!(r.updates.contains(&(3, 0, "Σ = 1".to_string())));
    // `a` has an empty cell: it is not defined as a column; `b` is.
    assert_eq!(r.columns, vec!["b"]);
    // A table with no column names is not a calculation.
    assert_eq!(ws.eval_table(&table(&[&["Name", "Notes here"], &["x", "y"]])).updates, vec![]);
}

#[test]
fn hostile_sums_vectors_and_tables_never_panic() {
    for text in [
        "∑",
        "∑_",
        "∑_(",
        "∑_(i=",
        "∑_(i=1)^",
        "∑▒",
        "Σ_^▒",
        "[",
        "]",
        "[;]",
        "[1;",
        "a[",
        "a[]",
        "1..",
        "..",
        "…",
        "⇒",
        "x ⇒",
        "⇒ = =",
        "x = ⇒ x =",
        "∏_(i=1)^n▒",
        "∑_(i=1)^3▒∑_i▒i",
        "i = 1..3",
        "[[1]] =",
        "x^x = 2",
        "sin(x) = 2",
        "0*x = 1",
        "1/x = 0",
        "x = x + 1",
        "x = [1; 2] + x",
        "〖〗",
        "_ = _",
    ] {
        let _ = run(&[text, "i = 1..3", text, &format!("{text} =")]);
    }
    let mut ws = Worksheet::new(Options::default());
    for rows in [
        table(&[]),
        table(&[&[]]),
        table(&[&["a [", "b []", "c [m] :=", "= x", "d [m] = ⚠"]]),
        table(&[&["a", "b = a"], &["Σ =", "Σ ="], &["1", "2"]]),
        table(&[&["x [m] = x"], &["1"]]),
    ] {
        let _ = ws.eval_table(&rows);
    }
}

#[test]
fn checks_compare_both_sides_in_one_unit() {
    let r = run(&["σ = 5.4 MPa", "f_yd = 355 N/mm^2", "σ ≤ f_yd", "σ > f_yd", "σ <= 5.4 MPa", "σ ≠ σ", "σ ≤ 3 m", "σ ≤ x"]);
    assert_eq!(r[2].display.as_deref(), Some("σ ≤ f_yd ⇒ 5.4 N/mm² ≤ 355 N/mm² ✓"));
    assert_eq!(r[2].error, None);
    assert_eq!(r[3].display.as_deref(), Some("σ > f_yd ⇒ 5.4 N/mm² > 355 N/mm² ✗"));
    assert!(matches!(r[3].error, Some(CalcError::CheckFailed(_))));
    // Nearly equal counts as equal; `<=` is `≤`.
    assert_eq!(r[4].error, None);
    assert!(matches!(r[5].error, Some(CalcError::CheckFailed(_))));
    assert!(matches!(r[6].error, Some(CalcError::DimensionMismatch { .. })));
    // An unknown name: not a check, left as written.
    assert_eq!(r[7].kind, RegionKind::NotCalc);
    // The stale tail after `⇒` is replaced.
    assert_eq!(last(&["a = 2", "a < 3 ⇒ 9 < 3 ✗"]), "a < 3 ⇒ 2 < 3 ✓");
}

#[test]
fn the_last_unit_written_is_the_working_unit() {
    assert_eq!(last(&["b = 300 mm", "h = 2*b", "h ="]), "h = 600 mm");
    // The last one written wins.
    assert_eq!(last(&["b = 300 mm", "h = 0.5 m", "b ="]), "b = 0.3 m");
    assert_eq!(last(&["b = 300 mm", "A = b*b", "A ="]), "A = 0.09 m²");
    assert_eq!(last(&["F = 2 kN", "G = 500 N", "F + G ="]), "F + G = 2500 N");
    assert_eq!(last(&["L = 4.5 m", "L = mm", "L ="]), "L = 4500 mm");
    // A unit asked for once keeps that kind of result in it.
    assert_eq!(last(&["x = 2 m", "x = cm", "y = 2*x", "y ="]), "y = 400 cm");
}

#[test]
fn unit_typos_get_a_hint() {
    assert_eq!(last_error(&["3 KN ="]).to_string(), "unknown name KN (did you mean kN?)");
    assert_eq!(last_error(&["3 m + 2 MM ="]).to_string(), "unknown name MM (did you mean mm?)");
    assert_eq!(last_error(&["3 m + 2 qq ="]).to_string(), "unknown name qq");
}

#[test]
fn dimensionless_units_are_kept() {
    assert_eq!(last(&["α = 30°", "α ="]), "α = 30°");
    assert_eq!(last(&["α = 30°", "β = 90° - α", "β ="]), "β = 60°");
    assert_eq!(last(&["α = 30°", "2 α ="]), "2 α = 60°");
    assert_eq!(last(&["α = 30°", "sin(α) ="]), "sin(α) = 0.5");
    assert_eq!(last(&["α = 30°", "α = rad"]), "α = 0.524 rad");
    assert_eq!(last(&["x = acos(0.5) ="]), "x = acos(0.5) = 60°");
    assert_eq!(last(&["p = 15 %", "p ="]), "p = 15 %");
    assert_eq!(last(&["p = 15 %", "2 p ="]), "2 p = 30 %");
    assert_eq!(last(&["p = 15 %", "1 + p ="]), "1 + p = 115 %");
    assert_eq!(last(&["F = 10 kN", "p = 15 %", "F·(1 + p) ="]), "F·(1 + p) = 11.5 kN");
    assert_eq!(last(&["w = 2 ‰", "w ="]), "w = 2 ‰");
    assert_eq!(last(&["c = 450 ppm", "c ="]), "c = 450 ppm");
    assert_eq!(last(&["a = 100 gon", "a = °"]), "a = 90°");
    // Solved unknowns keep the unit of what they equal.
    assert_eq!(last(&["α = 30°", "β + α = 90°"]), "β + α = 90° ⇒ β = 60°");
    // Ratios of the same unit are plain numbers.
    assert_eq!(last(&["α = 30°", "β = 60°", "α/β ="]), "α/β = 0.5");
    // Comparisons read in the left side's unit.
    assert_eq!(last(&["α = 30°", "α ≤ 45°"]), "α ≤ 45° ⇒ 30° ≤ 45° ✓");
    assert_eq!(last(&["x = 0.5", "x = %"]), "x = 50 %");
}

#[test]
fn dimensionless_table_columns() {
    let mut ws = Worksheet::new(Options::default());
    let rows = table(&[&["p [%]", "α [°]", "k = p*2"], &["15", "30", ""], &["20 %", "0.5 rad", ""]]);
    let r = ws.eval_table(&rows);
    assert_eq!(r.errors, vec![]);
    assert!(r.updates.contains(&(1, 2, "30 %".to_string())), "{:?}", r.updates);
    assert!(r.updates.contains(&(2, 2, "40 %".to_string())), "{:?}", r.updates);
    assert_eq!(ws.eval_statement("α =").display.as_deref(), Some("α = [30; 28.648]°"));
    assert_eq!(ws.eval_statement("p =").display.as_deref(), Some("p = [15; 20] %"));
}

#[test]
fn every_cell_can_be_data_a_formula_or_an_equation() {
    let mut ws = Worksheet::new(Options::default());
    let rows = table(&[
        &["Beam", "L [m]", "q [kN/m]", "M [kN·m]", "V [kN] = q*L/2"],
        &["B1", "4.5", "12", "q*L^2/8 =", ""],
        &["B2", "6", "10", "= q*L^2/8", "= q*L/2 + 1 kN"],
        &["B3", "2", "5", "20", ""],
        &["B4", "", "", "x = 3 m", ""],
        &["", "", "", "Σ =", "Σ ="],
    ]);
    let r = ws.eval_table(&rows);
    assert_eq!(r.errors, vec![]);
    let at = |row: usize, col: usize| r.updates.iter().find(|(a, b, _)| (*a, *b) == (row, col)).map(|(_, _, t)| t.as_str());
    assert_eq!(at(1, 3), Some("q*L^2/8 = 30.375 kN·m"));
    assert_eq!(at(2, 3), Some("q*L^2/8 = 45 kN·m"), "a leading = reads as an evaluation");
    assert_eq!(at(3, 3), None, "data stays as typed");
    assert_eq!(at(1, 4), Some("27"));
    // A statement in a formula column replaces the formula for that cell; `V` is the row's value.
    assert_eq!(at(2, 4), Some("q*L/2 + 1 kN = 31 kN"));
    assert_eq!(at(5, 3), Some("Σ = 95.375"));
    assert_eq!(at(5, 4), Some("Σ = 63"));
    // A definition in a cell defines a parameter for later regions; the row's names don't leak.
    assert_eq!(ws.eval_statement("x =").display.as_deref(), Some("x = 3 m"));
    assert_eq!(ws.eval_statement("M =").display.as_deref(), Some("M = [30.375; 45; 20] kN·m"));
}

fn steps(statements: &[&str]) -> Option<Vec<String>> {
    let opts = Options { steps: true, ..Options::default() };
    evaluate(statements, &opts).pop().and_then(|r| r.steps)
}

#[test]
fn solutions_show_formula_substitution_and_result() {
    // Solved for the unknown: formula, substitution, result.
    assert_eq!(steps(&["F = 10 N", "m = 4 kg", "F = m·a"]).unwrap(), ["a = F/m", "a = (10 N)/(4 kg)", "a = 2.5 m/s²"]);
    // A formula already solved: substitution and result.
    assert_eq!(steps(&["L = 4.5 m", "q = 12 kN/m", "M = q·L^2/8"]).unwrap(), ["M = (12 kN/m)·(4.5 m)²/8", "M = 30.375 kN·m"]);
    assert_eq!(steps(&["A = 50 cm^2", "A = π·d^2/4"]).unwrap(), ["d = √(4·A/π)", "d = √(4·(50 cm²)/π)", "d = 0.0798 m"]);
    // Plain numbers in the substitution need no parentheses; negatives do.
    assert_eq!(steps(&["a = 2", "b = -3", "c = a·b"]).unwrap(), ["c = 2·(-3)", "c = -6"]);
    // Solved numerically: the equation with the values, then the result.
    assert_eq!(steps(&["k = 6", "x^2 + x = k"]).unwrap(), ["x² + x = 6", "x = 2"]);
    // Data, and statements that ask for a unit, have no steps.
    assert_eq!(steps(&["L = 4.5 m"]), None);
    assert_eq!(steps(&["L = 4.5 m", "M = 2 L = mm"]), None);
    // With steps, a solved equation stays as written.
    let opts = Options { steps: true, ..Options::default() };
    let r = evaluate(&["F = 10 N", "m = 4 kg", "F = m·a ⇒ a = 1 m/s^2"], &opts).pop().unwrap();
    assert_eq!(r.display.as_deref(), Some("F = m·a"));
}

#[test]
fn a_unit_after_a_number_is_always_the_unit() {
    // `m` is a mass here, yet `4.5 m` is four and a half metres.
    assert_eq!(last(&["m = 400 kg", "L = 4.5 m", "L ="]), "L = 4.5 m");
    assert_eq!(last(&["m = 400 kg", "q = 12 kN/m", "q ="]), "q = 12 kN/m");
    assert_eq!(last(&["s = 3", "v = 9.81 m/s^2", "v ="]), "v = 9.81 m/s²");
    // Multiplying by the parameter takes a dot.
    assert_eq!(last(&["m = 400 kg", "F = 2·m", "F ="]), "F = 800 kg");
    // A chain stops at what is not a unit.
    assert_eq!(last(&["a = 3 m/s^2", "F = 2 kg·a", "F ="]), "F = 6 N");
    assert_eq!(last(&["2 kN/4 m^2 ="]), "2 kN/4 m^2 = 0.5 kN/m²");
    // The styling finds the units after numbers even when a parameter shares the name.
    let mut ws = Worksheet::new(Options::default());
    ws.eval_statement("m = 400 kg");
    assert_eq!(ws.units_in("x = 4.5 m"), ["m"]);
    assert_eq!(ws.units_in("F = m·a"), Vec::<String>::new());
}

/// Quantities of one kind written in different units are worked in one: the first one's.
#[test]
fn mixed_units_are_worked_in_one() {
    let opts = Options { steps: true, ..Options::default() };
    let run_steps = |st: &[&str]| evaluate(st, &opts).pop().unwrap();
    // `5 m·30 cm =`: the working, then the result under it.
    let r = run_steps(&["5 m·30 cm ="]);
    assert_eq!(r.display.as_deref(), Some("5 m·30 cm = 5 m·0.3 m"));
    assert_eq!(r.steps, Some(vec!["= 1.5 m²".to_string()]));
    assert_eq!(r.kind, RegionKind::Evaluation);
    // Read back (as a document stores it), it is worked again, not checked.
    let again = run_steps(&["5 m·30 cm = 5 m·0.3 m"]);
    assert_eq!((again.kind, again.display.clone(), again.steps.clone()), (RegionKind::Evaluation, r.display.clone(), r.steps.clone()));
    // Changing the data: worked anew.
    let changed = run_steps(&["5 m·40 cm = 5 m·0.3 m"]);
    assert_eq!(changed.display.as_deref(), Some("5 m·40 cm = 5 m·0.4 m"), "{changed:?}");
    // One unit already, a unit asked for, or no steps: as before.
    assert_eq!(run_steps(&["5 m·3 m ="]).steps, None);
    assert_eq!(run_steps(&["5 m·30 cm = cm^2"]).display.as_deref(), Some("5 m·30 cm = 15000 cm²"));
    assert_eq!(last(&["5 m·30 cm ="]), "5 m·30 cm = 1.5 m²");
    // In a formula's substitution, parameters of one kind in one unit.
    assert_eq!(steps(&["x = 0.5 mm", "y = 3 m", "z = x + y"]).unwrap(), ["z = (5·10^-4 m) + (3 m)", "z = 3.001 m"]);
    // `a` is shown in mm: every length is worked in mm.
    assert_eq!(steps(&["a = 20 cm", "b = 2 m", "s = 5 mm", "A = a·b + s·b"]).unwrap()[0], "A = (200 mm)·(2000 mm) + (5 mm)·(2000 mm)");
}

/// A data table: every value is a parameter named by its column and its row's label, joined by
/// a subscript one way (`F_A`) or the other (`A_F`); formula columns and totals still work.
#[test]
fn data_tables_name_their_cells() {
    let rows: Vec<Vec<String>> =
        [vec!["Support", "F [kN]", "a [m]", "M [kN·m] = F·a"], vec!["A", "10", "2", ""], vec!["B 2", "12", "0.5", ""], vec!["", "Σ =", "", ""]]
            .iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect();
    let mut ws = Worksheet::new(Options::default());
    let r = ws.eval_table_named(&rows, Some(CellNames::ColumnRow));
    assert_eq!(r.cells, ["F_A", "a_A", "M_A", "F_B2", "a_B2", "M_B2"]);
    assert_eq!(ws.eval_statement("F_A =").display.as_deref(), Some("F_A = 10 kN"));
    assert_eq!(ws.eval_statement("M_B2 =").display.as_deref(), Some("M_B2 = 6 kN·m"));
    // The columns stay vectors too.
    assert_eq!(ws.eval_statement("sum(F) =").display.as_deref(), Some("sum(F) = 22 kN"));
    let mut ws = Worksheet::new(Options::default());
    let r = ws.eval_table_named(&rows, Some(CellNames::RowColumn));
    assert_eq!(r.cells.first().map(String::as_str), Some("A_F"));
    assert_eq!(ws.eval_statement("B2_M =").display.as_deref(), Some("B2_M = 6 kN·m"));
    // Without names, a table defines its columns only.
    let mut ws = Worksheet::new(Options::default());
    assert!(ws.eval_table(&rows).cells.is_empty());
    assert!(ws.eval_statement("F_A =").error.is_some());
}

/// A force times a length to a power keeps the units it is written in (a moment in kN·m, loads
/// in kN/m and kN/m²); other compounds take the derived unit; a working unit wins.
#[test]
fn moments_and_loads_keep_their_composed_units() {
    assert_eq!(last(&["F = 12 kN", "a = 2 m", "F·a ="]), "F·a = 24 kN·m");
    assert_eq!(last(&["F = 12 kN", "a = 2 m", "M = F·a", "M ="]), "M = 24 kN·m");
    assert_eq!(last(&["g = 25 kN/m^3", "t = 0.2 m", "g·t ="]), "g·t = 5 kN/m²");
    assert_eq!(last(&["a = 3 m/s^2", "F = 2 kg·a", "F ="]), "F = 6 N");
    assert_eq!(last(&["F = 10 N", "m = 4 kg", "F/m ="]), "F/m = 2.5 m/s²");
    assert_eq!(last(&["1 kWh = J"]), "1 kWh = 3600000 J");
    // Lengths are shown in one unit (`a` as 0.5 m), so the compound still reads kN·m.
    assert_eq!(last(&["F = 2 kN", "a = 500 mm", "b = 1 m", "F·a/b·a ="]), "F·a/b·a = 0.5 kN·m");
    // A working unit for the kind wins.
    assert_eq!(last(&["E = 2 J", "F = 12 kN", "a = 2 m", "F·a ="]), "F·a = 24000 J");
}

// ---- Temperatures in °C and °F ----

fn kelvin(statements: &[&str]) -> f64 {
    run(statements).pop().and_then(|r| r.value).and_then(|v| v.scalar()).map(|q| q.value).expect("a value")
}

#[test]
fn celsius_and_fahrenheit_are_absolute_temperatures() {
    assert!(approx(kelvin(&["T = 20 °C"]), 293.15));
    assert!(approx(kelvin(&["T = 20 degC"]), 293.15));
    assert!(approx(kelvin(&["T = 20 ℃"]), 293.15));
    assert!(approx(kelvin(&["T = 68 °F"]), 293.15));
    assert!(approx(kelvin(&["T = 68 degF"]), 293.15));
    assert!(approx(kelvin(&["T = -40 °F"]), kelvin(&["T = -40 °C"])));
    assert!(approx(kelvin(&["T = -20 °C"]), 253.15));
    assert!(approx(kelvin(&["T = 20°C"]), 293.15));
    // Shown back in the unit written (the working unit), or in the one asked for.
    assert_eq!(last(&["T = 20 °C", "T ="]), "T = 20 °C");
    assert_eq!(last(&["T = 68 °F", "T = °C"]), "T = 20 °C");
    assert_eq!(last(&["T = 20 °C", "T = °F"]), "T = 68 °F");
    assert_eq!(last(&["T = 20 °C", "T = K"]), "T = 293.15 K");
    assert_eq!(last(&["T = 300 K", "T = °C"]), "T = 26.85 °C");
    assert_eq!(last(&["T = 20 degC", "T = degF"]), "T = 68 degF");
    assert_eq!(last(&["20 °C = K"]), "20 °C = 293.15 K");
    // A vector of temperatures.
    assert_eq!(last(&["T = [0; 100] °C", "T ="]), "T = [0; 100] °C");
    assert_eq!(last(&["T = [0; 100] °C", "T = K"]), "T = [273.15; 373.15] K");
    // The angle degree is still the angle degree.
    assert_eq!(last(&["α = 30°", "α ="]), "α = 30°");
}

#[test]
fn temperature_differences_are_in_kelvin() {
    let s = ["T_1 = 20 °C", "T_2 = 80 °C"];
    let with = |x: &str| {
        let mut v = s.to_vec();
        v.push(x);
        last(&v)
    };
    // Absolute − absolute is a difference: kelvin, never with the °C offset.
    assert_eq!(with("T_2 - T_1 ="), "T_2 - T_1 = 60 K");
    assert_eq!(with("ΔT = T_2 - T_1 ="), "ΔT = T_2 - T_1 = 60 K");
    let mut v = s.to_vec();
    v.extend(["ΔT = T_2 - T_1", "ΔT =", "ΔT = °C", "T_3 = T_1 + ΔT", "T_3 =", "T_1 + 5 K =", "T_2 - 10 K =", "2·ΔT =", "mean(T_1; T_2) ="]);
    let shown: Vec<String> = run(&v).into_iter().skip(3).filter_map(|r| r.display).collect();
    assert_eq!(
        shown,
        vec!["ΔT = 60 K", "ΔT = 60 °C", "T_3 = 80 °C", "T_1 + 5 K = 25 °C", "T_2 - 10 K = 70 °C", "2·ΔT = 120 K", "mean(T_1; T_2) = 50 °C"]
    );
    // A sum of absolute temperatures is a plain kelvin value.
    assert_eq!(with("T_1 + T_2 ="), "T_1 + T_2 = 646.3 K");
    // °C inside a compound unit is the size of a degree (no offset).
    assert_eq!(last(&["c = 4.2 kJ/(kg·°C)", "c = J/(kg·K)"]), "c = 4200 J/(kg·K)");
    assert_eq!(last(&["r = 3 °C/min", "r = K/h"]), "r = 180 K/h");
    // Checks compare absolute temperatures in °C.
    assert_eq!(with("T_1 ≤ 30 °C"), "T_1 ≤ 30 °C ⇒ 20 °C ≤ 30 °C ✓");
}

#[test]
fn temperature_columns_in_tables() {
    let mut ws = Worksheet::new(Options::default());
    let rows = table(&[&["T [°C]", "K [K] = T"], &["20", ""], &["68 °F", ""]]);
    let r = ws.eval_table(&rows);
    assert_eq!(r.errors, vec![]);
    assert_eq!(r.updates, vec![(1, 1, "293.15".to_string()), (2, 1, "293.15".to_string())]);
    let rows = table(&[&["K [K]", "T [°F] = K"], &["293.15", ""]]);
    assert_eq!(ws.eval_table(&rows).updates, vec![(1, 1, "68".to_string())]);
}

#[test]
fn hostile_temperatures_never_panic() {
    for s in [
        "T = 1e308 °C",
        "T = -1e308 °F",
        "T = °C °C",
        "T = 2 °C^2",
        "T = 1 m °C",
        "°C =",
        "T = [1e308; 1] °C",
        "x = 1e308 K = °F",
        "20 °C = °",
        "°F(3) =",
    ] {
        let _ = run(&[s]);
    }
    assert!(matches!(last_error(&["T = 1e308 °C", "T + T ="]), CalcError::Overflow));
}

// ---- User functions ----

#[test]
fn user_functions_are_defined_and_called() {
    let r = run(&["f(x) = 2·x + 1", "f(3) ="]);
    assert_eq!(r[0].kind, RegionKind::Definition);
    assert_eq!(r[0].name.as_deref(), Some("f"));
    assert_eq!(r[0].display, None);
    assert_eq!(r[0].error, None);
    assert_eq!(r[1].display.as_deref(), Some("f(3) = 7"));
    assert_eq!(last(&["g(a, b) = a·b", "g(2; 5) ="]), "g(2; 5) = 10");
    assert_eq!(last(&["g(a; b) := a - b", "g(2, 5) ="]), "g(2, 5) = -3");
    // Units flow through; parameters spelled like units (`A` ampere, `h` hour) are parameters.
    assert_eq!(last(&["A(b, h) = b·h", "A(2 m, 3 m) ="]), "A(2 m, 3 m) = 6 m²");
    assert_eq!(last(&["A(b, h) = b·h", "A(20 cm, 3 m) = cm^2"]), "A(20 cm, 3 m) = 6000 cm²");
    // In expressions, nested, with vectors, and defining parameters.
    assert_eq!(last(&["f(x) = x^2", "y = f(3) + 2·f(2)", "y ="]), "y = 17");
    assert_eq!(last(&["f(x) = x^2", "f(f(2)) ="]), "f(f(2)) = 16");
    assert_eq!(last(&["f(x) = x^2", "f(3)^2 ="]), "f(3)^2 = 81");
    assert_eq!(last(&["f(x) = x^2", "f([1; 2; 3]) ="]), "f([1; 2; 3]) = [1; 4; 9]");
    assert_eq!(last(&["f(x) = 2·x", "∑_(i=1)^3▒f(i) ="]), "∑_(i=1)^3▒f(i) = 12");
    // A later function calls an earlier one.
    assert_eq!(last(&["sq(x) = x·x", "M(q, L) = q·sq(L)/8", "M(12 kN/m, 4.5 m) = kN·m"]), "M(12 kN/m, 4.5 m) = 30.375 kN·m");
    // Redefined, the new body wins from there on.
    assert_eq!(last(&["f(x) = x + 1", "f(x) = x + 2", "f(0) ="]), "f(0) = 2");
}

#[test]
fn function_parameters_shadow_and_free_names_are_read_when_called() {
    // The parameter x shadows the worksheet's x.
    assert_eq!(last(&["x = 100", "f(x) = x + 1", "f(1) ="]), "f(1) = 2");
    assert_eq!(last(&["x = 100", "f(x) = x + 1", "x ="]), "x = 100");
    // Other names are read from the worksheet when the function is called.
    assert_eq!(last(&["f(x) = k·x", "k = 3", "f(2) ="]), "f(2) = 6");
    assert_eq!(last(&["k = 2", "f(x) = k·x", "k := 5", "f(2) ="]), "f(2) = 10");
    assert!(matches!(last_error(&["f(x) = k·x", "f(2) ="]), CalcError::UnknownName(ref n) if n == "k"));
    // A ∑ variable of the caller is not seen in the body.
    assert!(matches!(last_error(&["f(x) = x + i", "∑_(i=1)^3▒f(i) ="]), CalcError::UnknownName(ref n) if n == "i"));
}

#[test]
fn a_function_definition_is_not_an_equation() {
    // `f(x) = …` with x unknown would otherwise be solved for x.
    let r = run(&["f(x) = 7"]);
    assert_eq!(r[0].kind, RegionKind::Definition);
    assert_eq!(r[0].display, None);
    // Not plain names in brackets: an ordinary statement (a product).
    assert_eq!(last(&["a = 2", "b = 3", "a(b + 1) ="]), "a(b + 1) = 8");
    assert_eq!(last(&["a = 2", "a(3) ="]), "a(3) = 6");
    // Solving with a function: the call on the right.
    assert_eq!(last(&["f(x) = 2·x + 1", "7 = f(y)"]), "7 = f(y) ⇒ y = 3");
    // A check of a call.
    assert_eq!(run(&["f(x) = 2·x + 1", "f(3) = 7"])[1].error, None);
}

#[test]
fn hostile_user_functions_return_errors() {
    // Recursion stops at the depth cap, with an error.
    assert!(matches!(last_error(&["f(x) = f(x)", "f(1) ="]), CalcError::TooDeep { .. }));
    assert!(matches!(last_error(&["f(x) = f(x + 1) + 1", "f(1) ="]), CalcError::TooDeep { .. }));
    assert!(matches!(last_error(&["g(x) = x", "f(x) = g(x)", "g(x) = f(x)", "g(1) ="]), CalcError::TooDeep { .. }));
    // A deep body at every level of the recursion still stops before the stack does.
    let deep = format!("f(x) = f(x){}", " + 1".repeat(250));
    assert!(matches!(last_error(&[&deep, "f(1) ="]), CalcError::TooDeep { .. }));
    let nested = format!("f(x) = {}f(x){}", "(".repeat(60), ")".repeat(60));
    assert!(matches!(last_error(&[&nested, "f(1) ="]), CalcError::TooDeep { .. }));
    // Exponential fan-out stops at the step budget.
    let fan = |name: &str, inner: &str| format!("{name}(x) = {}", vec![format!("{inner}(x)"); 100].join(" + "));
    let (f1, f2, f3, f4) = (fan("b", "a"), fan("c", "b"), fan("d", "c"), fan("e", "d"));
    assert!(matches!(last_error(&["a(x) = x", &f1, &f2, &f3, &f4, "e(1) ="]), CalcError::TooMuchWork));
    // Wrong arity.
    assert!(matches!(last_error(&["g(a, b) = a·b", "g(1) ="]), CalcError::ArgCount { expected: 2, found: 1, .. }));
    assert!(matches!(last_error(&["f(x) = x", "f(1, 2, 3) ="]), CalcError::ArgCount { expected: 1, found: 3, .. }));
    assert!(matches!(last_error(&["f(x) = x", "f() ="]), CalcError::ArgCount { expected: 1, found: 0, .. }));
    // Non-finite values.
    assert!(matches!(last_error(&["f(x) = 1/x", "f(0) ="]), CalcError::DivisionByZero));
    assert!(matches!(last_error(&["f(x) = x^100", "f(1e10) ="]), CalcError::Overflow));
    assert!(matches!(last_error(&["f(x) = sqrt(x)", "f(-1) ="]), CalcError::NotReal));
    // Bad definitions.
    // A built-in function is never redefined: `sin(x) = x` is an equation in x.
    assert_eq!(run(&["sin(x) = x"])[0].kind, RegionKind::Definition);
    assert!(matches!(last_error(&["sin(x) := x"]), CalcError::Syntax { .. }));
    assert!(matches!(last_error(&["f(x, x) = x"]), CalcError::BadDefinition(_)));
    assert!(matches!(last_error(&["f(x) = x = 3"]), CalcError::Syntax { .. }));
    assert!(matches!(last_error(&["f(x) = (x"]), CalcError::Syntax { .. }));
    // A failed definition undefines the function.
    assert!(matches!(last_error(&["f(x) = x", "f(x) = (x", "f(1) ="]), CalcError::Syntax { .. } | CalcError::UnknownName(_)));
    for s in ["f(x) =", "f( = 1", "f(x, = 1", "f(x y) = 1", "f(1) = x", "(x) = 1", "f(x)(y) = 1", "f(x) := f", "f(x) = f", "f(f) = f(f)"] {
        let _ = run(&[s, "f(1) ="]);
    }
    // A function whose name is a unit is still called as a function.
    assert_eq!(last(&["m(x) = x + 1", "m(1) ="]), "m(1) = 2");
}

#[test]
fn deep_recursion_fits_a_small_stack() {
    // A Windows main thread has 1 MiB of stack: the depth caps stop well within it.
    let deep = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let left = format!("f(x) = f(x){}", " + 1".repeat(250));
            let right = format!("f(x) = {}f(x)", "1 + ".repeat(250));
            [last_error(&[&left, "f(1) ="]), last_error(&[&right, "f(1) ="])]
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(deep.iter().all(|e| matches!(e, CalcError::TooDeep { .. })), "{deep:?}");
}

/// `A = 3 m · 5 m =`: a definition worked out at once. `A` is defined and the last `=` shows
/// its value; a product of quantities is no working unit (it used to make the result read
/// `A = 3 m · 5 m = 3 m·5 m`, in the "unit" `m·5 m`).
#[test]
fn a_definition_worked_out_at_once() {
    let r = run(&["A = 3 m · 5 m ="]).pop().unwrap();
    assert_eq!(r.kind, RegionKind::DefineAndShow);
    assert_eq!(r.name.as_deref(), Some("A"));
    assert_eq!(r.display.as_deref(), Some("A = 3 m · 5 m = 15 m²"));
    assert_eq!(r.error, None);
    // `*` (the editor types `·`), `:=`, and the value is a parameter for what follows.
    assert_eq!(last(&["A = 3 m * 5 m ="]), "A = 3 m * 5 m = 15 m²");
    assert_eq!(last(&["A := 3 m·5 m ="]), "A := 3 m·5 m = 15 m²");
    assert_eq!(last(&["A = 3 m·5 m =", "A ="]), "A = 15 m²");
    assert_eq!(last(&["A = 3 m·5 m =", "V = A·2 m ="]), "V = A·2 m = 30 m³");
    // Read back with its result, refreshed; a stale number is replaced; a unit asked for is kept.
    assert_eq!(last(&["A = 3 m·5 m = 15 m²"]), "A = 3 m·5 m = 15 m²");
    assert_eq!(last(&["A = 3 m·5 m = 99 m²"]), "A = 3 m·5 m = 15 m²");
    assert_eq!(last(&["A = 3 m·5 m = cm²"]), "A = 3 m·5 m = 150000 cm²");
    assert_eq!(last(&["A = 3 m·5 m = 150000 cm²"]), "A = 3 m·5 m = 150000 cm²");
    assert_eq!(last(&["A := 3 m·5 m = 99 m²"]), "A := 3 m·5 m = 15 m²");
    // The working unit stays the one written: a plain definition of a product doesn't set one.
    assert_eq!(last(&["A = 3 m·5 m", "A ="]), "A = 15 m²");
    assert_eq!(last(&["A = 3 m·5 m", "B = 2 m·2 m", "B ="]), "B = 4 m²");
    assert_eq!(last(&["x = 300 cm·1 m", "x ="]), "x = 3 m²");
    assert_eq!(last(&["a = 2 cm^2", "A = 3 m·5 m ="]), "A = 3 m·5 m = 150000 cm²");
    // A unit asked for with a `1` over it still becomes the working unit.
    assert_eq!(last(&["n_1 = 2 Hz", "n_2 = 3 Hz", "n_1 = 1/min", "n_2 ="]), "n_2 = 180 1/min");
    // With parameters, and in the number format.
    assert_eq!(last(&["b = 2 m", "A = b·5 m =", "A ="]), "A = 10 m²");
    assert_eq!(last(&["L := 4.5 m", "q := 12 kN/m", "M = q·L^2/8 ="]), "M = q·L^2/8 = 30.375 kN·m");
    let opts = Options { digits: 1, ..Options::default() };
    assert_eq!(evaluate(&["A = 1.25 m·1 m ="], &opts).pop().unwrap().display.as_deref(), Some("A = 1.25 m·1 m = 1.3 m²"));
    // Only a last bare `=` (or a unit, or a stale result) after the definition: a middle value
    // makes three sides, an error.
    for bad in ["A = 3 m = 4 m =", "A = 3 m = =", "A = 3 m · 5 m = = "] {
        let r = run(&[bad]).pop().unwrap();
        assert!(matches!(r.error, Some(CalcError::Syntax { .. })), "{bad}: {r:?}");
        assert_eq!(r.display, None, "{bad} is left as typed");
    }
    for alone in ["A = =", "= 3 m =", "= =", "=", "A = = 3 m"] {
        let r = run(&[alone]).pop().unwrap();
        assert_eq!(r.display, None, "{alone}: {r:?}");
    }
}

/// With Show Steps, `A = 3 m·50 cm =` shows the working in one unit and the result under it,
/// like `3 m·50 cm =`; read back, the working is worked again.
#[test]
fn a_definition_worked_out_at_once_with_steps() {
    let opts = Options { steps: true, ..Options::default() };
    let one = |st: &[&str]| evaluate(st, &opts).pop().unwrap();
    let r = one(&["A = 3 m·50 cm ="]);
    assert_eq!(r.kind, RegionKind::DefineAndShow);
    assert_eq!(r.display.as_deref(), Some("A = 3 m·50 cm = 3 m·0.5 m"));
    assert_eq!(r.steps, Some(vec!["= 1.5 m²".to_string()]));
    let again = one(&["A = 3 m·50 cm = 3 m·0.5 m"]);
    assert_eq!((again.kind, again.display.clone(), again.steps.clone()), (r.kind, r.display.clone(), r.steps.clone()));
    assert_eq!(one(&["A = 3 m·50 cm = 3 m·0.5 m", "A ="]).display.as_deref(), Some("A = 1.5 m²"));
    // New data, worked anew; steps off, the result after `=` again.
    assert_eq!(one(&["A = 3 m·40 cm = 3 m·0.5 m"]).display.as_deref(), Some("A = 3 m·40 cm = 3 m·0.4 m"));
    assert_eq!(last(&["A = 3 m·50 cm = 3 m·0.5 m"]), "A = 3 m·50 cm = 1.5 m²");
    // `:=` the same.
    let d = one(&["A := 3 m·50 cm ="]);
    assert_eq!((d.display.as_deref(), d.steps.clone()), (Some("A := 3 m·50 cm = 3 m·0.5 m"), Some(vec!["= 1.5 m²".to_string()])));
    assert_eq!(one(&["A := 3 m·50 cm = 3 m·0.5 m"]).display.as_deref(), Some("A := 3 m·50 cm = 3 m·0.5 m"));
    // One unit already: the result after `=`, no steps.
    let plain = one(&["A = 3 m·5 m ="]);
    assert_eq!((plain.display.as_deref(), plain.steps), (Some("A = 3 m·5 m = 15 m²"), None));
    // A unit asked for: kept, no working.
    assert_eq!(one(&["A = 3 m·50 cm = cm^2"]).display.as_deref(), Some("A = 3 m·50 cm = 15000 cm²"));
}

/// The other uses of `=` are unchanged by the definition worked out at once.
#[test]
fn one_equals_sign_still_solves_checks_and_shows() {
    assert_eq!(last(&["x + 2 = 5"]), "x + 2 = 5 ⇒ x = 3");
    assert_eq!(run(&["a = 2", "b = 2", "a = b"]).pop().unwrap().kind, RegionKind::Check);
    assert!(run(&["a = 2", "b = 3", "a = b"]).pop().unwrap().error.is_some());
    assert_eq!(last(&["a = 2 m", "a ="]), "a = 2 m");
    assert_eq!(last(&["A = 3 m·5 m", "A = cm^2"]), "A = 150000 cm²");
    assert_eq!(last(&["f(x) = 2·x + 1", "f(3) ="]), "f(3) = 7");
    assert_eq!(last(&["F = 10 N", "m = 4 kg", "F = m·a = m/s^2"]), "F = m·a = 2.5 m/s²");
}

/// A calculated table's ordinal column numbers the data rows with something in them: not the
/// header, not a totals row, not an empty row.
#[test]
fn table_rows_are_numbered_in_order() {
    let rows: Vec<Vec<String>> = [vec!["L [m]", "M = L^2"], vec!["4", ""], vec!["", ""], vec!["6", ""], vec!["", "Σ ="], vec!["7", ""]]
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    assert_eq!(crate::ordinals(&rows), vec![None, Some(1), None, Some(2), None, Some(3)]);
    assert!(crate::ordinals(&[]).is_empty());
}

/// Gauss's method (the shoelace formula) in a calculated table: `x[i+1]` is the next row's `x`,
/// wrapping round to the first, so the polygon closes; `i` is the row's number, `n` the rows'.
#[test]
fn table_rows_are_reached_by_index() {
    let rows: Vec<Vec<String>> = [
        vec![
            "x [m]",
            "y [m]",
            "a [m^2] = (x·y[i+1] - x[i+1]·y)/2",
            "s [m^3] = (x + x[i+1])·(x·y[i+1] - x[i+1]·y)/6",
            "t [m^3] = (y + y[i+1])·(x·y[i+1] - x[i+1]·y)/6",
            "k = i/n",
        ],
        vec!["0", "0", "", "", "", ""],
        vec!["4", "0", "", "", "", ""],
        vec!["4", "3", "", "", "", ""],
        vec!["0", "3", "", "", "", ""],
        vec!["", "", "Σ =", "Σ =", "Σ =", ""],
    ]
    .iter()
    .map(|r| r.iter().map(|s| s.to_string()).collect())
    .collect();
    let mut ws = Worksheet::new(Options::default());
    let r = ws.eval_table(&rows);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let cell = |row: usize, col: usize| r.updates.iter().find(|(rr, c, _)| *rr == row && *c == col).map(|(_, _, t)| t.clone()).unwrap_or_default();
    assert_eq!(cell(5, 2), "Σ = 12", "area 4 × 3");
    assert_eq!((cell(1, 5), cell(4, 5)), ("0.25".to_string(), "1".to_string()), "i/n");
    // The centroid from the columns: x̄ = Σs / Σa, ȳ = Σt / Σa.
    assert_eq!(ws.eval_statement("x_G = sum(s)/sum(a) =").display.unwrap_or_default().replace(' ', ""), "x_G=sum(s)/sum(a)=2m");
    assert_eq!(ws.eval_statement("y_G = sum(t)/sum(a) =").display.unwrap_or_default().replace(' ', ""), "y_G=sum(t)/sum(a)=1.5m");
    // Cyclic both ways: x[i-1] in the first row is the last row's; a column may read its own
    // earlier rows, and a row not worked out yet says so.
    let rows: Vec<Vec<String>> = [vec!["x", "p = x[i-1]", "c = c[i-1] + x"], vec!["1", "", ""], vec!["2", "", ""], vec!["5", "", ""]]
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    let r = Worksheet::new(Options::default()).eval_table(&rows);
    let cell = |row: usize, col: usize| r.updates.iter().find(|(rr, c, _)| *rr == row && *c == col).map(|(_, _, t)| t.clone()).unwrap_or_default();
    assert_eq!((cell(1, 1), cell(2, 1), cell(3, 1)), ("5".to_string(), "1".to_string(), "2".to_string()));
    assert!(r.errors.iter().any(|(row, c, _)| *row == 1 && *c == 2), "c[0] is the last row's, not worked out yet: {:?}", r.errors);
    // An index that is not a whole number is an error, not a panic.
    let rows: Vec<Vec<String>> = [vec!["x", "q = x[i + 0.5]"], vec!["1", ""]].iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect();
    assert!(!Worksheet::new(Options::default()).eval_table(&rows).errors.is_empty());
}
