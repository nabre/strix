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
                    // Without the dust first: 0.975 points in hundredths is 97.49999999999999 of
                    // them, which is 97.5 and goes up.
                    clean((round_half_away(clean(self.exact_points(g, max_pts) / m)) * m).clamp(0.0, max_pts))
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

/// Word's default cell margin left and right, points.
const CELL_PAD: f32 = 5.4;
/// Points columns: `[`+whole, fraction, `;`, whole, fraction, `[`; then the grade's whole and
/// fraction. Neighbouring sub-columns share no visible border, so the numbers line up at their
/// decimal separators without lines between them on the page or in print.
const POINT_COLS: usize = 6;
const GRADE_COLS: usize = 2;
const COLS: usize = POINT_COLS + GRADE_COLS;

/// One cell's look: alignment, bold, its left and right margins and whether its left and right
/// borders are drawn (`None`: the table's own).
#[derive(Clone, Copy)]
struct Look {
    align: Align,
    bold: bool,
    italic: bool,
    pad: [f32; 2],
    open: [bool; 2],
}

impl Look {
    fn plain(align: Align) -> Self {
        Look { align, bold: false, italic: false, pad: [CELL_PAD, CELL_PAD], open: [false, false] }
    }
}

fn cell(text: &str, look: Look, hidden: bool, width: f32, span: usize) -> Cell {
    let chr =
        CharProps { bold: look.bold.then_some(true), italic: look.italic.then_some(true), hidden: hidden.then_some(true), ..CharProps::default() };
    let mut para = Paragraph::with_text(if hidden { "" } else { text }, chr);
    para.props.align = Some(look.align);
    para.props.space_after = Some(0.0);
    para.props.space_before = Some(0.0);
    if hidden {
        para.mark.hidden = Some(true);
    }
    let mut c = Cell::empty();
    c.blocks = vec![para_block(para)];
    c.props.width = Some(width);
    c.props.span = span.max(1) as u32;
    c.props.partial_margins = Some([None, Some(look.pad[0]), None, Some(look.pad[1])]);
    if look.open.iter().any(|o| *o) {
        let none = Some(Border::default());
        c.props.borders =
            Some(Borders { left: if look.open[0] { none } else { None }, right: if look.open[1] { none } else { None }, ..Borders::default() });
    }
    c
}

/// `v` as shown, split at its decimal separator: the whole part and the fraction with its
/// separator (`"12,5"` -> `("12", ",5")`).
fn split_number(v: f64, comma: bool) -> (String, String) {
    let t = super::scores::shown(v, comma);
    match t.find([',', '.']) {
        Some(at) => (t.get(..at).unwrap_or_default().to_string(), t.get(at..).unwrap_or_default().to_string()),
        None => (t, String::new()),
    }
}

/// A conservative estimate of `text`'s width at `size` points (sans-serif digits, narrow
/// punctuation), so the table can be as narrow as its content.
fn text_w(text: &str, size: f32, bold: bool) -> f32 {
    let em: f32 = text
        .chars()
        .map(|c| match c {
            '0'..='9' => 0.556,
            '[' | ']' | ';' | ',' | '.' | ' ' | ':' => 0.3,
            '\u{2014}' => 1.0,
            c if c.is_uppercase() => 0.72,
            _ => 0.56,
        })
        .sum();
    em * size * if bold { 1.1 } else { 1.04 } + 0.5
}

/// The texts of one scale row's cells.
fn row_texts(band: &Band, comma: bool) -> Option<[String; COLS]> {
    let (gw, gf) = split_number(band.grade, comma);
    if band.is_empty() {
        return None;
    }
    let (fw, ff) = split_number(band.from, comma);
    let (to_w, to_f, semi, close) = match band.to {
        Some(t) => {
            let (w, f) = split_number(t, comma);
            (w, f, if comma { ";" } else { "," }.to_string(), "[".to_string())
        }
        None => Default::default(),
    };
    let open = if band.to.is_some() { "[" } else { "" };
    Some([format!("{open}{fw}"), ff, semi, to_w, to_f, close, gw, gf])
}

struct HeadWords<'a> {
    points: &'a str,
    grade: &'a str,
    rounded: &'a str,
}

