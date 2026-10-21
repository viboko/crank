# Any language, and emoji

crank isn't limited to English. Letters in any script, accents and
emoji all work in file and folder names, task group names, data (keys
and values), template text, and output paths. Selector keys and pocket
names can use letters and digits in any script too, though not emoji.

Only names that Tera reads as code have to be ASCII: a variable in
`{{ ... }}`, and a filter's name. So `{{ name }}` works but `{{ 名前 }}`
doesn't. To read a key like `日本語`, loop over the mapping that holds
it, as `dish.txt.tera` does with `nazwy`.

Here `dania/寿司.txt` was generated earlier, and someone wrote a note in
its pocket, `notatki-寿司`. That note survives this run — the file already
holds what crank generates, so it shows as `(unchanged)` — while each
new dish gets the default. `snake_case` keeps letters in every script, so
`Crème brûlée` becomes `crème_brûlée`. Try adding a dish, or changing
`{{ name }}` to `{{ 名前 }}` to see the error.

## `.crank`

```yaml
taskGroups:
  - name: メニュー
    array: menu/料理.yaml#料理
    tasks:
      - template: шаблони/dish.txt.tera
        output: "dania/{{ name }}.txt"
```

## `menu/料理.yaml`

```yaml
料理:
  - name: 寿司
    emoji: 🍣
    nazwy: { 日本語: すし, polski: suszi, українська: суші }
  - name: Żurek
    emoji: 🥣
    nazwy: { polski: żurek, français: soupe au levain }
  - name: Crème brûlée
    emoji: 🍮
    nazwy: { français: crème brûlée, 한국어: 크렘 브륄레 }
  - name: 김치
    emoji: 🥬
    nazwy: { 한국어: 김치, English: kimchi }
```

## `шаблони/dish.txt.tera`

```text
{{ emoji }} {{ name }}
id: {{ name | snake_case }}
{% for language, word in nazwy %}
  {{ language }}: {{ word }}
{%- endfor %}

// notatki-{{ name | snake_case }} [
Brak notatek.
// ]
```

## `dania/寿司.txt`

```text
🍣 寿司
id: 寿司

  日本語: すし
  polski: suszi
  українська: суші

// notatki-寿司 [
Najlepsze z łososiem. 🐟
// ]
```

## `.crank.manifest`

```yaml
version: 1
taskGroups:
  メニュー:
  - dania/寿司.txt
```

## Output

```text
Generated 4 files
  - dania/寿司.txt (unchanged)
  - dania/Żurek.txt (created)
  - dania/Crème brûlée.txt (created)
  - dania/김치.txt (created)
```

## Generated `dania/Żurek.txt`

```text
🥣 Żurek
id: żurek

  polski: żurek
  français: soupe au levain

// notatki-żurek [
Brak notatek.
// ]
```

## Generated `dania/Crème brûlée.txt`

```text
🍮 Crème brûlée
id: crème_brûlée

  français: crème brûlée
  한국어: 크렘 브륄레

// notatki-crème_brûlée [
Brak notatek.
// ]
```

## Generated `dania/김치.txt`

```text
🥬 김치
id: 김치

  한국어: 김치
  English: kimchi

// notatki-김치 [
Brak notatek.
// ]
```
