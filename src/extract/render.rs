//! Markdown rendering: frontmatter + blocks, with token-war
//! policies (link-farm drops, bare-link line drops, table caps).

use super::blocks::Block;
use super::metadata::Meta;

pub fn render(meta: &Meta, url: &str, kept: &[&Block], opts: &super::ExtractOptions) -> String {
    // Repeated boilerplate sections collapse before rendering
    // (#288): see `collapse_repeats`.
    let kept = collapse_repeats(kept);
    let mut out = String::new();

    // Frontmatter : compact, agent-first.
    let first_is_title = kept.first().is_some_and(|b| {
        matches!(b, Block::Heading { level: 1, text, .. }
            if Some(text) == meta.title.as_ref())
    });
    if let Some(t) = &meta.title
        && !first_is_title
    {
        out.push_str(&format!("# {t}\n"));
    }
    let mut byline_parts: Vec<&str> = Vec::new();
    if let Some(s) = &meta.site {
        byline_parts.push(s);
    }
    if let Some(b) = &meta.byline {
        byline_parts.push(b);
    }
    if let Some(p) = &meta.published {
        byline_parts.push(p);
    }
    if !byline_parts.is_empty() {
        out.push_str(&byline_parts.join(" · "));
        out.push('\n');
    }
    out.push_str(url);
    out.push('\n');
    // Description as a one-line summary : agents use it to
    // decide relevance before reading the body. Always surface it
    // (capped): for JS-rendered SPAs the meta description is often
    // the only real content in the initial HTML.
    if let Some(d) = &meta.description {
        let trimmed: String = d.chars().take(500).collect();
        out.push_str(&format!("> {}\n", trimmed));
    }
    out.push('\n');

    let mut last_path: Vec<String> = Vec::new();
    let mut last_was_heading = true; // frontmatter counts
    let mut title_heading_dropped = false;
    // Cross-block exact-duplicate suppression: badge
    // dupes, repeated teasers. Keyed on normalized text.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for block in &kept {
        match block {
            Block::Heading { level, text, .. } => {
                // Skip the H1 that repeats the frontmatter title :
                // but only if the frontmatter title was actually
                // shown. If first_is_title, the frontmatter was
                // skipped, so this H1 IS the title display.
                if !title_heading_dropped
                    && *level == 1
                    && Some(text) == meta.title.as_ref()
                    && !first_is_title
                {
                    title_heading_dropped = true;
                    continue;
                }
                out.push_str(&format!("{} {text}\n\n", "#".repeat(*level as usize)));
                last_path = block.path().to_vec();
            }
            Block::Para {
                md, link_density, ..
            } => {
                // Bare-link / one-word lines: pure noise.
                if md.len() < 25 && *link_density > 0.9 {
                    continue;
                }
                // A widget's serialized data dumped into a text node
                // is not prose (#288).
                if looks_like_data_blob(md) {
                    continue;
                }
                if !seen.insert(normalize(md)) {
                    continue; // exact duplicate of an earlier block
                }
                // Bare numbers: vote counts, rank numbers.
                if md.len() < 8 && md.chars().all(|c| c.is_ascii_digit() || c == ',') {
                    continue;
                }
                // Wiki section-edit junk: "[edit]", "[ edit ]".
                if md.len() < 14 {
                    let inner = md.trim_matches(['[', ']']).trim();
                    if !inner.is_empty()
                        && inner.chars().all(|c| c.is_alphabetic() || c == ' ')
                        && md.starts_with('[')
                        && md.ends_with(']')
                    {
                        continue;
                    }
                }
                emit_path(
                    &mut out,
                    block.path(),
                    &mut last_path,
                    &mut last_was_heading,
                );
                out.push_str(md);
                out.push_str("\n\n");
            }
            Block::List {
                ordered,
                items,
                link_density,
                ..
            } => {
                // Link-farm drop: many items, all bare links.
                if items.len() > 6 && *link_density > 0.8 && !opts.include_links {
                    continue;
                }
                emit_path(
                    &mut out,
                    block.path(),
                    &mut last_path,
                    &mut last_was_heading,
                );
                push_list(&mut out, items, *ordered);
            }
            Block::Table {
                headers,
                rows,
                truncated,
                ..
            } => {
                emit_path(
                    &mut out,
                    block.path(),
                    &mut last_path,
                    &mut last_was_heading,
                );
                let cols = headers
                    .len()
                    .max(rows.first().map(|r| r.len()).unwrap_or(0));
                if cols == 0 {
                    continue;
                }
                let mut h = headers.clone();
                h.resize(cols, String::new());
                out.push_str(&format!("| {} |\n", h.join(" | ")));
                out.push_str(&format!("|{}\n", " --- |".repeat(cols)));
                for row in rows {
                    let mut r = row.clone();
                    r.resize(cols, String::new());
                    out.push_str(&format!("| {} |\n", r.join(" | ")));
                }
                if *truncated {
                    out.push_str("*(table truncated)*\n");
                }
                out.push('\n');
            }
            Block::Code { lang, code, .. } => {
                emit_path(
                    &mut out,
                    block.path(),
                    &mut last_path,
                    &mut last_was_heading,
                );
                let fence = code_fence(code);
                out.push_str(&format!(
                    "{fence}{}\n{code}\n{fence}\n\n",
                    lang.as_deref().unwrap_or("")
                ));
            }
            Block::Quote { md, .. } => {
                emit_path(
                    &mut out,
                    block.path(),
                    &mut last_path,
                    &mut last_was_heading,
                );
                for line in md.lines() {
                    out.push_str(&format!("> {line}\n"));
                }
                out.push('\n');
            }
            Block::Media { alt, src, .. } => {
                // Token war: media lines are opt-in. (Segmentation
                // still records them for on-demand OCR.)
                if !opts.include_media {
                    continue;
                }
                emit_path(
                    &mut out,
                    block.path(),
                    &mut last_path,
                    &mut last_was_heading,
                );
                out.push_str(&format!("![{alt}]({src})\n\n"));
            }
        }
        last_was_heading = false;
    }

    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

