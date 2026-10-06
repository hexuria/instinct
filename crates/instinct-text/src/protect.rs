//! Protected span detection (spec §4.2): fenced and inline code, URLs, paths, quoted text,
//! version numbers and decimals. Runs on NFC text **before** folding, and is equivariant by
//! construction: every test is case-insensitive or case-free and depends only on
//! whitespace-delimited token boundaries, so folding case or widening a whitespace run outside a
//! span can neither create nor destroy a span.

use core::ops::Range;

/// What kind of protected span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtectedKind {
    /// ```` ``` ```` fenced block (an unterminated fence runs to the end of the text).
    FencedCode,
    /// `` `inline` `` code (an unterminated backtick protects nothing).
    InlineCode,
    /// `scheme://…` or `www.…`.
    Url,
    /// A token containing `/` or `\` (with a letter or digit), or starting with `~/`, `./`, `../`.
    Path,
    /// `"double"` or `“curly”` quoted text (unterminated quotes protect nothing).
    Quoted,
    /// A version or decimal such as `1.2`, `v2.0.1`.
    Number,
}

impl ProtectedKind {
    /// Stable name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::FencedCode => "fenced_code",
            Self::InlineCode => "inline_code",
            Self::Url => "url",
            Self::Path => "path",
            Self::Quoted => "quoted",
            Self::Number => "number",
        }
    }
}

const TRAILING_PUNCT: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"', '>'];

fn pair_spans(
    s: &str,
    open: &str,
    close: &str,
    kind: ProtectedKind,
    to_end: bool,
    out: &mut Vec<(Range<usize>, ProtectedKind)>,
) {
    let mut i = 0;
    while let Some(off) = s[i..].find(open) {
        let start = i + off;
        if covered(out, start) {
            i = start + open.len();
            continue;
        }
        let after = start + open.len();
        let Some(off2) = s[after..].find(close) else {
            if to_end {
                out.push((start..s.len(), kind));
            }
            return;
        };
        let end = after + off2 + close.len();
        if overlaps(out, &(start..end)) {
            i = after;
            continue;
        }
        out.push((start..end, kind));
        i = end;
    }
}

fn covered(spans: &[(Range<usize>, ProtectedKind)], pos: usize) -> bool {
    spans.iter().any(|(r, _)| r.start <= pos && pos < r.end)
}

fn overlaps(spans: &[(Range<usize>, ProtectedKind)], r: &Range<usize>) -> bool {
    spans
        .iter()
        .any(|(s, _)| s.start < r.end && r.start < s.end)
}

fn is_url(tok: &str) -> bool {
    let lower: String = tok
        .chars()
        .take(8)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    ["http://", "https://", "ftp://", "file://", "www."]
        .iter()
        .any(|p| lower.starts_with(p))
        || tok.contains("://")
}

fn is_path(tok: &str) -> bool {
    tok.starts_with("~/")
        || tok.starts_with("./")
        || tok.starts_with("../")
        || ((tok.contains('/') || tok.contains('\\')) && tok.chars().any(char::is_alphanumeric))
}

fn is_number(tok: &str) -> bool {
    let t = tok.strip_prefix(['v', 'V']).unwrap_or(tok);
    let mut parts = t.split('.');
    let first_ok = parts
        .next()
        .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let mut rest = 0;
    for p in parts {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        rest += 1;
    }
    first_ok && rest >= 1
}

