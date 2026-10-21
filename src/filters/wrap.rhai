// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// crank's built-in `wrap` filter — a documentation example of the
// `fn filter(value, args)` convention for named parameters. Word-wraps
// `value` at `width` columns, breaking on whitespace, prefixing every
// line with `indent` (default "") — handy for wrapping long text into
// a fixed-width comment block. Not one of Tera's own built-in filters
// (https://keats.github.io/tera/#filters).
//
// Usage: {{ text | wrap(width=20) }}
//        {{ text | wrap(width=20, indent="// ") }}
//
// A missing key in `args` doesn't error on its own — `args.get(...)`
// just returns `()` — so an unchecked required parameter fails later
// with a confusing type error instead of a clear one. Check with
// `args.contains(...)` up front and `throw` a specific message for a
// required parameter; use the same check with a fallback value for an
// optional one (see `indent` below).
fn filter(value, args) {
    if !args.contains("width") {
        throw "wrap filter requires a `width` named parameter, e.g. `wrap(width=20)`";
    }
    let width = args.get("width");
    let indent = if args.contains("indent") { args.get("indent") } else { "" };

    let lines = [];
    let current = "";
    for word in value.split() {
        let candidate = if current.is_empty() { word } else { current + " " + word };
        if candidate.len() > width && !current.is_empty() {
            lines.push(current);
            current = word;
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    let result = "";
    for line in lines {
        if !result.is_empty() {
            result += "\n";
        }
        result += indent + line;
    }
    result
}
