//! Assessment pre-flight diagnostics (`F9` — `exercise.check`).
//!
//! Pure diagnostic command inspecting all variants (`A..Z`) across exercises, scores, subdocuments,
//! durations and laid-out student/teacher pages without mutating the active `Session`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wordcraft_doc::graph_paper::PT_PER_MM;
use wordcraft_doc::{Block, InlineObject, Path, Pos, StoryRef};

use super::items::{Mode, STYLE_CHOICE, TAG_LINE, choice_text, choices, items, span};
use super::{Exercise, Layout, SUBDOC_TAG, body_path, plain, scan};
use crate::{CmdResult, Session};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckIssue {
    pub kind: &'static str,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exercise: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub pos: Pos,
}

fn body_pos(s: &Session, block_idx: usize) -> Pos {
    s.doc.clamp(&Pos::new(StoryRef::Body, body_path(block_idx), 0))
}

fn variant_statements_at(ex: &Exercise, v_idx: usize) -> Vec<String> {
    ex.data
        .items
        .iter()
        .filter_map(|i| {
            let val = i.values.get(v_idx)?;
            (!i.symbol.is_empty() && !val.is_empty()).then(|| format!("{} = {val} {}", i.symbol, i.unit).trim().to_string())
        })
        .collect()
}

fn check_subdocuments(s: &Session, out: &mut Vec<CheckIssue>) {
    for r in s.doc.control_ranges(StoryRef::Body) {
        if r.depth != 0 || !r.control.block {
            continue;
        }
        let Some(rest) = r.control.tag.strip_prefix(SUBDOC_TAG).and_then(|t| t.strip_prefix(' ')) else {
            continue;
        };
        let rest = rest.strip_prefix("conflict ").unwrap_or(rest);
        let Some(path) = rest.strip_prefix("path=").filter(|p| !p.is_empty()) else { continue };
        #[cfg(not(target_arch = "wasm32"))]
        if !crate::io::readable_file_exists(std::path::Path::new(path)) {
            out.push(CheckIssue {
                kind: "error",
                code: "missing_subdocument",
                message: format!("Missing linked subdocument file: {path}"),
                exercise: None,
                variant: None,
                pos: s.doc.clamp(&r.start),
            });
        }
        #[cfg(target_arch = "wasm32")]
        let _ = path;
    }
}

