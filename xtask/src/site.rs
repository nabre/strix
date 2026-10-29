//! `cargo xtask site [--check]`: builds the project site (https://nabre.github.io/strix/) into
//! `_site/`, one landing page per language: English at the root, the others in `it/`, `fr/`,
//! `de/`. The model is `site/landing.html`; `{{> name}}` includes `site/partials/name.html` and
//! `{{key}}` takes a string from `site/strings/<lang>.json` (every language has the same keys) or
//! a value made here (alternates, JSON-LD, the language menu, the version in the download links).
//! It also writes `sitemap.xml` with every language's alternates and a `404.html`, and copies the
//! site's assets, the app icon, the brand files of `docs/brand` and the screenshots of
//! `site/shots`. `--check` then checks that every page is there and in the sitemap, that no key is
//! left, and that every relative link and image leads to a file. Published by
//! `.github/workflows/site.yml`; the rules for the copy are in `docs/brand/MARKETING.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};

/// English first: the root page, `x-default`, and the keys every language must have.
const LANGS: &[(&str, &str)] = &[("en", "en_US"), ("it", "it_IT"), ("fr", "fr_FR"), ("de", "de_DE")];
const BASE: &str = "https://nabre.github.io/strix/";
const REPO: &str = "https://github.com/nabre/strix";
/// What the pages are found by, in English on every page (as the repository's topics).
const KEYWORDS: &[&str] = &[
    "WYSIWYG math",
    "engineering calculations",
    "calculation report",
    "units of measurement",
    "equation solver",
    "math document editor",
    "live equations",
    "structural engineering",
    "worked exercises",
    "math education",
    "parametric CAD",
    "technical drawing",
    "open source",
    "Rust",
    "WebAssembly",
    "MCP server",
    "AI agents",
    "Windows",
    "macOS",
    "Linux",
    "FreeBSD",
];

