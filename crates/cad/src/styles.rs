//! Named text and dimension styles (docs/cad.md, Styles): made, renamed and deleted here, so the
//! objects and styles that name one follow. Standard ([`STANDARD`]) is every drawing's own and
//! is never made, renamed or deleted.

use crate::model::{Drawing, Kind, MAX_STYLES, NamedDimStyle, STANDARD, TextStyle};

/// Which family of styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Text,
    Dim,
}

impl StyleKind {
    pub fn parse(s: &str) -> Option<StyleKind> {
        match s.trim() {
            "text" => Some(StyleKind::Text),
            "dim" | "dimension" => Some(StyleKind::Dim),
            _ => None,
        }
    }

    /// Whether entity kind `k` takes a style of this family.
    fn takes(self, k: &Kind) -> bool {
        match self {
            StyleKind::Text => matches!(k, Kind::Text { .. }),
            StyleKind::Dim => matches!(k, Kind::Dim { .. } | Kind::Angle { .. } | Kind::Level { .. }),
        }
    }
}

/// A style name as typed: trimmed, at most 64 characters; an error when empty or Standard.
fn clean(name: &str) -> Result<String, String> {
    let n: String = name.trim().chars().take(64).collect();
    if n.is_empty() {
        return Err("a style needs a name".into());
    }
    if n == STANDARD {
        return Err(format!("{STANDARD} is the drawing's own style"));
    }
    Ok(n)
}

impl Drawing {
    /// The names of the styles of `kind`, Standard first.
    pub fn styles(&self, kind: StyleKind) -> Vec<String> {
        let named: Vec<String> = match kind {
            StyleKind::Text => self.text_styles.iter().map(|s| s.name.clone()).collect(),
            StyleKind::Dim => self.dim_styles.iter().map(|s| s.name.clone()).collect(),
        };
        std::iter::once(STANDARD.to_string()).chain(named).collect()
    }

    fn has_style(&self, kind: StyleKind, name: &str) -> bool {
        self.styles(kind).iter().any(|n| n == name)
    }

    /// A name no style of `kind` has yet: `base`, else `base 2`, `base 3`…
    pub fn free_style_name(&self, kind: StyleKind, base: &str) -> String {
        let base = clean(base).unwrap_or_else(|_| "Style".to_string());
        if !self.has_style(kind, &base) {
            return base;
        }
        (2..=MAX_STYLES + 2).map(|n| format!("{base} {n}")).find(|n| !self.has_style(kind, n)).unwrap_or(base)
    }

    /// Makes a style of `kind` called `name`, a copy of style `from` (Standard when `None` or
    /// unknown), and makes it current. Its name, as kept.
    pub fn new_style(&mut self, kind: StyleKind, name: &str, from: Option<&str>) -> Result<String, String> {
        let name = clean(name)?;
        if self.has_style(kind, &name) {
            return Err(format!("there is already a style {name}"));
        }
        match kind {
            StyleKind::Dim => {
                if self.dim_styles.len() >= MAX_STYLES {
                    return Err(format!("a drawing has at most {MAX_STYLES} dimension styles"));
                }
                let style = self.dim_style_named(from).clone();
                self.dim_styles.push(NamedDimStyle { name: name.clone(), style });
                self.current_dim_style = Some(name.clone());
            }
            StyleKind::Text => {
                if self.text_styles.len() >= MAX_STYLES {
                    return Err(format!("a drawing has at most {MAX_STYLES} text styles"));
                }
                let style = self
                    .text_style_named(from)
                    .cloned()
                    .unwrap_or_else(|| TextStyle { height: self.dim_style.text, ..TextStyle::default() });
                self.text_styles.push(TextStyle { name: name.clone(), ..style });
                self.current_text_style = Some(name.clone());
            }
        }
        self.sanitize();
        Ok(name)
    }

    /// Renames style `old` of `kind` to `new`: the objects that take it, the current style and
    /// the dimension styles whose texts use it (a text style) follow.
    pub fn rename_style(&mut self, kind: StyleKind, old: &str, new: &str) -> Result<(), String> {
        let new = clean(new)?;
        if old == new {
            return Ok(());
        }
        if old == STANDARD || !self.has_style(kind, old) {
            return Err(format!("no style {old} to rename"));
        }
        if self.has_style(kind, &new) {
            return Err(format!("there is already a style {new}"));
        }
        let rename = |slot: &mut Option<String>| {
            if slot.as_deref() == Some(old) {
                *slot = Some(new.clone());
            }
        };
        match kind {
            StyleKind::Dim => {
                for s in self.dim_styles.iter_mut().filter(|s| s.name == old) {
                    s.name = new.clone();
                }
                rename(&mut self.current_dim_style);
            }
            StyleKind::Text => {
                for s in self.text_styles.iter_mut().filter(|s| s.name == old) {
                    s.name = new.clone();
                }
                rename(&mut self.current_text_style);
                rename(&mut self.dim_style.text_style);
                for s in &mut self.dim_styles {
                    rename(&mut s.style.text_style);
                }
            }
        }
        for e in self.entities.iter_mut().filter(|e| kind.takes(&e.kind)) {
            rename(&mut e.style);
        }
        self.sanitize();
        Ok(())
    }

    /// Deletes style `name` of `kind`: what took it takes Standard. How many objects it had.
    pub fn delete_style(&mut self, kind: StyleKind, name: &str) -> Result<usize, String> {
        if name == STANDARD || !self.has_style(kind, name) {
            return Err(format!("no style {name} to delete"));
        }
        let clear = |slot: &mut Option<String>| {
            if slot.as_deref() == Some(name) {
                *slot = None;
            }
        };
        match kind {
            StyleKind::Dim => {
                self.dim_styles.retain(|s| s.name != name);
                clear(&mut self.current_dim_style);
            }
            StyleKind::Text => {
                self.text_styles.retain(|s| s.name != name);
                clear(&mut self.current_text_style);
                clear(&mut self.dim_style.text_style);
                for s in &mut self.dim_styles {
                    clear(&mut s.style.text_style);
                }
            }
        }
        let mut n = 0;
        for e in self.entities.iter_mut().filter(|e| kind.takes(&e.kind) && e.style.as_deref() == Some(name)) {
            e.style = None;
            n += 1;
        }
        self.sanitize();
        Ok(n)
    }

    /// Makes style `name` of `kind` the one new objects take.
    pub fn set_current_style(&mut self, kind: StyleKind, name: &str) -> Result<(), String> {
        if !self.has_style(kind, name) {
            return Err(format!("no style {name}"));
        }
        let slot = (name != STANDARD).then(|| name.to_string());
        match kind {
            StyleKind::Dim => self.current_dim_style = slot,
            StyleKind::Text => self.current_text_style = slot,
        }
        Ok(())
    }

    /// The dimension style called `name` to change (Standard: the drawing's own); `None` when the
    /// drawing has no such style.
    pub fn dim_style_mut(&mut self, name: &str) -> Option<&mut crate::model::DimStyle> {
        if name == STANDARD {
            return Some(&mut self.dim_style);
        }
        self.dim_styles.iter_mut().find(|s| s.name == name).map(|s| &mut s.style)
    }

    /// The text style called `name` to change; `None` for Standard (it has nothing to change) or
    /// a name the drawing doesn't have.
    pub fn text_style_mut(&mut self, name: &str) -> Option<&mut TextStyle> {
        self.text_styles.iter_mut().find(|s| s.name == name)
    }
}
