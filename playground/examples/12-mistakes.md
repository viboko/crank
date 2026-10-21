# When something goes wrong

A mistake in one task group or task doesn't stop the others: crank
runs everything it can, reports every problem it found, and exits with
an error. Referencing a value the data doesn't have is an error too,
rather than quietly writing nothing.

Here `greeting.txt.tera` uses `{{ nmae }}` instead of `{{ name }}`,
and `farewell` reads `people.yaml`, which doesn't exist — yet
`thanks.txt` is still written. Try fixing both: correct the typo, and
add `people.yaml` (with a `name`) using the new file button above the
file list.

## `.crank`

```yaml
taskGroups:
  - name: greeting
    object: person.yaml
    tasks:
      - template: greeting.txt.tera
        output: greeting.txt
      - template: thanks.txt.tera
        output: thanks.txt
  - name: farewell
    object: people.yaml
    tasks:
      - template: farewell.txt.tera
        output: farewell.txt
```

## `person.yaml`

```yaml
name: Ada
```

## `greeting.txt.tera`

```text
Hello, {{ nmae }}!
```

## `thanks.txt.tera`

```text
Thank you, {{ name }}.
```

## `farewell.txt.tera`

```text
Goodbye, {{ name }}.
```

## Output

```text
Failed to render ./greeting.txt.tera: error: Variable `nmae` is not defined. Available variables: crank, name
 --> template:1:11
  |
1 | Hello, {{ nmae }}!
  |           ^^^^
Failed to read ./people.yaml: No such file or directory (os error 2)
```

## Generated `thanks.txt`

```text
Thank you, Ada.
```
