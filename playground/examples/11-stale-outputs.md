# Cleaning up stale outputs

crank records every file it generates in `.crank.manifest` — commit it
alongside your code. On the next successful run, any file the manifest
lists that crank no longer generates is deleted, so renaming an
`output` or removing a task doesn't leave old files behind.

Here crank has generated `schema/users.sql` and recorded it in
`.crank.manifest`. Try changing the task's `output` in `.crank` to
`tables/{{ name }}.sql`: crank writes the new file, deletes
`schema/users.sql` because the manifest says it generated it, and
updates the manifest.

The emptied `schema` folder is left behind: crank only deletes the
files it generated, never folders. That's harmless, since git doesn't
track empty folders anyway.

If anything fails, crank skips this clean-up entirely, so a broken run
never deletes a file it might still want.

## `.crank`

```yaml
taskGroups:
  - name: tables
    array: tables.yaml
    tasks:
      - template: table.sql.tera
        output: schema/{{ name }}.sql
```

## `tables.yaml`

```yaml
- name: users
  columns:
    - id integer primary key
    - email text not null
```

## `table.sql.tera`

```text
create table {{ name }} (
{%- for column in columns %}
  {{ column }}{% if not loop.last %},{% endif %}
{%- endfor %}
);
```

## Output

```text
Generated 1 file
  - schema/users.sql (created)
```

## Generated `schema/users.sql`

```text
create table users (
  id integer primary key,
  email text not null
);
```
