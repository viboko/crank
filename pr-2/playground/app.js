// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// crank playground: a pretend project on the left, the open file in the
// middle, and crank's output below — re-run after every change by crank
// itself, compiled to WebAssembly (see playground/src/lib.rs). What each
// run writes and deletes is applied to the tree, just as crank changes a
// real directory, and the files it generates are marked.

import init, { examples as examplesJson, pocketMarkers, run } from "./pkg/crank_playground.js";

const CRANK = ".crank";
const MANIFEST = ".crank.manifest";
const STARTER_CRANK = `taskGroups:
  - name: example
    object: data.yaml
    tasks:
      - template: template.txt.tera
        output: output.txt
`;

// ---------------------------------------------------------------- icons

const svg = (body, viewBox = "0 0 16 16") =>
  `<svg viewBox="${viewBox}" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

// The logo, as the page header draws it.
const LOGO_PATH = document.querySelector(".logo path").getAttribute("d");
const logo = (extra = "") =>
  `<svg viewBox="-0.25 -0.25 9.47 9.47" aria-hidden="true"><path d="${LOGO_PATH}" fill="currentColor" fill-rule="evenodd"/>${extra}</svg>`;

const ICONS = {
  chevron: svg('<path d="M3 1.5 7.5 6 3 10.5"/>', "0 0 10 12"),
  folder: svg(
    '<path d="M1.5 4.5a1 1 0 0 1 1-1h3.6l1.4 1.5h5.9a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1H2.5a1 1 0 0 1-1-1z"/>',
  ),
  file: svg(
    '<path d="M4 1.5h5L12.5 5v9a.5.5 0 0 1-.5.5H4a.5.5 0 0 1-.5-.5V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5V5h3.5"/>',
  ),
  generated: svg(
    '<path d="M4 1.5h5L12.5 5v9a.5.5 0 0 1-.5.5H4a.5.5 0 0 1-.5-.5V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5V5h3.5"/><path d="M8 7.5v4M6 9.5h4" />',
  ),
  crank: logo(),
  newFile: svg(
    '<path d="M4 1.5h5L12.5 5v4"/><path d="M3.5 2v12a.5.5 0 0 0 .5.5h5"/><path d="M9 1.5V5h3.5"/><path d="M12.5 11v4M10.5 13h4"/>',
  ),
  newFolder: svg(
    '<path d="M8 13.5H2.5a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1h3.6l1.4 1.5h5.9a1 1 0 0 1 1 1V8"/><path d="M12.5 10v5M10 12.5h5"/>',
  ),
  newCrank: svg(
    `<g transform="translate(0.5 0.5) scale(1.05)"><path d="${LOGO_PATH}" fill="currentColor" stroke="none" fill-rule="evenodd"/></g><path d="M12.5 10v5M10 12.5h5"/>`,
  ),
  rename: svg('<path d="M10.5 2.5 13.5 5.5 6 13H3v-3z"/>'),
  trash: svg('<path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4"/>'),
};

// ---------------------------------------------------------------- state

const state = {
  /** The project: path -> { directory: boolean, contents: string } */
  files: new Map(),
  /** Paths of the files crank has generated, as far as the page has seen. */
  generated: new Set(),
  /** Path of the file open in the editor, or null. */
  open: null,
  /** Path of the selected tree row: where "new file" goes. */
  selected: null,
  collapsed: new Set(),
  /** The example loaded, or null for a blank or shared tree. */
  example: null,
  /** The tree as loaded, so "Copy link" can link to an unedited example by name. */
  pristine: null,
  /** crank's outcome for the last run (see Outcome in lib.rs). */
  outcome: null,
  /** Whether crank re-runs after every edit, or waits for "Run crank". */
  autorun: true,
  /** Whether the files have changed since crank last ran (only without autorun). */
  stale: false,
  /** An inline name input in the tree: { mode: "new", dir, directory } or { mode: "rename", path }. */
  editing: null,
};

let examples = [];

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...children) => {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children.filter((child) => child !== null && child !== undefined));
  return node;
};

// ---------------------------------------------------------------- paths

const parentOf = (path) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
const nameOf = (path) => path.slice(path.lastIndexOf("/") + 1);
const join = (dir, name) => (dir ? `${dir}/${name}` : name);
const ancestorsOf = (path) => {
  const result = [];
  for (let dir = parentOf(path); dir; dir = parentOf(dir)) result.push(dir);
  return result;
};

const isFile = (path) => state.files.has(path) && !state.files.get(path).directory;
const isDirectory = (path) => path === "" || state.files.get(path)?.directory === true;

/** Why `name` can't be a file or folder name, or null if it can. */
function invalidName(name) {
  if (name === "") return "A name can't be empty.";
  if (name === "." || name === "..") return `"${name}" can't be used as a name.`;
  return null;
}

/**
 * Entries directly inside `dir`: hidden files (so `.crank` and
 * `.crank.manifest` sit together at the top), hidden folders, folders,
 * then files — each group by name.
 */
function childrenOf(dir) {
  const children = [...state.files.keys()].filter((path) => parentOf(path) === dir);
  const rank = (path) => {
    const hidden = nameOf(path).startsWith(".");
    if (isDirectory(path)) return hidden ? 1 : 2;
    return hidden ? 0 : 3;
  };
  return children.sort((a, b) => rank(a) - rank(b) || (nameOf(a) < nameOf(b) ? -1 : 1));
}

function addEntry(path, directory, contents = "") {
  for (const dir of ancestorsOf(path).reverse()) {
    if (!state.files.has(dir)) state.files.set(dir, { directory: true, contents: "" });
  }
  state.files.set(path, { directory, contents });
}

function removeEntry(path) {
  for (const other of [...state.files.keys()]) {
    if (other === path || other.startsWith(`${path}/`)) state.files.delete(other);
  }
}

function moveEntry(from, to) {
  const moved = new Map();
  for (const [path, entry] of state.files) {
    if (path === from || path.startsWith(`${from}/`)) {
      moved.set(to + path.slice(from.length), entry);
      state.files.delete(path);
    }
  }
  for (const [path, entry] of moved) state.files.set(path, entry);
  const rebase = (path) =>
    path === from || path?.startsWith(`${from}/`) ? to + path.slice(from.length) : path;
  state.open = rebase(state.open);
  state.selected = rebase(state.selected);
  state.collapsed = new Set([...state.collapsed].map(rebase));
}

const toEntry = ([path, { directory, contents }]) =>
  directory ? { path, directory } : { path, contents };
const treeJson = () => JSON.stringify([...state.files].map(toEntry));

// ---------------------------------------------------------------- running

/** Runs crank over the tree, and applies what it wrote and deleted. */
function rerun() {
  const result = JSON.parse(run(treeJson()));
  // The page only ever sends valid JSON, so an error here is a bug.
  if (result.error) {
    console.error(result.error);
    return;
  }
  for (const { path, contents } of result.written) {
    if (isFile(path)) state.files.get(path).contents = contents;
    else addEntry(path, false, contents);
  }
  // Including the files crank left alone because they already held
  // what it generates.
  for (const path of result.generated) state.generated.add(path);
  for (const path of result.removed) state.files.delete(path);
  for (const path of state.generated) if (!isFile(path)) state.generated.delete(path);
  state.outcome = result;
  state.stale = false;
  ensureOpen();
  render();
}