/// #288: repeated boilerplate sections. Upsell blocks repeat per
/// plan with the same heading and a near-identical body, and
/// "Add to your order" headings stack. Two conservative collapses,
/// applied before rendering:
/// - an immediately repeated heading (same level, same normalized
///   text) is kept once;
/// - a section whose normalized body repeats its predecessor's is
///   dropped whole: the first copy already showed it. Bodies under
///   160 normalized chars never qualify, so short same-named
///   sections on one page survive ("Overview" twice is structure).
///
/// Near-identical = equal after digits and punctuation are stripped,
/// or a token-set Jaccard of 0.85 inside a 4k-char cap.
fn collapse_repeats<'a>(kept: &[&'a Block]) -> Vec<&'a Block> {
    const MIN_BODY: usize = 160;
    const JACCARD_MIN: f64 = 0.85;
    const JACCARD_CAP: usize = 4_000;

    fn norm(s: &str) -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    fn norm_body(s: &str) -> String {
        let stripped: String = s
            .chars()
            .filter(|c| !c.is_ascii_digit() && !c.is_ascii_punctuation())
            .collect();
        norm(&stripped)
    }
    fn near_identical(a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        if a.len() > JACCARD_CAP || b.len() > JACCARD_CAP {
            return false;
        }
        let toks = |s: &str| -> std::collections::HashSet<String> {
            s.split_whitespace().map(str::to_string).collect()
        };
        let (ta, tb) = (toks(a), toks(b));
        if ta.is_empty() || tb.is_empty() {
            return false;
        }
        let inter = ta.intersection(&tb).count() as f64;
        let union = ta.union(&tb).count() as f64;
        union > 0.0 && inter / union >= JACCARD_MIN
    }

    let mut out: Vec<&Block> = Vec::with_capacity(kept.len());
    let mut last_section: Option<(String, String)> = None;
    let mut prev_heading: Option<(u8, String)> = None;
    let mut i = 0usize;
    while i < kept.len() {
        match kept[i] {
            Block::Heading { level, text, .. } => {
                let h = norm(text);
                if prev_heading
                    .as_ref()
                    .is_some_and(|(l, t)| l == level && t == &h)
                {
                    i += 1;
                    continue;
                }
                // The section: this heading through the block
                // before the next heading of same-or-higher level.
                let mut end = i + 1;
                while end < kept.len() {
                    if let Block::Heading { level: l2, .. } = kept[end]
                        && l2 <= level
                    {
                        break;
                    }
                    end += 1;
                }
                let body = kept[i + 1..end]
                    .iter()
                    .map(|b| b.text())
                    .collect::<Vec<_>>()
                    .join(" ");
                let nb = norm_body(&body);
                let repeated = nb.len() >= MIN_BODY
                    && last_section
                        .as_ref()
                        .is_some_and(|(h2, b2)| h2 == &h && near_identical(b2, &nb));
                if repeated {
                    i = end;
                    continue;
                }
                if nb.len() >= MIN_BODY {
                    last_section = Some((h.clone(), nb));
                } else {
                    last_section = None;
                }
                prev_heading = Some((*level, h));
                out.push(kept[i]);
                i += 1;
            }
            _ => {
                prev_heading = None;
                out.push(kept[i]);
                i += 1;
            }
        }
    }
    out
}

