# Custom filters

When Tera's filters don't cover it, write your own in
[Rhai](https://rhai.rs), a small scripting language, and declare it
under `filters:`. The script defines `fn filter(value)` — or
`fn filter(value, args)` to take named parameters, like
`pad(width=8)`.

Scripts run sandboxed: no files, no network, and limits on how long
they can run. Try making `pad` use a different fill character.

## `.crank`

```yaml
filters:
  backwards: filters/backwards.rhai
  pad: filters/pad.rhai
taskGroups:
  - name: words
    object: words.yaml
    tasks:
      - template: words.txt.tera
        output: words.txt
```

## `filters/backwards.rhai`

```text
fn filter(value) {
    let result = "";
    for ch in value {
        result = ch.to_string() + result;
    }
    result
}
```

## `filters/pad.rhai`

```text
fn filter(value, args) {
    let width = if args.contains("width") { args.get("width") } else { 10 };
    let text = value.to_string();
    while text.len() < width {
        text += ".";
    }
    text
}
```

## `words.yaml`

```yaml
words:
  - level
  - stressed
  - drawer
```

## `words.txt.tera`

```text
{% for word in words -%}
{{ word | pad(width=10) }} {{ word | backwards }}
{% endfor -%}
```

## Output

```text
Generated 1 file
  - words.txt (created)
```

## Generated `words.txt`

```text
level..... level
stressed.. desserts
drawer.... reward
```
