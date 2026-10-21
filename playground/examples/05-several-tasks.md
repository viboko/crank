# Several tasks, one source of truth

A task group can list any number of tasks, all rendered against the
same data — so one YAML file can keep code in several languages in
step. Here one list of colors becomes both a Rust enum and a TypeScript
union type.

Try adding a color: both files update together.

## `.crank`

```yaml
taskGroups:
  - name: colors
    object: colors.yaml
    tasks:
      - template: templates/colors.rs.tera
        output: generated/colors.rs
      - template: templates/colors.ts.tera
        output: generated/colors.ts
```

## `colors.yaml`

```yaml
type: Color
values:
  - red
  - green
  - blue
```

## `templates/colors.rs.tera`

```text
pub enum {{ type }} {
{%- for value in values %}
    {{ value | capitalize }},
{%- endfor %}
}
```

## `templates/colors.ts.tera`

```text
export type {{ type }} = {% for value in values %}"{{ value }}"{% if not loop.last %} | {% endif %}{% endfor %};
```

## Output

```text
Generated 2 files
  - generated/colors.rs (created)
  - generated/colors.ts (created)
```

## Generated `generated/colors.rs`

```text
pub enum Color {
    Red,
    Green,
    Blue,
}
```

## Generated `generated/colors.ts`

```text
export type Color = "red" | "green" | "blue";
```
