# Contributing to Grassfield Notes

Thanks for wanting to help! All of these are welcome: bug reports, ideas, translations, themes, documentation and code.
By taking part you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help
- **Report a bug**: open an issue with the *Bug report* template. Include your system, the steps that lead to the problem, and what you expected to happen.
- **Suggest a feature**: open an issue with the *Feature request* template. Describe the problem you want solved before the solution you have in mind.
- **Translate**: improve an existing language or add a new one (see below). Native speakers reviewing the current translations would be very helpful.
- **Code**: pick an issue labelled `good first issue` or `help wanted`, say in the issue that you're working on it, then open a pull request.

## Development setup
1. Install stable Rust with [rustup](https://rustup.rs), plus the `clippy` and `rustfmt` components:
   ```sh
   rustup component add clippy rustfmt
   ```
2. On Linux, install the windowing and OpenGL headers (Debian/Ubuntu: `sudo apt install libxkbcommon-dev libgl1-mesa-dev libwayland-dev libx11-dev`).
3. Optional, to work on sync or scanning: install `git`, `tesseract` (with language data) and `poppler` (`pdftotext`).

```sh
cargo run                        # start the app
cargo run -- path/to/file.md     # open a Markdown file on its own
cargo test                       # run the test suite
cargo clippy --all-targets -- -D warnings
cargo fmt
```

### Keep your real notes safe while developing
Point the app at a throwaway library:

```sh
GRASSFIELD_DATA=/tmp/gf-dev/library.json cargo run
```

### Screenshot mode
`--screenshot <file.png>` starts the app with built-in sample notes in a temporary folder, draws about 120 frames, saves a PNG and exits. It never touches your real notes. Add `GRASSFIELD_SHOT_STATE` (comma-separated) to capture a specific screen:

| Value | Shows |
|---|---|
| `en`, `fr`, `es`, `de`, `it`, `pt` | interface language |
| `dark`, `night` | a dark theme |
| `info`, `focus`, `trash`, `tag` | info panel, focus mode, trash, a tag filter |
| `settings`, `lang`, `sync`, `icons`, `export` | settings pages |
| `picker`, `attach`, `crop`, `sketch` | file chooser, inline picture, crop window, sketch window |
| `edit` | editing mode for Markdown files given on the command line |

```sh
GRASSFIELD_SHOT_STATE=en,dark cargo run -- --screenshot /tmp/dark.png
```

Please update the pictures in `docs/screenshots/` if your change alters the look of a screen shown in the README.

## Project layout
```
src/
  main.rs          start-up, fonts, command line (--screenshot, files to open)
  model.rs         notes, tags, filters, settings, saving library.json
  markdown.rs      live Markdown styling for the editor, HTML export
  theme.rs         the 28 themes
  i18n.rs          languages; i18n_a/b/c/d.rs hold the translation tables
  attach.rs        attachments, text reading (tesseract / pdftotext), thumbnails
  sketch.rs        sketch data, drawing into PNG
  export.rs        TXT, Markdown, TextBundle, RTF, DOCX, ePub (+ a small ZIP writer)
  export/render.rs PDF (embedded fonts) and JPG rendering
  sync.rs          GitHub sync through the git program
  icons.rs         the 15 app icons, drawn in code
  fonts.rs         finds system fonts for the window and for exports
  filepicker.rs    built-in file chooser
  app.rs           application state, actions, shortcuts, main frame
  app/             one file per part of the window: sidebar, list, editor, overlay
                   (check boxes / pictures over text), panels, settings, attachments,
                   sketcher, imageedit, files (standalone Markdown files)
  tests.rs         unit and integration tests
packaging/         desktop entry for Linux
docs/screenshots/  pictures used in the README
```

A few design notes:
- **Editor**: notes are edited in an egui `TextEdit` with a custom layouter (`markdown::layout_job`). Checkboxes, quote bars, rules and inline pictures are drawn on top of the text in `app/overlay.rs`, so the stored text is always plain Markdown.
- **Pictures inline**: a line `![](file)` gets a tall, transparent row in the layout, and the image is painted over it.
- **Background work**: OCR, PDF text extraction and git sync run on threads, and results come back through channels. Never block the UI thread on the network or on external programs.
- **Saving**: the library is written to a temporary file, then renamed into place, about one second after the last change. Only one window owns the library (`instance.lock`).

## Translations
All interface text is written in French in the code and wrapped in `crate::i18n::tr("…")` (or `trf("… {name} …", &[("name", &value)])` for text with values). Translations live in `src/i18n_a.rs` … `src/i18n_d.rs` as rows of `[fr, en, es, de, it, pt]`.

- **To fix a translation**: edit the matching row.
- **To add text**: wrap it with `tr`/`trf` and add a row with all six columns. Keep `{placeholders}` identical in every column.
- **To add a language**: add a variant to `Lang` in `i18n.rs` (with `native_name`, `code` and `ocr_code`), add it to `ALL`, then add a column to every row. The test `translations_are_complete_and_fill_placeholders` fails if a cell is missing.

## Themes and icons
- Themes are in `src/theme.rs`. Either hand-write a `Theme` in `BASE`, or add a line to the `extra` list (name, dark?, dark sidebar?, tint colour, accent colour) and let `derive` compute the rest.
- App icons are drawn in `src/icons.rs`. Add an `IconStyle` to `ICONS`.

## Coding guidelines
- `cargo clippy --all-targets -- -D warnings` and `cargo test` must pass, and CI checks both. Format your code with `cargo fmt` (settings in `rustfmt.toml`). The CI formatting check is advisory until the whole code base has been formatted once.
- Keep files focused. If a file grows past a few hundred lines, split it the way `src/app/` is split.
- Comments explain *why*, not *what*.
- Don't add a dependency when a small amount of code will do, and explain any new dependency in the pull request.
- Never lose user data: write files atomically, don't overwrite without being sure, and treat content coming from sync or from files as untrusted (see `sync::safe_file_name`).
- New features that work on data should come with a test in `src/tests.rs`.

## Pull requests
1. Fork the repository and create a branch from `main` (`fix/…`, `feat/…`, `i18n/…`).
2. Keep each pull request to one topic, with clear commit messages (for example `fix: keep cursor when switching notes`).
3. Fill in the pull request template: what changed, why, how you tested it, and screenshots for anything visible.
4. A maintainer will review it. Please be patient and open to suggestions; reviewers try to be kind and quick.

## Releases (maintainers)
1. Update `CHANGELOG.md` and the version in `Cargo.toml`.
2. Tag the release: `git tag v1.2.3 && git push --tags`.
3. The *Release* workflow builds Linux, Windows and macOS binaries and attaches them to a draft GitHub release. Review the draft and publish it.