/// Detects protected spans in `s` (NFC text). Returns sorted, non-overlapping byte ranges.
pub(crate) fn detect(s: &str) -> Vec<(Range<usize>, ProtectedKind)> {
    let mut out: Vec<(Range<usize>, ProtectedKind)> = Vec::new();
    pair_spans(s, "```", "```", ProtectedKind::FencedCode, true, &mut out);
    pair_spans(s, "`", "`", ProtectedKind::InlineCode, false, &mut out);
    pair_spans(s, "\"", "\"", ProtectedKind::Quoted, false, &mut out);
    pair_spans(
        s,
        "\u{201c}",
        "\u{201d}",
        ProtectedKind::Quoted,
        false,
        &mut out,
    );
    // Whitespace-delimited tokens for URL / path / number.
    let mut start = None;
    let mut tokens = Vec::new();
    for (i, c) in s.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(st)) => {
                tokens.push(st..i);
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(st) = start {
        tokens.push(st..s.len());
    }
    for r in tokens {
        let raw = &s[r.clone()];
        let trimmed = raw.trim_end_matches(TRAILING_PUNCT);
        let lead = raw.len() - raw.trim_start_matches(['(', '[', '<', '{']).len();
        if trimmed.len() <= lead {
            continue;
        }
        let core = &raw[lead..trimmed.len()];
        let kind = if is_url(core) {
            ProtectedKind::Url
        } else if is_path(core) {
            ProtectedKind::Path
        } else if is_number(core) {
            ProtectedKind::Number
        } else {
            continue;
        };
        let span = r.start + lead..r.start + trimmed.len();
        if !overlaps(&out, &span) {
            out.push((span, kind));
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<(&str, ProtectedKind)> {
        detect(s).into_iter().map(|(r, k)| (&s[r], k)).collect()
    }

    #[test]
    fn spans_before_after_and_touching_a_fence_survive() {
        use ProtectedKind::*;
        let fence = ("```x```", FencedCode);
        assert_eq!(kinds("`y` ```x```"), vec![("`y`", InlineCode), fence]);
        assert_eq!(kinds("```x``` `y`"), vec![fence, ("`y`", InlineCode)]);
        assert_eq!(kinds("\"a\"```x```"), vec![("\"a\"", Quoted), fence]);
        assert_eq!(kinds("```x```\"a\""), vec![fence, ("\"a\"", Quoted)]);
        assert_eq!(kinds("```x``` \"a\""), vec![fence, ("\"a\"", Quoted)]);
    }

    #[test]
    fn bare_relative_path_prefixes_are_paths() {
        use ProtectedKind::*;
        assert_eq!(kinds("cd ~/ now"), vec![("~/", Path)]);
        assert_eq!(kinds("cd ./ now"), vec![("./", Path)]);
        assert_eq!(kinds("cd ../ now"), vec![("../", Path)]);
        assert_eq!(kinds("a / b"), vec![]);
    }

    #[test]
    fn detects_each_kind() {
        use ProtectedKind::*;
        assert_eq!(kinds("run `stop` now"), vec![("`stop`", InlineCode)]);
        assert_eq!(
            kinds("a ```x\nstop``` b"),
            vec![("```x\nstop```", FencedCode)]
        );
        assert_eq!(kinds("a ```x stop"), vec![("```x stop", FencedCode)]);
        assert_eq!(
            kinds("see HTTPS://x.io/stop."),
            vec![("HTTPS://x.io/stop", Url)]
        );
        assert_eq!(kinds("edit src/stop.rs, then"), vec![("src/stop.rs", Path)]);
        assert_eq!(kinds("say \"stop\" ok"), vec![("\"stop\"", Quoted)]);
        assert_eq!(
            kinds("say \u{201c}stop\u{201d}"),
            vec![("\u{201c}stop\u{201d}", Quoted)]
        );
        assert_eq!(kinds("bump to v1.2.3."), vec![("v1.2.3", Number)]);
        assert_eq!(kinds("pi is 3.14"), vec![("3.14", Number)]);
        assert_eq!(kinds("(~/notes)"), vec![("~/notes", Path)]);
        assert_eq!(kinds("www.example.com"), vec![("www.example.com", Url)]);
    }

    #[test]
    fn unterminated_inline_and_quotes_protect_nothing() {
        assert_eq!(kinds("a `stop").len(), 0);
        assert_eq!(kinds("a \"stop").len(), 0);
        assert_eq!(kinds("3. stop. 1.x v.2").len(), 0);
        assert_eq!(kinds("/").len(), 0);
        assert_eq!(kinds("...").len(), 0);
    }

    #[test]
    fn spans_do_not_overlap() {
        let s = "`a \"b\" c` \"d `e` f\" http://x/`y` 1.2";
        let spans = detect(s);
        for w in spans.windows(2) {
            assert!(w[0].0.end <= w[1].0.start, "{spans:?}");
        }
        assert_eq!(ProtectedKind::Url.name(), "url");
    }
}
