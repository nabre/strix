//! A reader for thesaurus files in the MyThes text format (`th_*.dat`), written from the format's
//! description (never from MyThes's or LibreOffice's code):
//!
//! - the first line names the encoding of the rest (`UTF-8`, `ISO8859-1`, `KOI8-R`, `CP-1251`…);
//! - then each entry is a line `word|n` followed by `n` lines `category|synonym|synonym…`, the
//!   category often in parentheses (`(noun)`, `(s.m.)`), `-` or empty when there is none.
//!
//! The whole file is decoded into one string and indexed in memory (lower-case word → where its
//! meanings start); a lookup reads only that word's lines. The files come from the network or
//! from the disk, so everything about them is untrusted: the size is capped before reading, line
//! length, entries, meanings and synonyms are capped, the declared `n` is checked against the
//! lines there are, and nothing panics.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// Largest thesaurus read (the largest published, American English, is ~18 MB).
pub const MAX_FILE: usize = 32 << 20;
/// Longest line read, in bytes.
pub const MAX_LINE: usize = 64 << 10;
/// Most entries in one thesaurus (the largest has ~150,000).
pub const MAX_ENTRIES: usize = 2_000_000;
/// Most meanings one entry may declare.
pub const MAX_MEANINGS: usize = 512;
/// Most synonyms kept for one meaning.
pub const MAX_SYNONYMS: usize = 256;
/// Stray lines tolerated (a published file has a meaning broken over two lines): this many, plus
/// one per hundred entries. More, and the file isn't a thesaurus.
pub const STRAY_LINES: usize = 64;

/// One meaning of a word: its category as the file writes it, without parentheses (`noun`,
/// `s.m.`; `None` when it has none) and its synonyms, in the file's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meaning {
    pub category: Option<String>,
    pub synonyms: Vec<String>,
}

/// A thesaurus read whole into memory, with its index.
#[derive(Debug)]
pub struct Thesaurus {
    text: String,
    /// Lower-case word → each entry for it: the byte offset of its first meaning line and how
    /// many meaning lines follow.
    index: HashMap<Box<str>, Vec<(usize, usize)>>,
}

/// Read the thesaurus at `path`, refusing a file larger than `max` bytes before reading it.
pub fn load(path: &Path, max: usize) -> Result<Thesaurus, String> {
    let shown = path.display();
    let len = std::fs::metadata(path).map_err(|e| format!("{shown}: {e}"))?.len();
    if len > max as u64 {
        return Err(format!("{shown}: larger than {max} bytes"));
    }
    let file = std::fs::File::open(path).map_err(|e| format!("{shown}: {e}"))?;
    let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0).min(max));
    // The file may grow between the size check and the read: read at most one byte past the cap.
    file.take((max as u64).saturating_add(1)).read_to_end(&mut bytes).map_err(|e| format!("{shown}: {e}"))?;
    if bytes.len() > max {
        return Err(format!("{shown}: larger than {max} bytes"));
    }
    parse(&bytes).map_err(|e| format!("{shown}: {e}"))
}

/// Read a thesaurus from its bytes.
pub fn parse(bytes: &[u8]) -> Result<Thesaurus, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err("empty".into());
    }
    let first_end = bytes.iter().position(|&b| b == b'\n').unwrap_or(bytes.len());
    if first_end > MAX_LINE {
        return Err("line 1: longer than the longest line read".into());
    }
    let label = String::from_utf8_lossy(bytes.get(..first_end).unwrap_or_default()).trim().to_string();
    if label.starts_with('<') {
        return Err("not a thesaurus (a web page)".into());
    }
    let rest = bytes.get(first_end.saturating_add(1)..).unwrap_or_default();
    if rest.contains(&0) {
        return Err("not a text file".into());
    }
    let text = decode(&label, rest)?;
    let index = build_index(&text)?;
    Ok(Thesaurus { text, index })
}

