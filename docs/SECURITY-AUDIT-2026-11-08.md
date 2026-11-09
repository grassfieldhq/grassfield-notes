# Security audit of 2026-11-08: findings and fixes

The first security audit of Grassfield Notes, run on 2026-11-08 before the app ships as the
built-in Notes app of the Grassfield desktop, failed at commit `2260d9a` with the verdict
"fixable; re-audit after H1–H3 and M1–M4". This page lists each finding, what was changed,
and the commit that changed it. File and line references in the findings are to `2260d9a`.

All fixes are on the branch `fix/security-audit`. Each commit was built and tested on its own
(`cargo test`, no warnings); the final state builds with `cargo build --release --locked`
without warnings and passes `cargo test`.

## High

### H1. Sync followed links committed to the repository
*sync.rs:141-152, 238-274.* A collaborator with push access could commit links that made a
sync read local files into the notes (and push them on the next sync) or write outside the
notebook (startup or shell files).

Fixed in `bec4cf5`:
- git always runs with `-c core.symlinks=false`: a committed link is checked out as a small
  plain file holding the link's text.
- Every file read from the repository is checked with lstat (regular files only, every folder
  on the way a real folder), opened with `O_NOFOLLOW | O_NONBLOCK` and checked again with
  fstat after opening, and its canonical path must stay inside the repository folder. This
  also covers links left by a checkout made before the fix.
- Every file written into the repository or the library goes to a new temporary file
  (created with `O_EXCL | O_NOFOLLOW`) in the same folder and is renamed over its target: a
  link at that name is replaced, never written through. The folders written into
  (`notes/`, `attachments/`, `attachments/<id>/`) are made real folders first, and removing a
  deleted note's folder removes a link itself, never what it points to.
- Note and attachment sizes are capped (16 MB and 512 MB).
- Tests with a hostile repository: a note that is a link to a stand-in `~/.bashrc`, an
  attachment that is a link to a secret file, an attachment folder that is a link to an
  outside folder, a `README.md` link, a whole `attachments` link (second round), and a note
  naming an attachment `../../.bashrc`; plus a test with real links already in the local copy
  (`sync_never_follows_links_from_a_hostile_repository`,
  `links_already_in_the_local_copy_are_refused`). Run against the sync code of `2260d9a`, the
  first test fails: the committed `README.md` link is written through into the stand-in
  `.bashrc`.

### H2. The sync token was exposed
*sync.rs:41-50, 220, 301; model.rs:311, 428-437.* The token was in git's command line
(visible to every local user in `ps`) and in plain text in `library.json` (mode 0644, folder
0755).

Fixed in `c279e10`:
- The token lives in the system keyring, through `keyring-core` with the
  `zbus-secret-service-keyring-store` (Secret Service, as libsecret uses) on Linux, the
  Keychain on macOS and the Credential Manager on Windows. Keyring calls run on background
  threads, so an unlock prompt never freezes the window. Tests use an in-memory store and
  never touch the keyring of the person running them.
- git gets the token through `GIT_ASKPASS`: git runs the app itself in a small helper mode
  (`GRASSFIELD_NOTES_ASKPASS`) that prints the user name or the token, which travels in the
  environment of git and its helpers (readable by the same user only), never in `argv`. The
  repository address no longer carries the token. The helper answers only for the
  repository's own server, so a git-lfs server named by the repository gets nothing (and LFS
  downloads are skipped). With a token, `credential.helper` is reset for that git call, so no
  credential helper stores a copy.
- `library.json` never contains the token any more (`skip_serializing`). A token saved in
  plain text by version 1.0.0 is read once at start, moved to the keyring, and the library is
  rewritten without it at once. If the keyring is unavailable, the token is kept in memory
  for the session only and the settings say so. A `FETCH_HEAD` that still holds a token in
  its address is deleted.
- The data folder is made 0700 and `library.json` (and its `.bak`) 0600; the library is
  written through a 0600 temporary file and an atomic rename.

### H3. A small Markdown file froze the app
*markdown.rs:198, 293-343; files.rs:39.* Styling cost grew faster than the file size, on the
window's thread, without any cap (one 48 KB line took 26.7 s).

Fixed in `cc91fa1`:
- Inline styling is linear: every forward search (closing backtick, `]]`, `]`, `)`, closing
  `**`/`*`/`~~`/`==`/`__`) jumps through a precomputed "next match" table, and byte offsets
  are mapped to characters by binary search. The rules are unchanged.
- A line is styled up to 20,000 bytes and shown plain after that; a text over 1 MiB is shown
  without styling.
