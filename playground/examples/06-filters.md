# Filters

A filter transforms a value: `{{ name | upper }}`. Tera comes with a
[large set of them](https://keats.github.io/tera/#built-in-filters)
— `upper`, `title`, `replace`, `truncate`, `join` and many more — and
crank adds two of its own:

- `snake_case` turns `maxRetries` or `max retries` into `max_retries`.
- `wrap(width=N)` word-wraps text, with an optional `indent` before each line.

Filters chain left to right, and some take named parameters.

## `.crank`

```yaml
taskGroups:
  - name: settings
    object: settings.yaml
    tasks:
      - template: settings.py.tera
        output: settings.py
```

## `settings.yaml`

```yaml
description: Settings for the order service. Every value here can be overridden by an environment variable of the same name in upper case.
settings:
  maxRetries: 3
  requestTimeout: 30
  proxyPort: 8080
```

## `settings.py.tera`

```text
{{ description | wrap(width=60, indent="# ") }}
{% for key, value in settings %}
{{ key | snake_case | upper }} = {{ value }}
{%- endfor %}
```

## Output

```text
Generated 1 file
  - settings.py (created)
```

## Generated `settings.py`

```text
# Settings for the order service. Every value here can be
# overridden by an environment variable of the same name in
# upper case.

MAX_RETRIES = 3
REQUEST_TIMEOUT = 30
PROXY_PORT = 8080
```
