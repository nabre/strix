//! Automatic grading scale table (`strix:grading`, `exercise.grading`): one row per grade of the
//! grid (lowest to highest grade by `step`), each with the points that earn it as a half-open
//! interval `[from; to[` (the next grade's points excluded; the highest grade at full points), the
//! points rounded to a multiple (`pointsRound`), bound dynamically to the grand total of `strix:scores` (`maxPoints: null`) or an
//! explicit maximum, with national presets (`it-10`, `ch-6`, `de-6`, `fr-20`, `percent`, `custom`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::props::{Align, Border, Borders, CharProps};
use wordcraft_doc::table::{Cell, Row, Table};
use wordcraft_doc::{Block, Paragraph, StoryRef, para_block};

use super::{doc_words, scan};
use crate::{CmdError, CmdResult, Session, p};

pub const TAG_GRADING: &str = "strix:grading";
const EL: &str = "strix:gradingTable";
const MAX_ROWS_SHOWN: usize = 101;
/// Points rounded to hundredths when no multiple is set.
const DEFAULT_POINTS_ROUND: f64 = 0.01;
const MAX_POINTS_CAP: f64 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Preset {
    #[default]
    #[serde(rename = "it-10")]
    It10,
    #[serde(rename = "ch-6")]
    Ch6,
    #[serde(rename = "de-6")]
    De6,
    #[serde(rename = "fr-20")]
    Fr20,
    #[serde(rename = "percent")]
    Percent,
    #[serde(rename = "custom")]
    Custom,
}

impl Preset {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "it-10" => Self::It10,
            "ch-6" => Self::Ch6,
            "de-6" => Self::De6,
            "fr-20" => Self::Fr20,
            "percent" => Self::Percent,
            "custom" => Self::Custom,
            _ => return None,
        })
    }

    fn for_lang(lang: &str) -> Self {
        let lower = lang.trim().to_ascii_lowercase();
        if lower.starts_with("it") {
            Self::It10
        } else if lower == "de-ch" || lower.starts_with("gsw") || lower.ends_with("-ch") {
            Self::Ch6
        } else if lower.starts_with("de") {
            Self::De6
        } else if lower.starts_with("fr") {
            Self::Fr20
        } else {
            Self::Percent
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    Linear,
    #[default]
    PiecewiseLinear,
}

impl Curve {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "linear" => Self::Linear,
            "piecewiseLinear" | "piecewise" => Self::PiecewiseLinear,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    #[default]
    Teacher,
    Both,
}

impl Visibility {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "teacher" => Self::Teacher,
            "both" => Self::Both,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub preset: Preset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_points: Option<f64>,
    pub min_grade: f64,
    pub max_grade: f64,
    pub pass_ratio: f64,
    pub pass_grade: f64,
    pub curve: Curve,
    pub step: f64,
    /// The multiple the points of each grade are rounded to (`None`: hundredths).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points_round: Option<f64>,
    pub visibility: Visibility,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_preset(Preset::It10)
    }
}

impl Config {
    pub fn from_preset(preset: Preset) -> Self {
        match preset {
            Preset::It10 => Self {
                preset,
                max_points: None,
                min_grade: 1.0,
                max_grade: 10.0,
                pass_ratio: 0.6,
                pass_grade: 6.0,
                curve: Curve::PiecewiseLinear,
                step: 0.5,
                points_round: None,
                visibility: Visibility::Teacher,
            },
            Preset::Ch6 => Self {
                preset,
                max_points: None,
                min_grade: 1.0,
                max_grade: 6.0,
                pass_ratio: 0.6,
                pass_grade: 4.0,
                curve: Curve::Linear,
                step: 0.5,
                points_round: None,
                visibility: Visibility::Teacher,
            },
            Preset::De6 => Self {
                preset,
                max_points: None,
                min_grade: 6.0,
                max_grade: 1.0,
                pass_ratio: 0.5,
                pass_grade: 4.0,
                curve: Curve::PiecewiseLinear,
                step: 0.25,
                points_round: None,
                visibility: Visibility::Teacher,
            },
            Preset::Fr20 => Self {
                preset,
                max_points: None,
                min_grade: 0.0,
                max_grade: 20.0,
                pass_ratio: 0.5,
                pass_grade: 10.0,
                curve: Curve::Linear,
                step: 0.5,
                points_round: None,
                visibility: Visibility::Teacher,
            },
            Preset::Percent | Preset::Custom => Self {
                preset,
                max_points: None,
                min_grade: 0.0,
                max_grade: 100.0,
                pass_ratio: 0.6,
                pass_grade: 60.0,
                curve: Curve::Linear,
                step: 1.0,
                points_round: None,
                visibility: Visibility::Teacher,
            },
        }
    }