- Files over 1 MiB open read-only and plain, with a notice; files over 32 MiB are not opened;
  only regular files are opened (a named pipe no longer hangs the window).
- Tests: a 1 MB single line of openers that never close is laid out for the editor in about
  2 ms (limit 500 ms), and fully styled without the line cap in about 0.7 s on a heavily
  loaded machine in a debug build (limit 3 s; the old code needed 26.7 s for 48 KB)
  (`styling_a_one_megabyte_line_is_fast`), plus tests for the caps, the read-only mode and
  named pipes.

## Medium

### M1. One click opened any local path or attachment
*files.rs:610-625, app.rs:498-501.* In Reading mode a click passed any path (absolute or
`../`) to `xdg-open`, and synced attachments of any type opened the same way.

Fixed in `f9dc61f`:
- `http`, `https` and `mailto` links (in Markdown files and in notes) open only after a
  confirmation that shows the full address.
- Other links open without asking only when they are Markdown files (opened in the app) or
  pictures inside the file's own folder; anything else (absolute paths, `..`, other types)
  asks first, naming the file; other schemes (`file:`, `javascript:`, `smb:`…) are refused.
- A file is handed to the default app only when its name and its content (magic bytes) both
  say picture or PDF and it is not executable. Attachments of any other type, and files that
  fail that check after confirmation, are shown in the file manager instead of opened.

### M2. Picture paths could leave the file's folder
*files.rs:123-131, 204-215; markdown.rs:518; export.rs:398.*

Fixed in `1024322`:
- Picture and link paths written in a document resolve only inside the document's folder: no
  absolute path, no `..`, and the canonical path must stay inside (`safefs::confine`).
- Pictures are read only from regular files (no named pipes or devices) of at most 64 MB whose
  content is a picture; decoding has dimension and memory limits.
- "Add to my notes" copies only such pictures; attaching accepts only pictures and PDFs by
  content. The HTML, ePub, PDF and JPG exports follow the same rule.

### M3. Text readers ran on every attachment at once, without limits
*attach.rs:81-87, 130-147.*

Fixed in `cebf9e4`:
- A queue with at most two readers at a time.
- Each `tesseract`/`pdftotext` run gets `setrlimit` limits in the child (1.5 GB address
  space, 90 s CPU, no file writes, no core dump), a 90 s wall-clock timeout after which it is
  killed, `OMP_THREAD_LIMIT=1`, and at most 4 MB of text is kept.
- Only regular files whose content is a picture or a PDF are read, up to 64 MB.

### M4. Auto-save wrote in place, followed links and saved over conflicts
*files.rs:73-84, 164-173; app.rs:925-929.*

Fixed in `3f1cb17`:
- Saving writes a temporary file in the same folder, flushes it to disk and renames it over
  the file, keeping the file's permissions.
- Links: the path is resolved once, when the file is opened, and saving writes only to that
  resolved file. If a link has appeared at that path since, saving is refused (a link is never
  written through nor silently replaced). This choice is written in `write_file`.
- A detected conflict is never saved silently: closing a file, `Ctrl+S` and quitting ask
  *Keep mine / Keep theirs / Save as… / Cancel*; quitting is held back until every conflict is
  settled. Auto-save checks the file on disk right before writing. If the app ends without a
  chance to ask, the edits go to a new file next to the original ("name (my version).md").

## Low

- **Renaming a tag such as `#İİ/€` crashed** (model.rs:603): tag names are matched character
  by character on the original text (`tag_match_len`), never by offsets into a lowercased
  copy. Fixed in `348038f`, with a test.
- **Id overflow in debug builds** (model.rs:442): checked arithmetic, and an id is never handed
  out twice even after a synced note carried `u64::MAX`. Fixed in `348038f`, with a test.
- **Attribute injection in exported HTML/ePub** (markdown.rs:523, export.rs:402): HTML
  escaping now covers quotes, and the ePub picture name and type come from the picture's
  content (fixed extension and MIME type), never from the text. Fixed in `1024322`.
- **SSH accepted unknown host keys** (sync.rs:57): `StrictHostKeyChecking=yes`. The simplest
  safe option was chosen: the user checks the server's fingerprint once in a terminal
  (`ssh -T git@github.com`, compared with the fingerprints the service publishes), which adds
  it to `~/.ssh/known_hosts`; when the key is unknown the sync error explains exactly that.
  Written down in SECURITY.md. Fixed in `a2c5354`.
- **Grassfield integration: the app used its own file picker.** Open, attach, scan and import
  use the XDG desktop portal FileChooser (`ashpd`, on the same D-Bus stack as the keyring), so
  Grassfield's own Open and Save panels appear; exports use the Save panel (a folder panel for
  several notes), and so does "Save as…" in a file conflict. When no portal answers, the
  built-in chooser, the default export folder and the "(my version)" copy are used, for the
  rest of the session. Fixed in `cc1ce7e`.

