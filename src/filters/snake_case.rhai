// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// crank's built-in `snake_case` filter — a documentation example of the
// plain `fn filter(value)` convention. Converts `value` into snake_case:
// letters and digits in any script are kept (an uppercase letter is
// lowercased), along with any combining mark following one (so a
// decomposed "é" keeps its accent), and a run of anything else (spaces,
// punctuation, emoji, an existing `_`/`-`) collapses to a single "_",
// trimmed from the start/end of the result. A "_" is also inserted where
// an uppercase letter follows a lowercase letter or digit, so
// "helloWorld" becomes "hello_world" and "приветМир" becomes
// "привет_мир" — but a run of consecutive uppercase letters (an acronym,
// e.g. "HTTPServer") is treated as one segment rather than split at its
// last letter, to keep this reference example simple: "HTTPServer"
// becomes "httpserver", not "http_server". Handy for turning a data
// field's display text into a valid identifier in generated code. Not
// one of Tera's own built-in filters (https://keats.github.io/tera/#filters).
//
// `is_alphanumeric`, `is_uppercase` and `is_mark` are character classes
// crank adds to every filter script — see docs/crank-file-format.md.
fn filter(value) {
    let result = "";
    let last_was_separator = true; // suppresses a leading "_"
    let last_was_lower_or_digit = false;

    for ch in value {
        if ch.is_uppercase() {
            if last_was_lower_or_digit {
                result += "_";
            }
            result += ch.to_string().to_lower();
            last_was_lower_or_digit = false;
            last_was_separator = false;
        } else if ch.is_alphanumeric() {
            result += ch;
            last_was_lower_or_digit = true;
            last_was_separator = false;
        } else if ch.is_mark() && !last_was_separator {
            // Belongs to the letter before it; changes nothing else.
            result += ch;
        } else if !last_was_separator {
            result += "_";
            last_was_lower_or_digit = false;
            last_was_separator = true;
        }
    }
    if result.ends_with("_") {
        result = result.sub_string(0, result.len() - 1);
    }
    result
}