    pub fn sanitize(&mut self) {
        let fin = |v: f64, d: f64| if v.is_finite() { v } else { d };
        self.max_points = self.max_points.filter(|v| v.is_finite() && *v > 0.0).map(|v| v.clamp(0.5, MAX_POINTS_CAP));
        self.min_grade = fin(self.min_grade, 1.0).clamp(-1000.0, 1000.0);
        self.max_grade = fin(self.max_grade, 10.0).clamp(-1000.0, 1000.0);
        if (self.max_grade - self.min_grade).abs() < 1e-6 {
            self.max_grade = self.min_grade + 1.0;
        }
        self.pass_ratio = fin(self.pass_ratio, 0.6).clamp(0.05, 0.95);
        let (lo, hi) = (self.min_grade.min(self.max_grade), self.min_grade.max(self.max_grade));
        self.pass_grade = fin(self.pass_grade, 0.5 * (lo + hi)).clamp(lo, hi);
        self.step = fin(self.step, 0.5).clamp(0.01, 100.0);
        self.points_round = self.points_round.filter(|v| v.is_finite() && *v > 0.0).map(|v| v.clamp(DEFAULT_POINTS_ROUND, 1000.0));
    }

    pub fn is_passing(&self, grade: f64) -> bool {
        if self.max_grade >= self.min_grade { grade + 1e-9 >= self.pass_grade } else { grade - 1e-9 <= self.pass_grade }
    }

    /// The multiple the points are rounded to.
    pub fn points_multiple(&self) -> f64 {
        self.points_round.unwrap_or(DEFAULT_POINTS_ROUND)
    }

    /// The grid of grades, lowest to highest by `step` (the highest always last), the step widened
    /// to a multiple of itself when the grid would pass `MAX_ROWS_SHOWN` rows.
    fn grades(&self) -> Vec<f64> {
        let span = (self.max_grade - self.min_grade).abs();
        let dir = if self.max_grade >= self.min_grade { 1.0 } else { -1.0 };
        let mut step = self.step.max(0.01);
        let n = (span / step + 1e-9).floor();
        if n > (MAX_ROWS_SHOWN - 1) as f64 {
            step *= (n / (MAX_ROWS_SHOWN - 1) as f64).ceil();
        }
        let n = ((span / step + 1e-9).floor() as usize).min(MAX_ROWS_SHOWN - 1);
        let mut out: Vec<f64> = (0..=n).map(|k| clean(self.min_grade + dir * step * k as f64)).collect();
        if out.last().is_some_and(|g| (g - self.max_grade).abs() > 1e-6) {
            if out.len() >= MAX_ROWS_SHOWN {
                out.pop();
            }
            out.push(self.max_grade);
        }
        out
    }

    /// The exact points at which the curve reaches `grade` (out of `max_pts`), before rounding.
    fn exact_points(&self, grade: f64, max_pts: f64) -> f64 {
        let ratio = |a: f64, b: f64| if (b - a).abs() < 1e-12 { 1.0 } else { ((grade - a) / (b - a)).clamp(0.0, 1.0) };
        let x = match self.curve {
            Curve::Linear => max_pts * ratio(self.min_grade, self.max_grade),
            Curve::PiecewiseLinear => {
                let pass_pts = (self.pass_ratio * max_pts).clamp(0.0, max_pts);
                let below = if self.max_grade >= self.min_grade { grade < self.pass_grade } else { grade > self.pass_grade };
                if below {
                    pass_pts * ratio(self.min_grade, self.pass_grade)
                } else {
                    pass_pts
                        + (max_pts - pass_pts)
                            * if (self.max_grade - self.pass_grade).abs() < 1e-12 { 0.0 } else { ratio(self.pass_grade, self.max_grade) }
                }
            }
        };
        x.clamp(0.0, max_pts)
    }