/// Emit `list_items` output as markdown: "  " indentation per
/// nesting level is already in each item; top-level items of an
/// ordered list are numbered (nested ones get "-"), and the
/// number counts top-level items only -- nested entries used to
/// advance it, so "1. First / - Sub / 3. Second".
pub(crate) fn push_list(out: &mut String, items: &[String], ordered: bool) {
    let mut n = 0;
    for item in items {
        let indent: String = item.chars().take_while(|c| *c == ' ').collect();
        let body = item.trim_start();
        let bullet = if ordered && indent.is_empty() {
            n += 1;
            format!("{n}. ")
        } else {
            "- ".to_string()
        };
        out.push_str(&format!("{indent}{bullet}{body}\n"));
    }
    out.push('\n');
}

/// A paragraph that is a serialized data blob rather than prose:
/// JSON object/array syntax with real key density (#288: an
/// injected buy-box widget's JSON landed in the output as a
/// paragraph). The key-count and length floors keep prose ABOUT
/// JSON, which practically never opens with a brace and carries
/// `":` runs, out of the net.
fn looks_like_data_blob(md: &str) -> bool {
    let t = md.trim_start();
    if t.len() < 80 || !(t.starts_with('{') || t.starts_with('[')) {
        return false;
    }
    t.matches("\":").count() >= 4
}

fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Emit the heading breadcrumb when the path changes mid-focus
/// (gives agents section context for sliced blocks).
fn emit_path(
    out: &mut String,
    path: &[String],
    last_path: &mut Vec<String>,
    last_was_heading: &mut bool,
) {
    if path.is_empty() || *last_was_heading {
        return;
    }
    // Emit any headings in the path that aren't already shown.
    let common = path
        .iter()
        .zip(last_path.iter())
        .take_while(|(a, b)| a == b)
        .count();
    for (i, h) in path.iter().enumerate().skip(common) {
        out.push_str(&format!("{} {h}\n\n", "#".repeat(i + 1)));
    }
    *last_path = path.to_vec();
    *last_was_heading = true;
}