/// The address of a language's landing page, from the site's root.
fn path_of(lang: &str) -> String {
    if lang == "en" { String::new() } else { format!("{lang}/") }
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let site = root.join("site");
    let out = root.join("_site");
    let read = |p: &Path| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));

    let mut partials = BTreeMap::new();
    for name in ["head", "header", "footer"] {
        let text = read(&site.join("partials").join(format!("{name}.html")))?;
        partials.insert(name.to_string(), text.trim_end_matches('\n').to_string());
    }
    let landing = include_partials(&read(&site.join("landing.html"))?, &partials, "landing.html")?;

    let mut strings: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    for (lang, _) in LANGS {
        let file = site.join("strings").join(format!("{lang}.json"));
        let v: Value = serde_json::from_str(&read(&file)?).map_err(|e| format!("{}: {e}", file.display()))?;
        let Value::Object(map) = v else { return Err(format!("{}: not a JSON object", file.display())) };
        let mut m = BTreeMap::new();
        for (k, v) in map {
            let Value::String(s) = v else { return Err(format!("{}: `{k}` is not a string", file.display())) };
            m.insert(k, s);
        }
        strings.insert(lang, m);
    }
    // Same keys in every language: a missing one would stop the build later with less context.
    let keys = |l: &str| strings.get(l).map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    for (lang, _) in LANGS.iter().skip(1) {
        if keys(lang) != keys("en") {
            let (en, other) = (keys("en"), keys(lang));
            let missing: Vec<_> = en.iter().filter(|k| !other.contains(k)).collect();
            let extra: Vec<_> = other.iter().filter(|k| !en.contains(k)).collect();
            return Err(format!("site/strings/{lang}.json: keys differ from en.json (missing {missing:?}, extra {extra:?})"));
        }
    }

    let version = workspace_version(&read(&root.join("Cargo.toml"))?)?;

    // The folder's contents, not the folder: a preview server started in _site keeps it busy.
    fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    for entry in fs::read_dir(&out).map_err(|e| format!("{}: {e}", out.display()))? {
        let p = entry.map_err(|e| e.to_string())?.path();
        let r = if p.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
        r.map_err(|e| format!("{}: {e}", p.display()))?;
    }

    for (lang, locale) in LANGS {
        let s = strings.get(lang).ok_or("no strings")?;
        let here = path_of(lang);
        let mut values: BTreeMap<String, String> = s.iter().map(|(k, v)| (k.clone(), v.replace('"', "&quot;"))).collect();
        let computed = [
            ("lang", (*lang).to_string()),
            ("root", "../".repeat(here.matches('/').count())),
            ("base", BASE.to_string()),
            ("path", here.clone()),
            ("langPath", here.clone()),
            ("home", String::new()),
            ("version", version.clone()),
            ("dl", format!("{REPO}/releases/download/v{version}/strix-{version}-")),
            ("readmePath", if *lang == "en" { "README.md".to_string() } else { format!("docs/README.{lang}.md") }),
            ("keywords", KEYWORDS.join(", ")),
            ("alternates", alternates()),
            ("ogLocales", og_locales(locale)),
            ("langLinks", lang_menu(lang, &here, &strings)?),
            ("jsonLd", json_ld(lang, s, &version)?),
        ];
        for (k, v) in computed {
            values.insert(k.to_string(), v);
        }
        let page = fill(&landing, &values, "landing.html")?;
        let dir = out.join(&here);
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        fs::write(dir.join("index.html"), page).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    // Assets: the site's own, the app icon, the brand files and the screenshots per language.
    let assets = out.join("assets");
    copy_dir(&site.join("assets"), &assets)?;
    let copy = |from: &Path, to: &Path| fs::copy(from, to).map(|_| ()).map_err(|e| format!("{}: {e}", from.display()));
    copy(&root.join("assets/app-icon/strix.svg"), &assets.join("strix.svg"))?;
    copy(&root.join("assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png"), &assets.join("strix-256.png"))?;
    for f in ["wordmark-light.svg", "wordmark-dark.svg", "social.png"] {
        copy(&root.join("docs/brand").join(f), &assets.join(f))?;
    }
    for (lang, _) in LANGS {
        let shots = site.join("shots").join(lang);
        if shots.is_dir() {
            copy_dir(&shots, &assets.join("shots").join(lang))?;
        }
    }

    fs::write(out.join("404.html"), not_found()).map_err(|e| format!("404.html: {e}"))?;
    fs::write(out.join("sitemap.xml"), sitemap()).map_err(|e| format!("sitemap.xml: {e}"))?;
    println!("_site: {} pages, version {version}", LANGS.len());

    if args.contains(&"--check") {
        check(&out)?;
        println!("site check: {} pages, the sitemap and every relative link in order", LANGS.len());
    }
    Ok(())
}

/// The version in `[workspace.package]` of the root Cargo.toml: the release the download links
/// point at.
fn workspace_version(cargo_toml: &str) -> Result<String, String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[workspace.package]";
        } else if in_package && let Some(v) = t.strip_prefix("version = \"").and_then(|r| r.strip_suffix('"')) {
            return Ok(v.to_string());
        }
    }
    Err("Cargo.toml: no version in [workspace.package]".into())
}

/// `{{> name}}` replaced by the partial `name`.
fn include_partials(model: &str, partials: &BTreeMap<String, String>, name: &str) -> Result<String, String> {
    let mut out = String::with_capacity(model.len() * 2);
    let mut rest = model;
    while let Some(i) = rest.find("{{> ") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 4..];
        let end = after.find("}}").ok_or_else(|| format!("{name}: unclosed {{{{> "))?;
        let key = after[..end].trim();
        out.push_str(partials.get(key).ok_or_else(|| format!("{name}: no partial `{key}`"))?);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Every `{{key}}` replaced by its value, in one pass: a value is never read for keys itself.