    /// One band per grade of the grid: `(grade, from, to, is_passing)`, the points from `from`
    /// (included) to `to` (excluded; `None` for the highest grade, earned with full points),
    /// rounded to `points_multiple`. The lowest grade starts at 0.
    pub fn scale(&self, max_pts: f64) -> Vec<Band> {
        let max_pts = max_pts.clamp(0.5, MAX_POINTS_CAP);
        let m = self.points_multiple();
        let grades = self.grades();
        let last = grades.len().saturating_sub(1);
        let from: Vec<f64> = grades
            .iter()
            .enumerate()
            .map(|(k, &g)| {
                if k == 0 {
                    0.0
                } else if k == last {
                    max_pts
                } else {
                    clean((round_half_away(self.exact_points(g, max_pts) / m) * m).clamp(0.0, max_pts))
                }
            })
            .collect();
        grades
            .iter()
            .enumerate()
            .map(|(k, &grade)| Band { grade, from: from.get(k).copied().unwrap_or(0.0), to: from.get(k + 1).copied(), pass: self.is_passing(grade) })
            .collect()
    }
}

/// One row of the scale: `grade` for points in `[from; to[` (`to: None`: exactly `from`, full points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub grade: f64,
    pub from: f64,
    pub to: Option<f64>,
    pub pass: bool,
}

impl Band {
    /// No points earn this grade (rounding gave it the same points as the next one).
    pub fn is_empty(&self) -> bool {
        self.to.is_some_and(|t| t <= self.from + 1e-9)
    }

    /// The points column: `[from; to[` (`;` between the ends with decimal commas, else `,`), the
    /// full points alone on the highest grade, a dash when no points earn the grade.
    pub fn points_text(&self, comma: bool) -> String {
        let sep = if comma { "; " } else { ", " };
        match self.to {
            _ if self.is_empty() => "\u{2014}".to_string(),
            Some(t) => format!("[{}{sep}{}[", super::scores::shown(self.from, comma), super::scores::shown(t, comma)),
            None => super::scores::shown(self.from, comma),
        }
    }
}

/// `v` without floating-point dust (to a millionth) and without `-0`.
fn clean(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r.abs() < 1e-12 { 0.0 } else { r }
}

fn round_half_away(v: f64) -> f64 {
    if v >= 0.0 { (v + 0.5).floor() } else { -((-v + 0.5).floor()) }
}

fn control_of(t: &Table) -> Option<&ContentControl> {
    t.controls.open.iter().find(|c| c.tag == TAG_GRADING)
}

fn read_config(ctl: &ContentControl) -> Config {
    let open = format!("<{EL}>");
    let close = format!("</{EL}>");
    let Some(raw) = ctl.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close)) else { return Config::default() };
    let json = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&");
    let mut cfg: Config = serde_json::from_str(&json).unwrap_or_default();
    cfg.sanitize();
    cfg
}

fn write_config(ctl: &mut ContentControl, cfg: &Config) {
    let open = format!("<{EL}>");
    ctl.extra.retain(|x| !x.starts_with(&open));
    let json = serde_json::to_string(cfg).unwrap_or_default();
    let escaped = json.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    ctl.extra.push(format!("{open}{escaped}</{EL}>"));
}

pub(crate) fn tables(s: &Session) -> Vec<usize> {
    s.doc.body.iter().enumerate().filter(|(_, b)| matches!(&***b, Block::Table(t) if control_of(t).is_some())).map(|(i, _)| i).collect()
}

/// Resolves the maximum points for `cfg` in `s`: explicit `max_points`, else `scores::grand_total`,
/// else the sum of all exercises' question points, else `10.0`.
pub(crate) fn resolved_max_points(s: &Session, cfg: &Config) -> f64 {
    if let Some(m) = cfg.max_points {
        return m;
    }
    if let Some(tot) = super::scores::grand_total(s).filter(|&v| v > 0.0) {
        return tot;
    }
    let layout = scan(s);
    let ex_sum: f64 = (0..layout.exercises.len()).filter_map(|e| super::items::total(s, &layout, e)).sum();
    if ex_sum > 0.0 && ex_sum.is_finite() { ex_sum } else { 10.0 }
}

fn should_hide(s: &Session, cfg: &Config) -> bool {
    if cfg.visibility == Visibility::Both {
        return false;
    }
    let layout = scan(s);
    !layout.exercises.is_empty() && layout.exercises.iter().all(|e| !e.show.solution)
}

