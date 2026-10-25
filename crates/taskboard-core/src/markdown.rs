//! Markdown for task, meeting and client notes, rendered and sanitized in core
//! so the webview only ever receives safe HTML (SPEC §2, §3 security).

use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Event, Options, Parser, Tag, html};

/// Notes larger than this are shown as plain text instead (keeps rendering cheap).
const MAX_RENDER_BYTES: usize = 256 * 1024;

/// Renders Markdown to sanitized HTML.
///
/// Supports tables, strikethrough and task lists (shown as ☐ / ☑, so no form
/// controls reach the page). Raw HTML in the source is escaped, links keep only
/// `http`, `https` and `mailto` URLs and open with `rel="noopener noreferrer"`,
/// and images are dropped (notes never load remote content).
pub fn render_markdown(source: &str) -> String {
    if source.len() > MAX_RENDER_BYTES {
        return format!("<pre>{}</pre>", ammonia::clean_text(source));
    }
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut events: Vec<Event<'_>> = Parser::new_ext(source, options)
        .map(|event| match event {
            // Show raw HTML as typed rather than interpreting it.
            Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
            other => other,
        })
        .collect();
    // A task-list item opens as `<li class="task">` (or "task done") so the
    // page can show its box instead of a bullet; the box itself is a symbol.
    for i in 0..events.len() {
        let done = match events.get(i + 1) {
            Some(Event::TaskListMarker(done)) => *done,
            _ => continue,
        };
        if let Some(start @ Event::Start(Tag::Item)) = events.get_mut(i) {
            *start = Event::Html(
                if done {
                    r#"<li class="task done">"#
                } else {
                    r#"<li class="task">"#
                }
                .into(),
            );
        }
        if let Some(marker) = events.get_mut(i + 1) {
            *marker = Event::Text(if done { "☑ " } else { "☐ " }.into());
        }
    }
    let events = events.into_iter().map(|event| match event {
        Event::TaskListMarker(done) => Event::Text(if done { "☑ " } else { "☐ " }.into()),
        other => other,
    });
    let mut unsafe_html = String::with_capacity(source.len() * 3 / 2);
    html::push_html(&mut unsafe_html, events);

    let mut tags = ammonia::Builder::default().clone_tags();
    tags.remove("img");
    ammonia::Builder::default()
        .tags(tags)
        // Only the two classes made above survive; any typed class is dropped.
        .allowed_classes(HashMap::from([("li", HashSet::from(["task", "done"]))]))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(Some("noopener noreferrer"))
        .clean(&unsafe_html)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::render_markdown;

    #[test]
    fn renders_common_markdown() {
        let html = render_markdown(
            "# Agenda\n\n- **budget**\n- ~~old~~\n\n| a | b |\n|---|---|\n| 1 | 2 |",
        );
        assert!(html.contains("<h1>Agenda</h1>"), "{html}");
        assert!(html.contains("<strong>budget</strong>"), "{html}");
        assert!(html.contains("<del>old</del>"), "{html}");
        assert!(html.contains("<td>1</td>"), "{html}");
    }

    #[test]
    fn task_lists_become_symbols() {
        let html = render_markdown("- [ ] draft\n- [x] send");
        assert!(
            html.contains("☐ draft") && html.contains("☑ send"),
            "{html}"
        );
        assert!(!html.contains("<input"), "{html}");
    }

    #[test]
    fn task_list_items_are_marked_so_they_show_without_a_bullet() {
        let html = render_markdown("- [ ] draft\n- [x] send\n- plain");
        assert!(html.contains(r#"<li class="task">☐ draft"#), "{html}");
        assert!(html.contains(r#"<li class="task done">☑ send"#), "{html}");
        assert!(html.contains("<li>plain"), "{html}");
        // Classes from the source never get through.
        let typed = render_markdown(r#"<li class="evil task">x</li>"#);
        assert!(!typed.contains("<li class"), "{typed}");
    }

    #[test]
    fn raw_html_and_scripts_are_neutralized() {
        let html = render_markdown("<script>alert(1)</script>\n\n<b onclick=\"x()\">hi</b>");
        assert!(!html.contains("<script"), "{html}");
        // The tag shows as text; no element (and so no handler) is created.
        assert!(!html.contains("<b"), "{html}");
        assert!(html.contains("&lt;b onclick"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn links_are_restricted_and_safe() {
        let html = render_markdown(
            "[ok](https://example.com) [mail](mailto:a@example.com) [bad](javascript:alert(1)) ![img](https://example.com/x.png)",
        );
        assert!(html.contains(r#"href="https://example.com""#), "{html}");
        assert!(html.contains(r#"href="mailto:a@example.com""#), "{html}");
        assert!(html.contains(r#"rel="noopener noreferrer""#), "{html}");
        assert!(!html.contains("javascript"), "{html}");
        assert!(!html.contains("<img"), "{html}");
    }

    #[test]
    fn huge_notes_fall_back_to_escaped_text() {
        let big = format!("<b>{}</b>", "x".repeat(300 * 1024));
        let html = render_markdown(&big);
        assert!(html.starts_with("<pre>&lt;b&gt;"));
    }
}
