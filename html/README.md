<!--
Copyright (c) 2026 Jurjen Stellingwerff
SPDX-License-Identifier: LGPL-3.0-or-later
-->

# html — escape text so it is safe inside HTML

One function. It escapes the five characters that can break out of an HTML element body or an
attribute value, so text you did not write can be interpolated into markup without becoming
markup.

It is deliberately small: escaping is a library concern rather than a core one, so it is not
in the always-loaded standard library.

A guide: [docs/01-getting-started.loft](docs/01-getting-started.loft).

## Install

```sh
loft install html
```

```loft
use html;
```

## Use it

`escape_html` is a **method on `text`**, so it reads in the direction the data flows:

```loft
use html;

fn main() {
    raw = "Tom & Jerry's <b>\"show\"</b>";
    println("<p>{raw.escape_html()}</p>");
}
```

```
<p>Tom &amp; Jerry&#x27;s &lt;b&gt;&quot;show&quot;&lt;/b&gt;</p>
```

| character | becomes |
|---|---|
| `&` | `&amp;` |
| `<` | `&lt;` |
| `>` | `&gt;` |
| `"` | `&quot;` |
| `'` | `&#x27;` |

Each character is replaced once, in one pass, so the entities a call writes are not escaped
again by that call.  A second call does escape them — `"&amp;"` becomes `"&amp;amp;"` — so
escape once, where the text enters markup ([`@HTM-001`](tests/02-worked-examples.loft)).

## Where to use it

**Every** place text you did not write reaches HTML output: an element body, and an attribute
value in either quoting style. A `<p>` needs it as much as an `<a title='…'>` does
([`@HTM-002`](tests/02-worked-examples.loft)).

It is **not** a sanitiser. It does not filter markup, strip scripts, or make attacker-supplied
*markup* safe — it makes attacker-supplied *text* safe to place in markup, which is a
different and much stronger guarantee. If you need to accept a subset of HTML from a user,
this is not the tool.

It also does not escape for other contexts. Text going into a URL, a JavaScript string or a
CSS value needs that context's own escaping; HTML entities protect none of them.

## The one that surprises people — `markdown` has its own, and it differs

The `markdown` package exports `html_escape`, which looks like the same function with its
words swapped. It is not quite:

```loft
use html;
use markdown;

fn main() {
    raw = "Jerry's";
    println("html:     {raw.escape_html()}");
    println("markdown: {markdown::html_escape(raw)}");
}
```

```
html:     Jerry&#x27;s
markdown: Jerry's
```

**`markdown::html_escape` does not escape the apostrophe.** That is fine for the element
bodies the renderer uses it for, and unsafe for a single-quoted attribute value. When you are
writing markup yourself, reach for this package.

## Testing

```sh
cd html && loft test
```

`tests/01-escape.loft` covers each entity, the ordering rule, and text that needs no escaping;
`tests/02-worked-examples.loft` holds `@HTM-001` and `@HTM-002`, the examples the source cites.

## Status

Stable. One function, `#pure`, no dependencies, no state — so it behaves identically on the
interpreter, `--native`, wasm and in the browser, and there is nothing here to leak.

## License

LGPL-3.0-or-later.
