# Picking part of a file

A `#selector` after a data file's name picks part of it: a key, a key
inside that (`settings/database`), or an item of a list (`servers[0]`).
Whatever an `object` selector picks must be a mapping, since its keys
become the template's variables.

Here two task groups read different parts of the same `config.yaml`.
Try changing `servers[0]` to `servers[1]`, or selecting a key that
doesn't exist.

## `.crank`

```yaml
taskGroups:
  - name: database
    object: config.yaml#settings/database
    tasks:
      - template: database.env.tera
        output: database.env
  - name: primary
    object: config.yaml#servers[0]
    tasks:
      - template: server.txt.tera
        output: primary.txt
```

## `config.yaml`

```yaml
settings:
  database:
    host: db.internal
    port: 5432
    name: shop
servers:
  - host: web-1.internal
    region: eu-west
  - host: web-2.internal
    region: us-east
```

## `database.env.tera`

```text
DATABASE_URL=postgres://{{ host }}:{{ port }}/{{ name }}
```

## `server.txt.tera`

```text
{{ host }} ({{ region }})
```

## Output

```text
Generated 2 files
  - database.env (created)
  - primary.txt (created)
```

## Generated `database.env`

```text
DATABASE_URL=postgres://db.internal:5432/shop
```

## Generated `primary.txt`

```text
web-1.internal (eu-west)
```