/** After any edit: re-runs crank, or, without autorun, marks its last run out of date. */
function edited() {
  if (state.autorun) {
    rerun();
    return;
  }
  state.stale = true;
  ensureOpen();
  render();
}

/**
 * Whether to mark `path` as generated. Not while crank's last run is out
 * of date: what it would generate now isn't known until it runs again.
 */
const markedGenerated = (path) => !state.stale && state.generated.has(path);

/**
 * The files crank needs to recreate the tree, for a download or a link:
 * none of the ones it generated that it would generate the same from the
 * rest, and no folders that leaves empty. What it generates differently
 * — a file with its own contents in a pocket — stays, and so does
 * .crank.manifest with it, since that's how crank finds the old pockets.
 */
function sourceTree() {
  const generated = (path) => state.generated.has(path) || path === MANIFEST;
  const sources = [...state.files].filter(([path]) => !generated(path)).map(toEntry);
  // A run that fails writes less, so every file it misses is kept.
  const written = JSON.parse(run(JSON.stringify(sources))).written ?? [];
  const regenerated = new Map(written.map((file) => [file.path, file.contents]));
  const kept = [...state.generated].filter(
    (path) =>
      isFile(path) && path !== MANIFEST && regenerated.get(path) !== state.files.get(path).contents,
  );
  if (kept.length > 0 && isFile(MANIFEST)) kept.push(MANIFEST);
  const dropped = (path) => generated(path) && !kept.includes(path);
  const inside = (dir) => [...state.files.keys()].filter((path) => path.startsWith(`${dir}/`));
  return [...state.files]
    .filter(([path, { directory }]) => {
      if (!directory) return !dropped(path);
      const files = inside(path).filter(isFile);
      return !files.some(dropped) || files.some((file) => !dropped(file));
    })
    .sort(([a], [b]) => (a < b ? -1 : 1))
    .map(toEntry);
}

/** The paths `text` mentions — as `./path`, crank's way, or bare. */
function pathsIn(text) {
  const found = [];
  for (const word of text.split(/[\s"`(),]+/)) {
    const path = word.replace(/:$/, "").replace(/^\.\//, "");
    if (path && isFile(path) && !found.includes(path)) found.push(path);
  }
  return found;
}

/**
 * crank's errors, located: which of your files each one is about, the
 * line in it where that's known, and files it needs but can't find.
 */
function indexProblems() {
  const byPath = new Map();
  const lineNotes = new Map();
  const missing = new Map();
  const troubledDirs = new Set();
  const add = (map, key, value) => map.set(key, [...(map.get(key) ?? []), value]);
  const note = (path, line, message) => {
    if (!lineNotes.has(path)) lineNotes.set(path, new Map());
    add(lineNotes.get(path), line, message);
  };

  // An error is a line naming a file, perhaps followed by Tera's
  // `--> template:LINE:COLUMN` pointing into it.
  // Nothing to locate while the last run is out of date.
  let current = null;
  for (const line of (!state.stale && state.outcome?.lines) || []) {
    if (line.stream !== "stderr") continue;
    const text = line.text;
    const at = text.match(/^\s*--> template:(\d+):\d+/);
    if (at && current?.path) {
      note(current.path, Number(at[1]), current.message);
      current = null;
      continue;
    }
    if (/^\s*(\||\d+ \||$)/.test(text)) continue; // the rest of Tera's snippet
    const unreadable = text.match(
      /Failed to read (?:filter script )?\.\/([^\s:]+): No such file or directory/,
    );
    if (unreadable && !state.files.has(unreadable[1])) {
      add(missing, unreadable[1], text);
      for (const dir of ancestorsOf(unreadable[1])) troubledDirs.add(dir);
    }
    const path = pathsIn(text)[0] ?? null;
    const message = text.replace(/^.*?: (error: )?/, "");
    current = { path, message };
    if (!path) continue;
    add(byPath, path, text);
    for (const dir of ancestorsOf(path)) troubledDirs.add(dir);
    const lineNumber = text.match(/\bline (\d+)/);
    if (lineNumber) note(path, Number(lineNumber[1]), message);
  }
  return { byPath, lineNotes, missing, troubledDirs };
}

// ---------------------------------------------------------------- rendering

function render() {
  renderTree();
  renderEditor();
  renderOutput();
  renderExampleState();
}

function renderTree() {
  const focused = document.activeElement?.closest?.(".row")?.dataset.key;
  $("new-crank").hidden = state.files.has(CRANK);
  $("files-foot").hidden = emptyFolders().length === 0;
  const index = indexProblems();
  const rows = rowsFor("", 0, index);
  const tree = $("tree");
  if (rows.length === 0) {
    tree.replaceChildren(
      el("p", {
        className: "tree-empty",
        textContent: "No files. Add some with the buttons above.",
      }),
    );
  } else {
    tree.replaceChildren(...rows);
  }
  if (focused) tree.querySelector(`[data-key="${CSS.escape(focused)}"]`)?.focus();
  const input = tree.querySelector("input");
  if (input && document.activeElement !== input) {
    input.focus();
    input.select();
  }
}

function rowsFor(dir, depth, index) {
  const rows = [];
  if (state.editing?.mode === "new" && state.editing.dir === dir) {
    rows.push(newEntryRow(depth));
  }
  for (const path of childrenOf(dir)) {
    rows.push(entryRow(path, depth, index));
    if (isDirectory(path) && !state.collapsed.has(path))
      rows.push(...rowsFor(path, depth + 1, index));
  }
  for (const [path, messages] of index.missing) {
    if (placeGhost(path).dir === dir) rows.push(ghostRow(path, messages, depth));
  }
  return rows;
}

/** Where to show a missing file: in the deepest folder of its path that exists. */
function placeGhost(path) {
  const parts = path.split("/");
  let depth = parts.length - 1;
  while (depth > 0 && !isDirectory(parts.slice(0, depth).join("/"))) depth -= 1;
  return { dir: parts.slice(0, depth).join("/"), label: parts.slice(depth).join("/") };
}

function rowBase(key, depth, classes) {
  const row = el("div", { className: `row ${classes}`, tabIndex: 0 });
  row.setAttribute("role", "treeitem");
  row.dataset.key = key;
  row.style.setProperty("--depth", depth);
  return row;
}

function entryRow(path, depth, index) {
  const directory = isDirectory(path);
  const generated = markedGenerated(path);
  const problems = index.byPath.get(path) ?? [];
  const classes = [
    directory ? "directory" : "file",
    path === CRANK && "crank",
    generated && "generated",
    path === state.open && "open",
    path === state.selected && "selected",
    problems.length > 0 && "problem",
  ].filter(Boolean);
  const row = rowBase(path, depth, classes.join(" "));
  row.dataset.path = path;
  if (directory) row.setAttribute("aria-expanded", String(!state.collapsed.has(path)));

  const twisty = el("span", { className: "twisty" });
  if (directory) twisty.innerHTML = ICONS.chevron;
  const icon = el("span", { className: "icon" });
  icon.innerHTML = directory
    ? ICONS.folder
    : path === CRANK
      ? ICONS.crank
      : generated
        ? ICONS.generated
        : ICONS.file;

  let name;
  if (state.editing?.mode === "rename" && state.editing.path === path) {
    name = nameInput(nameOf(path));
  } else {
    name = el("span", { className: "name", textContent: nameOf(path) });
  }
  row.append(twisty, icon, name);

  if (problems.length > 0) {
    row.append(el("span", { className: "badge", textContent: "error" }));
    row.title = problems.join("\n");
  } else if (generated) {
    row.append(el("span", { className: "badge generated", textContent: "generated" }));
    row.title =
      "crank generated this file. Edit it and crank re-runs, putting it back — except inside a pocket.";
  }
  if (directory && state.collapsed.has(path) && index.troubledDirs.has(path)) {
    row.append(el("span", { className: "dot", title: "Something inside needs attention" }));
  }

  const actions = el("span", { className: "actions" });
  if (directory) {
    actions.append(
      actionButton(ICONS.newFile, "New file here", () => startNew(path, false)),
      actionButton(ICONS.newFolder, "New folder here", () => startNew(path, true)),
    );
  }
  actions.append(
    actionButton(ICONS.rename, "Rename", () => startRename(path)),
    actionButton(ICONS.trash, "Delete", () => deleteEntry(path)),
  );
  row.append(actions);
  return row;
}

const MISSING_HINT = "crank can't find this file. Create it with the New file button.";

function ghostRow(path, messages, depth) {
  const row = rowBase(`ghost:${path}`, depth, "ghost file");
  const icon = el("span", { className: "icon" });
  icon.innerHTML = ICONS.file;
  row.append(
    el("span", { className: "twisty" }),
    icon,
    el("span", { className: "name", textContent: placeGhost(path).label }),
    el("span", { className: "badge", textContent: "missing" }),
  );
  row.title = `${messages.join("\n")}\n${MISSING_HINT}`;
  row.dataset.ghost = path;
  return row;
}

function newEntryRow(depth) {
  const row = rowBase("new", depth, state.editing.directory ? "directory" : "file");
  const icon = el("span", { className: "icon" });
  icon.innerHTML = state.editing.directory ? ICONS.folder : ICONS.file;
  row.append(el("span", { className: "twisty" }), icon, nameInput(""));
  return row;
}

function actionButton(icon, label, onClick) {
  const button = el("button", { type: "button", title: label, tabIndex: -1 });
  button.setAttribute("aria-label", label);
  button.innerHTML = icon;
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    onClick();
  });
  return button;
}

function nameInput(value) {
  const input = el("input", { value, spellcheck: false });
  input.setAttribute("aria-label", state.editing.mode === "new" ? "New name" : "Rename to");
  let done = false;
  // Returns whether the input is finished with; `commit` that fails
  // validation leaves it open, explaining why.
  const finish = (commit) => {
    if (done) return true;
    done = true; // before committing, which re-renders and so blurs this
    if (commit && !commitName(input)) {
      done = false;
      return false;
    }
    state.editing = null;
    renderTree();
    return true;
  };
  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") finish(true);
    if (event.key === "Escape") finish(false);
  });
  input.addEventListener("input", () => input.setCustomValidity(""));
  input.addEventListener("blur", () => {
    // Clicking away keeps a valid name and drops anything else.
    if (!done && !finish(input.value.trim() !== "")) finish(false);
  });
  input.addEventListener("click", (event) => event.stopPropagation());
  return input;
}

