# One file per item

An `array` task group runs its tasks once for each item in a list,
instead of once for the whole file. Each item becomes the template's
data, so one task can fan out into many files.

Because every item runs the same task, its `output` has to contain a
`{{ ... }}` substitution — otherwise every item would write the same
file. The `#pizzas` selector, as in the previous example, picks the
`pizzas` list out of the file — an `array` selector must pick a list.

Try adding a pizza.

## `.crank`

```yaml
taskGroups:
  - name: pizzas
    array: pizzas.yaml#pizzas
    tasks:
      - template: pizza.txt.tera
        output: pizzas/{{ name | lower }}.txt
```

## `pizzas.yaml`

```yaml
pizzas:
  - name: Hawaiian
    price: 17.99
    toppings: [ham, pineapple]
  - name: Diabolo
    price: 19.99
    toppings: [salami, chilli, jalapeños]
```

## `pizza.txt.tera`

```text
{{ name }} — ${{ price }}
With {{ toppings | join(sep=", ") }}.
```

## Output

```text
Generated 2 files
  - pizzas/hawaiian.txt (created)
  - pizzas/diabolo.txt (created)
```

## Generated `pizzas/diabolo.txt`

```text
Diabolo — $19.99
With salami, chilli, jalapeños.
```

## Generated `pizzas/hawaiian.txt`

```text
Hawaiian — $17.99
With ham, pineapple.
```
