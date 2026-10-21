// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compiling and invoking user-defined Tera filters written as sandboxed
//! Rhai scripts. Each filter gets its own isolated [`Engine`]/[`AST`] pair
//! — scripts never see each other's state, so the fixed `filter` function
//! name never collides across filters.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use rhai::{AST, Dynamic, Engine, Scope};

/// The fixed function name every filter script must define.
const FILTER_FN_NAME: &str = "filter";

/// A `.rhai` filter script that failed to compile: invalid syntax, or use
/// of a disabled language feature (`eval`, `import`).
#[derive(Debug)]
pub struct CompileError(rhai::ParseError);

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CompileError {}

/// A filter invocation that failed at runtime: no `filter` function of the
/// right arity, a script-level error (e.g. dividing by zero, exceeding a
/// sandbox limit), or a value that can't convert to/from a Tera value.
#[derive(Debug)]
pub struct CallError(Box<dyn std::error::Error + Send + Sync>);

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CallError {}

impl From<Box<rhai::EvalAltResult>> for CallError {
    fn from(err: Box<rhai::EvalAltResult>) -> Self {
        Self(err)
    }
}

// `tera::Value` has no `Deserialize` impl, so converting a script's return
// value back into one goes via `Value::try_from_serializable` instead of
// `rhai::serde::from_dynamic`, which is a distinct failure mode from the
// rhai-side errors above.
impl From<tera::Error> for CallError {
    fn from(err: tera::Error) -> Self {
        Self(Box::new(err))
    }
}

/// A `.rhai` script compiled into a filter. Cheap to clone (an [`Arc`]
/// bump each for the engine and AST), so it can be captured by value in
/// every render call's filter closure without recompiling.
#[derive(Debug, Clone)]
pub struct CompiledFilter {
    /// The sandboxed engine the script runs in.
    engine: Arc<Engine>,
    /// The compiled script.
    ast: Arc<AST>,
    /// Whether the script defines `filter` at arity 2 (`fn filter(value,
    /// args)`) rather than the plain arity-1 `fn filter(value)`. Rhai
    /// dispatches script functions purely by name+arity (no
    /// parameter-label overloading), so this is decided once at compile
    /// time by inspecting the AST, rather than re-discovered on every call.
    accepts_args: bool,
}

impl CompiledFilter {
    /// Compiles `script_source` (the raw contents of a filter's `.rhai`
    /// file) using a fresh, sandboxed engine.
    ///
    /// # Errors
    /// Returns [`CompileError`] if `script_source` has invalid Rhai syntax,
    /// or uses a disabled language feature (`eval`, `import`).
    pub fn compile(script_source: &str) -> Result<Self, CompileError> {
        let engine = sandboxed_engine();
        let ast = engine.compile(script_source).map_err(CompileError)?;
        let accepts_args = ast
            .iter_functions()
            .any(|f| f.name == FILTER_FN_NAME && f.params.len() == 2);
        Ok(Self {
            engine: Arc::new(engine),
            ast: Arc::new(ast),
            accepts_args,
        })
    }

    /// Calls the script's `filter` function with `value` and, if the
    /// script defines `filter` at arity 2, `args` (Tera's named filter
    /// parameters, converted to a Rhai object map) as well. A plain
    /// arity-1 filter ignores `args` entirely. Converts to and from Rhai's
    /// `Dynamic` via serde.
    ///
    /// # Errors
    /// Returns [`CallError`] if `value`/`args` can't convert into Rhai
    /// values, no matching `filter` function is found (wrong name or
    /// arity), the script errors or exceeds a sandbox limit at runtime
    /// (including an explicit `throw` for a missing required parameter),
    /// or its return value can't convert back into a Tera value.
    pub fn call(
        &self,
        value: &tera::Value,
        args: &HashMap<String, tera::Value>,
    ) -> Result<tera::Value, CallError> {
        let input: Dynamic = rhai::serde::to_dynamic(value).map_err(CallError::from)?;
        let mut scope = Scope::new();
        let output: Dynamic = if self.accepts_args {
            let args: Dynamic = rhai::serde::to_dynamic(args).map_err(CallError::from)?;
            self.engine
                .call_fn(&mut scope, &self.ast, FILTER_FN_NAME, (input, args))
                .map_err(CallError::from)?
        } else {
            self.engine
                .call_fn(&mut scope, &self.ast, FILTER_FN_NAME, (input,))
                .map_err(CallError::from)?
        };
        tera::Value::try_from_serializable(&output).map_err(CallError::from)
    }
}