/** Applies the inline input's name, or explains why it can't. */
function commitName(input) {
  const value = input.value.trim();
  const reject = (message) => {
    input.setCustomValidity(message);
    input.reportValidity();
    return false;
  };

  if (state.editing.mode === "rename") {
    const { path } = state.editing;
    if (value === nameOf(path)) return true;
    if (value.includes("/")) return reject("A name can't contain /.");
    const invalid = invalidName(value);
    if (invalid) return reject(invalid);
    const target = join(parentOf(path), value);
    if (state.files.has(target)) return reject(`${value} already exists here.`);
    moveEntry(path, target);
  } else {
    // A new name may be a path, to make folders on the way.
    const parts = value.split("/");
    const invalid = parts.map(invalidName).find(Boolean);
    if (invalid) return reject(invalid);
    const target = join(state.editing.dir, value);
    if (state.files.has(target)) return reject(`${value} already exists here.`);
    for (const dir of ancestorsOf(target)) {
      if (state.files.has(dir) && !state.files.get(dir).directory) {
        return reject(`${dir} is a file, so it can't contain anything.`);
      }
    }
    addEntry(target, state.editing.directory);
    for (const dir of ancestorsOf(target)) state.collapsed.delete(dir);
    state.selected = target;
    if (!state.editing.directory) openFile(target);
  }
  state.editing = null;
  edited();
  return true;
}

// ---------------------------------------------------------------- tree actions

function startNew(dir, directory) {
  if (dir) state.collapsed.delete(dir);
  state.editing = { mode: "new", dir, directory };
  renderTree();
}

function startRename(path) {
  state.editing = { mode: "rename", path };
  renderTree();
}

/** Adds an empty file at `path` (if there isn't one) and opens it. */
function createFile(path, contents = "") {
  if (!state.files.has(path)) addEntry(path, false, contents);
  for (const dir of ancestorsOf(path)) state.collapsed.delete(dir);
  state.selected = path;
  openFile(path);
  edited();
  $("source").focus();
}

/** Folders with no files anywhere inside them, however deeply nested. */
function emptyFolders() {
  const paths = [...state.files.keys()];
  return paths.filter(
    (dir) => isDirectory(dir) && !paths.some((path) => path.startsWith(`${dir}/`) && isFile(path)),
  );
}

function removeEmptyFolders() {
  for (const dir of emptyFolders()) removeEntry(dir);
  if (state.selected && !state.files.has(state.selected)) state.selected = null;
  if (state.editing?.mode === "new" && !isDirectory(state.editing.dir)) state.editing = null;
  edited();
}

function deleteEntry(path) {
  const contents = [...state.files.keys()].filter((other) => other.startsWith(`${path}/`));
  if (
    contents.length > 0 &&
    !confirm(`Delete ${path} and the ${contents.length} item(s) inside it?`)
  ) {
    return;
  }
  removeEntry(path);
  if (state.selected && !state.files.has(state.selected)) state.selected = null;
  edited();
}

/** Keeps a file open whenever there is one, preferring .crank. */
function ensureOpen() {
  if (state.open && isFile(state.open)) return;
  const files = [...state.files.keys()].filter(isFile).sort();
  state.open = isFile(CRANK) ? CRANK : (files[0] ?? null);
}

function reveal(path) {
  for (const dir of ancestorsOf(path)) state.collapsed.delete(dir);
  renderTree();
  const row = $("tree").querySelector(`[data-key="${CSS.escape(path)}"]`);
  if (!row) return;
  row.scrollIntoView({ block: "nearest" });
  row.classList.remove("flash");
  void row.offsetWidth; // restart the animation
  row.classList.add("flash");
}

