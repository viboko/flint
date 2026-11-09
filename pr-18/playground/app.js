// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// flint playground: a pretend file tree on the left, the open .flint file
// in the middle, and flint's output below — re-linted on every change by
// flint itself, compiled to WebAssembly (see playground/src/lib.rs).

import init, { examples as examplesJson, lint } from "./pkg/flint_playground.js";

const FLINT = ".flint";
const INDENT = "    ";

// ---------------------------------------------------------------- icons

const svg = (body, viewBox = "0 0 16 16") =>
  `<svg viewBox="${viewBox}" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

const ICONS = {
  chevron: svg('<path d="M3 1.5 7.5 6 3 10.5"/>', "0 0 10 12"),
  folder: svg(
    '<path d="M1.5 4.5a1 1 0 0 1 1-1h3.6l1.4 1.5h5.9a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1H2.5a1 1 0 0 1-1-1z"/>',
  ),
  file: svg(
    '<path d="M4 1.5h5L12.5 5v9a.5.5 0 0 1-.5.5H4a.5.5 0 0 1-.5-.5V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5V5h3.5"/>',
  ),
  flint: svg('<path d="M9.5 1.5 4 9h3.5l-1 5.5L12 7H8.5z" fill="currentColor" stroke="none"/>'),
  newFile: svg(
    '<path d="M4 1.5h5L12.5 5v4"/><path d="M3.5 2v12a.5.5 0 0 0 .5.5h5"/><path d="M9 1.5V5h3.5"/><path d="M12.5 11v4M10.5 13h4"/>',
  ),
  newFolder: svg(
    '<path d="M8 13.5H2.5a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1h3.6l1.4 1.5h5.9a1 1 0 0 1 1 1V8"/><path d="M12.5 10v5M10 12.5h5"/>',
  ),
  newFlint: svg(
    '<path d="M9 1 3.5 8.5h3l-1 5 5.5-7.5H8z" fill="currentColor" stroke="none"/><path d="M12.5 10v5M10 12.5h5"/>',
  ),
  rename: svg('<path d="M10.5 2.5 13.5 5.5 6 13H3v-3z"/>'),
  trash: svg('<path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4"/>'),
};

// ---------------------------------------------------------------- state

const state = {
  /** path -> { directory: boolean, contents: string } */
  files: new Map(),
  /** Path of the .flint file open in the editor, or null. */
  open: null,
  /** Path of the selected tree row: where "new file" goes. */
  selected: null,
  collapsed: new Set(),
  /** The example loaded, or null for a blank or shared tree. */
  example: null,
  /** The tree as loaded, to tell whether it's been edited since. */
  pristine: null,
  /** flint's outcome for the current tree (see Outcome in lib.rs). */
  outcome: null,
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
const isFlint = (path) => nameOf(path) === FLINT;
const isDirectory = (path) => path === "" || state.files.get(path)?.directory === true;
const ancestorsOf = (path) => {
  const result = [];
  for (let dir = parentOf(path); dir; dir = parentOf(dir)) result.push(dir);
  return result;
};

/** Why `name` can't be a file or folder name, or null if it can. */
function invalidName(name) {
  if (name === "") return "A name can't be empty.";
  if (name === "." || name === "..") return `"${name}" can't be used as a name.`;
  return null;
}

