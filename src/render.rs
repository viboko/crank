// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rendering a template against a Tera context, with a caller-supplied set
//! of custom filters (crank's built-ins plus any user-declared ones)
//! available.

use std::collections::HashMap;
use std::fmt;

use tera::{Tera, Value};

use crate::rhai_filter::CompiledFilter;

/// The name a template is registered under within the throwaway [`Tera`]
/// instance built for each [`render`] call — never seen by the caller,
/// since a single template is rendered per instance.
const TEMPLATE_NAME: &str = "template";

/// A template that failed to parse or render: malformed Tera syntax, a
/// reference to a variable missing from the context, a filter/function
/// error, and so on.
#[derive(Debug)]
pub struct RenderError(tera::Error);

impl fmt::Display for RenderError {
    /// Tera's own `Display` for [`tera::Error`] only prints the outermost
    /// message (often just `Failed to render 'template'`), leaving the
    /// actual cause (an undefined variable, a filter erroring, malformed
    /// syntax, ...) sitting unprinted in its `source()` chain. Walk that
    /// chain and append every level, so the real reason is always in the
    /// message crank shows, not just discoverable by a caller that happens
    /// to inspect `source()` itself.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = std::error::Error::source(&self.0);
        while let Some(err) = source {
            write!(f, ": {err}")?;
            source = err.source();
        }
        Ok(())
    }
}

impl std::error::Error for RenderError {}

