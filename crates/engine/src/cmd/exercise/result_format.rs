//! Formatting stored on the question and applied to its result equations.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wordcraft_doc::{Block, ContentControl, ControlLock, InlineObject, Paragraph};

use super::{Layout, Words, items};
use crate::cmd::calc::{CalcOverrides, step_param};
use crate::{CmdError, Session};

pub(super) const TAG: &str = "strix:exercise.resultFormat";
const OPEN: &str = "<strix:resultFormat>";
const CLOSE: &str = "</strix:resultFormat>";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Hash)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Format {
    pub unit: Option<String>,
    pub decimals: Option<u8>,
    pub step: Option<String>,
    pub step_mode: Option<String>,
}

impl Format {
    pub fn apply(&self, opts: &mut strix_calc::Options) {
        CalcOverrides {
            digits: self.decimals,
            count: self.decimals.map(|_| "decimals".into()),
            zeros: self.decimals.map(|_| true),
            step: self.step.clone(),
            step_mode: self.step_mode.clone(),
            ..Default::default()
        }
        .apply_to(opts);
    }

    pub fn statement(&self, ws: &strix_calc::Worksheet, text: &str) -> Option<String> {
        ws.show_result_in(text, self.unit.as_deref())
    }
}

pub(super) fn parse(value: &Value) -> Result<Option<Format>, CmdError> {
    if value.is_null() {
        return Ok(None);
    }
    let mut fields = value.clone();
    let step = if value.get("step").is_some_and(|s| !s.is_null()) { step_param(value)? } else { None };
    if let Some(fields) = fields.as_object_mut() {
        fields.remove("step");
    }
    let mut format: Format = serde_json::from_value(fields).map_err(|e| CmdError::Params(format!("resultFormat: {e}")))?;
    format.step = step;
    format.unit = format.unit.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    let ws = strix_calc::Worksheet::new(Default::default());
    if format.decimals.is_some_and(|n| n > 15)
        || format.step_mode.as_deref().is_some_and(|m| strix_calc::StepMode::from_name(m).is_none())
        || format.unit.as_deref().is_some_and(|u| u.len() > 128 || ws.with_result_unit("x := 1 =", Some(u)).is_none())
    {
        return Err(CmdError::Params("resultFormat needs a valid unit, 0–15 decimals and a direction of nearest, up or down".into()));
    }
    Ok((format.unit.is_some() || format.decimals.is_some() || format.step.is_some() || format.step_mode.is_some()).then_some(format))
}

pub(super) fn read(p: &Paragraph) -> Option<Format> {
    p.objects.iter().find_map(|o| {
        let InlineObject::ControlStart { control } = o else { return None };
        if control.tag != TAG {
            return None;
        }
        let raw = control.extra.iter().find_map(|x| x.strip_prefix(OPEN)?.strip_suffix(CLOSE))?;
        let json = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&");
        serde_json::from_str(&json).ok()
    })
}

/// What `format` asks, as written after the question: ` (in cm, 2 decimali, arrotondato per
/// eccesso a 0,05)`; nothing when it asks for nothing shown.
pub(super) fn hint(format: &Format, w: &Words, comma: bool) -> String {
    let h = &w.result_hint;
    let mut parts = Vec::new();
    if let Some(unit) = &format.unit {
        parts.push(h.unit.replace("{}", unit));
    }
    match format.decimals {
        Some(0) => parts.push(h.no_decimals.to_string()),
        Some(1) => parts.push(h.decimal.replace("{}", "1")),
        Some(n) => parts.push(h.decimals.replace("{}", &n.to_string())),
        None => {}
    }
    let step = format.step.as_deref().filter(|t| *t != "none").map(|t| if comma { t.replace('.', ",") } else { t.to_string() });
    let rounded = match format.step_mode.as_deref() {
        Some("up") => Some(h.rounded_up),
        Some("down") => Some(h.rounded_down),
        _ => step.is_some().then_some(h.rounded),
    };
    if let Some(rounded) = rounded {
        parts.push(match &step {
            Some(step) => format!("{rounded} {}", h.to_step.replace("{}", step)),
            None => rounded.to_string(),
        });
    }
    if parts.is_empty() { String::new() } else { format!(" ({})", parts.join(", ")) }
}

