//! The privacy statement as the page shows it (`ol_privacy`): the Markdown of
//! docs/omsihub/PRIVACY.md (built in by `omsihub_online::privacy`) read into simple blocks - the
//! headings, paragraphs, lists, tables and quotes it uses, with bold in the lines - so the page
//! draws them without any HTML of the text's own. The statement stays English, the binding text.

use serde_json::{json, Value};

/// A line's text as parts: `[text, bold]` each (`**…**` bold).
fn spans(line: &str) -> Value {
    let mut out = Vec::new();
    for (k, part) in line.split("**").enumerate() {
        if !part.is_empty() {
            out.push(json!([part, k % 2 == 1]));
        }
    }
    Value::Array(out)
}

/// A table's row: its cells (`| a | b |`).
fn cells(line: &str) -> Vec<Value> {
    line.trim().trim_start_matches('|').trim_end_matches('|').split('|').map(|c| spans(c.trim())).collect()
}

/// The Markdown as blocks: `{h: level, text}`, `{p}`, `{ul: [..]}`, `{quote}`, `{table: {head, rows}}`
/// (every text as `spans`).
pub fn blocks(md: &str) -> Vec<Value> {
    // (comments - the version marker, the maintainer's notes - are never shown)
    let md = omsihub_online::privacy::without_comments(&md.replace("\r\n", "\n"));
    let mut out: Vec<Value> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut list: Vec<Value> = Vec::new();
    let mut table: Vec<&str> = Vec::new();
    fn flush(out: &mut Vec<Value>, para: &mut Vec<&str>, list: &mut Vec<Value>, table: &mut Vec<&str>) {
        if !para.is_empty() {
            out.push(json!({ "p": spans(&para.join(" ")) }));
            para.clear();
        }
        if !list.is_empty() {
            out.push(json!({ "ul": std::mem::take(list) }));
        }
        if !table.is_empty() {
            // (the second row is the line under the head: `| --- | --- |`)
            let head = cells(table[0]);
            let rows: Vec<Vec<Value>> = table.iter().skip(1).filter(|l| !l.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))).map(|l| cells(l)).collect();
            out.push(json!({ "table": { "head": head, "rows": rows } }));
            table.clear();
        }
    }
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with('|') {
            if table.is_empty() {
                flush(&mut out, &mut para, &mut list, &mut table);
            }
            table.push(t);
            continue;
        }
        if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            if list.is_empty() {
                flush(&mut out, &mut para, &mut list, &mut table);
            }
            list.push(spans(item));
            continue;
        }
        let level = t.chars().take_while(|c| *c == '#').count();
        let heading = (1..=6).contains(&level) && t[level..].starts_with(' ');
        // (a paragraph's lines run on until a blank line, a heading, a quote, a list or a table)
        if t.is_empty() || heading || t.starts_with('>') || !list.is_empty() || !table.is_empty() {
            flush(&mut out, &mut para, &mut list, &mut table);
        }
        if t.is_empty() {
            continue;
        }
        if heading {
            out.push(json!({ "h": level, "text": spans(t[level..].trim()) }));
        } else if let Some(q) = t.strip_prefix('>') {
            out.push(json!({ "quote": spans(q.trim()) }));
        } else {
            para.push(t);
        }
    }
    flush(&mut out, &mut para, &mut list, &mut table);
    out
}

/// The statement for the page: its version and its blocks.
pub fn doc() -> Value {
    json!({ "version": omsihub_online::privacy::VERSION, "blocks": blocks(&omsihub_online::privacy::text()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_markdown_is_read_into_blocks() {
        let md = "<!-- privacy-version: 3 -->\r\n<!-- Maintainer: a\r\nnote -->\r\n# Title\r\n\r\n**Version 3.** More\r\ntext.\r\n\r\n> A note\r\n\r\n## Data\r\n\r\n| Data | Why |\r\n| --- | --- |\r\n| **Email** | To sign in |\r\n| Name | Friends |\r\n\r\n- one\r\n- **two** too\r\nAfter.";
        let b = blocks(md);
        assert_eq!(b[0], json!({ "h": 1, "text": [["Title", false]] }));
        assert_eq!(b[1], json!({ "p": [["Version 3.", true], [" More text.", false]] }));
        assert_eq!(b[2], json!({ "quote": [["A note", false]] }));
        assert_eq!(b[3]["h"], 2);
        assert_eq!(b[4]["table"]["head"], json!([[["Data", false]], [["Why", false]]]));
        assert_eq!(b[4]["table"]["rows"], json!([[[["Email", true]], [["To sign in", false]]], [[["Name", false]], [["Friends", false]]]]));
        assert_eq!(b[5], json!({ "ul": [[["one", false]], [["two", true], [" too", false]]] }));
        assert_eq!(b[6], json!({ "p": [["After.", false]] }));
        assert_eq!(b.len(), 7);
    }

    /// The statement built in reads whole: its title, its tables, its version.
    #[test]
    fn the_statement_is_shown_whole() {
        let d = doc();
        assert_eq!(d["version"], omsihub_online::privacy::VERSION);
        let b = d["blocks"].as_array().unwrap();
        assert_eq!(b[0]["h"], 1);
        assert!(b.iter().filter(|x| x.get("table").is_some()).count() >= 2);
        assert!(b.iter().any(|x| x.get("ul").is_some()));
        let all = d.to_string();
        assert!(all.contains("Delete account") && !all.contains("<!--") && !all.contains('\r'));
        assert!(!all.to_lowercase().contains("maintainer"), "a note for the maintainer is shown");
        assert!(all.contains("info@omsihub.com") && !all.contains("privacy@omsihub.com"));
    }
}