/** Opens `path`, at `line` if given, and shows it in the tree. */
function show(path, line = null) {
  state.selected = path;
  openFile(path, line);
  reveal(path);
}

function activateRow(row) {
  // A missing file can't be opened: it's for you to create.
  if (row.dataset.ghost) return;
  const path = row.dataset.path;
  if (!path) return;
  state.selected = path;
  if (isDirectory(path)) {
    if (state.collapsed.has(path)) state.collapsed.delete(path);
    else state.collapsed.add(path);
    renderTree();
    return;
  }
  openFile(path);
  renderTree();
}

$("tree").addEventListener("click", (event) => {
  const row = event.target.closest(".row");
  if (row && !row.querySelector("input")) activateRow(row);
});

$("tree").addEventListener("keydown", (event) => {
  const row = event.target.closest(".row");
  if (!row || event.target.tagName === "INPUT") return;
  const rows = [...$("tree").querySelectorAll(".row")];
  const at = rows.indexOf(row);
  const path = row.dataset.path;
  const yours = Boolean(path);
  switch (event.key) {
    case "Enter":
    case " ":
      activateRow(row);
      break;
    case "ArrowDown":
      rows[at + 1]?.focus();
      break;
    case "ArrowUp":
      rows[at - 1]?.focus();
      break;
    case "ArrowRight":
      if (path && isDirectory(path) && state.collapsed.has(path)) activateRow(row);
      break;
    case "ArrowLeft":
      if (path && isDirectory(path) && !state.collapsed.has(path)) activateRow(row);
      break;
    case "F2":
      if (yours) startRename(path);
      break;
    case "Delete":
    case "Backspace":
      if (yours) deleteEntry(path);
      break;
    default:
      return;
  }
  event.preventDefault();
});

$("new-file").innerHTML = ICONS.newFile;
$("new-folder").innerHTML = ICONS.newFolder;
$("new-crank").innerHTML = ICONS.newCrank;
// The header's buttons always add at the root; each folder's row has its own.
$("new-file").addEventListener("click", () => startNew("", false));
$("new-folder").addEventListener("click", () => startNew("", true));
$("new-crank").addEventListener("click", () => createFile(CRANK, STARTER_CRANK));
$("add-root-crank").addEventListener("click", () => createFile(CRANK, STARTER_CRANK));
$("remove-empty").addEventListener("click", removeEmptyFolders);

// ---------------------------------------------------------------- editor

const source = $("source");
const highlight = $("highlight");
const gutter = $("gutter");
const editorScroll = $("editor-scroll");

function openFile(path, line = null) {
  state.open = path;
  renderEditor();
  if (path && line) {
    const lines = source.value.split("\n");
    const offset = lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0);
    source.focus({ preventScroll: true });
    source.setSelectionRange(offset, offset + (lines[line - 1] ?? "").length);
    keepCaretVisible();
  }
}

function renderEditor() {
  const path = state.open;
  $("editor").hidden = !path;
  $("editor-empty").hidden = Boolean(path);
  $("editor-path").textContent = path ?? CRANK;
  // The same lozenge as the file's row in the tree: an error outranks "generated".
  const problems = (path && indexProblems().byPath.get(path)) ?? [];
  const errorNote = $("error-note");
  errorNote.hidden = problems.length === 0;
  errorNote.title = problems.join("\n");
  $("generated-note").hidden = problems.length > 0 || !markedGenerated(path);
  if (!path) return;
  const generated = markedGenerated(path);
  const icon = $("editor-icon");
  icon.className = `file-icon-inline ${path === CRANK ? "crank" : generated ? "generated" : ""}`;
  icon.innerHTML = path === CRANK ? ICONS.crank : generated ? ICONS.generated : ICONS.file;
  source.setAttribute("aria-label", `${path} contents`);
  const text = state.files.get(path).contents;
  if (source.value !== text) {
    // crank may have just put back what was typed into a generated file:
    // keep the caret where it was, as near as the new text allows.
    const { selectionStart, selectionEnd } = source;
    source.value = text;
    if (document.activeElement === source) {
      source.setSelectionRange(
        Math.min(selectionStart, text.length),
        Math.min(selectionEnd, text.length),
      );
    }
  }
  renderHighlight();
}

function renderHighlight() {
  const notes = indexProblems().lineNotes.get(state.open) ?? new Map();
  const lines = tokensToLines(tokenize(state.open, source.value));
  highlight.replaceChildren(
    ...lines.map((tokens, i) => {
      const lineNotes = notes.get(i + 1) ?? [];
      const line = el("span", { className: `line ${lineNotes.length ? "error" : ""}` });
      line.innerHTML = tokens
        .map(([kind, text]) => (kind ? token(kind, text) : escapeHtml(text)))
        .join("");
      if (lineNotes.length) {
        line.append(el("span", { className: "note", textContent: lineNotes.join(" · ") }));
      }
      return line;
    }),
  );
  gutter.replaceChildren(
    ...lines.map((_, i) => {
      const lineNotes = notes.get(i + 1) ?? [];
      return el("div", {
        className: lineNotes.length ? "error" : "",
        textContent: String(i + 1),
        title: lineNotes.join("\n"),
      });
    }),
  );
}

const escapeHtml = (text) =>
  text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const token = (kind, text) => `<span class="tok-${kind}">${escapeHtml(text)}</span>`;

/** Splits `[kind, text]` tokens at newlines into one array of tokens per line. */
function tokensToLines(tokens) {
  const lines = [[]];
  for (const [kind, text] of tokens) {
    text.split("\n").forEach((part, i) => {
      if (i > 0) lines.push([]);
      if (part) lines.at(-1).push([kind, part]);
    });
  }
  return lines;
}

/**
 * `text` as `[kind, text]` tokens, by what `path` looks like — for show
 * only; crank does the real parsing. A null kind is plain text.
 */
function tokenize(path, text) {
  const name = nameOf(path);
  if (name === CRANK || name === MANIFEST || /\.ya?ml$/.test(name)) return tokenizeYaml(text);
  if (name.endsWith(".rhai")) return tokenizeRhai(text);
  const markers = markersFor(path, text);
  if (name.endsWith(".tera")) return tokenizeTera(text, markerRegex(markers, TERA_NAME));
  return tokenizePockets(text, markerRegex(markers, NAME));
}

/**
 * The pocket markers `path` (holding `text`) uses, as `[{start, end}]`
 * patterns: crank's own answer — a template's header, what
 * .crank.manifest records for any other file, or else .crank's own
 * `pocketMarkers` (see playground/src/lib.rs).
 */
function markersFor(path, text) {
  const contents = (file) => (isFile(file) ? state.files.get(file).contents : undefined);
  return JSON.parse(pocketMarkers(path, text, contents(MANIFEST), contents(CRANK)));
}

/** A pocket's name: what crank's own `\w` allows (letters, marks and digits in any script, `_`), plus `-`. */
const NAME = String.raw`[\p{Alphabetic}\p{M}\p{Nd}\p{Pc}\p{Join_Control}-]+`;

