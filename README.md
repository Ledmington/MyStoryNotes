# MyStoryNotes

An offline note-taking app for planning stories, built around a graph of
linked notes rather than a flat list.

Write your notes in Markdown, link them to each other, and MyStoryNotes
automatically lays out a live, physics-based graph of how everything
connects — characters to locations, plot points to each other, and so on.

## Features

- **Offline-only.** No accounts, no sync, no network access of any kind —
  the one exception is that clicking a link to a web address opens it in
  your browser, a separate program the app itself never talks to.
- **A project is a single file.** Everything — every note and its content —
  is saved as one human-readable file (`.mystorynotes`, plain TOML) you can
  read, diff, back up, or put under version control yourself.
- **Markdown notes**, rendered live or edited as raw text, your choice per
  note.
- **Automatic graph view.** Any Markdown link from one note to another
  (`[Bob](Bob)`) becomes an edge in a graph view of the whole project. The
  graph runs a real-time force simulation: linked notes are pulled closer
  together, unrelated notes drift apart, and a note's outgoing links spread
  out around it so the whole thing stays readable as it settles.
- **Themeable UI**, with a few built-in color themes and full control over
  every color if you want to make your own.
- **A single static executable.** No installer, no runtime dependencies to
  manage — build it once and run the binary.

## Getting started

MyStoryNotes isn't published as a binary release yet, so you'll need a Rust
toolchain to build it from source.

### Prerequisites

- Rust 1.95.0 or newer (this is the minimum supported Rust version, tracked
  via `rust-version` in `Cargo.toml`; CI also tests against 1.97.0)
- On Linux, the usual system libraries needed to build a windowed GUI app:
  `libgtk-3-dev`, `libxkbcommon-dev`, `libx11-dev`, `libxi-dev`,
  `libxrandr-dev`, `libxcursor-dev`, `libxinerama-dev`, `libwayland-dev`,
  `libgl1-mesa-dev` (package names as on Debian/Ubuntu)

### Build and run

```bash
git clone <this-repository-url>
cd my_story_notes
cargo run --release
```

The compiled binary ends up at `target/release/my_story_notes` and can be
copied anywhere and run on its own.

## Usage

- **New Project** / **Open Project** / **Save Project** manage the current
  `.mystorynotes` file from the toolbar.
- **+ New Note** creates a note; click any note in the sidebar to open it.
- Each open note toggles between a rendered Markdown view and a raw editing
  view.
- Link one note to another with a normal Markdown link whose destination is
  the other note's exact name, e.g. `[my sister](Alice)` to link to a note
  named "Alice". Linked notes automatically show up connected in the graph
  view.
- A Markdown link whose destination is a `http://` or `https://` address,
  e.g. `[source](https://en.wikipedia.org/wiki/Cartography)`, opens in your
  browser instead — it's just not treated as a link to another note.
- The graph view opens alongside a note, or fills the screen when no note is
  open; click any node to open that note.

### Keyboard shortcuts

`Ctrl` below is `Cmd` on macOS.

- `Ctrl+N` — New Project
- `Ctrl+O` — Open Project
- `Ctrl+S` — Save Project
- `Ctrl+M` — New Note (while a project is open)

While editing a note, these wrap the selected text (or, with nothing
selected, insert an empty pair and place the cursor ready to type).
Pressing the same shortcut again on the same selection removes the
formatting instead of doubling it up.

- `Ctrl+B` — **bold**
- `Ctrl+I` — *italic*
- `Ctrl+U` — <u>underline</u>
- `Ctrl+E` — `verbatim` (inline code)
- `Ctrl+K` — hyperlink

Also while editing a note, `Ctrl+Z` undoes and `Ctrl+Shift+Z` (or `Ctrl+Y`)
redoes — both typed text and applied formatting. Undo history is kept
per note for as long as the app stays open, but isn't saved with the
project.

An example project with sample notes is included at
[`tests/fixtures/example_project.mystorynotes`](tests/fixtures/example_project.mystorynotes)
— open it from the app to see the format and the graph view in action. It
doubles as a fixture for the integration tests (see Development below).

## Project file format

A project is a single [TOML](https://toml.io) file: a list of notes, each
with a `name` and its Markdown `source`. There's no hidden state or
external assets — the file is the whole project.

## Development

```bash
cargo build              # debug build
cargo build --release    # release build
cargo run                # run in debug mode
cargo run --release      # run in release mode
cargo test                # run the test suite
cargo clippy --all-targets --all-features   # lint
cargo doc --no-deps       # check documentation (e.g. broken intra-doc links)
cargo fmt                 # format
```

`cargo test` runs both unit tests (next to the code they test) and the
integration tests under [`tests/`](tests), which exercise the app through
its public API using fixture project files from
[`tests/fixtures/`](tests/fixtures).

See [`CLAUDE.md`](CLAUDE.md) for the project's requirements and code style
guidelines.

### Running in Docker

[`etc/Dockerfile`](etc/Dockerfile) builds a container image as an
alternative to installing the system libraries above locally. Build it from
the repository root (it needs the whole source tree, not just `etc/`):

```bash
docker build -t my_story_notes -f etc/Dockerfile .
```

MyStoryNotes is a windowed GUI app, so running the container still needs
access to a display server on the host. On Linux with X11, forward the X11
socket and grant the container access to it:

```bash
xhost +local:docker
docker run --rm -it \
  -e DISPLAY=$DISPLAY \
  -v /tmp/.X11-unix:/tmp/.X11-unix:ro \
  -v "$HOME:/root" \
  my_story_notes
```

Mounting `$HOME` to `/root` (the container's default user's home) makes
your own `.mystorynotes` project files reachable from the in-app file
dialogs.

## License

MIT — see [`LICENSE`](LICENSE).
