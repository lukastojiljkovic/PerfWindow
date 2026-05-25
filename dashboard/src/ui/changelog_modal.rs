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
    /// A `- ` bullet item, with any wrapped continuation lines folded into a
    /// single space-separated string. Inline markdown (`**bold**`, `[text](url)`,
    /// `` `code` ``) is stripped to its visible text.
    Bullet(String),
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
            out.push(ChangelogNode::Bullet(strip_inline_md(&b)));
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

/// Replace inline markdown spans with their visible text. Handles `**bold**`,
/// `*italic*`, `[text](url)` and `` `code` `` — enough to render our own
/// changelog cleanly without pulling in a full markdown parser.
fn strip_inline_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // `**bold**`
        if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some(end) = find_substring(&bytes[i + 2..], b"**") {
                out.push_str(std::str::from_utf8(&bytes[i + 2..i + 2 + end]).unwrap_or(""));
                i += 2 + end + 2;
                continue;
            }
        }
        // `*italic*`
        if bytes[i] == b'*' {
            if let Some(end) = find_byte(&bytes[i + 1..], b'*') {
                out.push_str(std::str::from_utf8(&bytes[i + 1..i + 1 + end]).unwrap_or(""));
                i += 1 + end + 1;
                continue;
            }
        }
        // `` `code` ``
        if bytes[i] == b'`' {
            if let Some(end) = find_byte(&bytes[i + 1..], b'`') {
                out.push_str(std::str::from_utf8(&bytes[i + 1..i + 1 + end]).unwrap_or(""));
                i += 1 + end + 1;
                continue;
            }
        }
        // `[text](url)`
        if bytes[i] == b'[' {
            if let Some(close) = find_byte(&bytes[i + 1..], b']') {
                let text_end = i + 1 + close;
                if text_end + 1 < bytes.len() && bytes[text_end + 1] == b'(' {
                    if let Some(paren_end) = find_byte(&bytes[text_end + 2..], b')') {
                        out.push_str(std::str::from_utf8(&bytes[i + 1..text_end]).unwrap_or(""));
                        i = text_end + 2 + paren_end + 1;
                        continue;
                    }
                }
            }
        }
        // Pass through the next UTF-8 char as-is.
        let ch_end = next_char_end(s, i);
        out.push_str(&s[i..ch_end]);
        i = ch_end;
    }
    out
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
        ChangelogNode::Bullet(text) => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    RichText::new("\u{00b7}")
                        .family(theme.font_data.egui())
                        .size(11.0)
                        .color(theme.dim),
                );
                ui.label(
                    RichText::new(text)
                        .family(theme.font_data.egui())
                        .size(11.0)
                        .color(theme.ink),
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let ChangelogNode::Bullet(text) = bullets[0] else {
            unreachable!();
        };
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
            if let ChangelogNode::Bullet(text) = n {
                assert!(!text.contains("http"), "bullet leaked link line: {text}");
            }
        }
    }

    #[test]
    fn strip_inline_md_removes_bold_italic_code_and_links() {
        assert_eq!(strip_inline_md("**HEALTH** column"), "HEALTH column");
        assert_eq!(strip_inline_md("a *small* bit"), "a small bit");
        assert_eq!(
            strip_inline_md("call `format_uptime`"),
            "call format_uptime"
        );
        assert_eq!(
            strip_inline_md("[Keep a Changelog](https://keepachangelog.com) is here"),
            "Keep a Changelog is here"
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
