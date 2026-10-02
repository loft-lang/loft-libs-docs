<!--
Copyright (c) 2026 Jurjen Stellingwerff
SPDX-License-Identifier: LGPL-3.0-or-later
-->

# markdown — CommonMark and a GFM subset, rendered to HTML

A markdown renderer written in loft. Give it a document, get HTML back. It also extracts a
heading list, so a page can build its own table of contents from the same source it renders.

Pure loft — no C, no parser generator, one pass over the text.

A guide: [docs/01-getting-started.loft](docs/01-getting-started.loft).

## Install

```sh
loft install markdown
```

```loft
use markdown;
```

## The smallest thing that does something

```loft
use markdown;

fn main() {
    md = "# Title\n\nSome *emphasis* and a [link](other.md).\n";
    println(markdown::render(md, "", "", ""));
    for h in markdown::extract_headings(md) {
        println("heading: {h.text} -> #{markdown::slugify(h.text)}");
    }
}
```

```
<h1 id="title">Title</h1><p>Some <em>emphasis</em> and a <a href="other.md">link</a>.</p>
heading: Title -> #title
```

## The four arguments to `render`

`render(source, base_dir, tag_url_prefix, image_url_prefix)`. Only the first is the document;
the other three are URL rewriting, and **passing `""` turns each one off**. That is the normal
call for rendering a standalone document:

```loft
markdown::render(source, "", "", "")
```

| argument | what it does when non-empty |
|---|---|
| `base_dir` | resolves relative `[text](other.md)` links against this repo-relative directory and routes them through `/file/` — `other.md` under `doc/sub` becomes `/file/doc/sub/other.md` ([`@MKD-001`](tests/02-worked-examples.loft)) |
| `tag_url_prefix` | body-text `@P123` / `@PLAN22` mentions become links to `<prefix><name>`; no other tag form is linked |
| `image_url_prefix` | relative `<img src>` URLs are routed through this prefix, e.g. `/raw/` so a viewer serves the bytes |

They exist because this renderer's first consumer serves a repository's own Markdown, where a
relative link between two files has to become a URL. If you are rendering a document that
stands alone, you want all three empty.

## The rest of the surface

| | |
|---|---|
| `render(…) -> text` | a whole document to HTML |
| `render_inline(…) -> text` | one line's *inline* markup only — no block wrapping, no `<p>` |
| `extract_headings(source) -> vector<Heading>` | every heading in source order: `level`, `text`, `slug` |
| `slugify(heading) -> text` | the GitHub-compatible anchor a heading gets as its `id` |
| `rewrite_link(url, base_dir) -> text` | the link rewriting on its own, for a caller doing its own rendering |
| `html_escape(s) -> text` | the escaper the renderer uses internally |

`extract_headings` returns the heading text **before** inline rendering, so `## The *hard*
way` gives you the raw text. Pass it through `render_inline` if the table of contents should
carry the emphasis too. The slug it returns is the same one `render` puts on the heading, so a
generated `#anchor` always lands ([`@MKD-002`](tests/02-worked-examples.loft)).

## What it supports

**Blocks** — ATX (`#`…`######`) and setext headings with GitHub-compatible slug ids,
paragraphs, fenced ` ```lang ` and indented code, block quotes, horizontal rules, unordered
and ordered lists with nesting, task lists (`- [x]`), GFM tables with alignment, and HTML
comments (stripped).

**Inline** — bold, italic (with a `snake_case` heuristic so `some_name_here` is not read as
emphasis), inline code, strikethrough, links with optional titles, images, autolinks, hard
line breaks, backslash escapes, and nesting between all of them.

## What it does not support

Deferred because they are rare in the documents this renders — reference-style links
(`[text][label]`), definition lists, footnotes, mermaid, math, multi-paragraph table cells,
and loose lists (a list whose items become paragraphs).  A reference-style link renders as its
literal text, and its `[label]: url` definition line as a paragraph of its own.

**Raw HTML in the source is escaped, not passed through.** That is a deliberate safety
default: markdown from an untrusted source cannot inject markup. It also means you cannot drop
a `<div>` into a document and have it survive.

## The one that surprises people — the apostrophe

`markdown::html_escape` escapes four characters — `&`, `<`, `>`, `"` — and **not** the
apostrophe. That is safe for the element bodies the renderer puts text into, and unsafe for a
single-quoted attribute value ([`@MKD-003`](tests/02-worked-examples.loft)).

The `html` package's `escape_html` escapes all five, including `'` as `&#x27;`. If you are
writing your own markup, use that one; this function is exported because the renderer needs
it, not because it is the general-purpose escaper.

## Testing

```sh
cd markdown && loft test
```

`tests/01-render.loft` walks the supported constructs and pins their HTML;
`tests/02-worked-examples.loft` holds `@MKD-001` … `@MKD-003`, the examples the source cites.

## Status

Stable and additive. Pure loft with no dependencies, so it behaves identically on the
interpreter, `--native`, wasm and in the browser. loft's repository viewer (`make view`)
renders the repository's Markdown with it.

## License

LGPL-3.0-or-later.