/// The text of `bytes` in the encoding named `label` (the file's first line).
fn decode(label: &str, bytes: &[u8]) -> Result<String, String> {
    let shown = label.chars().take(40).collect::<String>();
    let mut name = label.to_ascii_lowercase();
    // Labels written `CP-1251`, `WINDOWS1251` or `UTF8` are the usual ones without the dash.
    if let Some(n) = name.strip_prefix("cp-") {
        name = format!("cp{n}");
    }
    if let Some(n) = name.strip_prefix("windows").filter(|n| n.bytes().next().is_some_and(|b| b.is_ascii_digit())) {
        name = format!("windows-{n}");
    }
    let encoding = encoding_rs::Encoding::for_label(name.as_bytes()).ok_or_else(|| format!("unknown encoding \"{shown}\""))?;
    if encoding == encoding_rs::UTF_8 {
        return std::str::from_utf8(bytes).map(str::to_string).map_err(|e| format!("not valid UTF-8 ({e})"));
    }
    // Only the single-byte encodings the format names (ISO 8859, KOI8, Windows code pages).
    if !encoding.is_single_byte() {
        return Err(format!("unsupported encoding \"{shown}\""));
    }
    Ok(encoding.decode_without_bom_handling(bytes).0.into_owned())
}

/// Lines of `text` with the byte offset each starts at and its number in the file (the encoding
/// line is line 1).
fn lines(text: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let mut start = 0usize;
    let mut number = 1usize;
    std::iter::from_fn(move || {
        if start >= text.len() {
            return None;
        }
        let rest = text.get(start..)?;
        let len = rest.find('\n').unwrap_or(rest.len());
        let line = rest.get(..len)?;
        let at = start;
        start = start.saturating_add(len).saturating_add(1);
        number = number.saturating_add(1);
        Some((at, number, line.strip_suffix('\r').unwrap_or(line)))
    })
}

fn build_index(text: &str) -> Result<HashMap<Box<str>, Vec<(usize, usize)>>, String> {
    let mut index: HashMap<Box<str>, Vec<(usize, usize)>> = HashMap::new();
    let mut entries = 0usize;
    // Lines that are neither an entry where one is due nor a meaning where one is: skipped.
    let mut stray = 0usize;
    let mut it = lines(text);
    while let Some((_, number, line)) = it.next() {
        if line.len() > MAX_LINE {
            return Err(format!("line {number}: longer than the longest line read"));
        }
        if line.trim().is_empty() {
            continue;
        }
        let entry = line.rsplit_once('|').map(|(w, c)| (w.trim(), c.trim()));
        let Some((word, count)) = entry.filter(|(w, c)| !w.is_empty() && !c.is_empty() && c.bytes().all(|b| b.is_ascii_digit())) else {
            stray = stray.saturating_add(1);
            continue;
        };
        let n =
            count.parse::<usize>().ok().filter(|n| *n <= MAX_MEANINGS).ok_or_else(|| {
                format!("line {number}: \"{}\" declares more meanings than a word can have", word.chars().take(40).collect::<String>())
            })?;
        let mut first = None;
        for i in 0..n {
            let Some((at, mnumber, mline)) = it.next() else {
                return Err(format!(
                    "line {number}: \"{}\" declares {n} meanings, the file ends after {i}",
                    word.chars().take(40).collect::<String>()
                ));
            };
            if mline.len() > MAX_LINE {
                return Err(format!("line {mnumber}: longer than the longest line read"));
            }
            if mline.contains('|') {
                first.get_or_insert(at);
            } else {
                stray = stray.saturating_add(1);
            }
        }
        entries = entries.saturating_add(1);
        if entries > MAX_ENTRIES {
            return Err(format!("more than {MAX_ENTRIES} entries"));
        }
        if let Some(at) = first {
            index.entry(word.to_lowercase().into_boxed_str()).or_default().push((at, n));
        }
    }
    if stray > STRAY_LINES.saturating_add(entries / 100) {
        return Err(format!("not a thesaurus: {stray} lines are neither an entry (word|count) nor a meaning (category|synonym…)"));
    }
    if index.is_empty() {
        return Err("no entries".into());
    }
    Ok(index)
}

