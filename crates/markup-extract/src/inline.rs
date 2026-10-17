//! Inline runs shared by the HTML and Markdown extractors.
//!
//! Both formats mark bold, italic, links and images inline, and both write
//! that formatting as CommonMark into the block text (see the crate docs for
//! why). This module holds the one renderer so the two agree byte for byte.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::TextSpan;

/// A finished inline run or image, before inline Markdown is rendered.
#[derive(Debug)]
pub(crate) enum Inline {
    /// A run of text with uniform formatting. `link` is `Some` where the run
    /// sits inside a hyperlink.
    Run {
        text: String,
        bold: bool,
        italic: bool,
        strike: bool,
        link: Option<String>,
    },
    /// Inline code: rendered as a backtick span, never styled or linked.
    Code { text: String },
    /// An image: alt text plus the reference it points at. `link` is `Some`
    /// where the image sits inside a hyperlink.
    Image {
        alt: String,
        target: String,
        link: Option<String>,
    },
    /// A forced line break (`<br>`, a hard break): one newline.
    LineBreak,
}

/// Render inlines as inline Markdown, merging neighbours that share their
/// formatting. Returns the text plus one plain span per merged run: the
/// formatting already lives in the text, so styled span flags would mark
/// every run a second time in [`pdf_engine::to_markdown`].
pub(crate) fn render_inlines(inlines: Vec<Inline>) -> (String, Vec<TextSpan>) {
    let mut merged: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines {
        let Inline::Run {
            text,
            bold,
            italic,
            strike,
            link,
        } = inline
        else {
            merged.push(inline);
            continue;
        };
        match merged.last_mut() {
            Some(Inline::Run {
                text: previous,
                bold: same_bold,
                italic: same_italic,
                strike: same_strike,
                link: same_link,
            }) if *same_bold == bold
                && *same_italic == italic
                && *same_strike == strike
                && *same_link == link =>
            {
                previous.push_str(&text);
            }
            _ => merged.push(Inline::Run {
                text,
                bold,
                italic,
                strike,
                link,
            }),
        }
    }
    let mut out = String::new();
    let mut spans = Vec::new();
    for inline in merged {
        match inline {
            Inline::Run {
                text,
                bold,
                italic,
                strike,
                link,
            } => {
                if text.is_empty() {
                    continue;
                }
                // Edge spaces stay outside the marks: `** bold**` is literal
                // asterisks in CommonMark, not emphasis.
                let (lead, core, trail) = split_edge_spaces(&text);
                if core.is_empty() {
                    out.push_str(&text);
                    spans.push(TextSpan {
                        text,
                        ..Default::default()
                    });
                    continue;
                }
                let mut inner = escape_run(core, bold || italic || strike, link.is_some());
                if strike {
                    inner = format!("~~{inner}~~");
                }
                if bold && italic {
                    inner = format!("***{inner}***");
                } else if bold {
                    inner = format!("**{inner}**");
                } else if italic {
                    inner = format!("*{inner}*");
                }
                if let Some(url) = link {
                    inner = format!("[{inner}]({})", escape_url(&url));
                }
                out.push_str(lead);
                out.push_str(&inner);
                out.push_str(trail);
                spans.push(TextSpan {
                    text,
                    ..Default::default()
                });
            }
            Inline::Code { text } => {
                let code = normalize_code(&text);
                if code.is_empty() {
                    continue;
                }
                let rendered = fence_code(&code);
                spans.push(TextSpan {
                    text: code,
                    ..Default::default()
                });
                out.push_str(&rendered);
            }
            Inline::Image { alt, target, link } => {
                let mut rendered =
                    format!("![{}]({})", escape_link_text(&alt), escape_url(&target));
                if let Some(url) = link {
                    rendered = format!("[{rendered}]({})", escape_url(&url));
                }
                spans.push(TextSpan {
                    text: rendered.clone(),
                    ..Default::default()
                });
                out.push_str(&rendered);
            }
            Inline::LineBreak => {
                out.push('\n');
            }
        }
    }
    (out, spans)
}

/// Split edge whitespace off a run: `(leading, core, trailing)`.
fn split_edge_spaces(text: &str) -> (&str, &str, &str) {
    if text.trim().is_empty() {
        return (text, "", "");
    }
    let start = text.len() - text.trim_start().len();
    let end = text.trim_end().len();
    (&text[..start], &text[start..end], &text[end..])
}

/// Escape what the wrappers would otherwise read as syntax: `*` and `~`
/// inside a styled run, brackets inside a link, and backslashes everywhere.
fn escape_run(text: &str, styled: bool, linked: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '*' if styled => out.push_str("\\*"),
            '~' if styled => out.push_str("\\~"),
            '[' | ']' if linked => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Escape link and alt text: brackets and backslashes only.
fn escape_link_text(text: &str) -> String {
    escape_run(text, false, true)
}

/// Escape a link target: a space or parenthesis would end the destination.
fn escape_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for c in url.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            _ => out.push(c),
        }
    }
    out
}

/// Inline code content: line endings become spaces, and one surrounding space
/// is stripped where both ends have one (CommonMark's code span rule).
fn normalize_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\r' | '\n' => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            _ => out.push(c),
        }
    }
    if out.len() > 2 && out.starts_with(' ') && out.ends_with(' ') && out.trim() != "" {
        out.remove(0);
        out.pop();
    }
    out
}

/// Wrap code in the shortest backtick fence that cannot close early: one more
/// backtick than the longest run inside, with padding spaces where the code
/// touches the fence.
fn fence_code(code: &str) -> String {
    let longest = code
        .split(|c| c != '`')
        .map(|run| run.len())
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    if longest > 0 && (code.starts_with('`') || code.ends_with('`')) {
        format!("{fence} {code} {fence}")
    } else {
        format!("{fence}{code}{fence}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Inline {
        Inline::Run {
            text: text.to_string(),
            bold: false,
            italic: false,
            strike: false,
            link: None,
        }
    }

    #[test]
    fn neighbours_with_shared_formatting_merge_into_one_span() {
        let (text, spans) = render_inlines(vec![run("a"), run("b")]);
        assert_eq!(text, "ab");
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn bold_italic_and_links_wrap() {
        let (text, _) = render_inlines(vec![
            Inline::Run {
                text: "b".to_string(),
                bold: true,
                italic: true,
                strike: false,
                link: None,
            },
            Inline::Run {
                text: "l]".to_string(),
                bold: false,
                italic: false,
                strike: false,
                link: Some("https://x.test/a b".to_string()),
            },
        ]);
        assert_eq!(text, "***b***[l\\]](https://x.test/a%20b)");
    }

    #[test]
    fn edge_spaces_stay_outside_the_marks() {
        let (text, _) = render_inlines(vec![Inline::Run {
            text: " b ".to_string(),
            bold: true,
            italic: false,
            strike: false,
            link: None,
        }]);
        assert_eq!(text, " **b** ");
    }

    #[test]
    fn code_picks_a_fence_that_cannot_close_early() {
        let (text, _) = render_inlines(vec![Inline::Code {
            text: "a ` b".to_string(),
        }]);
        assert_eq!(text, "``a ` b``");
    }
}