/** Entries directly inside `dir`: its .flint file (its rules) first, then folders, then files. */
function childrenOf(dir) {
  const children = [...state.files.keys()].filter((path) => parentOf(path) === dir);
  const rank = (path) => (isDirectory(path) ? 1 : isFlint(path) ? 0 : 2);
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

const flintFiles = () =>
  [...state.files.keys()].filter((path) => isFlint(path) && !isDirectory(path)).sort();

const treeJson = () =>
  JSON.stringify(
    [...state.files].map(([path, { directory, contents }]) => ({ path, directory, contents })),
  );

// ---------------------------------------------------------------- linting

function relint() {
  const result = JSON.parse(lint(treeJson()));
  // The page only ever builds valid trees, so an error here is a bug.
  if (result.error) {
    console.error(result.error);
    return;
  }
  state.outcome = result;
  render();
}

/** Problems grouped the ways the tree and editor need them. */
function indexProblems() {
  const byPath = new Map();
  const lineNotes = new Map();
  const ghostsByDir = new Map();
  const troubledDirs = new Set();
  const add = (map, key, value) => map.set(key, [...(map.get(key) ?? []), value]);
  const note = (file, line, severity, message) => {
    if (!lineNotes.has(file)) lineNotes.set(file, new Map());
    add(lineNotes.get(file), line, { severity, message });
  };

  for (const problem of state.outcome?.problems ?? []) {
    switch (problem.kind) {
      case "unexpected":
        add(byPath, problem.path, problem);
        for (const dir of ancestorsOf(problem.path)) troubledDirs.add(dir);
        break;
      case "group_conflict":
        for (const path of problem.paths) {
          add(byPath, path, problem);
          for (const dir of ancestorsOf(path)) troubledDirs.add(dir);
        }
        note(problem.flint_file, problem.line, "warn", problem.message);
        break;
      case "missing": {
        const ghost = placeGhost(problem);
        add(ghostsByDir, ghost.dir, ghost);
        if (ghost.dir) {
          for (const dir of [ghost.dir, ...ancestorsOf(ghost.dir)]) troubledDirs.add(dir);
        }
        note(problem.flint_file, problem.line, "warn", `Missing: ${join(ghost.dir, ghost.label)}`);
        break;
      }
      case "parse_error":
        add(byPath, problem.flint_file, problem);
        for (const dir of ancestorsOf(problem.flint_file)) troubledDirs.add(dir);
        note(problem.flint_file, problem.line, "error", problem.message);
        break;
    }
  }
  return { byPath, lineNotes, ghostsByDir, troubledDirs };
}

/**
 * Where to show a missing entry: in the deepest folder of its path that
 * exists, labeled with the rest — usually just its name, but a pattern
 * like `*\/main.py` when no folder for it exists yet.
 */
function placeGhost(problem) {
  const directory = problem.path.endsWith("/");
  const parts = problem.path.replace(/\/$/, "").split("/");
  let depth = parts.length - 1;
  while (depth > 0 && !isDirectory(parts.slice(0, depth).join("/"))) depth -= 1;
  return {
    dir: parts.slice(0, depth).join("/"),
    label: parts.slice(depth).join("/") + (directory ? "/" : ""),
    directory,
    problem,
  };
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
  $("new-flint").hidden = state.files.has(FLINT);
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
  for (const ghost of index.ghostsByDir.get(dir) ?? []) rows.push(ghostRow(ghost, depth));
  return rows;
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
  const flint = !directory && isFlint(path);
  const problems = index.byPath.get(path) ?? [];
  const broken = problems.some((problem) => problem.kind === "parse_error");
  const classes = [
    directory ? "directory" : "file",
    flint && "flint",
    path === state.open && "open",
    path === state.selected && "selected",
    broken ? "broken" : problems.length > 0 && "problem",
  ].filter(Boolean);
  const row = rowBase(path, depth, classes.join(" "));
  row.dataset.path = path;
  if (directory) row.setAttribute("aria-expanded", String(!state.collapsed.has(path)));

  const twisty = el("span", { className: "twisty" });
  if (directory) twisty.innerHTML = ICONS.chevron;
  const icon = el("span", { className: "icon" });
  icon.innerHTML = directory ? ICONS.folder : flint ? ICONS.flint : ICONS.file;

  let name;
  if (state.editing?.mode === "rename" && state.editing.path === path) {
    name = nameInput(nameOf(path));
  } else {
    name = el("span", { className: "name", textContent: nameOf(path) });
  }
  row.append(twisty, icon, name);

  if (problems.length > 0) {
    const label = broken
      ? "error"
      : problems[0].kind === "group_conflict"
        ? "conflict"
        : "unexpected";
    row.append(el("span", { className: "badge", textContent: label }));
    row.title = `${problems.map(describe).join("\n")}\nClick to see the rule.`;
  } else if (flint) {
    row.title = `Open ${path}`;
  } else if (!directory) {
    row.title = "flint only checks this file's name, never its contents.";
  }
  if (directory && state.collapsed.has(path) && index.troubledDirs.has(path)) {
    row.append(el("span", { className: "dot", title: "Something inside needs attention" }));
  }

  const actions = el("span", { className: "actions" });
  if (directory) {
    if (!state.files.has(join(path, FLINT))) {
      actions.append(actionButton(ICONS.newFlint, "Add a .flint file here", () => addFlint(path)));
    }
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

function ghostRow(ghost, depth) {
  const row = rowBase(`ghost:${ghost.problem.path}`, depth, "ghost");
  const icon = el("span", { className: "icon" });
  icon.innerHTML = ghost.directory ? ICONS.folder : ICONS.file;
  row.append(
    el("span", { className: "twisty" }),
    icon,
    el("span", { className: "name", textContent: ghost.label }),
    el("span", { className: "badge", textContent: "missing" }),
  );
  row.title = `${describe(ghost.problem)}\nClick to see the rule.`;
  row.dataset.ghost = ghost.problem.path;
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
    if (!state.editing.directory && isFlint(target)) state.open = target;
  }
  state.editing = null;
  ensureOpen();
  relint();
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

function addFlint(dir) {
  const path = join(dir, FLINT);
  if (!state.files.has(path)) {
    addEntry(path, false, "# What's allowed in this folder, one rule per line\n");
  }
  if (dir) state.collapsed.delete(dir);
  state.selected = path;
  openFlint(path);
  relint();
  $("source").focus();
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
  ensureOpen();
  relint();
}

/** Keeps a .flint file open whenever there is one, preferring the root's. */
function ensureOpen() {
  if (state.open && state.files.has(state.open) && !isDirectory(state.open)) return;
  const flints = flintFiles();
  openFlint(flints.includes(FLINT) ? FLINT : (flints[0] ?? null));
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

/** Shows `problem` in the tree, and the rule responsible in the editor. */
function showProblem(problem) {
  openFlint(problem.flint_file, problem.line);
  if (problem.kind === "missing") {
    reveal(`ghost:${problem.path}`);
  } else if (problem.kind !== "parse_error") {
    const path = problem.paths?.[0] ?? problem.path;
    state.selected = path;
    reveal(path);
  } else {
    reveal(problem.flint_file);
  }
}

/** Where `problem`'s rule is, for a tooltip — if a rule is responsible. */
const ruleLocation = (problem) =>
  problem.line ? `the rule on line ${problem.line} of ${problem.flint_file}` : null;

/** `problem`'s message, saying which rule is responsible. */
function describe(problem) {
  if (problem.kind === "unexpected") return `${problem.message} (checked by ${problem.flint_file})`;
  if (problem.kind === "parse_error") return `${problem.flint_file}: ${problem.message}`;
  return `${problem.message} (${ruleLocation(problem)})`;
}

function activateRow(row) {
  if (row.dataset.ghost) {
    const problem = state.outcome.problems.find(
      (candidate) => candidate.kind === "missing" && candidate.path === row.dataset.ghost,
    );
    if (problem) showProblem(problem);
    return;
  }
  const path = row.dataset.path;
  if (!path) return;
  state.selected = path;
  if (isDirectory(path)) {
    if (state.collapsed.has(path)) state.collapsed.delete(path);
    else state.collapsed.add(path);
    renderTree();
    return;
  }
  if (isFlint(path)) {
    openFlint(path);
    renderTree();
    return;
  }
  const problem = state.outcome?.problems.find((candidate) => candidate.path === path);
  if (problem) showProblem(problem);
  else renderTree();
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
      if (path) startRename(path);
      break;
    case "Delete":
    case "Backspace":
      if (path) deleteEntry(path);
      break;
    default:
      return;
  }
  event.preventDefault();
});

$("new-file").innerHTML = ICONS.newFile;
$("new-folder").innerHTML = ICONS.newFolder;
$("new-flint").innerHTML = ICONS.newFlint;
$("editor-icon").innerHTML = ICONS.flint;
// The header's buttons always add at the root; each folder's row has its own.
$("new-file").addEventListener("click", () => startNew("", false));
$("new-folder").addEventListener("click", () => startNew("", true));
$("new-flint").addEventListener("click", () => addFlint(""));
$("add-root-flint").addEventListener("click", () => addFlint(""));

// ---------------------------------------------------------------- editor

const source = $("source");
const highlight = $("highlight");
const gutter = $("gutter");
const editorScroll = $("editor-scroll");

function openFlint(path, line) {
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
  $("editor-path").textContent = path ?? ".flint";
  // The same lozenge as the file's row in the tree when the .flint file doesn't parse.
  const errors = ((path && indexProblems().byPath.get(path)) ?? []).filter(
    (problem) => problem.kind === "parse_error",
  );
  const errorNote = $("error-note");
  errorNote.hidden = errors.length === 0;
  errorNote.title = errors.map(describe).join("\n");
  if (!path) return;
  const contents = state.files.get(path).contents;
  if (source.value !== contents) source.value = contents;
  renderHighlight();
}

function renderHighlight() {
  const notes = indexProblems().lineNotes.get(state.open) ?? new Map();
  const lines = source.value.split("\n");
  highlight.replaceChildren(
    ...lines.map((text, i) => {
      const lineNotes = notes.get(i + 1) ?? [];
      const severity = lineNotes.some((n) => n.severity === "error")
        ? "error"
        : lineNotes.length
          ? "warn"
          : "";
      const line = el("span", { className: `line ${severity}` });
      line.innerHTML = highlightLine(text);
      if (lineNotes.length) {
        line.append(
          el("span", {
            className: "note",
            textContent: lineNotes.map((n) => n.message).join(" · "),
          }),
        );
      }
      return line;
    }),
  );
  gutter.replaceChildren(
    ...lines.map((_, i) => {
      const lineNotes = notes.get(i + 1) ?? [];
      const severity = lineNotes.some((n) => n.severity === "error")
        ? "error"
        : lineNotes.length
          ? "warn"
          : "";
      return el("div", {
        className: severity,
        textContent: String(i + 1),
        title: lineNotes.map((n) => n.message).join("\n"),
      });
    }),
  );
}

const escapeHtml = (text) =>
  text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const token = (kind, text) => `<span class="tok-${kind}">${escapeHtml(text)}</span>`;

/** One .flint line as highlighted HTML — for show only; flint does the real parsing. */
function highlightLine(line) {
  let html = "";
  let i = 0;
  const indent = line.match(/^\s*/)[0];
  html += escapeHtml(indent);
  i = indent.length;
  if (line[i] === "!") {
    html += token("required", "!");
    i += 1;
  }
  // Where the rule ends and any trailing comment starts.
  let end = line.length;
  for (let j = i; j < line.length; j += 1) {
    if (line[j] === "\\") j += 1;
    else if (line[j] === "#") {
      end = j;
      break;
    }
  }
  const rule = line.slice(i, end);
  const slash = rule.trimEnd().endsWith("/") ? rule.trimEnd().length - 1 : -1;
  for (let j = 0; j < rule.length; ) {
    if (rule[j] === "\\" && j + 1 < rule.length) {
      html += token("escape", rule.slice(j, j + 2));
      j += 2;
    } else if (rule.startsWith("{{", j) && rule.indexOf("}}", j) > 0) {
      const close = rule.indexOf("}}", j) + 2;
      const inner = rule.slice(j, close);
      const pipe = inner.indexOf("|");
      html +=
        pipe < 0
          ? token("placeholder", inner)
          : token("placeholder", inner.slice(0, pipe)) +
            token("format", inner.slice(pipe, -2)) +
            token("placeholder", "}}");
      j = close;
    } else if (j === 0 && rule[0] === "<" && rule.indexOf(">") > 0) {
      // A group header, `<name>`: it declares the placeholders it shares.
      const close = rule.indexOf(">") + 1;
      html += token("placeholder", rule.slice(0, close));
      j = close;
    } else if (rule[j] === "*") {
      const stars = rule.startsWith("**", j) ? "**" : "*";
      html += token("wildcard", stars);
      j += stars.length;
    } else if (j === slash) {
      html += token("slash", "/");
      j += 1;
    } else {
      html += escapeHtml(rule[j]);
      j += 1;
    }
  }
  if (end < line.length) html += token("comment", line.slice(end));
  return html;
}

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
  if (event.key === "Tab" && !leaving && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault();
    const multiline = source.value.slice(source.selectionStart, source.selectionEnd).includes("\n");
    if (!event.shiftKey && !multiline) {
      insertText(INDENT);
      return;
    }
    // Indent or dedent every selected line.
    const [start, end] = selectedLines();
    const lines = source.value.slice(start, end).split("\n");
    const changed = lines.map((line) =>
      event.shiftKey ? line.replace(/^( {1,4}|\t)/, "") : INDENT + line,
    );
    source.setSelectionRange(start, end);
    insertText(changed.join("\n"));
    source.setSelectionRange(start, start + changed.join("\n").length);
  } else if (event.key === "Enter" && !event.ctrlKey && !event.metaKey && !event.altKey) {
    // Keep the indentation, and nest under a folder line (one ending in /).
    event.preventDefault();
    const lineStart = source.value.lastIndexOf("\n", source.selectionStart - 1) + 1;
    const before = source.value.slice(lineStart, source.selectionStart);
    const indent = before.match(/^\s*/)[0];
    const rule = before.replace(/(^|[^\\])#.*$/, "$1").trimEnd();
    insertText(`\n${indent}${rule.endsWith("/") ? INDENT : ""}`);
  }
});

source.addEventListener("input", () => {
  if (!state.open) return;
  state.files.get(state.open).contents = source.value;
  relint();
  keepCaretVisible();
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
  const status = $("status");
  status.className = `status ${outcome.passed ? "passed" : "failed"}`;
  status.textContent = outcome.passed ? "passes · exit 0" : "fails · exit 1";

  const terminal = $("terminal");
  terminal.replaceChildren(el("span", { className: "prompt", textContent: "$ " }), "flint\n");
  const flints = flintFiles();
  // With one .flint file flint prints paths relative to its folder; with
  // several, each block starts with the .flint file's path.
  let scope = flints.length === 1 ? parentOf(flints[0]) : "";
  for (const text of outcome.output.split("\n")) {
    const line = el("span", { textContent: text });
    if (flints.length > 1 && flints.includes(text)) scope = parentOf(text);
    linkify(line, scope);
    terminal.append(line, "\n");
  }
}

/** Makes the path a line of output names clickable, if it's a problem we know. */
function linkify(line, scope) {
  const text = line.textContent;
  const problems = state.outcome.problems;
  const candidates = (name) => [name, join(scope, name)];
  let target = null;
  let problem = null;

  const item = text.match(/^- (.+)$/);
  const parse = text.match(/^Failed to parse (.+?): line (\d+):/);
  if (item) {
    target = item[1];
    problem = problems.find(
      (p) =>
        (p.kind === "unexpected" || p.kind === "missing") && candidates(target).includes(p.path),
    );
  } else if (parse) {
    target = parse[1];
    problem = problems.find((p) => p.kind === "parse_error" && p.flint_file === target);
  }
  if (!problem) return;

  const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const at = node.data.lastIndexOf(target);
    if (at < 0) continue;
    const rest = node.splitText(at);
    rest.splitText(target.length);
    const link = el("button", { type: "button", className: "link", textContent: target });
    link.title = problem.kind === "parse_error" ? "Show the line" : "Show in the file tree";
    link.addEventListener("click", () => showProblem(problem));
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
    return button;
  });
  const blank = chip("Blank", null, loadBlank);
  blank.title = "Start from an empty .flint file";
  list.replaceChildren(...[...chips, blank].map((button) => el("li", {}, button)));
}

