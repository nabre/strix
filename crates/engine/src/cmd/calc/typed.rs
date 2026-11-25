//! A statement typed as text (`calc.insert`, the CLI, MCP) made ready for the equation: the
//! linear format reads spaces as the end of an operand, so `3 / 100` or `r ^ 2` would build an
//! empty fraction or power; and functions are spelled as the working writes them (`√`, `cos⁻¹`),
//! so the formula and its values look alike.

/// `text` with the spaces around `/`, `^` and `_` taken out and `sqrt`, `asin`, `acos`, `atan`
/// before a bracket written `√`, `sin⁻¹`, `cos⁻¹`, `tan⁻¹`.
pub(crate) fn tidy(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() {
            // Spaces before an operator that binds its neighbours, or after one, go.
            let mut j = i;
            while chars.get(j).is_some_and(|c| c.is_whitespace()) {
                j += 1;
            }
            let next_binds = chars.get(j).is_some_and(|&n| binds(n));
            let prev_binds = out.chars().last().is_some_and(binds);
            if !next_binds && !prev_binds {
                out.push(' ');
            }
            i = j;
            continue;
        }
        let word_start = !out.chars().last().is_some_and(|p| p.is_alphanumeric() || p == '_');
        if word_start && c.is_ascii_alphabetic() {
            let rest: String = chars.get(i..).unwrap_or_default().iter().take(5).collect();
            let spelled =
                [("sqrt(", "√("), ("asin(", "sin⁻¹("), ("acos(", "cos⁻¹("), ("atan(", "tan⁻¹(")].into_iter().find(|(from, _)| rest.starts_with(from));
            if let Some((from, to)) = spelled {
                out.push_str(to);
                i += from.chars().count();
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out.trim().to_string()
}

/// An operator whose operands are the characters next to it in the linear format.
fn binds(c: char) -> bool {
    matches!(c, '/' | '^' | '_')
}

#[cfg(test)]
mod tests {
    use super::tidy;

    #[test]
    fn spaces_around_fractions_and_powers_go() {
        assert_eq!(tidy("ε = atan(3 / 100) ="), "ε = tan⁻¹(3/100) =");
        assert_eq!(tidy("A = r ^ 2·pi ="), "A = r^2·pi =");
        assert_eq!(tidy("q = 12 kN / m"), "q = 12 kN/m");
        assert_eq!(tidy("x = f _ ck"), "x = f_ck");
        // Other spaces stay; names holding the letters stay.
        assert_eq!(tidy("y = sqrt(4) + 2 m ="), "y = √(4) + 2 m =");
        assert_eq!(tidy("y = xsqrt(4)"), "y = xsqrt(4)");
        assert_eq!(tidy("z = acos(0.5) + asin(x)"), "z = cos⁻¹(0.5) + sin⁻¹(x)");
        assert_eq!(tidy("atan2(1; 2)"), "atan2(1; 2)");
        assert_eq!(tidy("  "), "");
    }
}