fn fill(model: &str, values: &BTreeMap<String, String>, name: &str) -> Result<String, String> {
    let mut out = String::with_capacity(model.len() * 2);
    let mut rest = model;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let end = after.find("}}").ok_or_else(|| format!("{name}: unclosed {{{{"))?;
        let key = &after[..end];
        out.push_str(values.get(key).ok_or_else(|| format!("{name}: no value for {{{{{key}}}}}"))?);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn alternates() -> String {
    let mut links: Vec<String> =
        LANGS.iter().map(|(l, _)| format!("<link rel=\"alternate\" hreflang=\"{l}\" href=\"{BASE}{}\">", path_of(l))).collect();
    links.push(format!("<link rel=\"alternate\" hreflang=\"x-default\" href=\"{BASE}\">"));
    links.join("\n")
}

fn og_locales(locale: &str) -> String {
    let mut tags = vec![format!("<meta property=\"og:locale\" content=\"{locale}\">")];
    tags.extend(LANGS.iter().filter(|(_, l)| *l != locale).map(|(_, l)| format!("<meta property=\"og:locale:alternate\" content=\"{l}\">")));
    tags.join("\n")
}

/// The language menu: a `<details>` that needs no script.
fn lang_menu(lang: &str, here: &str, strings: &BTreeMap<&str, BTreeMap<String, String>>) -> Result<String, String> {
    let name = |l: &str| strings.get(l).and_then(|m| m.get("langName")).cloned().ok_or_else(|| format!("{l}: no langName"));
    let root = "../".repeat(here.matches('/').count());
    let mut items = String::new();
    for (l, _) in LANGS {
        let code = l.to_uppercase();
        if *l == lang {
            items.push_str(&format!("<li><span aria-current=\"page\" lang=\"{l}\"><b>{code}</b>{}</span></li>", name(l)?));
        } else {
            items.push_str(&format!("<li><a href=\"{root}{}\" hreflang=\"{l}\" lang=\"{l}\"><b>{code}</b>{}</a></li>", path_of(l), name(l)?));
        }
    }
    let label = strings.get(lang).and_then(|m| m.get("langLabel")).cloned().unwrap_or_default();
    Ok(format!(
        "<details class=\"lang-menu\"><summary aria-label=\"{label}: {}\"><b>{}</b></summary><ul>{items}</ul></details>",
        name(lang)?,
        lang.to_uppercase()
    ))
}

/// The app as search engines describe it (schema.org SoftwareApplication), in the page's
/// language; `<` escaped so no string can close the script.
fn json_ld(lang: &str, s: &BTreeMap<String, String>, version: &str) -> Result<String, String> {
    let description = s.get("description").ok_or("no description")?;
    let ld = json!({
        "@context": "https://schema.org",
        "@type": "SoftwareApplication",
        "name": "Strix",
        "alternateName": s.get("title"),
        "description": description,
        "url": format!("{BASE}{}", path_of(lang)),
        "inLanguage": lang,
        "applicationCategory": "EducationalApplication",
        "applicationSubCategory": "Engineering calculation document editor",
        "operatingSystem": "Windows 10, Windows 11, macOS, Linux, FreeBSD, web browser",
        "processorRequirements": "x64, x86, ARM64, RISC-V 64",
        "softwareVersion": version,
        "downloadUrl": format!("{REPO}/releases/latest"),
        "codeRepository": REPO,
        "license": "https://spdx.org/licenses/MIT.html",
        "isAccessibleForFree": true,
        "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
        "image": format!("{BASE}assets/social.png"),
        "screenshot": format!("{BASE}assets/shots/{lang}/shot-calc-light.png"),
        "author": {"@type": "Person", "name": "nabre", "url": "https://github.com/nabre"},
        "keywords": KEYWORDS.join(", "),
    });
    serde_json::to_string(&ld).map(|t| t.replace('<', "\\u003c")).map_err(|e| e.to_string())
}

fn sitemap() -> String {
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    for (lang, _) in LANGS {
        x.push_str(&format!("  <url>\n    <loc>{BASE}{}</loc>\n", path_of(lang)));
        for (l, _) in LANGS {
            x.push_str(&format!("    <xhtml:link rel=\"alternate\" hreflang=\"{l}\" href=\"{BASE}{}\"/>\n", path_of(l)));
        }
        x.push_str(&format!("    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{BASE}\"/>\n  </url>\n"));
    }
    x.push_str("</urlset>\n");
    x
}

/// GitHub Pages serves it for an address that has no page: absolute links, as it may be shown
/// at any depth.
fn not_found() -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Strix: page not found</title>\n<meta name=\"robots\" content=\"noindex\">\n<link rel=\"icon\" type=\"image/svg+xml\" href=\"{BASE}assets/strix.svg\">\n<link rel=\"stylesheet\" href=\"{BASE}assets/site.css\">\n</head>\n<body>\n<main class=\"wrap notfound\">\n<img src=\"{BASE}assets/strix.svg\" alt=\"\" width=\"96\" height=\"96\">\n<h1>Page not found</h1>\n<p class=\"lead\">This address has no page. <a href=\"{BASE}\">Strix home</a> · <a href=\"{BASE}it/\">Italiano</a> · <a href=\"{BASE}fr/\">Français</a> · <a href=\"{BASE}de/\">Deutsch</a></p>\n</main>\n</body>\n</html>\n"
    )
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// Every page there and in the sitemap, no key left, every relative link and image a file.
fn check(out: &Path) -> Result<(), String> {
    let mut problems = Vec::new();
    let sitemap = fs::read_to_string(out.join("sitemap.xml")).map_err(|e| format!("sitemap.xml: {e}"))?;
    for (lang, _) in LANGS {
        let here = path_of(lang);
        let file = out.join(&here).join("index.html");
        let Ok(html) = fs::read_to_string(&file) else {
            problems.push(format!("missing {here}index.html"));
            continue;
        };
        if !sitemap.contains(&format!("<loc>{BASE}{here}</loc>")) {
            problems.push(format!("not in sitemap.xml: {here}"));
        }
        if html.contains("{{") {
            problems.push(format!("{here}index.html: a {{{{key}}}} is left"));
        }
        for link in links(&html) {
            let target = link.split(['?', '#']).next().unwrap_or_default();
            let external = target
                .split_once(':')
                .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)));
            if target.is_empty() || external {
                continue;
            }
            let path = out.join(&here).join(target);
            let found = if path.is_dir() { path.join("index.html").is_file() } else { path.is_file() };
            if !found {
                problems.push(format!("{here}index.html: broken link {target}"));
            }
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(format!("site check failed:\n  {}", problems.join("\n  "))) }
}

