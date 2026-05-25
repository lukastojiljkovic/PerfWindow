//! In-app changelog viewer.
//!
//! The application's `CHANGELOG.md` is embedded into the binary at compile
//! time via [`include_str!`]; opening the modal runs a tiny hand-rolled
//! markdown scanner over it and paints the result as themed text. This keeps
//! the viewer offline and dependency-free — no `pulldown_cmark`, no browser
//! hand-off, no network.

use crate::theme::Theme;
use egui::{RichText, ScrollArea};

/// The file contents, frozen at compile time. The path is relative to this
/// source file: `dashboard/src/ui/changelog_modal.rs` → repo root.
pub const CHANGELOG_TEXT: &str = include_str!("../../../CHANGELOG.md");

/// A single structural element extracted from the changelog markdown.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangelogNode {
    /// `## [X.Y.Z] — YYYY-MM-DD` — `tag` and `date` are split apart and
    /// stripped of their square brackets and the leading em-dash.
    VersionHeader { tag: String, date: String },
    /// `### Added` / `### Changed` / `### Fixed` / `### Removed`.
    Subsection(String),
    /// A `- ` bullet item, with wrapped continuation lines folded in and
    /// inline markdown (`**bold**`, `*italic*`, `` `code` ``, `[text](url)`)
    /// preserved as typed spans so the renderer can paint them properly.
    Bullet(Vec<InlineSpan>),
}

/// One run of inline text inside a bullet. Produced by [`parse_inline_md`]
/// and consumed by [`render_bullet`] to pick the right `egui` styling.
#[derive(Debug, Clone, PartialEq)]
pub enum InlineSpan {
    Plain(String),
    Bold(String),
    Italic(String),
    Code(String),
    Link { text: String, url: String },
}

/// Line-based scanner over a CHANGELOG.md whose schema follows Keep a
/// Changelog 1.1.0. Skips the `## [Unreleased]` section and the trailing
/// `[X.Y.Z]: https://…` link reference block.
pub fn parse_changelog(md: &str) -> Vec<ChangelogNode> {
    let mut out: Vec<ChangelogNode> = Vec::new();
    let mut buffer: Option<String> = None;
    // While true, every non-structural line is discarded — used to skip the
    // `## [Unreleased]` section so it doesn't bleed into the next version.
    let mut suppress = false;

    let flush = |buffer: &mut Option<String>, out: &mut Vec<ChangelogNode>| {
        if let Some(b) = buffer.take() {
            out.push(ChangelogNode::Bullet(parse_inline_md(&b)));
        }
    };

    for raw_line in md.lines() {
        let line = raw_line.trim_end();
        let starts_structural = line.starts_with("##") || line.starts_with("- ") || line.is_empty();
        if starts_structural {
            flush(&mut buffer, &mut out);
        }

        // Trailing link references like `[0.3.0]: https://...`.
        if line.starts_with('[') && line.contains("]: http") {
            continue;
        }

        if let Some(rest) = line.strip_prefix("## [") {
            if let Some(end) = rest.find(']') {
                let tag = rest[..end].to_string();
                if tag.eq_ignore_ascii_case("unreleased") {
                    suppress = true;
                    continue;
                }
                suppress = false;
                let after = &rest[end + 1..]; // ` — YYYY-MM-DD` or empty
                let date = after
                    .trim_start_matches(' ')
                    .trim_start_matches('\u{2014}') // em dash
                    .trim_start_matches('-')
                    .trim_start()
                    .to_string();
                out.push(ChangelogNode::VersionHeader { tag, date });
                continue;
            }
        }

        if suppress {
            continue;
        }

        if let Some(rest) = line.strip_prefix("### ") {
            out.push(ChangelogNode::Subsection(rest.to_string()));
            continue;
        }

        if let Some(rest) = line.strip_prefix("- ") {
            buffer = Some(rest.to_string());
            continue;
        }

        // Continuation of an open bullet (an indented or follow-on line).
        if let Some(b) = buffer.as_mut() {
            if !line.is_empty() {
                b.push(' ');
                b.push_str(line.trim_start());
            }
        }
    }
    flush(&mut buffer, &mut out);
    out
}

