# flint playground

A web page for trying flint in the browser: a pretend file tree on the
left, the `.flint` file being edited in the middle, and flint's output
below, re-checked on every keystroke. It runs flint itself, compiled to
WebAssembly, through the same library code the CLI runs over a real
disk — so it prints exactly what `flint` would for the same files.
Nothing typed into it leaves the browser.

It's published at <https://flint.viboko.dev/>.

It also holds the playground's examples, which double as tests (see
[below](#examples)).

## How it fits together

- [`src/lib.rs`](src/lib.rs) builds a `flint::memory_tree::MemoryTree`
  from the page's files, runs `flint::run::run` and
  `flint::output::render` over it, and turns the report into a list of
  problems the page can highlight — each with the path it's about and,
  where a rule is responsible, that rule's `.flint` file and line. The
  page calls it through two functions exported to JavaScript, `lint` and
  `examples`, exchanging plain JSON.
- [`web/`](web) is the page: plain HTML, CSS and JavaScript, with no
  dependencies and no build step of its own. Its editor is a `<textarea>`
  over a highlighted copy of its own text, so typing, selection and undo
  are the browser's own. The gaps between the panels are splitters:
  dragging one (or focusing it and pressing the arrow keys) resizes the
  panels either side, by setting a custom property the layout in
  [`web/style.css`](web/style.css) reads.
- [`web/favicon/`](web/favicon) is the page's icon set: `favicon.svg`,
  which is the logo in [`docs/images/flint-logo.svg`](../docs/images/flint-logo.svg)
  framed square, and the PNG files and `favicon.ico` rendered from it by
  `just playground-favicons` (which needs ImageMagick) — including the
  `apple-touch-icon.png` an iOS home screen shows.
- This crate has its own `Cargo.lock` and `target/` (it's its own Cargo
  workspace, like [`fuzz/`](../fuzz)), so the playground's dependencies
  — `wasm-bindgen` and `serde` — never show up in the CLI's lockfile,
  its dependency audit, its SBOM or its published package.

## Shared links

"Copy link" puts the whole tree in the link's fragment, so a link needs
no server and keeps working as long as the site exists:
`#tree=<version>.<data>`. Version 1's `<data>` is the JSON
`{ "tree": [{ "path", "directory", "contents" }], "open" }`, compressed
with raw DEFLATE and encoded as base64url without padding — compression
keeps a 200-file tree's link under 1,000 characters.

The version is what keeps old links opening: **never change what an
existing version means.** A new encoding gets the next version number
and its own decoder in `SHARE_DECODERS` in [`web/app.js`](web/app.js),
and the old decoders stay. A link only stores flint's inputs, so an old
link re-runs on whatever flint the site has now — if flint's behavior
has changed since, it shows the new behavior.

## Running it locally

```bash
just playground-serve
```

That builds the site into `target/site/` (`just playground-build` alone just
builds it) and serves it at <http://localhost:8001>. It needs the
`wasm32-unknown-unknown` Rust target and `wasm-bindgen-cli` at exactly
the version [`Cargo.lock`](Cargo.lock) pins for the `wasm-bindgen`
crate; `just install-tools` installs both, and `just playground-build` says
which version to install if it finds the wrong one.

## Publishing

[`playground.yml`](../.github/workflows/playground.yml) builds the site
on every PR that could change it, and on each push to `main` publishes
it to the root of the [GitHub Pages site](../docs/github-pages.md),
which also hosts coverage and benchmark reports. A PR's build is
published as a preview at `/pr-<N>/playground/` (same-repo PRs only),
and the PR gets a comment linking to it; it's removed when the PR
closes. Every path in the playground is relative, so it works there
unchanged.

The site's domain, `flint.viboko.dev`, is set by [`web/CNAME`](web/CNAME):
with Pages deploying from a branch, GitHub keeps a custom domain as a
`CNAME` file at the branch's root, and since each publish replaces
everything there but `pr-*/`, `coverage/` and `benchmarks/`, the
playground has to bring its own. The
canonical link and Open Graph tags in [`web/index.html`](web/index.html),
[`web/robots.txt`](web/robots.txt) and [`web/sitemap.xml`](web/sitemap.xml)
all name the same domain; `just seo` checks they agree (see
[`docs/tooling.md`](../docs/tooling.md)). `robots.txt` keeps search
engines out of the reports and PR previews.

## Examples

The playground's example buttons load the examples in
[`examples/`](examples). Each is checked by `just test`, which runs
flint over it and fails if its documented output is wrong, so they
can't go stale.

Each example is one Markdown file in [`examples/`](examples), shown in
file name order — so name them with a two-digit prefix, like
`03-required.md`. It reads as normal Markdown on GitHub, and follows a
fixed shape so it can also be loaded and tested:

````markdown
# Required entries

A short explanation of what the example shows. Plain Markdown: this is
everything between the title and the first `##` section.

## Files

```text
README.md
src/
src/main.rs
```

## .flint

```text
!README.md
!LICENSE
src/
```

## Output

```text
Missing 1 required entry:
- LICENSE
```
````

- **`## Files`** lists every file and folder except `.flint` files, one
  path per line. A trailing `/` makes a folder; folders above a listed
  path are added automatically, so list a folder only to show it empty.
- **One section per `.flint` file**, headed by its path: `## .flint` for
  the root, ``## `src/.flint` `` for a nested one.
- **`## Output`** is exactly what flint prints, stdout and
  stderr interleaved as they'd appear in a terminal.

Each section holds just one fenced code block. To fill in or update an
example's output, run the tests (`just test`) and copy the actual output
from the failure — after checking it really is what the example should
show.