function renderExampleState() {
  for (const chip of $("example-list").querySelectorAll(".example-chip")) {
    const current = state.example ? chip.dataset.id === state.example.id : false;
    chip.setAttribute("aria-pressed", String(current));
  }
  const edited = state.pristine !== null && state.pristine !== treeJson();
  $("modified").hidden = !edited;
  $("reset").hidden = !edited || !state.example;
}

function loadTree(entries, { example = null, open = null } = {}) {
  state.files = new Map();
  for (const { path, directory, contents } of entries) addEntry(path, directory, contents ?? "");
  state.example = example;
  state.selected = null;
  state.collapsed = new Set();
  state.editing = null;
  state.open = open && state.files.has(open) ? open : null;
  state.pristine = treeJson();
  ensureOpen();

  $("explanation").hidden = !example;
  $("split-explanation").hidden = !example;
  if (example) {
    $("explanation-title").innerHTML = inlineMarkdown(example.title);
    $("explanation-body").innerHTML = markdown(example.explanation);
  }
  relint();
}

function loadExample(example) {
  loadTree(example.tree, { example });
  history.replaceState(null, "", `#${example.id}`);
}

function loadBlank() {
  loadTree([
    {
      path: FLINT,
      directory: false,
      contents: "# What's allowed in this folder, one rule per line\n",
    },
  ]);
  history.replaceState(null, "", location.pathname);
}

