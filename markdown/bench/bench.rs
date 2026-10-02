// Copyright (c) 2026 Jurjen Stellingwerff
// SPDX-License-Identifier: LGPL-3.0-or-later
//
// markdown-reference — the pure-Rust twin of the `markdown` package's performance pass
// (bench/bench.loft), one file built with `rustc -O`.  It builds the SAME workloads and
// runs the SAME algorithms — ports of src/markdown.loft's `render`, `render_inline`,
// `html_escape`, `slugify` and `extract_headings`, written as ordinary idiomatic Rust
// (one `String` grown by `push` / `push_str`, byte offsets into `&str`) — and prints the
// same rows, hash included: a row whose hash matches the loft build's is a like-for-like
// comparison.  No dependencies and no cleverness: the speed an industry implementation
// reaches without effort, which is what the bar should be.
//
//     mkdir -p bench/.build && rustc -O --edition=2021 bench/bench.rs -o bench/.build/stats_rs
//     bench/.build/stats_rs --n 20
//
// Offsets are BYTE offsets, as in the loft source; a character is read at a byte offset
// the way loft's `text[i]` reads one.
use std::hint::black_box;
use std::time::Instant;

const FNV_OFFSET: i64 = 2166136261;
const FNV_PRIME: i64 = 16777619;

fn fnv(h0: i64, v: &[i64]) -> i64 {
    let mut h = h0;
    for &x in v {
        let w = x & 0xFFFF_FFFF;
        for sh in [24, 16, 8, 0] {
            h = ((h ^ ((w >> sh) & 255)) * FNV_PRIME) & 0xFFFF_FFFF;
        }
    }
    h
}

fn fnv_text(h0: i64, s: &str) -> i64 {
    let mut h = h0;
    for &b in s.as_bytes() {
        h = ((h ^ b as i64) * FNV_PRIME) & 0xFFFF_FFFF;
    }
    h
}

struct Row {
    name: &'static str,
    iters: i64,
    us: i64,
    px: i64,
    hash: i64,
    sink: i64,
}

fn print_row(r: &Row) {
    let ns_op = r.us * 1000 / r.iters;
    let ns_px = if r.px > 0 { (r.us * 1000) as f64 / (r.iters * r.px) as f64 } else { 0.0 };
    println!("{}\t{}\t{}\t{}\t{}\t{:.3}\t{:x}", r.name, r.iters, r.us, ns_op, r.px, ns_px, r.hash);
    if r.sink == i64::MIN {
        println!("(unreachable — keeps the sink alive)");
    }
}

// ── src/markdown.loft ───────────────────────────────────────────────

