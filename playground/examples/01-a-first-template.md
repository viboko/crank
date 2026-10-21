# A first template

A `.crank` file says which data feeds which template, and where the
result goes. Here one task group loads `object.yaml` and renders
`output.py.tera` against it, writing `output.py`.

Templates use [Tera](https://keats.github.io/tera/)'s Jinja-like syntax:
`{{ name }}` substitutes a value, and `| upper` runs it through a filter.

Try adding a tag to `object.yaml`, or renaming the class: crank
re-runs as you type, and `output.py` changes with it. Every path in
`.crank` is relative to the `.crank` file's own folder.

## `.crank`

```yaml
taskGroups:
  - name: widget
    object: object.yaml
    tasks:
      - template: output.py.tera
        output: output.py
```

## `object.yaml`

```yaml
name: widget
tags:
  - alpha
  - beta
```

## `output.py.tera`

```text
class {{ name | upper }}:
    tags = [
{%- for tag in tags %}
        "{{ tag }}",
{%- endfor %}
    ]
```

## Output

```text
Generated 1 file
  - output.py (created)
```

## Generated `output.py`

```text
class WIDGET:
    tags = [
        "alpha",
        "beta",
    ]
```