/// Split a bullet body into typed [`InlineSpan`]s. Handles `**bold**`,
/// `*italic*`, `` `code` ``, and `[text](url)`. Anything else is `Plain`.
/// Unbalanced delimiters degrade gracefully into `Plain` runs so a single
/// stray `*` or `` ` `` cannot eat the rest of the bullet.
pub fn parse_inline_md(s: &str) -> Vec<InlineSpan> {
    let mut spans: Vec<InlineSpan> = Vec::new();
    let bytes = s.as_bytes();
    let mut plain_start = 0usize;
    let mut i = 0usize;

    let push_plain = |spans: &mut Vec<InlineSpan>, src: &str, from: usize, to: usize| {
        if to > from {
            spans.push(InlineSpan::Plain(src[from..to].to_string()));
        }
    };

    while i < bytes.len() {
        // `**bold**`
        if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some(end) = find_substring(&bytes[i + 2..], b"**") {
                push_plain(&mut spans, s, plain_start, i);
                let text = s[i + 2..i + 2 + end].to_string();
                spans.push(InlineSpan::Bold(text));
                i += 2 + end + 2;
                plain_start = i;
                continue;
            }
        }
        // `*italic*` — only when both delimiters are flanked by non-`*` bytes,
        // so `**bold**` (handled above) never falls through to here.
        if bytes[i] == b'*' && (i + 1 >= bytes.len() || bytes[i + 1] != b'*') {
            if let Some(end) = find_byte(&bytes[i + 1..], b'*') {
                let close = i + 1 + end;
                let after_close = close + 1;
                let next_is_star = after_close < bytes.len() && bytes[after_close] == b'*';
                if !next_is_star && end > 0 {
                    push_plain(&mut spans, s, plain_start, i);
                    spans.push(InlineSpan::Italic(s[i + 1..close].to_string()));
                    i = close + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        // `` `code` ``
        if bytes[i] == b'`' {
            if let Some(end) = find_byte(&bytes[i + 1..], b'`') {
                push_plain(&mut spans, s, plain_start, i);
                spans.push(InlineSpan::Code(s[i + 1..i + 1 + end].to_string()));
                i += 1 + end + 1;
                plain_start = i;
                continue;
            }
        }
        // `[text](url)`
        if bytes[i] == b'[' {
            if let Some(close) = find_byte(&bytes[i + 1..], b']') {
                let text_end = i + 1 + close;
                if text_end + 1 < bytes.len() && bytes[text_end + 1] == b'(' {
                    if let Some(paren_end) = find_byte(&bytes[text_end + 2..], b')') {
                        let url_end = text_end + 2 + paren_end;
                        push_plain(&mut spans, s, plain_start, i);
                        spans.push(InlineSpan::Link {
                            text: s[i + 1..text_end].to_string(),
                            url: s[text_end + 2..url_end].to_string(),
                        });
                        i = url_end + 1;
                        plain_start = i;
                        continue;
                    }
                }
            }
        }
        // Advance one UTF-8 char.
        i = next_char_end(s, i);
    }
    push_plain(&mut spans, s, plain_start, bytes.len());
    spans
}

fn find_byte(haystack: &[u8], needle: u8) -> Option<usize> {
    haystack.iter().position(|&b| b == needle)
}

fn find_substring(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn next_char_end(s: &str, i: usize) -> usize {
    s[i..]
        .char_indices()
        .nth(1)
        .map(|(off, _)| i + off)
        .unwrap_or(s.len())
}

/// Render the changelog modal when `*open` is `true`. Closing via the window
/// ✕ flips `open` back to `false` automatically.
pub fn changelog_modal(ctx: &egui::Context, theme: &Theme, open: &mut bool) {
    if !*open {
        return;
    }

    let nodes = parse_changelog(CHANGELOG_TEXT);

    egui::Window::new("Changelog")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(open)
        .show(ctx, |ui| {
            ui.set_min_width(560.0);
            ui.set_max_width(640.0);
            let viewport_h = ctx
                .input(|i| i.viewport().inner_rect.map(|r| r.height()))
                .unwrap_or(720.0);
            let max_h = (viewport_h - 120.0).max(200.0);
            ScrollArea::vertical()
                .max_height(max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for node in nodes {
                        render_node(ui, theme, &node);
                    }
                });
        });
}

fn render_node(ui: &mut egui::Ui, theme: &Theme, node: &ChangelogNode) {
    match node {
        ChangelogNode::VersionHeader { tag, date } => {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("v{tag}"))
                        .family(theme.font_data.egui())
                        .size(14.0)
                        .color(theme.ink),
                );
                if !date.is_empty() {
                    ui.label(
                        RichText::new(date)
                            .family(theme.font_data.egui())
                            .size(11.0)
                            .color(theme.dim),
                    );
                }
            });
            ui.add_space(2.0);
        }
        ChangelogNode::Subsection(name) => {
            ui.add_space(4.0);
            ui.label(
                RichText::new(name.to_uppercase())
                    .family(theme.font_data.egui())
                    .size(10.0)
                    .color(theme.accent),
            );
        }
        ChangelogNode::Bullet(spans) => render_bullet(ui, theme, spans),
    }
}