fn symbol_mentioned(text: &str, symbol: &str) -> bool {
    if symbol.is_empty() {
        return true;
    }
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
    for (idx, _) in text.match_indices(symbol) {
        let before_ok = text.get(..idx).and_then(|h| h.chars().next_back()).is_none_or(|c| !is_ident(c));
        let after_idx = idx + symbol.len();
        let after_ok = text.get(after_idx..).and_then(|t| t.chars().next()).is_none_or(|c| !is_ident(c));
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn check_exercise_variants_and_choices(s: &Session, layout: &Layout, e_idx: usize, ex: &Exercise, out: &mut Vec<CheckIssue>) {
    let v_names: Vec<String> = if ex.data.variants.is_empty() { vec!["A".to_string()] } else { ex.data.variants.clone() };
    let single_unvaried = ex.data.variants.len() <= 1;
    let its = items(s, layout, e_idx);

    for (q_idx, it) in its.iter().enumerate() {
        if it.settings.mode == Mode::Choice
            && let Some((a, b)) = it.answer
        {
            let cs = choices(s, (a, b));
            if !cs.iter().any(|(_, cor)| *cor) {
                out.push(CheckIssue {
                    kind: "error",
                    code: "choice_no_correct",
                    message: format!("Exercise {}, question {}: multiple-choice question has no correct option marked.", e_idx + 1, q_idx + 1),
                    exercise: Some(e_idx + 1),
                    variant: None,
                    pos: body_pos(s, it.question),
                });
            }
        }
    }

    for (vi, v_name) in v_names.iter().enumerate() {
        let stmts = variant_statements_at(ex, vi);
        let eval = crate::cmd::calc::eval_exercise_variant(s, e_idx, &stmts);
        let v_tag = (!single_unvaried).then(|| v_name.clone());
        for (top, msg) in eval.errors {
            out.push(CheckIssue {
                kind: "error",
                code: "calc_or_cad_error",
                message: match &v_tag {
                    Some(vn) => format!("Exercise {} (variant {vn}): {msg}", e_idx + 1),
                    None => format!("Exercise {}: {msg}", e_idx + 1),
                },
                exercise: Some(e_idx + 1),
                variant: v_tag.clone(),
                pos: body_pos(s, top),
            });
        }
        for (q_idx, it) in its.iter().enumerate() {
            if it.settings.mode != Mode::Choice {
                continue;
            }
            let Some((a, b)) = it.answer else { continue };
            let mut seen = BTreeSet::new();
            for bi in a..=b {
                let Some(p) = s.doc.body.get(bi).and_then(|bl| bl.as_para()) else { continue };
                if p.props.style.as_deref() != Some(STYLE_CHOICE) {
                    continue;
                }
                let rendered = super::expr::render_choice_with_sheet(p, &eval.ws);
                if !rendered.is_empty() && !seen.insert(rendered.clone()) {
                    out.push(CheckIssue {
                        kind: "error",
                        code: "choice_duplicate_option",
                        message: match &v_tag {
                            Some(vn) => format!("Exercise {} (variant {vn}), question {}: duplicate option '{}'.", e_idx + 1, q_idx + 1, rendered),
                            None => format!("Exercise {}, question {}: duplicate option '{}'.", e_idx + 1, q_idx + 1, rendered),
                        },
                        exercise: Some(e_idx + 1),
                        variant: v_tag.clone(),
                        pos: body_pos(s, bi),
                    });
                    break;
                }
            }
        }
    }
}

fn check_unused_data(s: &Session, layout: &Layout, e_idx: usize, ex: &Exercise, out: &mut Vec<CheckIssue>) {
    if ex.data.items.is_empty() {
        return;
    }
    let mut texts = crate::cmd::calc::exercise_formula_texts(s, e_idx);
    // Also scan question/solution equations & inline exprs already collected, plus any `{{sym}}`
    for (bi, &(owner, _)) in layout.parts.iter().enumerate() {
        if owner != Some(e_idx) {
            continue;
        }
        if Some(bi) >= ex.data_blocks.map(|(a, _)| a) && Some(bi) <= ex.data_blocks.map(|(_, b)| b) {
            continue;
        }
        if let Some(p) = s.doc.body.get(bi).and_then(|bl| bl.as_para())
            && p.text.contains("{{")
        {
            texts.push(p.text.clone());
        }
    }
    let data_pos = body_pos(s, ex.data_blocks.map_or(ex.start, |(a, _)| a));
    for item in &ex.data.items {
        if item.symbol.is_empty() {
            continue;
        }
        if !texts.iter().any(|t| symbol_mentioned(t, &item.symbol)) {
            out.push(CheckIssue {
                kind: "warning",
                code: "unused_initial_datum",
                message: format!("Exercise {}: initial datum '{}' is never referenced in calculations or drawings.", e_idx + 1, item.symbol),
                exercise: Some(e_idx + 1),
                variant: None,
                pos: data_pos.clone(),
            });
        }
    }
}

fn check_missing_solutions(s: &Session, layout: &Layout, e_idx: usize, ex: &Exercise, out: &mut Vec<CheckIssue>) {
    let its = items(s, layout, e_idx);
    if ex.kind.quiz() || its.iter().any(|i| i.answer.is_some()) {
        for (q_idx, it) in its.iter().enumerate() {
            let Some((a, b)) = it.answer else { continue };
            let missing = match it.settings.mode {
                Mode::Choice | Mode::Worked | Mode::Cloze | Mode::TableFill | Mode::Matching => false,
                Mode::Calc => !(a..=b).any(|i| {
                    s.doc
                        .body
                        .get(i)
                        .and_then(|bl| bl.as_para())
                        .is_some_and(|p| span(p, TAG_LINE).is_none() && p.objects.iter().any(|o| matches!(o, InlineObject::Equation { .. })))
                }),
                Mode::Open => !(a..=b).any(|i| {
                    s.doc.body.get(i).and_then(|bl| bl.as_para()).is_some_and(|p| {
                        !plain(p).is_empty() || p.objects.iter().any(|o| matches!(o, InlineObject::Equation { .. } | InlineObject::Graphic { .. }))
                    })
                }),
                Mode::Drawing => !(a..=b).any(|i| {
                    s.doc.body.get(i).and_then(|bl| bl.as_para()).is_some_and(|p| {
                        p.objects
                            .iter()
                            .any(|o| matches!(o, InlineObject::Graphic { graphic, .. } if graphic.kind == wordcraft_doc::graphic::GraphicKind::Cad))
                    })
                }),
            };
            if missing {
                out.push(CheckIssue {
                    kind: "warning",
                    code: "missing_question_solution",
                    message: format!("Exercise {}, question {}: no answer or solution worked out yet.", e_idx + 1, q_idx + 1),
                    exercise: Some(e_idx + 1),
                    variant: None,
                    pos: body_pos(s, a),
                });
            }
        }
        return;
    }
    if ex.no_questions {
        return;
    }
    let answered_qns: BTreeSet<u32> = layout
        .parts
        .iter()
        .enumerate()
        .filter(|(_, (owner, _))| *owner == Some(e_idx))
        .filter_map(|(i, _)| s.doc.body.get(i)?.as_para())
        .flat_map(|p| p.objects.iter())
        .filter_map(|o| match o {
            InlineObject::Equation { math, .. } if math.answer > 0 => Some(math.answer),
            _ => None,
        })
        .collect();
    for (q_idx, q) in ex.questions.iter().enumerate() {
        let qn = u32::try_from(q_idx + 1).unwrap_or(u32::MAX);
        if !answered_qns.contains(&qn) {
            let pos = its.get(q_idx).map_or_else(|| body_pos(s, ex.start), |it| body_pos(s, it.question));
            out.push(CheckIssue {
                kind: "warning",
                code: "missing_question_solution",
                message: format!("Exercise {}, question {}: no equation marked as Result for this question.", e_idx + 1, q.label),
                exercise: Some(e_idx + 1),
                variant: None,
                pos,
            });
        }
    }
}

fn check_scores_consistency(s: &Session, layout: &Layout, out: &mut Vec<CheckIssue>) {
    let Some((table_block, rows)) = super::scores::row_points(s) else { return };
    for (e_idx, ex) in layout.exercises.iter().enumerate() {
        let row_sum = rows.get(e_idx).copied().flatten();
        if row_sum.is_none() {
            out.push(CheckIssue {
                kind: "warning",
                code: "missing_scores_points",
                message: format!("Exercise {} has no points assigned in the scores table.", e_idx + 1),
                exercise: Some(e_idx + 1),
                variant: None,
                pos: body_pos(s, table_block),
            });
        } else if let (Some(q_tot), Some(r_tot)) = (super::items::total(s, layout, e_idx), row_sum)
            && (q_tot - r_tot).abs() > 1e-6
            && rows.len() == 1
        {
            out.push(CheckIssue {
                kind: "warning",
                code: "mismatched_scores_points",
                message: format!("Exercise {}: question points sum to {q_tot}, while scores table totals {r_tot}.", e_idx + 1),
                exercise: Some(e_idx + 1),
                variant: None,
                pos: body_pos(s, ex.start),
            });
        }
    }
}

fn check_durations(s: &Session, layout: &Layout, out: &mut Vec<CheckIssue>) {
    let ex_durations: Vec<u32> = layout.exercises.iter().filter_map(|e| e.data.duration).collect();
    if ex_durations.is_empty() {
        return;
    }
    let sum_ex: u32 = ex_durations.iter().copied().fold(0u32, u32::saturating_add);
    if let Some(doc_dur) = super::archive::duration_minutes(s)
        && sum_ex != doc_dur
    {
        out.push(CheckIssue {
            kind: "info",
            code: "duration_mismatch",
            message: format!("Exercises sum to {sum_ex} min, while archive document duration is {doc_dur} min."),
            exercise: None,
            variant: None,
            pos: body_pos(s, 0),
        });
    }
}

fn check_layout_metrics(s: &Session, layout: &Layout, out: &mut Vec<CheckIssue>) {
    if layout.exercises.is_empty() {
        return;
    }
    // 1. Check teacher laid-out solution height vs student answer space height.
    let mut teacher = s.fork_in_memory();
    let _ = teacher.run("exercise.show", &json!({"solution": true, "results": true}));
    let t_doc_layout = teacher.layout();
    let t_scan = scan(&teacher);
    for (e_idx, _ex) in t_scan.exercises.iter().enumerate() {
        for (q_idx, it) in items(&teacher, &t_scan, e_idx).iter().enumerate() {
            if !matches!(it.settings.mode, Mode::Open | Mode::Calc) {
                continue;
            }
            let Some((a, b)) = it.answer else { continue };
            let has_content = (a..=b).any(|bi| {
                teacher.doc.body.get(bi).and_then(|bl| bl.as_para()).is_some_and(|p| {
                    span(p, TAG_LINE).is_none() && (!plain(p).is_empty() || p.objects.iter().any(|o| matches!(o, InlineObject::Equation { .. })))
                })
            });
            if !has_content {
                continue;
            }
            let mut used_pt = 0.0f32;
            for bi in a..=b {
                let Some(Block::Para(p)) = teacher.doc.body.get(bi).map(|bl| &**bl) else { continue };
                if span(p, TAG_LINE).is_some() {
                    continue;
                }
                let path = Path(vec![u32::try_from(bi).unwrap_or(0)]);
                if let Some(locs) = t_doc_layout.index.get(&(StoryRef::Body, path)) {
                    let mut seen_para = false;
                    for &(pg, item_i) in locs {
                        if let Some(wordcraft_layout::Placed::Lines { para, .. }) = t_doc_layout.pages.get(pg).and_then(|page| page.items.get(item_i))
                            && !seen_para
                        {
                            // Wait: with `ParaProps::graph` on the first paragraph, `para.height` is
                            // the text's natural line height!
                            used_pt += para.height + para.rp.space_before + para.rp.space_after;
                            seen_para = true;
                        }
                    }
                }
            }
            let space_pt = it.settings.height * 10.0 * PT_PER_MM;
            if used_pt > space_pt + 2.0 {
                let used_cm = (used_pt / (10.0 * PT_PER_MM) * 10.0).round() / 10.0;
                out.push(CheckIssue {
                    kind: "warning",
                    code: "answer_space_overflow",
                    message: format!(
                        "Exercise {}, question {}: answer space is {:.1} cm high, while the solution needs ~{used_cm:.1} cm.",
                        e_idx + 1,
                        q_idx + 1,
                        it.settings.height
                    ),
                    exercise: Some(e_idx + 1),
                    variant: None,
                    pos: body_pos(s, a),
                });
            }
        }
    }

    // 2. Compare student-copy page counts across variants `A..Z`.
    let max_variants = layout.exercises.iter().map(|e| e.data.variants.len()).max().unwrap_or(0);
    if max_variants >= 2 {
        let mut counts: Vec<(String, usize)> = Vec::with_capacity(max_variants);
        for vi in 0..max_variants {
            let mut student = s.fork_in_memory();
            for (ei, ex) in layout.exercises.iter().enumerate() {
                if !ex.data.variants.is_empty() {
                    let pick = vi.min(ex.data.variants.len() - 1);
                    let _ = student.run("exercise.variant", &json!({"exercise": ei + 1, "active": pick}));
                }
            }
            let _ = student.run("exercise.show", &json!({"solution": false, "results": false}));
            let v_label = layout
                .exercises
                .iter()
                .find_map(|ex| ex.data.variants.get(vi).cloned())
                .unwrap_or_else(|| char::from(b'A' + u8::try_from(vi.min(25)).unwrap_or(25)).to_string());
            let pages = student.layout().pages.len();
            counts.push((v_label, pages));
        }
        if let Some((_, first_pages)) = counts.first()
            && counts.iter().any(|(_, p)| p != first_pages)
        {
            let summary = counts.iter().map(|(v, p)| format!("{v}: {p} pp.")).collect::<Vec<_>>().join(", ");
            out.push(CheckIssue {
                kind: "info",
                code: "variant_page_count_diff",
                message: format!("Student page count differs across variants ({summary})."),
                exercise: None,
                variant: None,
                pos: body_pos(s, 0),
            });
        }
    }
    let _ = choice_text;
}

/// Runs all pre-flight checks on `s` without mutating it.
pub(crate) fn run_checks(s: &Session) -> Vec<CheckIssue> {
    let mut issues = Vec::new();
    let layout = scan(s);
    check_subdocuments(s, &mut issues);
    for (e_idx, ex) in layout.exercises.iter().enumerate() {
        check_exercise_variants_and_choices(s, &layout, e_idx, ex, &mut issues);
        check_unused_data(s, &layout, e_idx, ex, &mut issues);
        check_missing_solutions(s, &layout, e_idx, ex, &mut issues);
    }
    check_scores_consistency(s, &layout, &mut issues);
    check_durations(s, &layout, &mut issues);
    check_layout_metrics(s, &layout, &mut issues);
    issues
}

/// `exercise.check`: pure pre-flight check command returning structured errors, warnings and infos.
pub(crate) fn command(s: &mut Session, _v: &Value) -> CmdResult {
    let issues = run_checks(s);
    let errors = issues.iter().filter(|i| i.kind == "error").count();
    let warnings = issues.iter().filter(|i| i.kind == "warning").count();
    let infos = issues.iter().filter(|i| i.kind == "info").count();
    Ok(json!({
        "ok": errors == 0,
        "errors": errors,
        "warnings": warnings,
        "infos": infos,
        "issues": issues,
    }))
}