/** A pocket's name in a template, which can include `{{ ... }}` substitutions — `// queries-{{ name }} [`. */
const TERA_NAME = String.raw`(?:[\p{Alphabetic}\p{M}\p{Nd}\p{Pc}\p{Join_Control}-]|\{\{[^\n]*?\}\})+`;

/** `text` with every character a `u`-flag regex treats as syntax escaped. */
const escapeRegex = (text) => text.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&");

/**
 * A marker pattern as a regex source, read the way crank reads one: its
 * text literally, any run of whitespace as any amount of it, and
 * `{name}` as `name`.
 */
const patternSource = (pattern, name) =>
  pattern
    .split(/\s+/)
    .filter(Boolean)
    .map((word) => word.split("{name}").map(escapeRegex).join(name))
    .join(String.raw`\s*`);

/** One regex finding every start and end marker of `markers`, or null if there are none. */
function markerRegex(markers, name) {
  const sources = [
    ...markers.map(({ start }) => patternSource(start, name)),
    ...markers.map(({ end }) => patternSource(end, name)),
  ].filter(Boolean);
  return sources.length
    ? new RegExp(sources.map((source) => `(?:${source})`).join("|"), "gu")
    : null;
}

/** Pocket markers in plain text, found by `pockets` (see markerRegex). */
function tokenizePockets(text, pockets) {
  const tokens = [];
  let last = 0;
  for (const match of pockets ? text.matchAll(pockets) : []) {
    tokens.push([null, text.slice(last, match.index)], ["pocket", match[0]]);
    last = match.index + match[0].length;
  }
  tokens.push([null, text.slice(last)]);
  return tokens;
}