/// The values of every `href`, `src` and `srcset` attribute (one address per srcset here).
fn links(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    for attr in [" href=\"", " src=\"", " srcset=\""] {
        let mut rest = html;
        while let Some(i) = rest.find(attr) {
            let after = &rest[i + attr.len()..];
            let end = after.find('"').unwrap_or(after.len());
            found.push(after[..end].to_string());
            rest = &after[end..];
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_keys_in_one_pass() {
        let mut v = BTreeMap::new();
        v.insert("a".to_string(), "{{b}}".to_string());
        v.insert("b".to_string(), "x".to_string());
        assert_eq!(fill("<{{a}}|{{b}}>", &v, "t").unwrap(), "<{{b}}|x>");
        assert!(fill("{{missing}}", &v, "t").unwrap_err().contains("missing"));
        assert!(fill("{{a", &v, "t").is_err());
    }

    #[test]
    fn includes_partials() {
        let mut p = BTreeMap::new();
        p.insert("head".to_string(), "<meta>".to_string());
        assert_eq!(include_partials("<head>{{> head}}</head>{{k}}", &p, "t").unwrap(), "<head><meta></head>{{k}}");
        assert!(include_partials("{{> nope}}", &p, "t").is_err());
    }

    #[test]
    fn reads_the_workspace_version() {
        let toml = "[workspace]\nmembers = []\n\n[workspace.package]\nedition = \"2024\"\nversion = \"0.13.0\"\n\n[dependencies]\nversion = \"9\"\n";
        assert_eq!(workspace_version(toml).unwrap(), "0.13.0");
        assert!(workspace_version("[package]\nversion = \"1.0.0\"\n").is_err());
    }

    #[test]
    fn finds_links_and_skips_none() {
        let l = links("<a href=\"x/\">a</a><img src=\"i.png\"><source srcset=\"d.png\"><a href=\"https://e\">");
        assert_eq!(l, vec!["x/", "https://e", "i.png", "d.png"]);
    }

    #[test]
    fn every_language_has_a_path() {
        assert_eq!(path_of("en"), "");
        assert_eq!(path_of("it"), "it/");
        assert!(sitemap().contains("hreflang=\"x-default\""));
    }
}