/// Fence for a code block: one backtick longer than the longest
/// backtick run inside it (min 3). A `<pre>` that itself shows a
/// markdown fence -- every "how to write markdown" page, every
/// README rendered by a docs site -- would otherwise close the
/// block at its inner ``` and spill the rest as prose.
pub(crate) fn code_fence(code: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in code.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

#[cfg(test)]
mod tests {
    use super::code_fence;

    #[test]
    fn fence_is_one_longer_than_the_longest_backtick_run() {
        assert_eq!(code_fence("fn main() {}"), "```");
        assert_eq!(code_fence("a `b` c"), "```");
        assert_eq!(code_fence("a ``b`` c"), "```");
        assert_eq!(code_fence("```rust\nx\n```"), "````");
        assert_eq!(code_fence("x\n````\ny"), "`````");
        assert_eq!(code_fence("`````\n"), "``````");
        assert_eq!(code_fence(""), "```");
    }
}

#[cfg(test)]
mod repeat_collapse_tests {
    use super::*;

    fn heading(level: u8, text: &str) -> Block {
        Block::Heading {
            level,
            text: text.to_string(),
            path: vec![text.to_string()],
        }
    }
    fn para(md: &str) -> Block {
        Block::Para {
            md: md.to_string(),
            link_density: 0.0,
            path: Vec::new(),
        }
    }

    const TERMS_A: &str = "Protect your purchase with an Asurion plan covering accidental damage, drops, spills, and mechanical failure for $24.99, with 24/7 support, no deductibles on approved claims, and cancellation any time.";
    const TERMS_B: &str = "Protect your purchase with an Asurion plan covering accidental damage, drops, spills, and mechanical failure for $31.99, with 24/7 support, no deductibles on approved claims, and cancellation any time.";

    // #288: the repeated upsell sections collapse to one; the digits
    // differ, everything else repeats.
    #[test]
    fn a_repeated_upsell_section_collapses_to_one() {
        let blocks = [
            heading(3, "Product Protection by Asurion, LLC"),
            para(TERMS_A),
            heading(3, "Product Protection by Asurion, LLC"),
            para(TERMS_B),
            heading(3, "Product Protection by Asurion, LLC"),
            para(TERMS_B),
        ];
        let refs: Vec<&Block> = blocks.iter().collect();
        let kept = collapse_repeats(&refs);
        assert_eq!(kept.len(), 2, "one heading + one body survive");
        assert!(kept[1].text().contains("24.99"), "first copy kept");
    }

    // Adjacent identical headings stack on real pages; keep one.
    #[test]
    fn adjacent_duplicate_headings_keep_one() {
        let blocks = [
            heading(3, "Add to your order"),
            heading(3, "Add to your order"),
            para(TERMS_A),
        ];
        let refs: Vec<&Block> = blocks.iter().collect();
        assert_eq!(collapse_repeats(&refs).len(), 2);
    }

    // Short same-named sections are structure, not boilerplate.
    #[test]
    fn short_same_named_sections_survive() {
        let blocks = [
            heading(2, "Overview"),
            para("First part."),
            heading(2, "Overview"),
            para("Second part."),
        ];
        let refs: Vec<&Block> = blocks.iter().collect();
        assert_eq!(collapse_repeats(&refs).len(), 4);
    }

    // Long sections with genuinely different bodies are content.
    #[test]
    fn different_bodies_below_the_same_heading_survive() {
        let a = "Alpha ".repeat(40);
        let b = "Beta ".repeat(40);
        let blocks = [
            heading(2, "Example"),
            para(&a),
            heading(2, "Example"),
            para(&b),
        ];
        let refs: Vec<&Block> = blocks.iter().collect();
        assert_eq!(collapse_repeats(&refs).len(), 4);
    }

    // #288 item 3: an injected widget's JSON is dropped, and prose
    // that merely talks about JSON survives.
    #[test]
    fn a_json_data_blob_is_dropped_but_prose_about_json_survives() {
        let blob = r#"{"desktop_buybox_group_1":[{"displayPrice":"$31.99","priceAmount":31.99,"currencySymbol":"$","integerValue":"31","decimalSeparator":".","fractionalValue":"99","symbolPosition":"left"}]}"#;
        assert!(looks_like_data_blob(blob));
        assert!(looks_like_data_blob(&format!("[{blob}]")));
        assert!(!looks_like_data_blob(
            "The API answers with {\"ok\": true} and that is all it says."
        ));
        assert!(!looks_like_data_blob(
            "{\"a\":1} is a valid JSON document that you can parse."
        ));
        assert!(!looks_like_data_blob(
            "A normal paragraph of prose with no braces at all, long enough to pass any length gate."
        ));
    }
}
