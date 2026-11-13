//! Measurement uncertainty and metrological output.

use crate::{Count, Options, format_number};

/// First-order propagation for independent measurements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UncertaintyMode {
    /// Sum of absolute derivative contributions (maximum uncertainty).
    #[default]
    Max,
    /// Root of the sum of squared derivative contributions (GUM, independent inputs).
    Rss,
}

impl UncertaintyMode {
    /// Stable calculation-option identifier.
    pub fn name(self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Rss => "rss",
        }
    }

    /// Parses a calculation-option identifier.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "max" => Some(Self::Max),
            "rss" => Some(Self::Rss),
            _ => None,
        }
    }
}

pub(crate) fn format_measurement(value: f64, uncertainty: Option<f64>, opts: &Options) -> String {
    let Some(u) = uncertainty else { return format_number(value, opts) };
    if !value.is_finite() || !u.is_finite() || u < 0.0 {
        return "⚠".into();
    }
    if u == 0.0 {
        return format!("{} ± 0", format_number(value, opts));
    }
    // Two significant figures for a leading 1 or 2, one otherwise. Both magnitudes
    // share the rounded uncertainty's decimal place, including trailing zeros.
    let exp = u.log10().floor() as i32;
    if exp < -300 {
        let uncertainty_options = Options { count: Count::Significant, digits: 2, zeros: false, ..*opts };
        return format!("{} ± {}", format_number(value, opts), format_number(u, &uncertainty_options));
    }
    let leading = u / 10_f64.powi(exp);
    let sig = if leading < 3.0 { 2 } else { 1 };
    let places = (sig - 1 - exp).clamp(-308, 308);
    let rounded = crate::format::step_decimal(u, places, crate::format::RoundMode::HalfAwayFromZero);
    let v = crate::format::step_decimal(value, places, crate::format::RoundMode::HalfAwayFromZero);
    let format = |x: f64| {
        if (0..=15).contains(&places) && x.abs() < 1e7 {
            let text = format!("{:.*}", places as usize, x);
            if opts.decimal_comma { text.replace('.', ",") } else { text }
        } else {
            format_number(x, &Options { count: Count::Significant, digits: 15, zeros: false, ..*opts })
        }
    };
    format!("{} ± {}", format(v), format(rounded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dim, Options, Quantity, Worksheet};

    fn q(w: &Worksheet, text: &str) -> Quantity {
        w.eval_expr(text).unwrap().scalar().unwrap()
    }

    #[test]
    fn literals_units_relative_and_accessors() {
        let w = Worksheet::new(Options::default());
        let x = q(&w, "12.4 ± 0.1 cm");
        assert!((x.value - 0.124).abs() < 1e-12);
        assert!((x.unc.unwrap() - 0.001).abs() < 1e-12);
        assert_eq!(x, q(&w, "12.4 +- 0.1 cm"));
        let r = q(&w, "12.4 ± 2% cm");
        assert!((r.unc.unwrap() - 0.00248).abs() < 1e-12);
        let speed = q(&w, "10 ± 2% m/s");
        assert_eq!(speed.value, 10.0);
        assert!((speed.unc.unwrap() - 0.2).abs() < 1e-12);
        assert_eq!(q(&w, "val(12.4 ± 0.1 cm)").unc, None);
        assert!((q(&w, "unc(12.4 ± 0.1 cm)").value - 0.001).abs() < 1e-12);
        assert!((q(&w, "relunc(12.4 ± 2% cm)").value - 0.02).abs() < 1e-12);
        for bad in ["12 m ± 1 s", "12 ± -1", "12 ± (1/0)", "relunc(0 ± 1)"] {
            assert!(w.eval_expr(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn independent_arithmetic_modes_and_zero() {
        let max = Worksheet::new(Options::default());
        let rss = Worksheet::new(Options { uncertainty: UncertaintyMode::Rss, ..Options::default() });
        assert_eq!(q(&max, "(3 ± 0.3) + (4 ± 0.4)").unc, Some(0.7));
        assert!((q(&rss, "(3 ± 0.3) + (4 ± 0.4)").unc.unwrap() - 0.5).abs() < 1e-12);
        assert!((q(&max, "(3 ± 0.3) * (4 ± 0.4)").unc.unwrap() - 2.4).abs() < 1e-12);
        assert!((q(&max, "(3 ± 0.3)^2").unc.unwrap() - 1.8).abs() < 1e-12);
        assert_eq!(q(&max, "-(3 ± 0.3)").unc, Some(0.3));
        assert_eq!(q(&max, "(0 ± 1)*2").unc, Some(2.0));
        assert!(Quantity::new(1.0, Dim::NONE).with_uncertainty(f64::INFINITY).is_err());
    }

    #[test]
    fn aligned_metrological_output_and_conversion() {
        let w = Worksheet::new(Options::default());
        let x = w.eval_expr("12.435 ± 0.12 cm").unwrap();
        assert_eq!(crate::format_value_in(&x, "cm", &Options::default()).unwrap().1, "12.44 ± 0.12 cm");
        assert_eq!(crate::format_value_in(&x, "mm", &Options::default()).unwrap().1, "124.4 ± 1.2 mm");
    }
    #[test]
    fn vector_temperature_and_recalculation_preserve_uncertainty() {
        let mut w = Worksheet::new(Options { uncertainty: UncertaintyMode::Rss, ..Options::default() });
        let v = w.eval_expr("[3 ± 0.3; 4 ± 0.4]*2").unwrap();
        assert_eq!(v.quantities().get(1).unwrap().unc, Some(0.8));
        assert!((q(&w, "sum([3 ± 0.3; 4 ± 0.4])").unc.unwrap() - 0.5).abs() < 1e-12);
        let t = q(&w, "20 ± 0.2 °C");
        assert!((t.value - 293.15).abs() < 1e-12);
        assert_eq!(t.unc, Some(0.2));
        assert_eq!(crate::format_value_in(&t.into(), "°C", &Options::default()).unwrap().1, "20.00 ± 0.20 °C");
        let first = w.eval_statement("x := (3 ± 0.3) + (4 ± 0.4) =");
        assert_eq!(first.error, None);
        let second = w.eval_statement(first.display.as_deref().unwrap());
        assert_eq!(second.error, None);
        assert_eq!(first.display, second.display);
    }
}
