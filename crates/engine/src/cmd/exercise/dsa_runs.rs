//! Reversible direct DSA formatting. A hidden character style marks each changed run,
//! so its identity survives editing, run splitting and Word round trips. Mathematical
//! objects keep their own ISO italic formatting; only prose run properties change.
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wordcraft_doc::styles::{Style, StyleKind};
use wordcraft_doc::{Document, StoryRef};

const PREFIX: &str = "StrixDsaRun-";
const NAME: &str = "Strix DSA run ";
const LIMIT: usize = 4096;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Original {
    style: Option<String>,
    italic: Option<bool>,
    bold: Option<bool>,
    tabular: Option<bool>,
}

fn stories(doc: &Document) -> Vec<StoryRef> {
    let mut out = vec![StoryRef::Body];
    out.extend(doc.parts.keys().map(|id| StoryRef::Part(*id)));
    out
}

pub(super) fn restore(doc: &mut Document) {
    let originals: std::collections::HashMap<String, Original> = doc
        .styles
        .styles
        .iter()
        .filter_map(|st| {
            if !st.id.starts_with(PREFIX) {
                return None;
            }
            let raw = st.name.strip_prefix(NAME).filter(|raw| raw.len() <= LIMIT)?;
            Some((st.id.clone(), serde_json::from_str(raw).ok()?))
        })
        .collect();
    if originals.is_empty() {
        return;
    }
    for story in stories(doc) {
        for path in doc.para_paths(story) {
            let Ok(p) = doc.para_mut(story, &path) else { continue };
            let mut changed = false;
            for r in &mut p.runs {
                if let Some(o) = r.props.style.as_ref().and_then(|id| originals.get(id)) {
                    r.props.style = o.style.clone();
                    r.props.italic = o.italic;
                    r.props.bold = o.bold;
                    r.props.tabular_digits = o.tabular;
                    changed = true;
                }
            }
            if changed {
                p.touch();
            }
        }
    }
    Arc::make_mut(&mut doc.styles).styles.retain(|st| !st.id.starts_with(PREFIX));
}

pub(super) fn apply(doc: &mut Document, tabular: bool) {
    let mut originals: Vec<(String, Original)> = Vec::new();
    for story in stories(doc) {
        for path in doc.para_paths(story) {
            let Ok(p) = doc.para_mut(story, &path) else { continue };
            let mut changed = false;
            for r in &mut p.runs {
                if r.props.italic != Some(true) && !(tabular && r.props.tabular_digits == Some(false)) {
                    continue;
                }
                let o = Original { style: r.props.style.clone(), italic: r.props.italic, bold: r.props.bold, tabular: r.props.tabular_digits };
                let id = if let Some((id, _)) = originals.iter().find(|(_, v)| *v == o) {
                    id.clone()
                } else {
                    if originals.len() >= LIMIT {
                        continue;
                    }
                    let id = format!("{PREFIX}{}", originals.len());
                    originals.push((id.clone(), o));
                    id
                };
                r.props.style = Some(id);
                if r.props.italic == Some(true) {
                    r.props.italic = Some(false);
                    r.props.bold = Some(true);
                }
                if tabular {
                    r.props.tabular_digits = Some(true);
                }
                changed = true;
            }
            if changed {
                p.touch();
            }
        }
    }
    let sheet = Arc::make_mut(&mut doc.styles);
    for (id, o) in originals {
        let Ok(json) = serde_json::to_string(&o) else { continue };
        sheet.styles.push(Style {
            id,
            name: format!("{NAME}{json}"),
            kind: StyleKind::Character,
            based_on: o.style,
            hidden: true,
            ..Style::default()
        });
    }
}
