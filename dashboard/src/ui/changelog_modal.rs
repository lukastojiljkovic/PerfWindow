//! In-app changelog viewer.
//!
//! The application's `CHANGELOG.md` is embedded into the binary at compile
//! time via [`include_str!`]; opening the modal runs a tiny hand-rolled
//! markdown scanner over it and paints the result as themed text. This keeps
//! the viewer offline and dependency-free — no `pulldown_cmark`, no browser
//! hand-off, no network.

use crate::app::PerfApp;
use crate::theme::Theme;
use egui::{Align, Layout, Margin, RichText, ScrollArea, Vec2};

const WINDOW_WIDTH: f32 = 600.0;
const BODY_PADDING_X: i8 = 18;
const BODY_PADDING_Y: i8 = 16;

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

/// Map a Keep a Changelog subsection heading to the title the user sees.
///
/// Keep a Changelog's words (`Added`, `Changed`, …) are replaced with the
/// plainer terms shared by every app in the portfolio's update notes; anything
/// else is shown as written. Used by both the changelog viewer and the update
/// modal so the two read the same.
pub fn subsection_title(name: &str) -> &str {
    let trimmed = name.trim();
    if trimmed.eq_ignore_ascii_case("added") {
        "New"
    } else if trimmed.eq_ignore_ascii_case("changed") {
        "Improved"
    } else if trimmed.eq_ignore_ascii_case("fixed") {
        "Fixed"
    } else if trimmed.eq_ignore_ascii_case("removed") {
        "Removed"
    } else if trimmed.eq_ignore_ascii_case("deprecated") {
        "Deprecated"
    } else if trimmed.eq_ignore_ascii_case("security") {
        "Security"
    } else {
        name
    }
}

/// Turn a GitHub release body into the nodes the update modal paints.
///
/// When the body carries a `## What's new` section (matched
/// case-insensitively with surrounding whitespace ignored), only the lines
/// between it and the next `## ` heading are used; otherwise the whole body is
/// parsed. The scanner is the same one the changelog viewer uses, so wrapped
/// bullets fold and inline spans survive.
pub fn release_notes_nodes(body: &str) -> Vec<ChangelogNode> {
    let lines: Vec<&str> = body.lines().collect();
    let Some(start) = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("## What's new"))
    else {
        return parse_changelog(body);
    };
    let after = start + 1;
    let end = lines[after..]
        .iter()
        .position(|line| line.trim_start().starts_with("## "))
        .map(|offset| after + offset)
        .unwrap_or(lines.len());
    parse_changelog(&lines[after..end].join("\n"))
}