/// Paint a single bullet — a centred dot plus one egui widget per inline
/// span. Uses `horizontal_wrapped` so long lines fold inside the modal's
/// fixed-width column.
fn render_bullet(ui: &mut egui::Ui, theme: &Theme, spans: &[InlineSpan]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(
            RichText::new("\u{00b7}  ")
                .family(theme.font_data.egui())
                .size(11.0)
                .color(theme.dim),
        );
        for span in spans {
            render_span(ui, theme, span);
        }
    });
}

/// Map one [`InlineSpan`] to its themed egui widget. `Bold` uses `.strong()`
/// for weight emphasis, `Italic` uses `.italics()`, `Code` is rendered in
/// egui's monospace family tinted with `theme.accent`, and `Link` becomes a
/// real clickable hyperlink (themed via `theme.accent`).
fn render_span(ui: &mut egui::Ui, theme: &Theme, span: &InlineSpan) {
    let family = theme.font_data.egui();
    match span {
        InlineSpan::Plain(t) => {
            ui.label(RichText::new(t).family(family).size(11.0).color(theme.ink));
        }
        InlineSpan::Bold(t) => {
            ui.label(
                RichText::new(t)
                    .family(family)
                    .size(11.0)
                    .color(theme.ink)
                    .strong(),
            );
        }
        InlineSpan::Italic(t) => {
            ui.label(
                RichText::new(t)
                    .family(family)
                    .size(11.0)
                    .color(theme.ink)
                    .italics(),
            );
        }
        InlineSpan::Code(t) => {
            ui.label(
                RichText::new(t)
                    .monospace()
                    .size(11.0)
                    .color(theme.accent)
                    .background_color(theme.bg),
            );
        }
        InlineSpan::Link { text, url } => {
            ui.hyperlink_to(
                RichText::new(text)
                    .family(family)
                    .size(11.0)
                    .color(theme.accent)
                    .underline(),
                url,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bullet_plain_text(node: &ChangelogNode) -> String {
        let ChangelogNode::Bullet(spans) = node else {
            panic!("not a bullet");
        };
        spans
            .iter()
            .map(|s| match s {
                InlineSpan::Plain(t)
                | InlineSpan::Bold(t)
                | InlineSpan::Italic(t)
                | InlineSpan::Code(t) => t.clone(),
                InlineSpan::Link { text, .. } => text.clone(),
            })
            .collect::<String>()
    }

    #[test]
    fn parses_a_version_header() {
        let md = "## [0.3.0] — 2026-05-24\n";
        let nodes = parse_changelog(md);
        assert_eq!(
            nodes,
            vec![ChangelogNode::VersionHeader {
                tag: "0.3.0".to_string(),
                date: "2026-05-24".to_string(),
            }]
        );
    }

    #[test]
    fn skips_the_unreleased_section() {
        let md = "## [Unreleased]\n\n## [0.1.0] — 2026-05-19\n";
        let nodes = parse_changelog(md);
        assert_eq!(nodes.len(), 1);
        assert!(matches!(
            &nodes[0],
            ChangelogNode::VersionHeader { tag, .. } if tag == "0.1.0"
        ));
    }

    #[test]
    fn folds_a_multi_line_bullet_into_one_string() {
        let md = "\
## [0.1.0] — 2026-05-19

### Added
- The Settings and Update modals can no longer extend past the bottom of
  the app window. Previously, on a non-maximised window, the modals grew.
";
        let nodes = parse_changelog(md);
        let bullets: Vec<&ChangelogNode> = nodes
            .iter()
            .filter(|n| matches!(n, ChangelogNode::Bullet(_)))
            .collect();
        assert_eq!(bullets.len(), 1);
        let text = bullet_plain_text(bullets[0]);
        assert!(text.contains("bottom of the app window"));
        assert!(text.contains("Previously"));
    }

    #[test]
    fn skips_trailing_link_references() {
        let md = "\
## [0.1.0] — 2026-05-19

### Added
- First.

[Unreleased]: https://example.com/compare/v0.1.0...HEAD
[0.1.0]: https://example.com/releases/tag/v0.1.0
";
        let nodes = parse_changelog(md);
        for n in &nodes {
            if let ChangelogNode::Bullet(spans) = n {
                for s in spans {
                    if let InlineSpan::Plain(t)
                    | InlineSpan::Bold(t)
                    | InlineSpan::Italic(t)
                    | InlineSpan::Code(t) = s
                    {
                        assert!(!t.contains("http"), "bullet leaked link line: {t}");
                    }
                }
            }
        }
    }

    #[test]
    fn inline_parser_emits_typed_spans() {
        assert_eq!(
            parse_inline_md("**HEALTH** column"),
            vec![
                InlineSpan::Bold("HEALTH".to_string()),
                InlineSpan::Plain(" column".to_string()),
            ]
        );
        assert_eq!(
            parse_inline_md("a *small* bit"),
            vec![
                InlineSpan::Plain("a ".to_string()),
                InlineSpan::Italic("small".to_string()),
                InlineSpan::Plain(" bit".to_string()),
            ]
        );
        assert_eq!(
            parse_inline_md("call `format_uptime`"),
            vec![
                InlineSpan::Plain("call ".to_string()),
                InlineSpan::Code("format_uptime".to_string()),
            ]
        );
        assert_eq!(
            parse_inline_md("[Keep a Changelog](https://keepachangelog.com) is here"),
            vec![
                InlineSpan::Link {
                    text: "Keep a Changelog".to_string(),
                    url: "https://keepachangelog.com".to_string(),
                },
                InlineSpan::Plain(" is here".to_string()),
            ]
        );
    }

    #[test]
    fn unbalanced_delimiters_degrade_to_plain() {
        // A lone `*` with no matching close should not eat the rest of the
        // bullet — it falls through as plain text.
        let spans = parse_inline_md("call * (a star) here");
        let only_plain = spans.iter().all(|s| matches!(s, InlineSpan::Plain(_)));
        assert!(
            only_plain,
            "unbalanced * leaked into a non-Plain span: {spans:?}"
        );
    }

    #[test]
    fn embedded_changelog_parses_without_panic() {
        let nodes = parse_changelog(CHANGELOG_TEXT);
        // The committed CHANGELOG.md has at minimum the 0.1.0 release.
        assert!(
            nodes
                .iter()
                .any(|n| matches!(n, ChangelogNode::VersionHeader { tag, .. } if tag == "0.1.0")),
            "expected v0.1.0 header in the embedded changelog"
        );
    }
}