/// Builds a fresh [`Engine`] with crank's fixed sandboxing limits. No
/// user-configurable knobs, per crank's convention against speculative
/// config: loosening a limit is a deliberate follow-up change, not a flag.
fn sandboxed_engine() -> Engine {
    let mut engine = Engine::new();
    engine
        .disable_symbol("eval")
        // `import` defaults to reading arbitrary `.rhai` files off disk by
        // path via Rhai's `FileModuleResolver` — disabling the keyword
        // itself (caught at compile time, like `eval`) closes that hole
        // without needing a custom resolver.
        .disable_symbol("import")
        .set_max_operations(500_000)
        .set_max_call_levels(64)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(1_000_000)
        .set_max_array_size(100_000)
        .set_max_map_size(100_000);
    // Scripts have no legitimate reason to print; keep crank's own console
    // output clean rather than letting a stray print()/debug() interleave.
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});
    // Rhai has no Unicode character classes of its own, so a script
    // couldn't otherwise tell a letter in another script (`ż`, `日`) from
    // punctuation or an emoji. Pure functions of a single `char`, so they
    // don't widen the sandbox.
    engine
        .register_fn("is_alphanumeric", char::is_alphanumeric)
        .register_fn("is_uppercase", char::is_uppercase)
        .register_fn("is_mark", crate::unicode::is_mark);
    engine
}