/// The category of a meaning line's first field: without one pair of surrounding parentheses or
/// brackets; `None` for none (`-`, empty).
fn category(field: &str) -> Option<String> {
    let t = field.trim();
    let inner = t.strip_prefix('(').and_then(|x| x.strip_suffix(')')).or_else(|| t.strip_prefix('[').and_then(|x| x.strip_suffix(']'))).unwrap_or(t);
    let inner = inner.trim();
    (!inner.is_empty() && inner != "-").then(|| inner.to_string())
}

impl Thesaurus {
    /// How many words it has entries for.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether it has no entries (never, once read).
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The meanings of `word` exactly as written, case aside.
    pub fn meanings(&self, word: &str) -> Option<Vec<Meaning>> {
        let spans = self.index.get(word.trim().to_lowercase().as_str())?;
        let mut out = Vec::new();
        for &(at, n) in spans {
            let Some(block) = self.text.get(at..) else { continue };
            for (_, _, line) in lines(block).take(n).filter(|(_, _, l)| l.contains('|')) {
                let mut fields = line.split('|');
                let cat = category(fields.next().unwrap_or_default());
                let synonyms: Vec<String> = fields.map(str::trim).filter(|s| !s.is_empty()).take(MAX_SYNONYMS).map(str::to_string).collect();
                if !synonyms.is_empty() {
                    out.push(Meaning { category: cat, synonyms });
                }
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// Look `word` up: its exact form (case aside), then its base forms ([`base_forms`] for the
    /// language `lang`, a BCP 47 tag). The entry found and its meanings.
    pub fn lookup(&self, word: &str, lang: Option<&str>) -> Option<(String, Vec<Meaning>)> {
        let word = word.trim().to_lowercase();
        if let Some(m) = self.meanings(&word) {
            return Some((word, m));
        }
        base_forms(&word, lang).into_iter().find_map(|base| self.meanings(&base).map(|m| (base, m)))
    }
}

/// The forms a dictionary would list `word` (lower case) under, most likely first: without an
/// elided article or a possessive and with typographic apostrophes made plain, then, for the
/// languages with simple regular endings (English, Italian, Spanish, Portuguese), without a
/// plural or verb ending. Only candidates: a lookup uses the first one the thesaurus has.
pub fn base_forms(word: &str, lang: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if s.chars().count() >= 2 && s != word && !out.contains(&s) {
            out.push(s);
        }
    };
    let plain = word.replace(['\u{2019}', '\u{2018}', '\u{02BC}'], "'");
    let plain = plain.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-').to_string();
    push(plain.clone());
    // "l'acqua", "dell'anno", "d'accordo", "qu'il": the word after the apostrophe.
    let mut stem = plain.clone();
    if let Some((head, tail)) = plain.split_once('\'')
        && head.chars().count() <= 5
        && tail.chars().count() >= 2
    {
        push(tail.to_string());
        stem = tail.to_string();
    }
    // "teacher's", "teachers'".
    if let Some(s) = plain.strip_suffix("'s").or_else(|| plain.strip_suffix('\'')) {
        push(s.to_string());
        stem = s.to_string();
    }
    let primary = lang.and_then(|l| l.split(['-', '_']).next()).map(str::to_ascii_lowercase).unwrap_or_default();
    let rules: &[(&str, &str)] = match primary.as_str() {
        "en" | "" => &[("ies", "y"), ("ves", "f"), ("es", ""), ("s", ""), ("ied", "y"), ("ed", "e"), ("ed", ""), ("ing", "e"), ("ing", "")],
        "it" => &[("i", "o"), ("e", "a"), ("i", "e"), ("he", "a"), ("hi", "o")],
        "es" => &[("es", ""), ("s", "")],
        "pt" => &[("ões", "ão"), ("ães", "ão"), ("ns", "m"), ("es", ""), ("s", "")],
        _ => &[],
    };
    for (end, base) in rules {
        if let Some(s) = stem.strip_suffix(end)
            && s.chars().count() >= 3
        {
            push(format!("{s}{base}"));
        }
    }
    out
}

#[cfg(test)]
#[path = "mythes_tests.rs"]
mod tests;