fn cell(text: &str, align: Align, bold: bool, hidden: bool, width: f32) -> Cell {
    let chr = CharProps { bold: bold.then_some(true), hidden: hidden.then_some(true), ..CharProps::default() };
    let mut para = Paragraph::with_text(if hidden { "" } else { text }, chr);
    para.props.align = Some(align);
    para.props.space_after = Some(0.0);
    para.props.space_before = Some(0.0);
    if hidden {
        para.mark.hidden = Some(true);
    }
    let mut c = Cell::empty();
    c.blocks = vec![para_block(para)];
    c.props.width = Some(width);
    c
}

fn build_table(cfg: &Config, max_pts: f64, points_word: &str, grade_word: &str, width: f32, comma: bool, hidden: bool) -> Table {
    let entries = cfg.scale(max_pts);
    // One pair of columns: laying the scale out side by side is the person's choice (page
    // columns), never automatic.
    let pairs = 1usize;
    let rows_per_col = entries.len().div_ceil(pairs).max(1);
    let pair_w = (width / pairs as f32).max(72.0);
    let col_w = pair_w * 0.5;
    let mut grid = Vec::with_capacity(pairs * 2);
    for _ in 0..pairs {
        grid.push(col_w);
        grid.push(col_w);
    }
    let mut t = Table::new(1, grid.len(), width);
    t.grid = grid;
    if hidden {
        t.props.borders = Some(Borders::all(Border::default()));
        t.rows = vec![Row { cells: (0..pairs * 2).map(|_| cell("", Align::Center, false, true, col_w)).collect(), ..Row::default() }];
        return t;
    }
    let mut header_cells = Vec::with_capacity(pairs * 2);
    for _ in 0..pairs {
        header_cells.push(cell(points_word, Align::Center, true, false, col_w));
        header_cells.push(cell(grade_word, Align::Center, true, false, col_w));
    }
    // The heading row repeats on every page the table runs onto.
    let mut header = Row { cells: header_cells, ..Row::default() };
    header.props.header = true;
    let mut out_rows = vec![header];
    // Find first passing entry index to highlight the sufficiency threshold row.
    let first_pass = entries.iter().position(|b| b.pass);
    for r in 0..rows_per_col {
        let mut row_cells = Vec::with_capacity(pairs * 2);
        for p_idx in 0..pairs {
            let idx = p_idx * rows_per_col + r;
            if let Some(band) = entries.get(idx) {
                let bold = first_pass == Some(idx);
                row_cells.push(cell(&band.points_text(comma), Align::Center, bold, false, col_w));
                row_cells.push(cell(&super::scores::shown(band.grade, comma), Align::Center, bold, false, col_w));
            } else {
                row_cells.push(cell("", Align::Right, false, false, col_w));
                row_cells.push(cell("", Align::Right, false, false, col_w));
            }
        }
        out_rows.push(Row { cells: row_cells, ..Row::default() });
    }
    t.rows = out_rows;
    t
}

fn text_width(s: &Session, i: usize) -> f32 {
    s.doc.sections().into_iter().find(|(end, _)| *end >= i).map_or_else(|| s.doc.last_section.text_width(), |(_, sp)| sp.text_width())
}

fn shape(t: &Table) -> Vec<Vec<(String, Option<bool>)>> {
    t.rows
        .iter()
        .map(|r| {
            r.cells
                .iter()
                .map(|c| {
                    let p = c.blocks.first().and_then(|b| b.as_para());
                    let txt = p.map(super::plain).unwrap_or_default();
                    let hid = p.and_then(|p| p.mark.hidden);
                    (txt, hid)
                })
                .collect()
        })
        .collect()
}

pub(super) fn fingerprint(s: &Session) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for i in tables(s) {
        if let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b)
            && let Some(cfg) = control_of(t).map(read_config)
        {
            let json = serde_json::to_string(&cfg).unwrap_or_default();
            (i, json, should_hide(s, &cfg), resolved_max_points(s, &cfg).to_bits()).hash(&mut h);
        }
    }
    h.finish()
}

