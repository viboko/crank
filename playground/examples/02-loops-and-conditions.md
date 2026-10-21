# Loops and conditions

Inside a `{% for %}` loop, `loop.first`, `loop.last` and `loop.index`
say where you are, and `{% if %}` chooses what to write. A loop can also
walk a mapping's keys and values with `{% for key, value in ... %}`.

A `-` just inside a tag, like `{%-` or `-%}`, trims the whitespace on
that side of it — handy for keeping a generated file's lines tidy.

Try marking another dish as vegetarian, or adding an opening day.

## `.crank`

```yaml
taskGroups:
  - name: menu
    object: menu.yaml
    tasks:
      - template: menu.md.tera
        output: menu.md
```

## `menu.yaml`

```yaml
restaurant: Luigi's
dishes:
  - name: Margherita
    price: 9.5
    vegetarian: true
  - name: Diavola
    price: 11
    vegetarian: false
  - name: Funghi
    price: 10.5
    vegetarian: true
hours:
  Monday: closed
  Friday: 12:00–23:00
  Saturday: 12:00–23:00
```

## `menu.md.tera`

```text
# {{ restaurant }}

{% for dish in dishes -%}
{{ loop.index }}. {{ dish.name }} — €{{ dish.price }}{% if dish.vegetarian %} (v){% endif %}
{% endfor %}
Open {% for day, time in hours %}{% if time != "closed" %}{{ day }} {{ time }}{% if not loop.last %}, {% endif %}{% endif %}{% endfor %}.
```

## Output

```text
Generated 1 file
  - menu.md (created)
```

## Generated `menu.md`

```text
# Luigi's

1. Margherita — €9.5 (v)
2. Diavola — €11
3. Funghi — €10.5 (v)

Open Friday 12:00–23:00, Saturday 12:00–23:00.
```
