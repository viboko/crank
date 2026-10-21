# Reusing parts with anchors

When several task groups share the same tasks, YAML can write them out
once: `&name` labels a value (an *anchor*), and `*name` (an *alias*)
repeats it anywhere else in the file. Here each section of a menu gets
a web page and a printed card per dish, and the tasks for both are
written just once, then reused by every group.

The anchor needs a key to live under, and crank rejects keys it
doesn't know — so that a typo like `objcet:` is an error rather than
silently ignored. A key starting with `x-` is the exception: crank
skips it, at any level, so it's free for anchors or your own notes.

Try adding a third task under `x-dish-tasks`: every section picks it
up. YAML's `<<:` merge keys, which splice one mapping into another,
aren't supported; an alias reuses a whole value as it is.

## `.crank`

```yaml
x-dish-tasks: &dish-tasks
  - template: templates/page.html.tera
    output: "site/{{ id }}.html"
  - template: templates/card.txt.tera
    output: "print/{{ id }}.txt"

taskGroups:
  - name: starters
    array: menu.yaml#starters
    tasks: *dish-tasks
  - name: mains
    array: menu.yaml#mains
    tasks: *dish-tasks
  - name: desserts
    array: menu.yaml#desserts
    tasks: *dish-tasks
```

## `menu.yaml`

```yaml
starters:
  - id: bruschetta
    name: Bruschetta
    price: 6
mains:
  - id: margherita
    name: Margherita
    price: 11
desserts:
  - id: tiramisu
    name: Tiramisu
    price: 7
```

## `templates/page.html.tera`

```text
<h1>{{ name }}</h1>
<p>€{{ price }}</p>
```

## `templates/card.txt.tera`

```text
{{ name | upper }} .... {{ price }}
```

## Output

```text
Generated 6 files
  - site/bruschetta.html (created)
  - print/bruschetta.txt (created)
  - site/margherita.html (created)
  - print/margherita.txt (created)
  - site/tiramisu.html (created)
  - print/tiramisu.txt (created)
```

## Generated `site/bruschetta.html`

```text
<h1>Bruschetta</h1>
<p>€6</p>
```

## Generated `print/bruschetta.txt`

```text
BRUSCHETTA .... 6
```

## Generated `site/margherita.html`

```text
<h1>Margherita</h1>
<p>€11</p>
```

## Generated `print/margherita.txt`

```text
MARGHERITA .... 11
```

## Generated `site/tiramisu.html`

```text
<h1>Tiramisu</h1>
<p>€7</p>
```

## Generated `print/tiramisu.txt`

```text
TIRAMISU .... 7
```
