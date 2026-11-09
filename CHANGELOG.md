# Changelog

All notable changes to Grassfield Notes are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Security
Fixes for the security audit of 2026-11-08; details in [docs/SECURITY-AUDIT-2026-11-08.md](docs/SECURITY-AUDIT-2026-11-08.md).
- Sync never follows links committed to the repository, and never reads or writes outside its folders.
- The GitHub token moves to the system keyring and reaches git through `GIT_ASKPASS`, never its command line; a token saved by 1.0.0 is moved at the next start. The data folder is private (0700, library 0600).
- Styling is linear in the text size and capped; files over 1 MiB open read-only and plain; named pipes are never opened.
- Web links, and files outside a document's folder, open only after you see them; only pictures and PDFs open in other apps, other attachments are shown in their folder.
- Picture paths in documents, exports and *Add to my notes* stay inside the document's folder and must be real pictures.
- Text reading runs two documents at a time, with memory, CPU and time limits.
- Markdown files are saved atomically with their permissions, never through a link, and a change made by another program is never overwritten without asking (*Keep mine / Keep theirs / Save as*).
- SSH sync refuses unknown host keys.
- Renaming tags with letters such as "İ" no longer crashes; note ids can no longer overflow.

### Added
- The desktop's own Open and Save panels (XDG desktop portal), with the built-in chooser as a fallback.
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
