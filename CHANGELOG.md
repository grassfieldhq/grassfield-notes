# Changelog

All notable changes to Grassfield Notes are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Release builds for all three systems: a Linux archive with icon and desktop file, a Windows zip, and a universal macOS `Grassfield Notes.app` (Apple Silicon and Intel).
- Windows: *Open .md files with Grassfield Notes* registers the app in *Open with*.
- macOS: shortcuts are shown with ⌘.
- `--version` and `--write-icon <file.png>` command-line options.

### Fixed
- Opening links, attachments, images and folders did nothing on every system. They now open with the system's default app.
- Windows: running git, tesseract or pdftotext no longer flashes a console window.
- Markdown files with Windows line endings keep them when saved.
- Fonts are found in the standard macOS and Windows font folders, including per-user fonts.
- macOS: the extra `-psn_…` launch argument is no longer mistaken for a file to open.

## [1.0.0]
First public release.

### Added
- Three-column notebook: sections and tag tree, note list with previews, live Markdown editor.
- Live styling for headings, bold, italic, strikethrough, highlight, inline and block code, quotes, lists, dividers and links; clickable to-do checkboxes; `[[note links]]` with backlinks.
- Tags with nesting, renaming and removal; Pinned, To-do, Today, Untagged, Archive and Trash sections; search by words or `#tag`.
- Sketches (pen, pencil, highlighter, ruler, eraser) that stay editable; inline images with crop, rotate and resize.
- Document scanning: photos, scans and PDFs attached to notes, with their text read in the background (Tesseract, pdftotext) and included in search.
- Standalone Markdown files: open any `.md` from the command line, by drag and drop or with `Ctrl+O`; reading and editing modes, tabs, autosave, change detection, links between files, and *Add to my notes*.
- Export to TXT, Markdown, TextBundle, RTF, PDF, JPG, HTML, DOCX and ePub, with pictures where the format allows.
- Two-way sync with a GitHub (or any git) repository, one Markdown file per note.
- 28 themes, 15 app icons, editor font, size, spacing and width settings.
- Interface in English, French, Spanish, German, Italian and Portuguese.
- Single-window protection for the notebook, with a read-only second window.

### Security
- Attachment names coming from a sync repository are restricted to plain file names, and git commands keep addresses and branch names from being read as options.