fn put(s: &mut Session, i: usize, cfg: &Config) -> bool {
    let mut cfg = cfg.clone();
    cfg.sanitize();
    let max_pts = resolved_max_points(s, &cfg);
    let hide = should_hide(s, &cfg);
    let w = doc_words(s);
    let width = text_width(s, i);
    let comma = super::scores::comma(s);
    let Some(Block::Table(old)) = s.doc.body.get(i).map(|b| &**b) else { return false };
    let old_cfg = control_of(old).map(read_config);
    let mut new = build_table(&cfg, max_pts, w.points, w.grade_mark, width, comma, hide);
    let same_header = old.rows.first().map(|r| r.props.header) == new.rows.first().map(|r| r.props.header);
    if old_cfg.as_ref() == Some(&cfg)
        && shape(old) == shape(&new)
        && old.grid.len() == new.grid.len()
        && old.props.borders == new.props.borders
        && same_header
    {
        return false;
    }
    new.controls = old.controls.clone();
    let keep_borders = new.props.borders;
    new.props = old.props.clone();
    new.props.borders = if hide { keep_borders } else { Some(Borders::all(Border::single(0.5))) };
    if let Some(ctl) = new.controls.open.iter_mut().find(|c| c.tag == TAG_GRADING) {
        write_config(ctl, &cfg);
    }
    if let Some(b) = s.doc.body.get_mut(i) {
        *b = Arc::new(Block::Table(new));
    }
    true
}

/// Rebuilds all `strix:grading` tables in `s` when scores, language or `Show.solution` change.
pub(crate) fn update(s: &mut Session) -> bool {
    let mut changed = false;
    for i in tables(s) {
        let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b) else { continue };
        let Some(ctl) = control_of(t) else { continue };
        let cfg = read_config(ctl);
        changed |= put(s, i, &cfg);
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

fn target(s: &Session) -> Option<usize> {
    let all = tables(s);
    let at = (s.sel.focus.story == StoryRef::Body).then(|| s.sel.focus.path.0.first().map(|&i| i as usize)).flatten();
    at.filter(|i| all.contains(i)).or_else(|| all.first().copied())
}

fn state(s: &Session) -> Value {
    let Some(i) = target(s) else { return json!({"on": false}) };
    let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b) else { return json!({"on": false}) };
    let Some(ctl) = control_of(t) else { return json!({"on": false}) };
    let cfg = read_config(ctl);
    let max_pts = resolved_max_points(s, &cfg);
    let comma = super::scores::comma(s);
    let scale: Vec<Value> = cfg
        .scale(max_pts)
        .into_iter()
        .map(|b| json!({"grade": b.grade, "points": b.from, "from": b.from, "to": b.to, "pass": b.pass, "empty": b.is_empty(), "text": b.points_text(comma)}))
        .collect();
    json!({
        "on": true,
        "preset": cfg.preset,
        "maxPoints": cfg.max_points,
        "effectiveMaxPoints": max_pts,
        "minGrade": cfg.min_grade,
        "maxGrade": cfg.max_grade,
        "passRatio": cfg.pass_ratio,
        "passGrade": cfg.pass_grade,
        "curve": cfg.curve,
        "step": cfg.step,
        "pointsRound": cfg.points_multiple(),
        "visibility": cfg.visibility,
        "hidden": should_hide(s, &cfg),
        "scale": scale,
    })
}

/// Inserts a grading table at the end of `s.doc.body` (or after the caret when outside controls).
pub(crate) fn insert_at_caret_or_end(s: &mut Session, cfg: &Config, at_end: bool) -> Result<usize, CmdError> {
    let i = if at_end {
        s.doc.body.len()
    } else {
        let at = s.sel.focus.clone();
        if at.story != StoryRef::Body || at.path.depth() != 0 || s.doc.control_at(&at).is_some() {
            // Fall back cleanly to after any outer control or before results summary.
            super::results_start(s).unwrap_or(s.doc.body.len())
        } else {
            (at.path.0.first().map_or(0, |&idx| idx as usize) + 1).min(s.doc.body.len())
        }
    };
    let w = doc_words(s);
    let mut ctl = ContentControl::new(ControlKind::RichText);
    ctl.tag = TAG_GRADING.into();
    ctl.title = w.grading_scale.into();
    ctl.block = true;
    ctl.lock = ControlLock::SdtLocked;
    write_config(&mut ctl, cfg);
    let mut t = Table::new(1, 2, text_width(s, i.saturating_sub(1)));
    t.controls.open.push(ctl);
    t.controls.close = 1;
    s.doc.body.insert(i, Arc::new(Block::Table(t)));
    if !matches!(s.doc.body.get(i + 1).map(|b| &**b), Some(Block::Para(_))) {
        s.doc.body.insert(i + 1, para_block(Paragraph::new()));
    }
    put(s, i, cfg);
    s.touch();
    Ok(i)
}