/// The nodes of one version's section of a Keep a Changelog document — the
/// content under its `## [X.Y.Z] — date` heading, up to the next `## `
/// heading, without the heading itself. `None` when the version is absent;
/// `## [Unreleased]` never matches.
pub fn version_section(md: &str, version: &str) -> Option<Vec<ChangelogNode>> {
    let mut body = String::new();
    let mut inside = false;
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("## [") {
            if let Some(end) = rest.find(']') {
                if inside {
                    break;
                }
                let tag = &rest[..end];
                if tag.eq_ignore_ascii_case(version) && !tag.eq_ignore_ascii_case("unreleased") {
                    inside = true;
                    continue;
                }
            }
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    inside.then(|| parse_changelog(&body))
}

/// Paint a section title: the accent-coloured, letter-spaced label shared by
/// the changelog viewer and the update modal.
pub fn subsection_label(ui: &mut egui::Ui, theme: &Theme, name: &str) {
    ui.label(
        RichText::new(subsection_title(name).to_uppercase())
            .family(theme.font_data.egui())
            .size(10.0)
            .color(theme.accent),
    );
}

/// A clickable, accent-coloured underlined text link. Returns `true` on click.
/// The changelog viewer's "Show all versions" and the update modal's GitHub
/// link share this look.
pub fn link_label(ui: &mut egui::Ui, theme: &Theme, text: &str) -> bool {
    let response = ui.add(
        egui::Label::new(
            RichText::new(text)
                .family(theme.font_data.egui())
                .size(11.0)
                .color(theme.accent)
                .underline(),
        )
        .sense(egui::Sense::click()),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

/// Render the changelog viewer when `app.show_changelog` is `true`. The title
/// bar's close button flips it back to `false`.
///
/// Normally the whole embedded changelog is shown. When
/// `app.changelog_version` is set — the first launch after an update — the
/// viewer opens on that version's section under a "PerfWindow was updated to
/// X.Y.Z" title, with a "Show all versions" link that reveals the full log.
pub fn changelog_modal(ctx: &egui::Context, app: &mut PerfApp) {
    if !app.show_changelog {
        return;
    }
    let theme = app.theme.clone();
    let updated_to = app.changelog_version.clone();
    let mut show_all = app.changelog_show_all;

    // The embedded changelog never changes within a process lifetime, so it
    // is parsed exactly once — not on every frame the modal stays open.
    static NODES: std::sync::OnceLock<Vec<ChangelogNode>> = std::sync::OnceLock::new();
    let full_nodes = NODES.get_or_init(|| parse_changelog(CHANGELOG_TEXT));

    // The post-update view always shows the running version's section.
    static UPDATED: std::sync::OnceLock<Vec<ChangelogNode>> = std::sync::OnceLock::new();
    let updated_nodes = updated_to.as_ref().map(|_| {
        UPDATED.get_or_init(|| {
            version_section(CHANGELOG_TEXT, env!("CARGO_PKG_VERSION")).unwrap_or_default()
        })
    });

    let updated_view = updated_to.is_some() && !show_all;
    let mut close = false;

    egui::Window::new("changelog")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(Vec2::new(WINDOW_WIDTH, f32::INFINITY))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(crate::ui::modal::frame(&theme))
        .show(ctx, |ui| {
            ui.set_width(WINDOW_WIDTH);
            let label = if updated_view {
                "WHAT'S NEW"
            } else {
                "CHANGELOG"
            };
            if crate::ui::modal::title_bar(ui, &theme, label).clicked() {
                close = true;
            }

            let viewport_h = ctx
                .input(|i| i.viewport().inner_rect.map(|r| r.height()))
                .unwrap_or(720.0);
            let max_h = (viewport_h - 120.0).max(200.0);
            let nodes: &[ChangelogNode] = match (&updated_to, show_all) {
                (Some(_), false) => updated_nodes.map(Vec::as_slice).unwrap_or(full_nodes),
                _ => full_nodes,
            };
            egui::Frame::NONE
                .inner_margin(Margin::symmetric(BODY_PADDING_X, BODY_PADDING_Y))
                .show(ui, |ui| {
                    if let (Some(version), true) = (&updated_to, updated_view) {
                        ui.label(
                            RichText::new(format!("PerfWindow was updated to {version}."))
                                .family(theme.font_data.egui())
                                .size(12.0)
                                .color(theme.ink),
                        );
                        ui.add_space(6.0);
                    }
                    ScrollArea::vertical()
                        .max_height(max_h)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for node in nodes {
                                render_node(ui, &theme, node);
                            }
                        });
                    if updated_view {
                        ui.add_space(10.0);
                        if link_label(ui, &theme, "Show all versions") {
                            show_all = true;
                        }
                    }
                });
        });
    app.show_changelog = !close;
    app.changelog_show_all = show_all;
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
            subsection_label(ui, theme, name);
        }
        ChangelogNode::Bullet(spans) => bullet_row(ui, theme, spans),
    }
}

/// Paint a single bullet — a centred dot plus one egui widget per inline
/// span. The dot sits in a fixed-width marker column and the spans wrap in
/// the remaining width, so continuation lines hang under the text instead of
/// under the dot.
pub fn bullet_row(ui: &mut egui::Ui, theme: &Theme, spans: &[InlineSpan]) {
    const MARKER_W: f32 = 12.0;
    let text_w = (ui.available_width() - MARKER_W).max(1.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(
            Vec2::new(MARKER_W, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(MARKER_W);
                ui.label(
                    RichText::new("\u{00b7}")
                        .family(theme.font_data.egui())
                        .size(11.0)
                        .color(theme.dim),
                );
            },
        );
        ui.allocate_ui_with_layout(Vec2::new(text_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_max_width(text_w);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for span in spans {
                    render_span(ui, theme, span);
                }
            });
        });
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

    /// The verbatim body of the 0.11.1 GitHub release.
    const CURRENT_RELEASE_BODY: &str = "\
### Changed