/// The filters crank ships out of the box, compiled from bundled `.rhai`
/// sources embedded at build time via `include_str!`. This doesn't violate
/// the "no filesystem access" rule: embedding happens at compile time, not
/// as a runtime read.
///
/// These also double as working reference examples of the two filter
/// conventions (`fn filter(value)` and `fn filter(value, args)`). Tera's
/// own built-in filters (<https://keats.github.io/tera/#filters>) cover
/// most common cases and should be preferred over a custom filter
/// wherever they already do the job — `render()` (`src/render.rs`)
/// rejects a name that collides with one of Tera's own, so duplicating
/// the list is caught at render time rather than silently shadowed.
///
/// # Panics
/// Panics if a bundled built-in script fails to compile — that would be a
/// bug in crank itself, not a user error.
#[must_use]
pub fn built_in_filters() -> HashMap<String, CompiledFilter> {
    let mut filters = HashMap::new();
    filters.insert(
        "snake_case".to_string(),
        CompiledFilter::compile(include_str!("filters/snake_case.rhai"))
            .expect("bundled snake_case filter script must compile"),
    );
    filters.insert(
        "wrap".to_string(),
        CompiledFilter::compile(include_str!("filters/wrap.rhai"))
            .expect("bundled wrap filter script must compile"),
    );
    filters
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(s: &str) -> tera::Value {
        tera::Value::from(s)
    }

    fn no_args() -> HashMap<String, tera::Value> {
        HashMap::new()
    }

    #[test]
    fn compiles_a_valid_filter_script() {
        assert!(CompiledFilter::compile("fn filter(value) { value }").is_ok());
    }

    #[test]
    fn invalid_rhai_syntax_is_a_compile_error() {
        assert!(CompiledFilter::compile("fn filter(value) {").is_err());
    }

    #[test]
    fn a_script_using_eval_is_a_compile_error() {
        let source = r#"fn filter(value) { eval("1 + 1") }"#;
        assert!(CompiledFilter::compile(source).is_err());
    }

    #[test]
    fn a_script_using_import_is_a_compile_error() {
        let source = r#"import "foo" as foo; fn filter(value) { value }"#;
        assert!(CompiledFilter::compile(source).is_err());
    }

    #[test]
    fn calls_a_simple_filter_and_converts_the_value() {
        let filter = CompiledFilter::compile("fn filter(value) { value.to_upper() }").unwrap();
        assert_eq!(
            filter.call(&string("hi"), &no_args()).unwrap(),
            string("HI")
        );
    }

    #[test]
    fn calling_a_filter_that_doesnt_exist_is_a_call_error() {
        let filter = CompiledFilter::compile("fn something_else(value) { value }").unwrap();
        assert!(filter.call(&string("hi"), &no_args()).is_err());
    }

    #[test]
    fn an_operation_limit_exceeded_script_is_a_call_error() {
        let filter =
            CompiledFilter::compile("fn filter(value) { let x = 0; loop { x += 1; } value }")
                .unwrap();
        assert!(filter.call(&string("hi"), &no_args()).is_err());
    }

    #[test]
    fn a_plain_filter_ignores_args_it_is_called_with() {
        let filter = CompiledFilter::compile("fn filter(value) { value.to_upper() }").unwrap();
        let mut args = HashMap::new();
        args.insert("unused".to_string(), string("whatever"));
        assert_eq!(filter.call(&string("hi"), &args).unwrap(), string("HI"));
    }

    #[test]
    fn a_filter_accepting_args_receives_their_contents() {
        let filter =
            CompiledFilter::compile(r#"fn filter(value, args) { value + args.get("suffix") }"#)
                .unwrap();
        let mut args = HashMap::new();
        args.insert("suffix".to_string(), string("!"));
        assert_eq!(filter.call(&string("hi"), &args).unwrap(), string("hi!"));
    }

    #[test]
    fn built_in_filters_includes_snake_case_and_it_works() {
        let filters = built_in_filters();
        let filter = filters
            .get("snake_case")
            .expect("snake_case should be built in");
        // Exercises both a camelCase boundary and separator collapsing.
        assert_eq!(
            filter.call(&string("helloWorld"), &no_args()).unwrap(),
            string("hello_world")
        );
    }

    #[test]
    fn built_in_filters_includes_wrap_and_it_works() {
        let filters = built_in_filters();
        let filter = filters.get("wrap").expect("wrap should be built in");
        let mut args = HashMap::new();
        args.insert("width".to_string(), tera::Value::from(10));
        assert_eq!(
            filter
                .call(&string("the quick brown fox jumps"), &args)
                .unwrap(),
            string("the quick\nbrown fox\njumps")
        );
    }

    #[test]
    fn wrap_without_a_width_is_a_clear_call_error() {
        let filters = built_in_filters();
        let filter = filters.get("wrap").expect("wrap should be built in");
        let err = filter
            .call(&string("some text"), &no_args())
            .expect_err("wrap should require width");
        assert!(
            err.to_string().contains("width"),
            "expected the error to name `width`, got: {err}"
        );
    }

    #[test]
    fn scripts_can_classify_characters_in_any_script() {
        let filter = CompiledFilter::compile(
            "fn filter(value) { let out = []; for c in value { out.push([c.is_alphanumeric(), c.is_uppercase(), c.is_mark()]); } out }",
        )
        .unwrap();
        let output = filter.call(&string("Ż日🍕\u{301}"), &no_args()).unwrap();
        assert_eq!(
            output,
            tera::Value::try_from_serializable(&[
                [true, true, false],
                [true, false, false],
                [false, false, false],
                [false, false, true],
            ])
            .unwrap()
        );
    }

    #[test]
    fn non_ascii_values_round_trip_through_a_script() {
        let filter = CompiledFilter::compile("fn filter(value) { value }").unwrap();
        for sample in ["日本語", "Zażółć", "👩‍👩‍👧‍👦", "e\u{301}", "שלום"]
        {
            assert_eq!(
                filter.call(&string(sample), &no_args()).unwrap(),
                string(sample)
            );
        }
    }

    #[test]
    fn a_compile_error_shows_rhais_message() {
        let err = CompiledFilter::compile("fn filter(value) {").unwrap_err();
        assert_eq!(err.to_string(), err.0.to_string());
        assert!(err.to_string().contains("Expecting '}'"), "{err}");
    }
}