fn build_table(cfg: &Config, max_pts: f64, w: &HeadWords<'_>, size: f32, max_width: f32, comma: bool, hidden: bool) -> Table {
    let entries = cfg.scale(max_pts);
    let size = if size.is_finite() { size.clamp(4.0, 72.0) } else { 11.0 };
    let first_pass = entries.iter().position(|b| b.pass);
    let texts: Vec<Option<[String; COLS]>> = entries.iter().map(|b| row_texts(b, comma)).collect();
    // Each sub-column as wide as its widest text (bold on the pass row) plus its margins.
    let pads: [[f32; 2]; COLS] = [[CELL_PAD, 0.0], [0.0, 0.0], [0.0, 3.0], [0.0, 0.0], [0.0, 0.0], [0.0, CELL_PAD], [CELL_PAD, 0.0], [0.0, CELL_PAD]];
    let mut grid = [0.0f32; COLS];
    for (k, t) in texts.iter().enumerate() {
        let bold = first_pass == Some(k);
        if let Some(t) = t {
            for (c, txt) in t.iter().enumerate() {
                if let Some(g) = grid.get_mut(c) {
                    *g = g.max(text_w(txt, size, bold));
                }
            }
        }
    }
    for (g, p) in grid.iter_mut().zip(pads.iter()) {
        *g += p[0] + p[1];
    }
    // The headings and a dash (a grade no points earn) fit across their columns: the room
    // missing goes to the outer sub-columns.
    let dash = text_w("\u{2014}", size, false) + 2.0 * CELL_PAD;
    let widen = |grid: &mut [f32; COLS], from: usize, to: usize, need: f32| {
        let have: f32 = grid.get(from..to).map(|g| g.iter().sum()).unwrap_or(0.0);
        if need > have {
            let extra = (need - have) / 2.0;
            for c in [from, to.saturating_sub(1)] {
                if let Some(g) = grid.get_mut(c) {
                    *g += extra;
                }
            }
        }
    };
    widen(&mut grid, 0, POINT_COLS, (text_w(w.points, size, true) + 2.0 * CELL_PAD).max(dash));
    widen(&mut grid, POINT_COLS, COLS, (text_w(w.grade, size, true) + 2.0 * CELL_PAD).max(dash));
    // Never wider than the text column.
    let total: f32 = grid.iter().sum();
    let max_width = max_width.max(72.0);
    if total > max_width {
        let k = max_width / total;
        grid.iter_mut().for_each(|g| *g *= k);
    }
    let width: f32 = grid.iter().sum();
    let pts_w: f32 = grid.get(..POINT_COLS).map(|g| g.iter().sum()).unwrap_or(0.0);
    let grade_w: f32 = grid.get(POINT_COLS..).map(|g| g.iter().sum()).unwrap_or(0.0);

    let mut t = Table::new(1, COLS, width);
    t.grid = grid.to_vec();
    t.props.width = Some(width);
    t.props.align = Some(Align::Center);
    if hidden {
        t.props.borders = Some(Borders::all(Border::default()));
        t.rows = vec![Row { cells: vec![cell("", Look::plain(Align::Center), true, width, COLS)], ..Row::default() }];
        return t;
    }
    let head = Look { bold: true, ..Look::plain(Align::Center) };
    // The heading row repeats on every page the table runs onto.
    let mut header =
        Row { cells: vec![cell(w.points, head, false, pts_w, POINT_COLS), cell(w.grade, head, false, grade_w, GRADE_COLS)], ..Row::default() };
    header.props.header = true;
    let mut out_rows = vec![header];
    for (k, t_row) in texts.iter().enumerate() {
        let bold = first_pass == Some(k);
        let grade_txt = entries.get(k).map(|b| split_number(b.grade, comma)).unwrap_or_default();
        let cells = match t_row {
            Some(t_row) => t_row
                .iter()
                .enumerate()
                .map(|(c, txt)| {
                    let align = if matches!(c, 0 | 3 | 6) { Align::Right } else { Align::Left };
                    let pad = pads.get(c).copied().unwrap_or([0.0, 0.0]);
                    let open = [!matches!(c, 0 | 6), !matches!(c, 5 | 7)];
                    cell(txt, Look { align, bold, italic: false, pad, open }, false, grid.get(c).copied().unwrap_or(0.0), 1)
                })
                .collect(),
            None => {
                let g = |c: usize| grid.get(c).copied().unwrap_or(0.0);
                vec![
                    cell("\u{2014}", Look { bold, ..Look::plain(Align::Center) }, false, pts_w, POINT_COLS),
                    cell(&grade_txt.0, Look { align: Align::Right, bold, italic: false, pad: [CELL_PAD, 0.0], open: [false, true] }, false, g(6), 1),
                    cell(&grade_txt.1, Look { align: Align::Left, bold, italic: false, pad: [0.0, CELL_PAD], open: [true, false] }, false, g(7), 1),
                ]
            }
        };
        out_rows.push(Row { cells, ..Row::default() });
    }
    // Under the scale, how finely the points are counted (half points, hundredths…).
    let note = format!("{} {}.", w.rounded, super::scores::shown(cfg.points_multiple(), comma));
    let mut note_cell = cell(&note, Look { italic: true, ..Look::plain(Align::Center) }, false, width, COLS);
    let none = Some(Border::default());
    note_cell.props.borders = Some(Borders { left: none, right: none, bottom: none, ..Borders::default() });
    out_rows.push(Row { cells: vec![note_cell], ..Row::default() });
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
    let dw = doc_words(s);
    let words = HeadWords { points: dw.points, grade: dw.grade_mark, rounded: dw.points_rounded };
    let width = text_width(s, i);
    let size = s.doc.styles.default_chr.size.unwrap_or(11.0);
    let comma = super::scores::comma(s);
    let Some(Block::Table(old)) = s.doc.body.get(i).map(|b| &**b) else { return false };
    let old_cfg = control_of(old).map(read_config);
    let mut new = build_table(&cfg, max_pts, &words, size, width, comma, hide);
    let same_header = old.rows.first().map(|r| r.props.header) == new.rows.first().map(|r| r.props.header);
    if old_cfg.as_ref() == Some(&cfg)
        && shape(old) == shape(&new)
        && old.grid == new.grid
        && old.props.width == new.props.width
        && old.props.align == new.props.align
        && old.props.borders == new.props.borders
        && same_header
    {
        return false;
    }
    new.controls = old.controls.clone();
    let keep_borders = new.props.borders;
    let (width, align) = (new.props.width, new.props.align);
    new.props = old.props.clone();
    // As narrow as its content, centred.
    new.props.width = width;
    new.props.width_pct = None;
    new.props.align = align;
    new.props.indent = None;
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

#[cfg(test)]
mod tests {
    use super::{Config, Curve, Preset};

    /// `a/b` rounded half away from zero, in whole numbers.
    fn div_round(a: i128, b: i128) -> i128 {
        (2 * a + b).div_euclid(2 * b)
    }

    /// The points of every grade are the curve's exact ones rounded to the multiple, a half
    /// going up, whatever dust the floating-point arithmetic leaves: checked against whole-number
    /// arithmetic for every preset, maximum (whole and half points) and usual multiple.
    #[test]
    fn points_are_rounded_from_the_exact_curve() {
        let mut wrong = Vec::new();
        for preset in [Preset::It10, Preset::Ch6, Preset::De6, Preset::Fr20, Preset::Percent] {
            for round in [None, Some(0.1), Some(0.25), Some(0.5), Some(1.0)] {
                for max2 in 2..=240_i128 {
                    let mut cfg = Config::from_preset(preset);
                    cfg.points_round = round;
                    let max = max2 as f64 / 2.0;
                    // Everything in units of 1/400: grades (steps of 0.25 or more), the
                    // multiples (hundredths of 0.01 to 1) and the pass ratio (0.5 or 0.6 = 240/400).
                    let u = |v: f64| (v * 400.0).round() as i128;
                    let (lo, hi, pass, m) = (u(cfg.min_grade), u(cfg.max_grade), u(cfg.pass_grade), u(cfg.points_multiple()));
                    let ratio = u(cfg.pass_ratio);
                    let scale = cfg.scale(max);
                    let last = scale.len() - 1;
                    for (k, band) in scale.iter().enumerate() {
                        if k == 0 || k == last {
                            continue;
                        }
                        let g = u(band.grade);
                        // The exact points as a fraction `num / den` of a point.
                        let (num, den) = match cfg.curve {
                            Curve::Linear => (max2 * (g - lo), 2 * (hi - lo)),
                            Curve::PiecewiseLinear => {
                                let below = if hi >= lo { g < pass } else { g > pass };
                                if below {
                                    (max2 * ratio * (g - lo), 2 * 400 * (pass - lo))
                                } else {
                                    (max2 * ratio * (hi - pass) + max2 * (400 - ratio) * (g - pass), 2 * 400 * (hi - pass))
                                }
                            }
                        };
                        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
                        // In multiples of `m/400` points: `num/den / (m/400)`.
                        let want = div_round(num * 400, den * m) as f64 * cfg.points_multiple();
                        let want = want.min(max);
                        if (band.from - want).abs() > 1e-6 {
                            wrong.push(format!("{preset:?} max {max} round {round:?} grade {}: {} not {want}", band.grade, band.from));
                        }
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{} wrong, e.g. {:?}", wrong.len(), wrong.iter().take(8).collect::<Vec<_>>());
    }
}