- Nothing in the app. 0.11.1 is the first release that 0.11.0's update check
  can find, so updating to it shows that the updater works end to end.
";

    #[test]
    fn keep_a_changelog_words_map_to_plain_titles() {
        assert_eq!(subsection_title("Added"), "New");
        assert_eq!(subsection_title("Changed"), "Improved");
        assert_eq!(subsection_title("Fixed"), "Fixed");
        assert_eq!(subsection_title("Removed"), "Removed");
        assert_eq!(subsection_title("Deprecated"), "Deprecated");
        assert_eq!(subsection_title("Security"), "Security");
        assert_eq!(subsection_title("Highlights"), "Highlights");
    }

    #[test]
    fn the_current_release_body_is_one_improved_section_with_one_bullet() {
        let nodes = release_notes_nodes(CURRENT_RELEASE_BODY);
        let sections: Vec<&str> = nodes
            .iter()
            .filter_map(|n| match n {
                ChangelogNode::Subsection(name) => Some(subsection_title(name)),
                _ => None,
            })
            .collect();
        assert_eq!(sections, vec!["Improved"]);
        let bullets: Vec<&ChangelogNode> = nodes
            .iter()
            .filter(|n| matches!(n, ChangelogNode::Bullet(_)))
            .collect();
        assert_eq!(bullets.len(), 1);
        let text = bullet_plain_text(bullets[0]);
        assert!(text.contains("works end to end"));
    }

    #[test]
    fn a_whats_new_section_selects_only_its_lines() {
        let body = "\
Intro paragraph nobody should see.

## What's new

### Added
- Only this bullet.

## Something else

- Not this bullet.
";
        let nodes = release_notes_nodes(body);
        assert_eq!(
            nodes
                .iter()
                .filter(|n| matches!(n, ChangelogNode::Bullet(_)))
                .count(),
            1
        );
        let text = bullet_plain_text(
            nodes
                .iter()
                .find(|n| matches!(n, ChangelogNode::Bullet(_)))
                .unwrap(),
        );
        assert!(text.contains("Only this bullet"));
    }

    #[test]
    fn a_wrapped_bullet_stays_one_item() {
        let nodes = release_notes_nodes(CURRENT_RELEASE_BODY);
        let bullets: Vec<&ChangelogNode> = nodes
            .iter()
            .filter(|n| matches!(n, ChangelogNode::Bullet(_)))
            .collect();
        assert_eq!(
            bullets.len(),
            1,
            "the folded continuation is not its own item"
        );
        assert!(bullet_plain_text(bullets[0]).contains("can find"));
    }

    #[test]
    fn crlf_bodies_parse_the_same_as_lf() {
        let lf = release_notes_nodes(CURRENT_RELEASE_BODY);
        let crlf = release_notes_nodes(&CURRENT_RELEASE_BODY.replace('\n', "\r\n"));
        assert_eq!(lf, crlf);
    }

    #[test]
    fn version_section_finds_the_release_and_stops_at_the_next_one() {
        let nodes = version_section(CHANGELOG_TEXT, "0.11.1").expect("0.11.1 is embedded");
        let text: String = nodes
            .iter()
            .filter(|n| matches!(n, ChangelogNode::Bullet(_)))
            .map(bullet_plain_text)
            .collect();
        assert!(text.contains("updater works end to end"));
        assert!(
            !text.contains("Terms of use"),
            "the 0.11.0 section must not leak into the 0.11.1 section"
        );
    }

    #[test]
    fn version_section_returns_none_for_unknown_and_unreleased() {
        assert!(version_section(CHANGELOG_TEXT, "9.9.9").is_none());
        assert!(version_section(CHANGELOG_TEXT, "Unreleased").is_none());
        assert!(version_section(CHANGELOG_TEXT, "unreleased").is_none());
    }
}