/// Stores `format` on question paragraph `p` in a locked control showing [`hint`], before the
/// question's points; `None` removes it.
pub(super) fn write(p: &mut Paragraph, format: Option<&Format>, w: &Words, comma: bool) -> Result<(), CmdError> {
    if let Some((a, b)) = items::span(p, TAG) {
        p.delete(a, b + wordcraft_doc::para::OBJ.len_utf8()).map_err(|e| CmdError::Failed(e.to_string()))?;
    }
    if let Some(format) = format {
        let json = serde_json::to_string(format).map_err(|e| CmdError::Failed(e.to_string()))?;
        let json = json.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let end = p.text.len().saturating_sub(items::trailing_block_ends(p) * wordcraft_doc::para::OBJ.len_utf8());
        let at = items::span(p, items::TAG_POINTS).map_or(end, |(a, _)| a.min(end));
        let props = p.props_of_char(at.saturating_sub(1)).clone();
        let mut ctl = ContentControl { tag: TAG.into(), lock: ControlLock::SdtContentLocked, ..Default::default() };
        ctl.extra.push(format!("{OPEN}{json}{CLOSE}"));
        p.insert_object(at, InlineObject::ControlStart { control: Box::new(ctl) }, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
        let inner = at + wordcraft_doc::para::OBJ.len_utf8();
        p.insert_object(inner, InlineObject::ControlEnd, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
        items::set_content(p, (at, inner), &hint(format, w, comma));
    }
    p.touch();
    Ok(())
}

/// Every question's result format shown in its words and decimal sign (a document from before
/// the hint, another language, a comma now); whether anything changed.
pub(super) fn sync(s: &mut Session) -> bool {
    let layout: Layout = super::scan(s);
    let comma = super::scores::comma(s);
    let mut changed = false;
    for e in 0..layout.exercises.len() {
        let w = super::exercise_words(s, &layout, e);
        for item in items::items(s, &layout, e) {
            let Some(p) = s.doc.body.get(item.question).and_then(|b| b.as_para()) else { continue };
            let (Some(span), Some(format)) = (items::span(p, TAG), read(p)) else { continue };
            let want = hint(&format, w, comma);
            if p.text.get(span.0 + wordcraft_doc::para::OBJ.len_utf8()..span.1) == Some(want.as_str()) {
                continue;
            }
            let Some(block) = s.doc.body.get_mut(item.question) else { continue };
            let Block::Para(p) = std::sync::Arc::make_mut(block) else { continue };
            changed |= items::set_content(p, span, &want);
        }
    }
    if changed {
        s.touch();
    }
    changed
}

pub(crate) struct Working {
    pub exercise: usize,
    pub quiz: bool,
    pub question: u32,
    pub range: (usize, usize),
    pub format: Option<Format>,
}

pub(crate) fn by_block(s: &Session) -> BTreeMap<usize, Working> {
    let layout: Layout = super::scan(s);
    let mut out = BTreeMap::new();
    for (exercise, ex) in layout.exercises.iter().enumerate() {
        for (k, item) in items::items(s, &layout, exercise).iter().enumerate() {
            let range = if item.settings.mode == items::Mode::Calc {
                item.answer
            } else if !ex.kind.quiz() {
                ex.prompts.get(k).zip(ex.solution_blocks).and_then(|(prompt, (_, end))| {
                    let last = ex.prompts.get(k + 1).map_or(end, |next| next.0.saturating_sub(1));
                    (prompt.1 < last).then_some((prompt.1 + 1, last))
                })
            } else {
                None
            };
            if let Some(range) = range {
                out.insert(
                    range.0,
                    Working {
                        exercise,
                        quiz: ex.kind.quiz(),
                        question: u32::try_from(k + 1).unwrap_or(u32::MAX),
                        range,
                        format: s.doc.body.get(item.question).and_then(|b| b.as_para()).and_then(read),
                    },
                );
            }
        }
    }
    out
}