/** A Tera tag or expression's insides: keywords, strings, filters and names. */
function tokenizeTeraCode(code, statement) {
  const tokens = [];
  let first = statement;
  let afterPipe = false;
  for (const part of code.split(/("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[A-Za-z_][\w.]*|\|)/)) {
    if (!part) continue;
    if (/^["']/.test(part)) tokens.push(["string", part]);
    else if (part === "|") {
      tokens.push(["delim", part]);
      afterPipe = true;
    } else if (/^[A-Za-z_]/.test(part)) {
      const keyword =
        first ||
        /^(in|and|or|not|is|if|else|true|false|loop)$/.test(part) ||
        part.startsWith("loop.");
      tokens.push([afterPipe ? "filter" : keyword ? "keyword" : "var", part]);
      first = false;
      afterPipe = false;
    } else tokens.push([/^\s*\d/.test(part) ? "literal" : null, part]);
  }
  return tokens;
}

/**
 * A template: its crank header (`{# crank: ... #}`, which lists the
 * markers themselves, so isn't searched for them) as a comment, then
 * pocket markers found by `pockets` (see markerRegex) — before the rest is
 * split at Tera tags, since a marker's name can hold `{{ ... }}`.
 */
function tokenizeTera(text, pockets) {
  const header = text.match(/^\uFEFF?\{#-?\s*crank:[\s\S]*?#\}/)?.[0] ?? "";
  const tokens = header ? [["comment", header]] : [];
  const rest = text.slice(header.length);
  let last = 0;
  for (const match of pockets ? rest.matchAll(pockets) : []) {
    tokens.push(...tokenizeTeraTags(rest.slice(last, match.index), null));
    tokens.push(...tokenizeTeraTags(match[0], "pocket"));
    last = match.index + match[0].length;
  }
  tokens.push(...tokenizeTeraTags(rest.slice(last), null));
  return tokens;
}

/** Tera tags in `text`, with the text between them as `plain` tokens. */
function tokenizeTeraTags(text, plain) {
  const tokens = [];
  let last = 0;
  for (const match of text.matchAll(/\{#[\s\S]*?#\}|\{%-?[\s\S]*?-?%\}|\{\{-?[\s\S]*?-?\}\}/g)) {
    tokens.push([plain, text.slice(last, match.index)]);
    const tag = match[0];
    if (tag.startsWith("{#")) tokens.push(["comment", tag]);
    else {
      const open = tag.match(/^\{[{%]-?/)[0];
      const close = tag.match(/-?[}%]\}$/)[0];
      tokens.push(
        ["delim", open],
        ...tokenizeTeraCode(tag.slice(open.length, -close.length), open.startsWith("{%")),
        ["delim", close],
      );
    }
    last = match.index + tag.length;
  }
  tokens.push([plain, text.slice(last)]);
  return tokens;
}

/** A YAML value: quoted strings, numbers and keywords, and Tera substitutions in .crank paths. */
function tokenizeYamlValue(value) {
  if (/^\s*["']/.test(value)) return [["string", value]];
  if (/^\s*(-?\d[\d._]*|true|false|null|~)\s*$/.test(value)) return [["literal", value]];
  const tokens = [];
  let last = 0;
  for (const match of value.matchAll(/\{\{[\s\S]*?\}\}|#[\p{L}\p{M}\p{N}_[\]/]+$|[[\]{},]/gu)) {
    tokens.push([null, value.slice(last, match.index)]);
    if (match[0].startsWith("{{")) {
      tokens.push(["delim", "{{"], ...tokenizeTeraCode(match[0].slice(2, -2), false), [
        "delim",
        "}}",
      ]);
    } else if (match[0].startsWith("#")) tokens.push(["keyword", match[0]]);
    else tokens.push(["punct", match[0]]);
    last = match.index + match[0].length;
  }
  tokens.push([null, value.slice(last)]);
  return tokens;
}

function tokenizeYaml(text) {
  const tokens = [];
  for (const [i, line] of text.split("\n").entries()) {
    if (i > 0) tokens.push([null, "\n"]);
    // A comment starts at a `#` with a space (or nothing) before it.
    const comment = line.search(/(^|\s)#(?![\p{L}\p{M}\p{N}_[\]/])|(^|\s)#$/u);
    const body = comment < 0 ? line : line.slice(0, comment);
    const rest = comment < 0 ? "" : line.slice(comment);
    const match = body.match(
      /^(\s*)((?:- +)*)(?:([^\s#'"[\]{},][^:#]*?|"[^"]*"|'[^']*')(\s*:)(?=\s|$))?(.*)$/,
    );
    const [, indent, dashes, key, colon, value] = match;
    tokens.push([null, indent]);
    if (dashes) tokens.push(["punct", dashes]);
    if (key) tokens.push(["key", key], ["punct", colon]);
    tokens.push(...tokenizeYamlValue(value));
    if (rest) tokens.push(["comment", rest]);
  }
  return tokens;
}

function tokenizeRhai(text) {
  const tokens = [];
  let last = 0;
  const pattern =
    /\/\/.*|\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])'|\b(?:fn|let|const|if|else|for|in|while|loop|do|until|return|throw|try|catch|break|continue|switch|true|false|this|private)\b|\b\d[\d_.]*\b/g;
  for (const match of text.matchAll(pattern)) {
    tokens.push([null, text.slice(last, match.index)]);
    const part = match[0];
    const kind = part.startsWith("/")
      ? "comment"
      : /^["']/.test(part)
        ? "string"
        : /^\d/.test(part)
          ? "literal"
          : "keyword";
    tokens.push([kind, part]);
    last = match.index + part.length;
  }
  tokens.push([null, text.slice(last)]);
  return tokens;
}

/** The indentation one Tab adds, by what `path` looks like. */
const indentFor = (path) =>
  path === CRANK || /\.(ya?ml|manifest)$/.test(path ?? "") ? "  " : "    ";

/** Inserts `text` over the selection, keeping the browser's own undo history. */
function insertText(text) {
  if (!document.execCommand("insertText", false, text)) {
    source.setRangeText(text, source.selectionStart, source.selectionEnd, "end");
    source.dispatchEvent(new Event("input"));
  }
}

/** The start and end offsets of every line the selection touches. */
function selectedLines() {
  const value = source.value;
  const start = value.lastIndexOf("\n", source.selectionStart - 1) + 1;
  let end = value.indexOf("\n", Math.max(source.selectionEnd - 1, start));
  if (end < 0 || source.selectionEnd === start) end = value.indexOf("\n", start);
  if (end < 0) end = value.length;
  return [start, end];
}

let escapeBeforeTab = false;

source.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    escapeBeforeTab = true;
    return;
  }
  const leaving = escapeBeforeTab;
  escapeBeforeTab = false;
  const indent = indentFor(state.open);
  if (event.key === "Tab" && !leaving && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault();
    const multiline = source.value.slice(source.selectionStart, source.selectionEnd).includes("\n");
    if (!event.shiftKey && !multiline) {
      insertText(indent);
      return;
    }
    // Indent or dedent every selected line.
    const [start, end] = selectedLines();
    const lines = source.value.slice(start, end).split("\n");
    const dedent = new RegExp(`^( {1,${indent.length}}|\\t)`);
    const changed = lines.map((line) =>
      event.shiftKey ? line.replace(dedent, "") : indent + line,
    );
    source.setSelectionRange(start, end);
    insertText(changed.join("\n"));
    source.setSelectionRange(start, start + changed.join("\n").length);
  } else if (event.key === "Enter" && !event.ctrlKey && !event.metaKey && !event.altKey) {
    // Keep the indentation, and nest under a YAML key with nothing after its colon.
    event.preventDefault();
    const lineStart = source.value.lastIndexOf("\n", source.selectionStart - 1) + 1;
    const before = source.value.slice(lineStart, source.selectionStart);
    const current = before.match(/^\s*/)[0];
    const nests = indent === "  " && /:\s*$/.test(before.replace(/\s#.*$/, ""));
    // A list item's own keys line up with the text after its dash.
    const item = indent === "  " && before.match(/^(\s*)- /);
    const next = item && !nests ? `${item[1]}  ` : current;
    insertText(`\n${next}${nests ? indent : ""}`);
  }
});

let pending = 0;
source.addEventListener("input", () => {
  if (!state.open) return;
  state.files.get(state.open).contents = source.value;
  // Re-highlight at once, so typing never lags; run crank once the
  // browser has caught up with the keystrokes that arrived together.
  renderHighlight();
  keepCaretVisible();
  clearTimeout(pending);
  pending = setTimeout(edited);
});

// The textarea is exactly as big as its text, inside a scrolling box —
// so it never scrolls itself, and the box follows the caret instead.
source.addEventListener("scroll", () => {
  source.scrollTop = 0;
  source.scrollLeft = 0;
});

let charWidth = 0;
function keepCaretVisible() {
  if (!charWidth) {
    const probe = el("span", { textContent: "M".repeat(20) });
    highlight.append(probe);
    charWidth = probe.getBoundingClientRect().width / 20;
    probe.remove();
  }
  const before = source.value.slice(0, source.selectionEnd).split("\n");
  const lineHeight = parseFloat(getComputedStyle(source).lineHeight);
  const top = 10 + (before.length - 1) * lineHeight;
  const left = gutter.offsetWidth + 12 + before.at(-1).length * charWidth;
  const box = editorScroll;
  if (top < box.scrollTop) box.scrollTop = top - 10;
  else if (top + lineHeight > box.scrollTop + box.clientHeight)
    box.scrollTop = top + lineHeight + 10 - box.clientHeight;
  if (left < box.scrollLeft + gutter.offsetWidth) box.scrollLeft = left - gutter.offsetWidth - 40;
  else if (left > box.scrollLeft + box.clientWidth - 20)
    box.scrollLeft = left - box.clientWidth + 60;
}

// ---------------------------------------------------------------- output

function renderOutput() {
  const outcome = state.outcome;
  if (!outcome) return;
  $("autorun").checked = state.autorun;
  // "Run crank" takes the exit status's place once there's something to run.
  const runButton = $("run");
  runButton.hidden = state.autorun;
  runButton.disabled = !state.stale;
  const status = $("status");
  status.hidden = state.stale;
  status.className = `status ${outcome.passed ? "passed" : "failed"}`;
  status.textContent = outcome.passed ? "succeeds · exit 0" : "fails · exit 1";

  const terminal = $("terminal");
  terminal.classList.toggle("stale", state.stale);
  if (state.stale) {
    terminal.replaceChildren(
      el("span", {
        className: "dim",
        textContent: "Edited since crank last ran. Run crank to see what it does now.",
      }),
    );
    return;
  }
  terminal.replaceChildren(el("span", { className: "prompt", textContent: "$ " }), "crank\n");
  let lastFile = null;
  for (const line of outcome.lines) {
    const node = el("span", { textContent: line.text });
    const text = line.text;
    const at = text.match(/^\s*--> template:(\d+):\d+/);
    if (at && lastFile) linkify(node, at[0].trim(), lastFile, Number(at[1]));
    lastFile = linkPaths(node, text) ?? (at ? null : lastFile);
    terminal.append(node, "\n");
  }
}

/** Makes every file `line` names clickable; returns the first it names. */
function linkPaths(line, text) {
  // A `  - path` item in a summary names a file crank wrote, or deleted.
  const item = text.match(/^ {2}- (.+?)(?: \(would \w+\))?$/);
  if (item) {
    if (isFile(item[1])) linkify(line, item[1], item[1], null);
    return null;
  }
  let first = null;
  for (const path of pathsIn(text)) {
    linkify(line, text.includes(`./${path}`) ? `./${path}` : path, path, null);
    first ??= path;
  }
  return first;
}

/** Turns the first `target` text in `line` into a button opening `path`. */
function linkify(line, target, path, lineNumber) {
  const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node.parentElement?.classList.contains("link")) continue;
    const at = node.data.indexOf(target);
    if (at < 0) continue;
    const rest = node.splitText(at);
    rest.splitText(target.length);
    const link = el("button", { type: "button", className: "link", textContent: target });
    link.title = lineNumber ? `Show line ${lineNumber} of ${path}` : `Open ${path}`;
    link.addEventListener("click", () => show(path, lineNumber));
    rest.replaceWith(link);
    return;
  }
}

// ---------------------------------------------------------------- examples

function renderExamples() {
  const list = $("example-list");
  const chip = (label, number, onClick) => {
    const button = el("button", { type: "button", className: "example-chip" });
    if (number) button.append(el("span", { className: "number", textContent: number }));
    button.append(el("span", { innerHTML: inlineMarkdown(label) }));
    button.addEventListener("click", onClick);
    return button;
  };
  const chips = examples.map((example, i) => {
    const button = chip(example.title, String(i + 1), () => loadExample(example));
    button.dataset.id = example.id;
    button.title = plainText(example.explanation.split(/\n\s*\n/)[0]);
    return button;
  });
  const blank = chip("Blank", null, loadBlank);
  blank.title = "Start from a minimal .crank file, data file and template";
  list.replaceChildren(...[...chips, blank].map((button) => el("li", {}, button)));
}

function renderExampleState() {
  for (const chip of $("example-list").querySelectorAll(".example-chip")) {
    const current = state.example ? chip.dataset.id === state.example.id : false;
    chip.setAttribute("aria-pressed", String(current));
  }
}

function loadTree(entries, { example = null, open = null } = {}) {
  state.files = new Map();
  for (const { path, directory, contents } of entries)
    addEntry(path, Boolean(directory), contents ?? "");
  state.generated = new Set();
  state.example = example;
  state.selected = null;
  state.collapsed = new Set();
  state.editing = null;
  // Checked once crank has run, since it may be a file crank generates.
  state.open = open;

  $("explanation").hidden = !example;
  $("split-explanation").hidden = !example;
  if (example) {
    $("explanation-title").innerHTML = inlineMarkdown(example.title);
    $("explanation-body").innerHTML = markdown(example.explanation);
  }
  // As if crank had just been run in the project.
  rerun();
  state.pristine = treeJson();
}

function loadExample(example) {
  loadTree(example.tree, { example });
  history.replaceState(null, "", `#${example.id}`);
}

function loadBlank() {
  loadTree([
    { path: CRANK, contents: STARTER_CRANK },
    { path: "data.yaml", contents: "name: world\n" },
    { path: "template.txt.tera", contents: "Hello, {{ name }}!\n" },
  ]);
  history.replaceState(null, "", location.pathname);
}

/** One line of the small Markdown subset examples use, as HTML. */
function inlineMarkdown(raw) {
  const codes = [];
  let html = escapeHtml(
    raw.replace(/`([^`]+)`/g, (_, code) => `\uE000${codes.push(code) - 1}\uE000`),
  );
  html = html
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*\s][^*]*)\*/g, "<em>$1</em>")
    .replace(
      /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
      '<a href="$2" target="_blank" rel="noopener">$1</a>',
    );
  return html.replace(/\uE000(\d+)\uE000/g, (_, i) => `<code>${escapeHtml(codes[i])}</code>`);
}

/** `raw` Markdown as plain text, for a tooltip. */
const plainText = (raw) =>
  raw
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/\*+([^*]+)\*+/g, "$1")
    .replace(/\s+/g, " ");

/** An example's explanation, as HTML: paragraphs and bulleted lists. */
function markdown(text) {
  const inline = inlineMarkdown;
  return text
    .split(/\n\s*\n/)
    .map((block) => {
      const lines = block.split("\n");
      if (lines.every((line) => /^\s*- /.test(line))) {
        return `<ul>${lines.map((line) => `<li>${inline(line.replace(/^\s*- /, ""))}</li>`).join("")}</ul>`;
      }
      return `<p>${inline(lines.join(" "))}</p>`;
    })
    .join("");
}

// ---------------------------------------------------------------- sharing

// A shared link holds the whole tree in its fragment, so it needs no
// server: `#tree=<version>.<data>`. The version says how <data> is
// encoded, so a link keeps opening after the encoding changes. A new
// encoding gets a new version and an entry in SHARE_DECODERS, and every
// older entry stays, since links to it are out in the world.
//
// Version 1: the JSON { tree: [{ path, directory?, contents? }], open },
// compressed with raw DEFLATE, in base64url without padding.
const SHARE_VERSION = 1;

const base64url = (bytes) => {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};
const fromBase64url = (text) =>
  Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));

/** `bytes` run through a CompressionStream or DecompressionStream. */
const transform = async (bytes, stream) =>
  new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(stream)).arrayBuffer());

const SHARE_DECODERS = {
  1: async (data) => {
    const json = await transform(fromBase64url(data), new DecompressionStream("deflate-raw"));
    return JSON.parse(new TextDecoder().decode(json));
  },
};

async function encodeShare(shared) {
  const json = new TextEncoder().encode(JSON.stringify(shared));
  const data = await transform(json, new CompressionStream("deflate-raw"));
  return `${SHARE_VERSION}.${base64url(data)}`;
}

/** Whether `tree` is one the page could have made: relative, `/`-joined, unique paths. */
const validTree = (tree) =>
  Array.isArray(tree) &&
  tree.every(
    (entry) =>
      typeof entry?.path === "string" &&
      entry.path.split("/").every((part) => !invalidName(part)) &&
      (entry.contents === undefined || typeof entry.contents === "string"),
  ) &&
  new Set(tree.map((entry) => entry.path)).size === tree.length;

/** The { tree, open } a link's `<version>.<data>` holds. */
async function decodeShare(text) {
  const dot = text.indexOf(".");
  const version = Number(text.slice(0, dot));
  const decoder = dot > 0 ? SHARE_DECODERS[version] : undefined;
  if (!decoder) {
    throw new Error(
      version > SHARE_VERSION
        ? "That link was made by a newer version of the playground."
        : "That link isn't one the playground made.",
    );
  }
  let shared;
  try {
    shared = await decoder(text.slice(dot + 1));
  } catch {
    throw new Error("That link is damaged — perhaps cut short when it was copied.");
  }
  // Hand-edited or truncated, a link could hold a tree the page can't show.
  if (!validTree(shared?.tree)) throw new Error("That link's tree is malformed.");
  return shared;
}

$("share").addEventListener("click", async () => {
  const edited = state.pristine !== treeJson();
  const hash =
    state.example && !edited
      ? state.example.id
      : `tree=${await encodeShare({ tree: sourceTree(), open: state.open })}`;
  const url = `${location.origin}${location.pathname}#${hash}`;
  history.replaceState(null, "", `#${hash}`);
  try {
    await navigator.clipboard.writeText(url);
    toast("Link copied — it holds every file crank needs, so it opens exactly this.");
  } catch {
    prompt("Copy this link:", url);
  }
});

// ---------------------------------------------------------------- downloading

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (bytes) => {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
};

/**
 * A zip of `entries` ({ path, directory, contents }), each file
 * DEFLATE-compressed unless that would make it bigger.
 */
async function zip(entries) {
  const now = new Date();
  const time = (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1);
  const date = ((now.getFullYear() - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate();
  const encoder = new TextEncoder();
  const parts = [];
  const central = [];
  let offset = 0;
  for (const { path, directory, contents } of entries) {
    const name = encoder.encode(directory ? `${path}/` : path);
    const raw = directory ? new Uint8Array() : encoder.encode(contents);
    const deflated = raw.length ? await transform(raw, new CompressionStream("deflate-raw")) : raw;
    const compress = deflated.length < raw.length;
    const data = compress ? deflated : raw;
    // The fields a local header and its central directory entry share.
    const common = (view, at) => {
      view.setUint16(at, 20, true); // version needed: 2.0
      view.setUint16(at + 2, 0x0800, true); // names are UTF-8
      view.setUint16(at + 4, compress ? 8 : 0, true);
      view.setUint16(at + 6, time, true);
      view.setUint16(at + 8, date, true);
      view.setUint32(at + 10, crc32(raw), true);
      view.setUint32(at + 14, data.length, true);
      view.setUint32(at + 18, raw.length, true);
      view.setUint16(at + 22, name.length, true);
    };
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    common(local, 4);
    const entry = new DataView(new ArrayBuffer(46));
    entry.setUint32(0, 0x02014b50, true);
    entry.setUint16(4, 0x0314, true); // made by: Unix, so the modes below apply
    common(entry, 6);
    entry.setUint32(38, (directory ? 0o40755 : 0o100644) << 16, true);
    entry.setUint32(42, offset, true);
    parts.push(local, name, data);
    central.push(entry, name);
    offset += 30 + name.length + data.length;
  }
  const size = central.reduce((sum, part) => sum + part.byteLength, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, entries.length, true);
  end.setUint16(10, entries.length, true);
  end.setUint32(12, size, true);
  end.setUint32(16, offset, true);
  return new Blob([...parts, ...central, end], { type: "application/zip" });
}

// Inside one folder, named for the example if there is one.
$("download").addEventListener("click", async () => {
  const name = state.example ? `crank-example-${state.example.id}` : "crank-example";
  const entries = sourceTree().map((entry) => ({ ...entry, path: `${name}/${entry.path}` }));
  const url = URL.createObjectURL(
    await zip([{ path: name, directory: true, contents: "" }, ...entries]),
  );
  el("a", { href: url, download: `${name}.zip` }).click();
  // After the download has had a chance to start.
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});

let toastTimer;
function toast(message) {
  const node = $("toast");
  node.textContent = message;
  node.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (node.hidden = true), 3000);
}

async function loadFromHash() {
  const hash = decodeURIComponent(location.hash.slice(1));
  if (hash.startsWith("tree=")) {
    try {
      const { tree, open } = await decodeShare(hash.slice("tree=".length));
      loadTree(tree, { open });
      return;
    } catch (error) {
      console.error("Couldn't read the shared tree:", error);
      toast(`${error.message} Here's the first example instead.`);
    }
  }
  loadExample(examples.find((example) => example.id === hash) ?? examples[0]);
}

// ---------------------------------------------------------------- running by hand

$("autorun").addEventListener("change", (event) => {
  state.autorun = event.target.checked;
  // Catch up on the edits made while it was off.
  if (state.autorun && state.stale) rerun();
  else renderOutput();
});

$("run").addEventListener("click", () => {
  if (state.stale) rerun();
});

// Ctrl+Enter (or Cmd+Enter) runs crank, from anywhere on the page.
document.addEventListener("keydown", (event) => {
  if (event.key !== "Enter" || !(event.ctrlKey || event.metaKey)) return;
  if (state.autorun || !state.stale) return;
  event.preventDefault();
  clearTimeout(pending);
  rerun();
});

// ---------------------------------------------------------------- splitters

const workspace = $("workspace");
const explanationPanel = $("explanation");
const filesPanel = workspace.querySelector(".files");
const editorPanel = workspace.querySelector(".editor-panel");
const outputPanel = workspace.querySelector(".output-panel");
const GAP = 16;
/** How far an arrow key moves a splitter, in pixels. */
const KEY_STEP = 16;

/**
 * Makes the gap `id` draggable, and movable with the arrow keys. `size`
 * is the current size of the panel it resizes, `min` and `max` its
 * limits, and `set` applies a new size; `horizontal` is the direction it
 * moves in, and `sign` is -1 where moving it forward shrinks the panel.
 */
function splitter(id, { horizontal, sign, size, min, max, set }) {
  const node = $(id);
  const position = (event) => (horizontal ? event.clientX : event.clientY);
  const resize = (wanted) => {
    set(Math.round(Math.max(min, Math.min(wanted, max()))));
    describeSize();
  };
  // For a screen reader. The panel's size also changes without the
  // splitter moving — with the window, or the output's length — so this
  // is brought up to date whenever the splitter is focused.
  const describeSize = () => {
    node.ariaValueNow = size();
    node.ariaValueMin = min;
    node.ariaValueMax = Math.round(max());
  };
  node.addEventListener("focus", describeSize);

  node.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    // Or the drag would select the text it passes over.
    event.preventDefault();
    const from = position(event);
    const initial = size();
    const move = (moved) => resize(initial + sign * (position(moved) - from));
    // Captured, the pointer keeps the splitter's cursor and keeps
    // reporting here, wherever it goes.
    node.setPointerCapture(event.pointerId);
    node.classList.add("dragging");
    node.addEventListener("pointermove", move);
    node.addEventListener(
      "lostpointercapture",
      () => {
        node.classList.remove("dragging");
        node.removeEventListener("pointermove", move);
      },
      { once: true },
    );
  });

  node.addEventListener("keydown", (event) => {
    const [back, forward] = horizontal ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
    if (event.key !== back && event.key !== forward) return;
    event.preventDefault();
    resize(size() + sign * (event.key === forward ? KEY_STEP : -KEY_STEP));
  });
}

/** The width of the workspace's columns, with the gaps between them. */
const columnsWidth = () => {
  const explanation = explanationPanel.hidden ? 0 : explanationPanel.offsetWidth + GAP;
  return explanation + filesPanel.offsetWidth + GAP + editorPanel.offsetWidth;
};

splitter("split-explanation", {
  horizontal: true,
  sign: 1,
  size: () => explanationPanel.offsetWidth,
  min: 160,
  // The same limit as the column's own, in style.css.
  max: () => 0.4 * columnsWidth(),
  set: (width) => workspace.style.setProperty("--explanation-width", `${width}px`),
});

splitter("split-files", {
  horizontal: true,
  sign: 1,
  size: () => filesPanel.offsetWidth,
  min: 160,
  // The same limit as the column's own, in style.css: less beside a
  // description.
  max: () => (explanationPanel.hidden ? 0.6 : 0.4) * columnsWidth(),
  set: (width) => workspace.style.setProperty("--files-width", `${width}px`),
});

splitter("split-output", {
  horizontal: false,
  sign: -1,
  size: () => outputPanel.offsetHeight,
  min: 80,
  // Leave the editor a few lines.
  max: () => editorPanel.offsetHeight + outputPanel.offsetHeight - 120,
  set: (height) => {
    // The editor takes what's left, and may now be shorter than it starts.
    workspace.style.setProperty("--editor-height", "minmax(120px, 1fr)");
    workspace.style.setProperty("--output-height", `${height}px`);
  },
});

// ---------------------------------------------------------------- start

await init();
examples = JSON.parse(examplesJson());
renderExamples();
await loadFromHash();
// Following a link to another example or shared tree from this page.
// (The page's own history.replaceState calls don't fire this.)
window.addEventListener("hashchange", loadFromHash);