fn html_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn slugify(heading: &str) -> String {
    let mut out = String::new();
    for c in heading.chars() {
        match c {
            ' ' => out.push('-'),
            'a'..='z' | '0'..='9' | '-' | '_' => out.push(c),
            'A'..='Z' => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out
}

/// The byte at `i`, or 0 past the end — loft's `(s[i] ?? '\0')` on ASCII text.
fn at(b: &[u8], i: usize) -> u8 {
    if i < b.len() { b[i] } else { 0 }
}

fn atx_heading_level(line: &str) -> usize {
    let b = line.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n && b[i] == b'#' {
        i += 1;
    }
    if i < 1 || i > 6 {
        return 0;
    }
    if i < n && b[i] != b' ' {
        return 0;
    }
    i
}

fn strip_trailing_hashes(s: &str) -> &str {
    let b = s.as_bytes();
    let n = b.len();
    if n == 0 {
        return s;
    }
    let mut end = n;
    while end > 0 && b[end - 1] == b'#' {
        end -= 1;
    }
    if end < n && end > 0 && b[end - 1] == b' ' {
        end -= 1;
    }
    s[..end].trim()
}

fn is_setext_underline(line: &str, ch: u8) -> bool {
    line.len() >= 2 && line.bytes().all(|b| b == ch)
}

fn is_ul_item(line: &str) -> bool {
    line.starts_with("- ") || line.starts_with("* ")
}

fn is_ol_item(line: &str) -> bool {
    let b = line.as_bytes();
    let n = b.len();
    if n < 3 {
        return false;
    }
    let i = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if i == 0 || i + 1 >= n {
        return false;
    }
    b[i] == b'.' && b[i + 1] == b' '
}

fn strip_list_marker(line: &str) -> &str {
    if line.starts_with("- ") || line.starts_with("* ") {
        return &line[2..];
    }
    let b = line.as_bytes();
    let i = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if i + 1 < b.len() && b[i] == b'.' && b[i + 1] == b' ' {
        return &line[i + 2..];
    }
    line
}

fn is_table_row(line: &str) -> bool {
    line.starts_with('|') && line.len() >= 3 && line.as_bytes()[1..].contains(&b'|')
}

fn is_table_separator(line: &str) -> bool {
    if !is_table_row(line) {
        return false;
    }
    let mut seen_dash = false;
    for b in line.bytes() {
        match b {
            b'-' => seen_dash = true,
            b'|' | b':' | b' ' => {}
            _ => return false,
        }
    }
    seen_dash
}

fn parse_table_align(sep: &str) -> String {
    let mut out = String::new();
    for cell in sep.split('|') {
        let t = cell.trim();
        if t.is_empty() {
            continue;
        }
        let starts = t.starts_with(':');
        let ends = t.ends_with(':');
        out.push(match (starts, ends) {
            (true, true) => 'c',
            (false, true) => 'r',
            (true, false) => 'l',
            _ => 'd',
        });
    }
    out
}

struct Ctx<'a> {
    base_dir: &'a str,
    tag_url_prefix: &'a str,
    image_url_prefix: &'a str,
}

fn render_table_row(line: &str, is_header: bool, align: &str, cx: &Ctx) -> String {
    let mut out = String::from("<tr>");
    let tag = if is_header { "th" } else { "td" };
    let align = align.as_bytes();
    let mut cell_idx = 0;
    for cell in line.split('|') {
        let trimmed = cell.trim();
        if trimmed.is_empty() {
            continue;
        }
        let attr = match align.get(cell_idx) {
            Some(b'c') => " style=\"text-align:center\"",
            Some(b'r') => " style=\"text-align:right\"",
            Some(b'l') => " style=\"text-align:left\"",
            _ => "",
        };
        out.push_str(&format!("<{tag}{attr}>{}</{tag}>", render_inline(trimmed, cx)));
        cell_idx += 1;
    }
    out.push_str("</tr>");
    out
}

fn parent_dir(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

fn rewrite_image(url: &str, cx: &Ctx) -> String {
    if cx.image_url_prefix.is_empty() {
        return url.to_string();
    }
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:") || url.starts_with('/') {
        return url.to_string();
    }
    let mut cleaned = url;
    while let Some(rest) = cleaned.strip_prefix("./") {
        cleaned = rest;
    }
    let mut cur_base = cx.base_dir;
    while let Some(rest) = cleaned.strip_prefix("../") {
        cur_base = parent_dir(cur_base);
        cleaned = rest;
    }
    if cur_base.is_empty() {
        return format!("{}{cleaned}", cx.image_url_prefix);
    }
    format!("{}{cur_base}/{cleaned}", cx.image_url_prefix)
}

fn rewrite_link(url: &str, base_dir: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") || url.starts_with('#') || url.starts_with('/') || base_dir.is_empty() {
        return url.to_string();
    }
    let (target, anchor) = match url.find('#') {
        Some(h) => (&url[..h], &url[h..]),
        None => (url, ""),
    };
    let mut cleaned = target;
    while let Some(rest) = cleaned.strip_prefix("./") {
        cleaned = rest;
    }
    let mut cur_base = base_dir;
    while let Some(rest) = cleaned.strip_prefix("../") {
        cur_base = parent_dir(cur_base);
        cleaned = rest;
    }
    if cur_base.is_empty() {
        return format!("/file/{cleaned}{anchor}");
    }
    format!("/file/{cur_base}/{cleaned}{anchor}")
}

fn url_part_of(raw: &str) -> &str {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i] != b' ' {
        i += 1;
    }
    &raw[start..i]
}

