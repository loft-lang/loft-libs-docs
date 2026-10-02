// Copyright (c) 2026 Jurjen Stellingwerff
// SPDX-License-Identifier: LGPL-3.0-or-later
//
// html-reference — the pure-Rust twin of the `html` package's performance pass
// (bench/bench.loft), one file built with `rustc -O`.  It builds the SAME workloads and
// runs the SAME algorithm — src/html.loft's `escape_html`, one pass over the characters
// written as ordinary idiomatic Rust (`String::with_capacity`, a `match`, `push_str` for an
// entity and `push` for everything else) — and prints the same rows, hash included: a row
// whose hash matches the loft build's is a like-for-like comparison.  No dependencies and
// no cleverness: the speed an industry implementation reaches without effort, which is
// what the bar should be.
//
//     mkdir -p bench/.build && rustc -O --edition=2021 bench/bench.rs -o bench/.build/stats_rs
//     bench/.build/stats_rs --n 20
use std::hint::black_box;
use std::time::Instant;

const FNV_OFFSET: i64 = 2166136261;
const FNV_PRIME: i64 = 16777619;

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

// ── src/html.loft ───────────────────────────────────────────────────

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

// ── The workloads (identical to bench.loft's) ───────────────────────

fn short_input(v: i64) -> String {
    format!("{v}: <a href='x'>Tom & \"Jerry\" & 'Spike'</a> is the plain text that the doc viewer escapes before it lands in a page body.")
}

fn blob_input(v: i64) -> String {
    let mut s = format!("{v}");
    for _ in 0..1024 {
        s.push_str("<li class='row'>item & \"value\" in one sixty-four byte line</li>\n");
    }
    s
}

fn bench_escape(n: i64, name: &'static str, vars: &[String], calls: usize) -> Row {
    let t0 = Instant::now();
    let mut sink = 0i64;
    for r in 0..n as usize {
        for k in 0..calls {
            let out = escape_html(black_box(&vars[(r + k) & 3]));
            sink = black_box(sink + out.len() as i64);
        }
    }
    let us = t0.elapsed().as_micros() as i64;
    let one = escape_html(&vars[0]);
    Row { name, iters: n, us, px: (calls * vars[0].len()) as i64, hash: fnv_text(FNV_OFFSET, &one), sink }
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
    let short: Vec<String> = (0..4).map(short_input).collect();
    let blob: Vec<String> = (0..4).map(blob_input).collect();
    println!("routine\titers\tus\tns_op\tpx\tns_px\thash");
    print_row(&bench_escape(n, "escape_html", &short, 10000));
    print_row(&bench_escape(n, "escape_html_blob", &blob, 16));
}
