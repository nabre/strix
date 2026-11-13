//! Assessment Pack batch export (`F4` — `file.exportPack`).
//!
//! Exports in one step (to a folder on desktop, or as a `.zip` archive on desktop and `strix-web`
//! WASM) without mutating the open `Session`:
//! - Student PDF per variant (`Fila_A.pdf`, `Fila_B.pdf`… with `Show = false`);
//! - DSA Student PDF per variant (`Fila_A_DSA.pdf`, `Fila_B_DSA.pdf`… with `design.dsa` active);
//! - Teacher Key PDF (`Correttore_Docente.pdf` with `Show = true`, worked solutions, grading grid
//!   `strix:grading`, and a final **Multi-Variant Synoptic Table** `Domanda | Fila A | Fila B | …`).

use std::io::{Cursor, Write};
use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{Align, CellProps, CharProps, ParaProps};
use wordcraft_doc::table::{Cell, Row, Table};
use wordcraft_doc::{Block, InlineObject, Paragraph, StoryRef, para_block};

use super::items::{Mode, TAG_LINE, choices, items, span};
use super::{STYLE_HEADING, doc_words, numbering, plain, scan};
use crate::{CmdError, CmdResult, Session, p};

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(B64_TABLE[usize::from(b0 >> 2)] as char);
        out.push(B64_TABLE[usize::from(((b0 & 0x03) << 4) | (b1 >> 4))] as char);
        if chunk.len() > 1 {
            out.push(B64_TABLE[usize::from(((b1 & 0x0f) << 2) | (b2 >> 6))] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_TABLE[usize::from(b2 & 0x3f)] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn sanitize_filename_part(raw: &str) -> String {
    let s: String = raw.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' }).take(32).collect();
    let trimmed = s.trim_matches('_');
    if trimmed.is_empty() { "A".to_string() } else { trimmed.to_string() }
}

fn collect_variant_names(s: &Session) -> Vec<String> {
    let layout = scan(s);
    let max_v = layout.exercises.iter().map(|e| e.data.variants.len()).max().unwrap_or(0);
    if max_v == 0 {
        return vec!["A".to_string()];
    }
    (0..max_v)
        .map(|vi| {
            layout
                .exercises
                .iter()
                .find_map(|e| e.data.variants.get(vi).cloned())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| char::from(b'A' + u8::try_from(vi.min(25)).unwrap_or(25)).to_string())
        })
        .collect()
}

fn apply_variant_to_session(sess: &mut Session, vi: usize) {
    let layout = scan(sess);
    for (ei, ex) in layout.exercises.iter().enumerate() {
        if !ex.data.variants.is_empty() {
            let pick = vi.min(ex.data.variants.len() - 1);
            let _ = sess.run("exercise.variant", &json!({"exercise": ei + 1, "active": pick}));
        }
    }
}

/// Extracts `(question_label, answer_summary)` rows from `sess` (evaluated with `solution: true`).
fn collect_synoptic_column(sess: &Session) -> Vec<(String, String)> {
    let layout = scan(sess);
    let calc_opts = crate::cmd::calc::options_now(sess);
    let mut calc_map: std::collections::HashMap<(usize, u32), Vec<String>> = std::collections::HashMap::new();
    for (top, qn, txt) in crate::cmd::calc::answers(sess, &calc_opts) {
        if let Some(Some(ex_i)) = layout.parts.get(top).map(|(x, _)| *x) {
            calc_map.entry((ex_i, qn)).or_default().push(txt);
        }
    }
    let num = numbering::current(sess);
    let ex_labels: Vec<String> = (0..layout.exercises.len()).map(|n| num.label(n)).collect();
    let mut rows: Vec<(String, String)> = Vec::new();
    for (e_idx, ex) in layout.exercises.iter().enumerate() {
        let ex_prefix = ex_labels.get(e_idx).cloned().unwrap_or_else(|| format!("{}", e_idx + 1));
        let its = items(sess, &layout, e_idx);
        if ex.kind.quiz() || its.iter().any(|i| i.answer.is_some()) {
            let lone = its.len() == 1;
            for (q_idx, it) in its.iter().enumerate() {
                let q_lbl = if lone {
                    ex_prefix.clone()
                } else {
                    let sub = ex.questions.get(q_idx).map(|q| q.label.as_str()).unwrap_or("");
                    format!("{ex_prefix} {sub}").trim().to_string()
                };
                let ans = match it.settings.mode {
                    Mode::Choice => {
                        let cs = it.answer.map(|r| choices(sess, r)).unwrap_or_default();
                        let letters: Vec<String> =
                            cs.iter().enumerate().filter(|(_, (_, cor))| *cor).map(|(j, _)| super::letter(j).to_ascii_uppercase()).collect();
                        if letters.is_empty() { "—".to_string() } else { letters.join(", ") }
                    }
                    Mode::Calc => {
                        let line = it.answer.and_then(|(a, b)| {
                            (a..=b).find_map(|bi| {
                                let p = sess.doc.body.get(bi)?.as_para()?;
                                let (x, y) = span(p, TAG_LINE)?;
                                p.text.get(x + OBJ.len_utf8()..y).map(|t| super::items::line_value(t).to_string())
                            })
                        });
                        line.unwrap_or_else(|| {
                            let qn = u32::try_from(q_idx + 1).unwrap_or(u32::MAX);
                            calc_map.get(&(e_idx, qn)).and_then(|v| v.last()).cloned().unwrap_or_else(|| "—".into())
                        })
                    }
                    Mode::Drawing => "[CAD]".to_string(),
                    Mode::Open | Mode::Worked | Mode::Cloze | Mode::TableFill | Mode::Matching => {
                        let snippet = it
                            .answer
                            .and_then(|(a, b)| {
                                let joined = (a..=b)
                                    .filter_map(|bi| sess.doc.body.get(bi)?.as_para())
                                    .map(plain)
                                    .filter(|s| !s.is_empty())
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                (!joined.is_empty()).then_some(joined)
                            })
                            .unwrap_or_else(|| "—".to_string());
                        truncate_cell(&snippet, 60)
                    }
                };
                rows.push((q_lbl, ans));
            }
        } else {
            let lone = ex.questions.len() <= 1;
            if ex.no_questions || ex.questions.is_empty() {
                let ans = calc_map.get(&(e_idx, 1)).map(|v| v.join("; ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "—".into());
                rows.push((ex_prefix, ans));
            } else {
                for (q_idx, q) in ex.questions.iter().enumerate() {
                    let q_lbl = if lone { ex_prefix.clone() } else { format!("{ex_prefix} {}", q.label) };
                    let qn = u32::try_from(q_idx + 1).unwrap_or(u32::MAX);
                    let ans = calc_map.get(&(e_idx, qn)).map(|v| v.join("; ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "—".into());
                    rows.push((q_lbl, ans));
                }
            }
        }
    }
    let _ = InlineObject::ControlEnd;
    rows
}

fn truncate_cell(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

fn synoptic_cell(text: &str, align: Align, bold: bool, width: f32) -> Cell {
    let chr = CharProps { bold: bold.then_some(true), ..Default::default() };
    let mut p = Paragraph::with_text(text, chr);
    p.props.align = Some(align);
    let mut c = Cell::empty();
    c.blocks = vec![para_block(p)];
    c.props = CellProps { width: Some(width), ..CellProps::default() };
    c
}

fn append_synoptic_table(teacher: &mut Session, variant_names: &[String], per_variant: &[Vec<(String, String)>]) {
    let Some(first_col) = per_variant.first() else { return };
    if first_col.is_empty() {
        return;
    }
    let w = doc_words(teacher);
    let mut heading = Paragraph::with_text(w.synoptic_heading, CharProps { bold: Some(true), ..Default::default() });
    heading.props = ParaProps {
        style: Some(STYLE_HEADING.into()),
        page_break_before: Some(true),
        space_after: Some(6.0),
        keep_next: Some(true),
        ..ParaProps::default()
    };
    teacher.doc.body.push(para_block(heading));

    let total_w = teacher.doc.last_section.text_width().max(240.0);
    let n_vars = variant_names.len().max(1);
    let q_w = (total_w * 0.24).clamp(70.0, 140.0);
    let v_w = ((total_w - q_w) / (n_vars as f32)).max(45.0);
    let mut grid = Vec::with_capacity(1 + n_vars);
    grid.push(q_w);
    for _ in 0..n_vars {
        grid.push(v_w);
    }

    let mut t = Table::new(1, 1 + n_vars, total_w);
    t.grid = grid;
    let mut header_cells = Vec::with_capacity(1 + n_vars);
    header_cells.push(synoptic_cell(w.question_col, Align::Left, true, q_w));
    for vn in variant_names {
        header_cells.push(synoptic_cell(&format!("{} {vn}", w.pack_row_prefix), Align::Center, true, v_w));
    }
    let mut table_rows = vec![Row { cells: header_cells, ..Row::default() }];
    for (r_idx, (q_label, _)) in first_col.iter().enumerate() {
        let mut cells = Vec::with_capacity(1 + n_vars);
        cells.push(synoptic_cell(q_label, Align::Left, false, q_w));
        for col in per_variant {
            let val = col.get(r_idx).map_or("—", |(_, a)| a.as_str());
            cells.push(synoptic_cell(val, Align::Center, false, v_w));
        }
        table_rows.push(Row { cells, ..Row::default() });
    }
    t.rows = table_rows;
    teacher.doc.body.push(Arc::new(Block::Table(t)));
    teacher.doc.body.push(para_block(Paragraph::new()));
    teacher.touch();
}

fn build_zip_bytes(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in files {
            zip.start_file(name, opts).map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// `file.exportPack`: generates Student, DSA Student, and Teacher Key PDFs (with Grading Scale and
/// Multi-Variant Synoptic Table) as a directory or `.zip` archive.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    if scan(s).exercises.is_empty() {
        return Err(CmdError::Disabled("the document has no exercise".into()));
    }
    let path_opt = p::str(v, "path").map(str::trim).filter(|p| !p.is_empty());
    let return_bytes = p::bool(v, "returnBytes").unwrap_or(cfg!(target_arch = "wasm32") && path_opt.is_none());
    if path_opt.is_none() && !return_bytes {
        s.push_ui_request(json!({"open": "exportPack"}));
        return Ok(json!({"exported": false}));
    }

    let include_dsa = p::bool(v, "includeDsa").unwrap_or(true);
    let include_teacher = p::bool(v, "includeTeacher").unwrap_or(true);
    let check_issues = super::check::run_checks(s);
    let check_errors = check_issues.iter().filter(|i| i.kind == "error").count();
    let check_warnings = check_issues.iter().filter(|i| i.kind == "warning").count();

    let w = doc_words(s);
    let variant_names = collect_variant_names(s);
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut synoptic_cols: Vec<Vec<(String, String)>> = Vec::with_capacity(variant_names.len());

    for (vi, v_name) in variant_names.iter().enumerate() {
        let safe_v = sanitize_filename_part(v_name);
        // 1. Standard Student PDF (`Fila_A.pdf`).
        let mut student = s.fork_in_memory();
        let _ = student.run("design.dsa", &json!({"none": true}));
        apply_variant_to_session(&mut student, vi);
        let _ = student.run("exercise.show", &json!({"solution": false, "results": false}));
        let student_name = format!("{}_{safe_v}.pdf", w.pack_row_prefix);
        let student_pdf = crate::io::save_bytes(&student_name, &student.doc).map_err(CmdError::Failed)?;
        files.push((student_name, student_pdf));

        // 2. DSA Student PDF (`Fila_A_DSA.pdf`).
        if include_dsa {
            let mut dsa = s.fork_in_memory();
            apply_variant_to_session(&mut dsa, vi);
            let _ = dsa.run("design.dsa", &json!({"reading": true, "writing": true, "math": true}));
            let _ = dsa.run("exercise.show", &json!({"solution": false, "results": false}));
            let dsa_name = format!("{}_{safe_v}_DSA.pdf", w.pack_row_prefix);
            let dsa_pdf = crate::io::save_bytes(&dsa_name, &dsa.doc).map_err(CmdError::Failed)?;
            files.push((dsa_name, dsa_pdf));
        }

        // 3. Variant Teacher evaluation for the Multi-Variant Synoptic Table.
        if include_teacher {
            let mut v_teacher = s.fork_in_memory();
            let _ = v_teacher.run("design.dsa", &json!({"none": true}));
            apply_variant_to_session(&mut v_teacher, vi);
            let _ = v_teacher.run("exercise.show", &json!({"solution": true, "results": true}));
            synoptic_cols.push(collect_synoptic_column(&v_teacher));
        }
    }

    if include_teacher {
        let mut teacher = s.fork_in_memory();
        let _ = teacher.run("design.dsa", &json!({"none": true}));
        apply_variant_to_session(&mut teacher, 0);
        let _ = teacher.run("exercise.show", &json!({"solution": true, "results": true}));
        if super::grading::tables(&teacher).is_empty() {
            teacher.sel = crate::Selection::caret(teacher.doc.end_of(StoryRef::Body));
            let _ = teacher.run("exercise.grading", &json!({"on": true, "visibility": "teacher"}));
        }
        append_synoptic_table(&mut teacher, &variant_names, &synoptic_cols);
        let teacher_name = format!("{}.pdf", w.pack_teacher_name);
        let teacher_pdf = crate::io::save_bytes(&teacher_name, &teacher.doc).map_err(CmdError::Failed)?;
        files.push((teacher_name, teacher_pdf));
    }

    let file_names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    let want_zip = return_bytes
        || p::str(v, "format").is_some_and(|f| f.eq_ignore_ascii_case("zip"))
        || path_opt.is_some_and(|p| p.to_ascii_lowercase().ends_with(".zip"));

    let mut out = json!({
        "exported": true,
        "files": file_names,
        "variants": variant_names,
        "check": {
            "ok": check_errors == 0,
            "errors": check_errors,
            "warnings": check_warnings,
        }
    });

    if want_zip {
        let zip_bytes = build_zip_bytes(&files).map_err(CmdError::Failed)?;
        if let Some(path) = path_opt
            && !cfg!(target_arch = "wasm32")
        {
            crate::io::write_file(path, &zip_bytes).map_err(CmdError::Failed)?;
            out["path"] = json!(path);
        }
        if return_bytes {
            let suggested = path_opt.unwrap_or("Pacchetto_Valutazione.zip");
            out["zipName"] = json!(suggested);
            out["zipBase64"] = json!(base64_encode(&zip_bytes));
        }
    } else if let Some(dir) = path_opt {
        let written = crate::io::write_pack_dir(dir, &files).map_err(CmdError::Failed)?;
        out["path"] = json!(dir);
        out["written"] = json!(written);
    }

    Ok(out)
}