$("reset").addEventListener("click", () => state.example && loadExample(state.example));

/** One line of the small Markdown subset examples use, as HTML. */
function inlineMarkdown(raw) {
  const codes = [];
  let html = escapeHtml(
    raw.replace(/`([^`]+)`/g, (_, code) => `\uE000${codes.push(code) - 1}\uE000`),
  );
  html = html
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*\s][^*]*)\*/g, "<em>$1</em>")
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2">$1</a>');
  return html.replace(/\uE000(\d+)\uE000/g, (_, i) => `<code>${escapeHtml(codes[i])}</code>`);
}

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
// Version 1: the JSON { tree: [{ path, directory, contents }], open },
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
  // Hand-edited or truncated, a link could hold a tree the page can't
  // show; flint itself rejects any path a real tree couldn't have.
  if (!Array.isArray(shared?.tree) || JSON.parse(lint(JSON.stringify(shared.tree))).error) {
    throw new Error("That link's tree is malformed.");
  }
  return shared;
}

$("share").addEventListener("click", async () => {
  const edited = state.pristine !== treeJson();
  const hash =
    state.example && !edited
      ? state.example.id
      : `tree=${await encodeShare({ tree: JSON.parse(treeJson()), open: state.open })}`;
  const url = `${location.origin}${location.pathname}#${hash}`;
  history.replaceState(null, "", `#${hash}`);
  try {
    await navigator.clipboard.writeText(url);
    toast("Link copied — it holds the whole tree, so it opens exactly this.");
  } catch {
    prompt("Copy this link:", url);
  }
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