/// Renders `template_source` against `context`, with every filter in
/// `filters` (name -> compiled Rhai script) available under its declared
/// name.
///
/// # Errors
/// Returns [`RenderError`] if `template_source` has malformed Tera syntax,
/// fails to render against `context` (e.g. a reference to a variable
/// missing from the context, or a filter erroring on its input), or a name
/// in `filters` collides with one of Tera's own built-in filters or isn't
/// one Tera's template syntax can call.
///
/// # Panics
/// Panics if Tera ever hands a filter call a keyword-argument key that
/// isn't a string — Tera's own template syntax only allows `name=value`
/// kwargs with an identifier name, so this should never actually happen.
// `filters` is always the std `HashMap` crank builds internally — this is
// a binary crate's private wiring, not a library API other crates hand
// arbitrary hashers to, so genericizing over `BuildHasher` buys nothing.
#[allow(clippy::implicit_hasher)]
pub fn render(
    template_source: &str,
    context: &tera::Context,
    filters: &HashMap<String, CompiledFilter>,
) -> Result<String, RenderError> {
    let mut tera = Tera::default();
    // `Tera::register_filter` silently overwrites an existing filter of the
    // same name, with no error either way — so without this check, a
    // future Tera release adding a built-in filter that happens to share a
    // name with one of crank's own (built-in or user-declared) would
    // silently change template behavior instead of failing loudly. Checked
    // up front against every filter before registering any of them, so
    // this can never depend on registration order.
    //
    // Tera doesn't expose any public way to ask "is a filter already
    // registered under this name" (its `filters` field is crate-private,
    // and there's no `contains_filter`/`get_filter` accessor), so this
    // detects a collision behaviorally instead: parsing a one-off template
    // that pipes a literal through the candidate name fails with a
    // specifically-worded "Unknown filter" message if and only if Tera
    // doesn't already have a filter under that name. That wording is
    // covered by Tera's own snapshot tests, so it's a reasonably stable
    // (if indirect) signal — see tera's `src/tera.rs` (`format!("Unknown
    // filter `{filter}`")`) and `src/snapshot_tests/`.
    for name in filters.keys() {
        let probe_result = tera.add_raw_template(
            "__crank_filter_collision_probe",
            &format!("{{{{ 1 | {name} }}}}"),
        );
        match probe_result {
            Err(err)
                if err
                    .to_string()
                    .contains(&format!("Unknown filter `{name}`")) => {}
            Ok(()) => {
                return Err(RenderError(tera::Error::message(format!(
                    "Filter `{name}` collides with a Tera built-in filter of the same name"
                ))));
            }
            // Not a collision: the probe didn't even parse, because Tera
            // can't read `name` as a filter name (e.g. `true`).
            Err(err) => {
                return Err(RenderError(tera::Error::message(format!(
                    "Filter `{name}` can't be called from a template: {err}"
                ))));
            }
        }
    }
    for (name, filter) in filters {
        let filter = filter.clone();
        tera.register_filter(
            name.clone(),
            move |value: &Value,
                  kwargs: tera::Kwargs,
                  _state: &tera::State|
                  -> Result<Value, tera::Error> {
                // Tera's `Kwargs` has no direct `HashMap` conversion, so
                // rebuild the plain name-to-value map `CompiledFilter::call`
                // expects.
                let args: HashMap<String, Value> = kwargs
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.as_str()
                                .expect("filter kwargs keys are always strings")
                                .to_string(),
                            value.clone(),
                        )
                    })
                    .collect();
                filter
                    .call(value, &args)
                    .map_err(|err| tera::Error::message(err.to_string()))
            },
        );
    }
    tera.add_raw_template(TEMPLATE_NAME, template_source)
        .map_err(RenderError)?;
    tera.render(TEMPLATE_NAME, context).map_err(RenderError)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_from(pairs: &[(&str, &str)]) -> tera::Context {
        let mut context = tera::Context::new();
        for (key, value) in pairs {
            context.insert((*key).to_string(), value);
        }
        context
    }

    fn no_filters() -> HashMap<String, CompiledFilter> {
        HashMap::new()
    }

    fn custom_filter(name: &str, script: &str) -> HashMap<String, CompiledFilter> {
        let mut filters = HashMap::new();
        filters.insert(name.to_string(), CompiledFilter::compile(script).unwrap());
        filters
    }

    #[test]
    fn substitutes_a_plain_variable() {
        let context = context_from(&[("name", "World")]);
        let output = render("Hello {{ name }}", &context, &no_filters()).unwrap();
        assert_eq!(output, "Hello World");
    }

    #[test]
    fn loops_over_an_array_using_loop_last() {
        let mut context = tera::Context::new();
        context.insert("items", &["a", "b", "c"]);
        let output = render(
            "{% for item in items %}{{ item }}{% if not loop.last %}, {% endif %}{% endfor %}",
            &context,
            &no_filters(),
        )
        .unwrap();
        assert_eq!(output, "a, b, c");
    }

    #[test]
    fn loops_over_a_map() {
        let mut map = std::collections::BTreeMap::new();
        map.insert("alpha", 1);
        map.insert("beta", 2);
        let mut context = tera::Context::new();
        context.insert("entries", &map);
        let output = render(
            "{% for key, value in entries %}{{ key }}={{ value }};{% endfor %}",
            &context,
            &no_filters(),
        )
        .unwrap();
        assert_eq!(output, "alpha=1;beta=2;");
    }

    #[test]
    fn applies_the_snake_case_filter_via_the_builtin_pipeline() {
        let context = context_from(&[("name", "Hello, World!")]);
        let output = render(
            "{{ name | snake_case }}",
            &context,
            &crate::rhai_filter::built_in_filters(),
        )
        .unwrap();
        assert_eq!(output, "hello_world");
    }

    #[test]
    fn applies_the_wrap_filter_with_a_named_parameter_via_the_builtin_pipeline() {
        let context = context_from(&[("text", "the quick brown fox jumps")]);
        let output = render(
            "{{ text | wrap(width=10) }}",
            &context,
            &crate::rhai_filter::built_in_filters(),
        )
        .unwrap();
        assert_eq!(output, "the quick\nbrown fox\njumps");
    }

    #[test]
    fn a_filter_colliding_with_a_tera_built_in_is_a_render_error() {
        // `upper` isn't one of crank's own built-ins, but Tera registers it
        // internally by default — this is standing in for a name crank
        // doesn't know is reserved until Tera adds it.
        let context = context_from(&[("name", "widget")]);
        let filters = custom_filter("upper", "fn filter(value) { value }");
        let err = render("{{ name | upper }}", &context, &filters).unwrap_err();
        assert!(err.to_string().contains("upper"));
    }

    #[test]
    fn referencing_an_undefined_variable_is_a_render_error() {
        let context = tera::Context::new();
        let err = render("{{ missing }}", &context, &no_filters()).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn malformed_template_syntax_is_a_render_error() {
        let context = tera::Context::new();
        assert!(render("{% if %}", &context, &no_filters()).is_err());
    }

    #[test]
    fn a_render_errors_message_includes_the_underlying_cause() {
        // Tera's `Display` for its own error type only prints the
        // outermost "Failed to render 'template'" message, not the
        // underlying cause — that only lives in the error's `source()`
        // chain. `RenderError`'s `Display` must walk that chain, or a
        // user sees a useless top-level message with no way to tell what
        // actually went wrong.
        let context = tera::Context::new();
        let err = render("{{ missing }}", &context, &no_filters()).unwrap_err();
        let message = err.to_string();
        assert!(
            message.len() > "Failed to render 'template'".len(),
            "expected the underlying cause to be included, got: {message}"
        );
    }

    #[test]
    fn applies_a_custom_rhai_filter() {
        let filters = custom_filter("identity", "fn filter(value) { value }");
        let context = context_from(&[("name", "world")]);
        let output = render("{{ name | identity }}", &context, &filters).unwrap();
        assert_eq!(output, "world");
    }

    #[test]
    fn a_custom_filter_can_transform_a_value() {
        let filters = custom_filter("shout", r#"fn filter(value) { value + "!" }"#);
        let context = context_from(&[("name", "world")]);
        let output = render("{{ name | shout }}", &context, &filters).unwrap();
        assert_eq!(output, "world!");
    }

    #[test]
    fn a_custom_filter_missing_its_filter_function_is_a_render_error() {
        let filters = custom_filter("broken", "fn something_else(value) { value }");
        let context = context_from(&[("name", "world")]);
        assert!(render("{{ name | broken }}", &context, &filters).is_err());
    }

    #[test]
    fn a_custom_filter_with_wrong_arity_is_a_render_error() {
        // Arity 2 is the named-parameter convention (`fn filter(value,
        // args)`), so a genuine arity mismatch needs a third parameter.
        let filters = custom_filter("broken", "fn filter(a, b, c) { a }");
        let context = context_from(&[("name", "world")]);
        assert!(render("{{ name | broken }}", &context, &filters).is_err());
    }

    #[test]
    fn a_custom_filter_can_use_a_named_parameter() {
        let filters = custom_filter(
            "greet",
            r#"fn filter(value, args) { value + args.get("suffix") }"#,
        );
        let context = context_from(&[("name", "world")]);
        let output = render("{{ name | greet(suffix=\"!\") }}", &context, &filters).unwrap();
        assert_eq!(output, "world!");
    }

    #[test]
    fn a_custom_filter_runtime_error_is_a_render_error() {
        let filters = custom_filter("broken", "fn filter(value) { 1 / 0 }");
        let context = context_from(&[("name", "world")]);
        assert!(render("{{ name | broken }}", &context, &filters).is_err());
    }

    #[test]
    fn a_custom_filter_returning_a_function_pointer_stringifies_it() {
        // Rhai's `Serialize` impl for `Dynamic` is total — every variant,
        // including a bare function pointer, converts to *something*
        // (here, the function's name) rather than failing. There's
        // currently no Rhai value this conversion actually rejects.
        let filters = custom_filter("broken", r#"fn filter(value) { Fn("filter") }"#);
        let context = context_from(&[("name", "world")]);
        let output = render("{{ name | broken }}", &context, &filters).unwrap();
        assert_eq!(output, "filter");
    }

    // -- Non-ASCII text ---------------------------------------------------

    fn built_ins() -> HashMap<String, CompiledFilter> {
        crate::rhai_filter::built_in_filters()
    }

    fn snake_case(text: &str) -> String {
        let context = context_from(&[("text", text)]);
        render("{{ text | snake_case }}", &context, &built_ins()).unwrap()
    }

    #[test]
    fn snake_case_keeps_letters_and_digits_in_any_script() {
        for (input, expected) in [
            ("Crème Brûlée", "crème_brûlée"),
            ("Zażółć Gęślą Jaźń", "zażółć_gęślą_jaźń"),
            ("Привет мир", "привет_мир"),
            ("日本語 テキスト", "日本語_テキスト"),
            ("안녕하세요 세계", "안녕하세요_세계"),
            ("Straße 42", "straße_42"),
            ("ÉCOLE", "école"),
            ("ŁÓDŹ", "łódź"),
        ] {
            assert_eq!(snake_case(input), expected, "{input}");
        }
    }

    #[test]
    fn snake_case_splits_a_non_ascii_camel_case_boundary() {
        assert_eq!(snake_case("приветМир"), "привет_мир");
        assert_eq!(snake_case("żółwŻółw"), "żółw_żółw");
        assert_eq!(snake_case("caféÉtoile"), "café_étoile");
    }

    #[test]
    fn snake_case_treats_emoji_and_non_ascii_punctuation_as_separators() {
        assert_eq!(snake_case("🍕 Pizza 🔥"), "pizza");
        assert_eq!(snake_case("pizza🍕pasta"), "pizza_pasta");
        assert_eq!(snake_case("«Bonjour», monde!"), "bonjour_monde");
        assert_eq!(snake_case("日本語、テキスト。"), "日本語_テキスト");
        assert_eq!(snake_case("👩‍👩‍👧‍👦"), "");
    }

    #[test]
    fn snake_case_keeps_a_combining_mark_attached_to_its_letter() {
        assert_eq!(snake_case("E\u{301}cole Normale"), "e\u{301}cole_normale");
    }

    #[test]
    fn wrap_counts_characters_not_bytes() {
        let context = context_from(&[("text", "żółw żółw żółw")]);
        let output = render("{{ text | wrap(width=9) }}", &context, &built_ins()).unwrap();
        // Each "żółw" is 4 characters but 7 bytes — counting bytes would
        // put every word on its own line.
        assert_eq!(output, "żółw żółw\nżółw");
    }

    #[test]
    fn wrap_handles_emoji_and_a_non_ascii_indent() {
        let context = context_from(&[("text", "🍕 🍕 🍕 🍕")]);
        let output = render(
            "{{ text | wrap(width=3, indent=\"» \") }}",
            &context,
            &built_ins(),
        )
        .unwrap();
        assert_eq!(output, "» 🍕 🍕\n» 🍕 🍕");
    }

    #[test]
    fn wrap_breaks_on_unicode_whitespace() {
        // An ideographic space (U+3000) between Japanese words.
        let context = context_from(&[("text", "日本語\u{3000}テキスト")]);
        let output = render("{{ text | wrap(width=4) }}", &context, &built_ins()).unwrap();
        assert_eq!(output, "日本語\nテキスト");
    }

    #[test]
    fn a_custom_filter_sees_non_ascii_values_and_named_arguments() {
        let filters = custom_filter(
            "decorate",
            "fn filter(value, args) { args.get(\"left\") + value + args.get(\"right\") }",
        );
        let context = context_from(&[("name", "Żurek")]);
        let output = render(
            "{{ name | decorate(left=\"«🍲 \", right=\" ✓»\") }}",
            &context,
            &filters,
        )
        .unwrap();
        assert_eq!(output, "«🍲 Żurek ✓»");
    }

    #[test]
    fn a_custom_filter_reads_a_map_with_non_ascii_keys() {
        let filters = custom_filter("pick", "fn filter(value) { value[\"名前\"] }");
        let mut context = tera::Context::new();
        let mut map = HashMap::new();
        map.insert("名前", "寿司");
        context.insert("dish", &map);
        let output = render("{{ dish | pick }}", &context, &filters).unwrap();
        assert_eq!(output, "寿司");
    }

    #[test]
    fn a_custom_filter_iterates_characters_not_bytes() {
        let filters = custom_filter("count", "fn filter(value) { value.len() }");
        let context = context_from(&[("text", "żółw日本🍕")]);
        assert_eq!(
            render("{{ text | count }}", &context, &filters).unwrap(),
            "7"
        );
    }

    #[test]
    fn tera_built_ins_handle_non_ascii_text() {
        let context = context_from(&[("text", "żółw ŻÓŁW 日本語 🍕")]);
        let filters = no_filters();
        assert_eq!(
            render("{{ text | upper }}", &context, &filters).unwrap(),
            "ŻÓŁW ŻÓŁW 日本語 🍕"
        );
        assert_eq!(
            render("{{ text | lower }}", &context, &filters).unwrap(),
            "żółw żółw 日本語 🍕"
        );
        assert_eq!(
            render("{{ text | length }}", &context, &filters).unwrap(),
            "15"
        );
        // Truncating never splits a multi-byte character...
        assert_eq!(
            render(
                "{{ text | truncate(length=2, end=\"\") }}",
                &context,
                &filters
            )
            .unwrap(),
            "żó"
        );
        // ...but Tera counts code points, not what a reader sees as one
        // character, so it can split a multi-code-point emoji (here, a
        // family: four people joined by zero-width joiners).
        let context = context_from(&[("text", "👩‍👩‍👧‍👦")]);
        assert_eq!(
            render(
                "{{ text | truncate(length=1, end=\"\") }}",
                &context,
                &filters
            )
            .unwrap(),
            "👩"
        );
    }

    #[test]
    fn a_template_full_of_non_ascii_literal_text_renders_verbatim() {
        let template =
            "日本語 {{ a }} Привет {{ b }} 🍕\n{# コメント #}한국어{% if true %} ✓{% endif %}";
        let context = context_from(&[("a", "Zażółć"), ("b", "👩‍👩‍👧‍👦")]);
        assert_eq!(
            render(template, &context, &no_filters()).unwrap(),
            "日本語 Zażółć Привет 👩‍👩‍👧‍👦 🍕\n한국어 ✓"
        );
    }

    #[test]
    fn a_non_ascii_variable_name_is_a_render_error_not_a_panic() {
        // Tera identifiers are ASCII-only.
        let context = context_from(&[("名前", "x")]);
        assert!(render("{{ 名前 }}", &context, &no_filters()).is_err());
    }

    #[test]
    fn a_filter_name_tera_cannot_parse_is_not_reported_as_a_collision() {
        for name in ["odwróć", "true"] {
            let filters = custom_filter(name, "fn filter(value) { value }");
            let message = render("x", &tera::Context::new(), &filters)
                .unwrap_err()
                .to_string();
            assert!(
                message.starts_with(&format!("Filter `{name}` can't be called from a template")),
                "{message}"
            );
        }
    }
}