pub(crate) fn default_config_for_session(s: &Session) -> Config {
    Config::from_preset(Preset::for_lang(&super::words_lang(s)))
}

/// `exercise.grading`: inserts, removes, configures or reads the automatic grading scale table.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    match p::bool(v, "on") {
        Some(true) if target(s).is_none() => {
            let mut cfg = default_config_for_session(s);
            apply_params(&mut cfg, v)?;
            insert_at_caret_or_end(s, &cfg, false)?;
            s.clamp_selection();
            return Ok(state(s));
        }
        Some(false) => {
            if let Some(i) = target(s) {
                s.doc.body.remove(i);
                s.doc.ensure_nonempty();
                s.touch();
                s.clamp_selection();
            }
            return Ok(state(s));
        }
        _ => {}
    }
    let edits = ["preset", "maxPoints", "minGrade", "maxGrade", "passRatio", "passGrade", "curve", "step", "pointsRound", "visibility"]
        .iter()
        .any(|k| v.get(*k).is_some());
    if !edits {
        return Ok(state(s));
    }
    let i = match target(s) {
        Some(i) => i,
        None => {
            let mut cfg = default_config_for_session(s);
            apply_params(&mut cfg, v)?;
            insert_at_caret_or_end(s, &cfg, false)?;
            s.clamp_selection();
            return Ok(state(s));
        }
    };
    let Some(Block::Table(t)) = s.doc.body.get(i).map(|b| &**b) else { return Err(CmdError::Failed("no grading table".into())) };
    let mut cfg = control_of(t).map(read_config).unwrap_or_else(|| default_config_for_session(s));
    apply_params(&mut cfg, v)?;
    put(s, i, &cfg);
    s.touch();
    s.clamp_selection();
    Ok(state(s))
}

fn apply_params(cfg: &mut Config, v: &Value) -> Result<(), CmdError> {
    if let Some(pr) = p::str(v, "preset") {
        let parsed = Preset::parse(pr).ok_or_else(|| CmdError::Params(format!("unknown preset `{pr}`")))?;
        let keep_max = cfg.max_points;
        let keep_vis = cfg.visibility;
        *cfg = Config::from_preset(parsed);
        cfg.max_points = keep_max;
        cfg.visibility = keep_vis;
    }
    if let Some(mp) = v.get("maxPoints") {
        cfg.max_points = match mp {
            Value::Null => None,
            x => Some(x.as_f64().filter(|n| n.is_finite() && *n > 0.0).ok_or_else(|| CmdError::Params("maxPoints: positive number or null".into()))?),
        };
    }
    for (key, manual) in [("minGrade", true), ("maxGrade", true), ("passRatio", false), ("passGrade", false), ("step", false)] {
        if let Some(val) = v.get(key) {
            let num = val.as_f64().filter(|n| n.is_finite()).ok_or_else(|| CmdError::Params(format!("{key}: finite number required")))?;
            match key {
                "minGrade" => cfg.min_grade = num,
                "maxGrade" => cfg.max_grade = num,
                "passRatio" => cfg.pass_ratio = num,
                "passGrade" => cfg.pass_grade = num,
                "step" => cfg.step = num,
                _ => {}
            }
            if manual && v.get("preset").is_none() {
                cfg.preset = Preset::Custom;
            }
        }
    }
    if let Some(ps) = v.get("pointsRound") {
        cfg.points_round = match ps {
            Value::Null => None,
            x => Some(
                x.as_f64().filter(|n| n.is_finite() && *n > 0.0).ok_or_else(|| CmdError::Params("pointsRound: positive number or null".into()))?,
            ),
        };
    }
    if let Some(cv) = p::str(v, "curve") {
        cfg.curve = Curve::parse(cv).ok_or_else(|| CmdError::Params(format!("unknown curve `{cv}`")))?;
    }
    if let Some(vis) = p::str(v, "visibility") {
        cfg.visibility = Visibility::parse(vis).ok_or_else(|| CmdError::Params(format!("unknown visibility `{vis}`")))?;
    }
    cfg.sanitize();
    Ok(())
}