fn title_part_of(raw: &str) -> &str {
    let Some(q) = raw.find('"') else { return "" };
    match raw[q + 1..].find('"') {
        Some(e) => &raw[q + 1..q + 1 + e],
        None => "",
    }
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

fn match_tracker_tag(b: &[u8], start: usize, n: usize) -> usize {
    if start >= n || b[start] != b'P' {
        return start;
    }
    if start + 4 < n && b[start + 1] == b'L' && b[start + 2] == b'A' && b[start + 3] == b'N' {
        let mut i = start + 4;
        let d0 = i;
        while i < n && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == d0 {
            return start;
        }
        while i < n && b[i] == b'-' {
            let mut j = i + 1;
            while j < n && (is_word(b[j]) || b[j] == b'_' || b[j] == b'.') {
                j += 1;
            }
            if j == i + 1 {
                break;
            }
            i = j;
        }
        return i;
    }
    let mut i = start + 1;
    while i < n && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == start + 1 {
        return start;
    }
    if i < n && b[i].is_ascii_lowercase() {
        i += 1;
    }
    i
}

/// The end of a bracketed text starting at `start` (depth 1), as the loft scan leaves it.
fn bracket_end(b: &[u8], start: usize, n: usize) -> usize {
    let mut j = start;
    let mut depth = 1;
    while j < n && depth > 0 {
        if b[j] == b']' {
            depth -= 1;
            if depth == 0 {
                break;
            }
        } else if b[j] == b'[' {
            depth += 1;
        }
        j += 1;
    }
    j
}

fn render_inline(src: &str, cx: &Ctx) -> String {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = b[i];
        if c == b'\n' {
            out.push_str("<br>");
            i += 1;
            continue;
        }
        if c == b'\\' && i + 1 < n && b"*_`\\|[]()!~".contains(&b[i + 1]) {
            out.push(b[i + 1] as char);
            i += 2;
            continue;
        }
        if c == b'~' && at(b, i + 1) == b'~' {
            i += 2;
            let start = i;
            while i + 1 < n && !(b[i] == b'~' && b[i + 1] == b'~') {
                i += 1;
            }
            let inner = &src[start..i];
            if i + 1 < n {
                i += 2;
            }
            out.push_str("<del>");
            out.push_str(&render_inline(inner, cx));
            out.push_str("</del>");
            continue;
        }
        if c == b'<' {
            let mut close = i + 1;
            let mut ok = true;
            while close < n && b[close] != b'>' {
                if b[close] == b' ' || b[close] == b'\n' {
                    ok = false;
                    break;
                }
                close += 1;
            }
            if ok && close < n {
                let inner = &src[i + 1..close];
                if inner.contains("://") {
                    let esc = html_escape(inner);
                    out.push_str(&format!("<a href=\"{esc}\">{esc}</a>"));
                    i = close + 1;
                    continue;
                }
                if inner.contains('@') {
                    let esc = html_escape(inner);
                    out.push_str(&format!("<a href=\"mailto:{esc}\">{esc}</a>"));
                    i = close + 1;
                    continue;
                }
            }
        }
        if c == b'!' && at(b, i + 1) == b'[' {
            let text_start = i + 2;
            let j = bracket_end(b, text_start, n);
            if j + 1 < n && b[j + 1] == b'(' {
                let alt = &src[text_start..j];
                let url_start = j + 2;
                if let Some(k) = b[url_start..].iter().position(|&x| x == b')').map(|p| p + url_start) {
                    let url = url_part_of(&src[url_start..k]);
                    let esc_url = html_escape(&rewrite_image(url, cx));
                    let esc_alt = html_escape(alt);
                    out.push_str(&format!("<img src=\"{esc_url}\" alt=\"{esc_alt}\">"));
                    i = k + 1;
                    continue;
                }
            }
        }
        if c == b'@' && !cx.tag_url_prefix.is_empty() && i + 1 < n && (i == 0 || !(is_word(b[i - 1]) || b[i - 1] == b'_')) {
            let tag_end = match_tracker_tag(b, i + 1, n);
            if tag_end > i + 1 {
                let esc_bare = html_escape(&src[i + 1..tag_end]);
                out.push_str(&format!("<a class=\"tag-ref\" href=\"{}{esc_bare}\">@{esc_bare}</a>", cx.tag_url_prefix));
                i = tag_end;
                continue;
            }
        }
        if c == b'`' {
            i += 1;
            let start = i;
            while i < n && b[i] != b'`' {
                i += 1;
            }
            let code = &src[start..i];
            if i < n {
                i += 1;
            }
            out.push_str("<code>");
            out.push_str(&html_escape(code));
            out.push_str("</code>");
            continue;
        }
        if c == b'*' && at(b, i + 1) == b'*' {
            i += 2;
            let start = i;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'*') {
                i += 1;
            }
            let inner = &src[start..i];
            if i + 1 < n {
                i += 2;
            }
            out.push_str("<strong>");
            out.push_str(&render_inline(inner, cx));
            out.push_str("</strong>");
            continue;
        }
        if (c == b'*' || c == b'_') && !(c == b'_' && i > 0 && is_word(b[i - 1])) {
            i += 1;
            let start = i;
            while i < n && b[i] != c {
                i += 1;
            }
            let inner = &src[start..i];
            if i < n {
                i += 1;
            }
            out.push_str("<em>");
            out.push_str(&render_inline(inner, cx));
            out.push_str("</em>");
            continue;
        }
        if c == b'[' {
            let text_start = i + 1;
            let j = bracket_end(b, text_start, n);
            if j + 1 < n && b[j + 1] == b'(' {
                let link_text = &src[text_start..j];
                let url_start = j + 2;
                if let Some(k) = b[url_start..].iter().position(|&x| x == b')').map(|p| p + url_start) {
                    let raw = &src[url_start..k];
                    let esc_url = html_escape(&rewrite_link(url_part_of(raw), cx.base_dir));
                    let title = title_part_of(raw);
                    let title_attr = if title.is_empty() { String::new() } else { format!(" title=\"{}\"", html_escape(title)) };
                    out.push_str(&format!("<a href=\"{esc_url}\"{title_attr}>{}</a>", render_inline(link_text, cx)));
                    i = k + 1;
                    continue;
                }
            }
        }
        match c {
            b'&' => out.push_str("&amp;"),
            b'<' => out.push_str("&lt;"),
            b'>' => out.push_str("&gt;"),
            b'"' => out.push_str("&quot;"),
            _ => {
                let ch = src[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// A list item, as a `<li>` — a task item when it opens with `[ ] ` / `[x] ` / `[X] `.
fn list_item(out: &mut String, item: &str, cx: &Ctx) {
    let b = item.as_bytes();
    if b.len() >= 4 && b[0] == b'[' && b[2] == b']' && b[3] == b' ' && matches!(b[1], b' ' | b'x' | b'X') {
        let chk = if b[1] != b' ' { " checked" } else { "" };
        out.push_str(&format!("<li><input type=\"checkbox\" disabled{chk}> {}</li>", render_inline(&item[4..], cx)));
    } else {
        out.push_str(&format!("<li>{}</li>", render_inline(item, cx)));
    }
}

fn flush_para(out: &mut String, para: &mut String, cx: &Ctx) {
    if !para.is_empty() {
        out.push_str(&format!("<p>{}</p>", render_inline(para, cx)));
        para.clear();
    }
}

/// `buf` grown by one line: the line itself when empty, else a space and the line.
fn join_line(buf: &mut String, line: &str) {
    if !buf.is_empty() {
        buf.push(' ');
    }
    buf.push_str(line);
}

fn render(source: &str, cx: &Ctx) -> String {
    let src: String = source.chars().filter(|&c| c != '\r').collect();
    let mut out = String::new();
    let mut in_code = false;
    let mut code_buf = String::new();
    let mut code_lang = String::new();
    let mut para = String::new();
    let mut list_kind = "";
    let mut item = String::new();
    let mut table_state = 0;
    let mut table_header = String::new();
    let mut table_align = String::new();
    let mut bq = String::new();
    let mut pending = String::new();
    let lines: Vec<&str> = if src.is_empty() { Vec::new() } else { src.split('\n').collect() };
    for &line in &lines {
        if in_code {
            if line.starts_with("```") {
                let esc_code = html_escape(&code_buf);
                let esc_lang = html_escape(&code_lang);
                if !esc_lang.is_empty() {
                    out.push_str(&format!("<pre><code class=\"language-{esc_lang}\">{esc_code}</code></pre>"));
                } else {
                    out.push_str(&format!("<pre><code>{esc_code}</code></pre>"));
                }
                code_buf.clear();
                code_lang.clear();
                in_code = false;
            } else {
                code_buf.push_str(line);
                code_buf.push('\n');
            }
            continue;
        }
        if !pending.is_empty() {
            let level = if is_setext_underline(line, b'=') {
                1
            } else if is_setext_underline(line, b'-') {
                2
            } else {
                0
            };
            if level > 0 {
                let esc_slug = html_escape(&slugify(&pending));
                out.push_str(&format!("<h{level} id=\"{esc_slug}\">{}</h{level}>", render_inline(&pending, cx)));
                pending.clear();
                continue;
            }
            join_line(&mut para, &pending);
            pending.clear();
        }
        if table_state == 1 {
            if is_table_separator(line) {
                table_align = parse_table_align(line);
                out.push_str(&format!("<table><thead>{}</thead><tbody>", render_table_row(&table_header, true, &table_align, cx)));
                table_state = 2;
                continue;
            }
            table_state = 0;
            join_line(&mut para, &table_header);
            table_header.clear();
        } else if table_state == 2 {
            if is_table_row(line) {
                out.push_str(&render_table_row(line, false, &table_align, cx));
                continue;
            }
            out.push_str("</tbody></table>");
            table_state = 0;
            table_align.clear();
        }
        if !list_kind.is_empty() {
            if is_ul_item(line) || is_ol_item(line) {
                list_item(&mut out, &item, cx);
                item = strip_list_marker(line).to_string();
                continue;
            }
            if line.is_empty() {
                continue;
            }
            if line.starts_with(' ') {
                join_line(&mut item, line.trim());
                continue;
            }
            list_item(&mut out, &item, cx);
            item.clear();
            out.push_str(&format!("</{list_kind}>"));
            list_kind = "";
        }
        if let Some(lang) = line.strip_prefix("```") {
            flush_para(&mut out, &mut para, cx);
            if !lang.is_empty() {
                code_lang = lang.to_string();
            }
            in_code = true;
            continue;
        }
        if line.starts_with("    ") && para.is_empty() && bq.is_empty() {
            out.push_str(&format!("<pre><code>{}\n</code></pre>", html_escape(&line[4..])));
            continue;
        }
        if line.starts_with("<!--") {
            continue;
        }
        if line == "---" || line == "***" || line == "___" {
            flush_para(&mut out, &mut para, cx);
            out.push_str("<hr>");
            continue;
        }
        let hlevel = atx_heading_level(line);
        if hlevel > 0 {
            flush_para(&mut out, &mut para, cx);
            let heading = strip_trailing_hashes(if hlevel + 1 < line.len() { &line[hlevel + 1..] } else { "" });
            let esc_slug = html_escape(&slugify(heading));
            out.push_str(&format!("<h{hlevel} id=\"{esc_slug}\">{}</h{hlevel}>", render_inline(heading, cx)));
            continue;
        }
        if is_ul_item(line) || is_ol_item(line) {
            flush_para(&mut out, &mut para, cx);
            list_kind = if is_ul_item(line) { "ul" } else { "ol" };
            out.push_str(&format!("<{list_kind}>"));
            item = strip_list_marker(line).to_string();
            continue;
        }
        if is_table_row(line) {
            flush_para(&mut out, &mut para, cx);
            if !bq.is_empty() {
                out.push_str(&format!("<blockquote>{}</blockquote>", render_inline(&bq, cx)));
                bq.clear();
            }
            table_header = line.to_string();
            table_state = 1;
            continue;
        }
        if line.starts_with("> ") || line == ">" {
            flush_para(&mut out, &mut para, cx);
            let quote = if line.len() > 2 { &line[2..] } else { "" };
            if bq.is_empty() {
                bq.push_str(quote);
            } else {
                bq.push(' ');
                bq.push_str(quote);
            }
            continue;
        }
        if !bq.is_empty() {
            out.push_str(&format!("<blockquote>{}</blockquote>", render_inline(&bq, cx)));
            bq.clear();
        }
        if line.is_empty() {
            flush_para(&mut out, &mut para, cx);
            continue;
        }
        if para.is_empty() && pending.is_empty() {
            pending.push_str(line);
            continue;
        }
        join_line(&mut para, line);
    }
    if !pending.is_empty() {
        join_line(&mut para, &pending);
    }
    if !list_kind.is_empty() {
        list_item(&mut out, &item, cx);
        out.push_str(&format!("</{list_kind}>"));
    }
    if !bq.is_empty() {
        out.push_str(&format!("<blockquote>{}</blockquote>", render_inline(&bq, cx)));
    }
    if table_state == 2 {
        out.push_str("</tbody></table>");
    }
    if in_code {
        out.push_str(&format!("<pre><code>{}</code></pre>", html_escape(&code_buf)));
    } else if !para.is_empty() {
        out.push_str(&format!("<p>{}</p>", render_inline(&para, cx)));
    }
    out
}

struct Heading {
    level: i64,
    text: String,
    slug: String,
}

fn extract_headings(source: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    let src: String = source.chars().filter(|&c| c != '\r').collect();
    let lines: Vec<&str> = if src.is_empty() { Vec::new() } else { src.split('\n').collect() };
    let mut in_code = false;
    let mut prev = "";
    for &line in &lines {
        if in_code {
            if line.starts_with("```") {
                in_code = false;
            }
            prev = "";
            continue;
        }
        if line.starts_with("```") {
            in_code = true;
            prev = "";
            continue;
        }
        if !prev.is_empty() {
            let level = if is_setext_underline(line, b'=') {
                1
            } else if is_setext_underline(line, b'-') {
                2
            } else {
                0
            };
            if level > 0 {
                out.push(Heading { level, text: prev.to_string(), slug: slugify(prev) });
                prev = "";
                continue;
            }
            prev = "";
        }
        let hlevel = atx_heading_level(line);
        if hlevel > 0 {
            let txt = strip_trailing_hashes(if hlevel + 1 < line.len() { &line[hlevel + 1..] } else { "" });
            out.push(Heading { level: hlevel as i64, text: txt.to_string(), slug: slugify(txt) });
            continue;
        }
        let prose = !line.is_empty()
            && !line.starts_with("- ")
            && !line.starts_with("* ")
            && !line.starts_with("> ")
            && !line.starts_with('|')
            && !line.starts_with("    ");
        prev = if prose { line } else { "" };
    }
    out
}

// ── The workloads (identical to bench.loft's) ───────────────────────

fn inline_para(v: i64) -> String {
    format!("The **render {v} pass with *nested emphasis* inside** walks each `span_{v}` of the paragraph and links [the guide](../guide.md#intro \"Guide\") next to ~~retired wording~~ plus <https://example.org/docs> and <help@example.org> while the plain prose between spans carries most of the bytes: alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi & a < b > c \"q\".")
}

fn escape_input(v: i64) -> String {
    let unit = format!("{v}: if (a < b) && (c > d) then print \"ok\" else keep the plain fallback value for every single caller.");
    format!("{unit}{unit}")
}

fn slug_input(v: i64) -> String {
    format!("{v}. Getting Started: The Loft Markdown Renderer, Part 2 (Beta)")
}

fn headings_doc(v: i64) -> String {
    let mut s = String::new();
    for k in 0..122 {
        s.push_str(&format!("## Section {k} Overview {v}\n"));
        s.push_str(&format!("Intro paragraph line for section {k}.\n"));
        s.push_str("Second line of prose.\n\n");
        s.push_str(&format!("Setext Title {k}\n"));
        s.push_str("----------------\n\n- item one\n- item two\n\n```loft\n# not a heading\n```\n");
        s.push_str(&format!("### Details {k} ###\n"));
        s.push_str("> quote line\n| a | b |\nTrailing prose.\n\n");
    }
    s
}

fn render_doc(v: i64) -> String {
    let mut s = String::new();
    for k in 0..160 {
        s.push_str(&format!("## Section {k}: Overview {v}\n"));
        s.push_str(&format!("This paragraph opens section {k} with **bold text**, `inline_code`, a [link](other.md#part) and\n"));
        s.push_str("*emphasis* that continues onto a second line with ~~struck~~ words & symbols < > \"quoted\".\n\n");
        s.push_str(&format!("Setext Heading {k}\n"));
        s.push_str("------------------\n\n- first item with `code`\n- [x] done task\n- [ ] open task\n");
        s.push_str("  continued on an indented line\n1. ordered one\n2. ordered two\n\n```loft\n");
        s.push_str(&format!("let x{k} = {k} < 3 && y > 2;\n"));
        s.push_str("```\n| Name | Value | Note |\n|:-----|:-----:|-----:|\n");
        s.push_str(&format!("| a{k} | **{k}** | `x` |\n"));
        s.push_str("| b | c | d |\n\n");
        s.push_str(&format!("> Quoted text for section {k}\n"));
        s.push_str("> with a second line\n\n<!-- a comment -->\n---\n");
        s.push_str(&format!("    indented code line {k}\n\n"));
        s.push_str(&format!("Closing paragraph @P259 with <https://example.org/{k}> text and ![img](pics/a.png).\n\n"));
    }
    s
}

const CTX: Ctx<'static> = Ctx { base_dir: "doc/claude", tag_url_prefix: "/tag/", image_url_prefix: "/raw/" };

// ── The rows ────────────────────────────────────────────────────────

/// `calls` calls per op of `f` over the four rotated inputs.
fn batched(name: &'static str, n: i64, calls: usize, vars: &[String], f: impl Fn(&str) -> String) -> Row {
    let t0 = Instant::now();
    let mut sink = 0i64;
    for r in 0..n as usize {
        for k in 0..calls {
            let out = f(black_box(&vars[(r + k) & 3]));
            sink = black_box(sink + out.len() as i64);
        }
    }
    let us = t0.elapsed().as_micros() as i64;
    let one = f(&vars[0]);
    Row { name, iters: n, us, px: (calls * vars[0].len()) as i64, hash: fnv_text(FNV_OFFSET, &one), sink }
}

const HEADING_PASSES: usize = 4;

fn bench_extract_headings(n: i64) -> Row {
    let vars: Vec<String> = (0..4).map(headings_doc).collect();
    let t0 = Instant::now();
    let mut sink = 0i64;
    for r in 0..n as usize {
        for k in 0..HEADING_PASSES {
            let out = extract_headings(black_box(&vars[(r + k) & 3]));
            sink = black_box(sink + out.len() as i64);
        }
    }
    let us = t0.elapsed().as_micros() as i64;
    let mut h = FNV_OFFSET;
    for e in extract_headings(&vars[0]) {
        h = fnv(h, &[e.level]);
        h = fnv_text(h, &e.text);
        h = fnv_text(h, &e.slug);
    }
    Row { name: "extract_headings", iters: n, us, px: (HEADING_PASSES * 2196) as i64, hash: h, sink }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut n: i64 = 20;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--n" && i + 1 < args.len() {
            n = args[i + 1].parse().unwrap_or(20);
            i += 1;
        }
        i += 1;
    }
    if n < 1 {
        n = 1;
    }
    println!("routine\titers\tus\tns_op\tpx\tns_px\thash");
    let paras: Vec<String> = (0..4).map(inline_para).collect();
    print_row(&batched("render_inline", n, 1000, &paras, |s| render_inline(s, &CTX)));
    let escs: Vec<String> = (0..4).map(escape_input).collect();
    print_row(&batched("html_escape", n, 10000, &escs, html_escape));
    let slugs: Vec<String> = (0..4).map(slug_input).collect();
    print_row(&batched("slugify", n, 10000, &slugs, slugify));
    print_row(&bench_extract_headings(n));
    let docs: Vec<String> = (0..4).map(render_doc).collect();
    print_row(&batched("render", n, 1, &docs, |s| render(s, &CTX)));
}
