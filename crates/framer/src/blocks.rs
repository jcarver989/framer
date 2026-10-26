//! Marked blocks: the parts of a hand-written text file that framer owns.
//!
//! ```markdown
//! <!-- framer:begin conventions -->
//! generated text
//! <!-- framer:end conventions -->
//! ```

use crate::format::CommentStyle;

fn markers(style: CommentStyle, id: &str) -> Option<(String, String)> {
    Some((
        style.comment(&format!("framer:begin {id}"))?,
        style.comment(&format!("framer:end {id}"))?,
    ))
}

/// The line indexes of the block's begin and end markers.
fn find(lines: &[&str], begin: &str, end: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| line.trim() == begin)?;
    let stop = start + lines[start..].iter().position(|line| line.trim() == end)?;
    Some((start, stop))
}

fn join(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

/// Replaces the block's contents, or appends the block when the file doesn't have it.
/// Returns `None` when the file format has no comments to mark the block with.
pub(crate) fn upsert(text: &str, style: CommentStyle, id: &str, body: &str) -> Option<String> {
    let (begin, end) = markers(style, id)?;
    let mut block = format!("{begin}\n{body}");
    if !body.is_empty() && !body.ends_with('\n') {
        block.push('\n');
    }
    block.push_str(&end);

    let lines: Vec<&str> = text.lines().collect();
    if let Some((start, stop)) = find(&lines, &begin, &end) {
        let mut replaced = lines[..start].to_vec();
        replaced.push(&block);
        replaced.extend_from_slice(&lines[stop + 1..]);
        return Some(join(&replaced));
    }

    let mut appended = text.to_owned();
    if !appended.is_empty() {
        if !appended.ends_with('\n') {
            appended.push('\n');
        }
        if !appended.ends_with("\n\n") {
            appended.push('\n');
        }
    }
    appended.push_str(&block);
    appended.push('\n');
    Some(appended)
}

/// Removes the block. With `keep_body`, only the markers are removed and the
/// block's contents become ordinary text.
pub(crate) fn remove(text: &str, style: CommentStyle, id: &str, keep_body: bool) -> String {
    let Some((begin, end)) = markers(style, id) else {
        return text.to_owned();
    };
    let lines: Vec<&str> = text.lines().collect();
    let Some((mut start, stop)) = find(&lines, &begin, &end) else {
        return text.to_owned();
    };

    let mut kept = Vec::with_capacity(lines.len());
    if keep_body {
        kept.extend_from_slice(&lines[..start]);
        kept.extend_from_slice(&lines[start + 1..stop]);
    } else {
        // Drop the blank line that separated the block from the text before it.
        let followed_by_blank = lines
            .get(stop + 1)
            .is_none_or(|line| line.trim().is_empty());
        if start > 0 && lines[start - 1].trim().is_empty() && followed_by_blank {
            start -= 1;
        }
        kept.extend_from_slice(&lines[..start]);
    }
    kept.extend_from_slice(&lines[stop + 1..]);
    join(&kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn appends_a_block_after_existing_text() {
        let text = upsert("# Title\nHello\n", CommentStyle::Html, "a", "generated\n").unwrap();

        assert_eq!(
            text,
            indoc! {"
                # Title
                Hello

                <!-- framer:begin a -->
                generated
                <!-- framer:end a -->
            "}
        );
    }

    #[test]
    fn replaces_an_existing_block_in_place() {
        let text = indoc! {"
            before
            # framer:begin a
            old
            # framer:end a
            after
        "};

        assert_eq!(
            upsert(text, CommentStyle::Hash, "a", "new").unwrap(),
            "before\n# framer:begin a\nnew\n# framer:end a\nafter\n"
        );
    }

    #[test]
    fn upsert_is_idempotent() {
        let once = upsert("text\n", CommentStyle::Hash, "a", "body\n").unwrap();
        let twice = upsert(&once, CommentStyle::Hash, "a", "body\n").unwrap();

        assert_eq!(once, twice);
    }

    #[test]
    fn remove_undoes_upsert() {
        let original = "# Title\n";
        let with_block = upsert(original, CommentStyle::Html, "a", "body\n").unwrap();

        assert_eq!(
            remove(&with_block, CommentStyle::Html, "a", false),
            original
        );
    }

    #[test]
    fn remove_can_keep_the_body() {
        let with_block = upsert("", CommentStyle::Hash, "a", "body\n").unwrap();

        assert_eq!(remove(&with_block, CommentStyle::Hash, "a", true), "body\n");
    }

    #[test]
    fn formats_without_comments_cannot_hold_blocks() {
        assert_eq!(upsert("{}", CommentStyle::None, "a", "body"), None);
    }
}