## Re-audit of 2026-11-09 (at `e9ee20c`): PASS WITH CONDITIONS

The re-audit confirmed H1–H3, M1–M4 and the lows fixed, and raised one condition (C1) and
eight smaller points. All are fixed on the same branch:

- **C1 / N1, the token over plain HTTP** (sync.rs:66-70, 88-101, 109-118). The askpass
  helper now answers only a prompt whose address is `https://` and whose server is the
  repository's own; an `http://` prompt, a prompt without a scheme and another server get
  nothing. A sync to an `http://`, `git://` or any other non-https, non-SSH address is
  refused before anything is sent, with a message; SSH addresses and local folders still
  sync (they never use the token). Fixed in `854253b`, with tests for each case.
- **N2, the token in the environment of every program git starts.** The token is written
  to a 0600 file in a private 0700 folder (`grassfield-notes/` in the runtime directory,
  else in the private data folder), removed when the sync ends; git and the programs it
  starts only see the file's path, set only for `fetch` and `push`; an inherited token
  variable is cleared. Fixed in `e7c5776`. (Its commit message says the check was made with
  the release binary; it was the debug build of the same code.)
- **N3, unbounded sketch data.** Sketch data is read as a regular file of at most 16 MB and
  sanitized: size clamped to the canvas, finite widths and points, at most 100,000 points
  and 10^8 pixel visits of drawing work; drawing clamps the picture to the canvas and each
  segment only visits its own box. Fixed in `12a032c`.
- **N4, about 0.8 s per keystroke on a hostile text just under the cap.** A per-line cache
  restyles only the line a keystroke changed, the layout is assembled by format index, and
  styling stops at 512 KiB of text (files still open for editing up to 1 MiB). Measured on
  this loaded machine (load average 12–14) in a debug build, on about 512 KiB of different
  hostile lines: first styling 355–455 ms, then a keystroke 36–57 ms (the test's limit is
  150 ms); a release build measured 51 ms at 1 MiB before the cap was halved. Fixed in
  `f54f21d`.
- **N5, side effects of atomic saves.** The new file gets the old one's owner and group, its
  extended attributes (on Linux, POSIX ACLs included) and its mode. When that is not
  possible (a folder that is not writable, an owner that cannot be given to a new file), the
  file is written in place after a private backup of its old content, removed once the new
  text is on disk. Fixed in `1ed57a2`.
- **N6, chmod of any GRASSFIELD_DATA folder.** Only a folder the app creates is made 0700,
  plus its own default folder (created 0755 by 1.0.0) when it belongs to the user; this runs
  once at start. Fixed in `f81235b`.
- **N7, export edge cases.** The name chosen in the Save panel is kept as given (no extension
  added), and no second export starts while a panel is open. Fixed in `4d77bf7`.
- **N8, no portal timeout.** The FileChooser portal must report its version within 8 s, and a
  panel that never answers is given up after 15 minutes, with the built-in fallback. Fixed
  in `387e446`.
- **Integration.** The "Open .md files with Grassfield Notes" button is hidden in the
  Grassfield session (`GRASSFIELD_SESSION=1`), which sets the default itself. `5d2aac7`.
- **N9 and the recommendation.** Inherent to the Secret Service (no per-app access control).
  Anyone who synced with 1.0.0 should revoke and reissue their GitHub token: 1.0.0 kept it in
  plain text, and copies can survive in `library.json.bak` or in file-system snapshots.

## Not changed, and why

- **Notes in the library over 512 KiB stay editable.** They are shown without styling, but
  egui still lays the whole text out on the window's thread. Markdown files opened from disk
  are capped as described under H3.
- **The portal panels are not attached to the window.** egui does not expose the window
  handle the portal needs (`xdg-foreign` on Wayland), so the panels open as separate windows.
- **Only Linux was built and tested.** The macOS (Keychain) and Windows (Credential Manager)
  keyring stores and the askpass helper on those systems follow the same code paths but were
  not compiled here; the release workflow builds them.
- **Atomic saves give the file a new inode**, so hard links to a Markdown file keep the old
  text (owner, group, mode, extended attributes and ACLs are kept since `1ed57a2`).
- **`library.json.bak`** is only written when `library.json` cannot be read; one made by 1.0.0
  may still hold a plain-text token. It is now readable by its owner only; delete it if present.
- **Old `FETCH_HEAD`** files that hold a token are deleted at the next sync, not at start.
