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
    assert!(approx(results[2].value.unwrap().value, 30375.0));
    assert_eq!(results[3].kind, RegionKind::Evaluation);
    assert_eq!(results[3].display.as_deref(), Some("M = 30.38 kN·m"));
}

#[test]
fn requested_unit_variants() {
    assert_eq!(beam("M = 30.4 kN*m"), "M = 30.38 kN·m");
    assert_eq!(beam("M = kN·m"), "M = 30.38 kN·m");
    assert_eq!(beam("M = kN m"), "M = 30.38 kN·m");
    assert_eq!(beam("M = 99 kN*m"), "M = 30.38 kN·m");
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
    assert_eq!(beam("M ="), "M = 30.38 kJ");
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
    assert_eq!(last(&["100 % ="]), "100 % = 1");
    assert_eq!(last(&["2 kN/(4 m^2) ="]), "2 kN/(4 m^2) = 500 Pa");
    assert_eq!(last(&["1 kWh = J"]), "1 kWh = 3600000 J");
    assert_eq!(last(&["1 bar = kPa"]), "1 bar = 100 kPa");
    assert_eq!(last(&["1 t = kg"]), "1 t = 1000 kg");
    assert_eq!(last(&["90 min = h"]), "90 min = 1.5 h");
    assert_eq!(last(&["2 L = cm³"]), "2 L = 2000 cm³");
    assert_eq!(last(&["180° ="]), "180° = 3.142");
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
    // A parameter wins over a unit and a constant.
    assert_eq!(last(&["m := 3", "2 m ="]), "2 m = 6");
    assert_eq!(last(&["g := 2", "g ="]), "g = 2");
    assert_eq!(last(&["m := 3", "1 km = m"]), "1 km = 1000 m"); // display units are units only
    let mut sheet = Worksheet::new(Options::default());
    sheet.eval_statement("x := 1");
    sheet.eval_statement("y := 2");
    sheet.eval_statement("x := 3");
    let names: Vec<_> = sheet.scope().iter().map(|(n, q)| (n.to_string(), q.value)).collect();
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
    let q = |s: &str| run(&[s]).pop().unwrap().value.unwrap();
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
    for text in ["E = mc^2", "a + b", "∫ f dx = F", "", "x = y", "= 5", "a = b = c", "y = 2 x", "f(x) = x^2"] {
        let r = run(&[text]).pop().unwrap();
        assert_eq!(r.kind, RegionKind::NotCalc, "{text}");
        assert_eq!(r.display, None, "{text}");
        assert_eq!(r.error, None, "{text}");
    }
}

#[test]
fn number_formatting() {
    let o = Options::default();
    assert_eq!(format_number(30.375, &o), "30.38");
    assert_eq!(format_number(1003.0, &o), "1003");
    assert_eq!(format_number(123456.0, &o), "123500");
    assert_eq!(format_number(0.5, &o), "0.5");
    assert_eq!(format_number(0.000123456, &o), "0.0001235");
    assert_eq!(format_number(0.0000123456, &o), "1.235·10^-5");
    assert_eq!(format_number(1.23e9, &o), "1.23·10^9");
    assert_eq!(format_number(999999999.0, &o), "1·10^9");
    assert_eq!(format_number(-2.5, &o), "-2.5");
    assert_eq!(format_number(-0.0, &o), "0");
    assert_eq!(format_number(9.99996, &o), "10");
    assert_eq!(format_number(f64::NAN, &o), "⚠");
    assert_eq!(format_number(2.0 / 3.0, &Options { sig_digits: 0, ..o }), "0.7");
    assert_eq!(format_number(2.0 / 3.0, &Options { sig_digits: 200, ..o }), "0.666666666666667");
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
    assert_eq!(run(&[&prose])[0], RegionResult { kind: RegionKind::NotCalc, name: None, value: None, display: None, error: None });
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
        let opts = Options { sig_digits: (round % 17) as u8, decimal_comma: round % 3 == 0 };
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
        let _ = evaluate(&[s.as_str()], &Options { sig_digits: 255, decimal_comma: true });
    }
}
