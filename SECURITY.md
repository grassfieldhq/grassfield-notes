# Security Policy

Grassfield Notes stores personal notes, so we take security problems seriously.

## Supported versions
Security fixes go into the latest released version. Please update before reporting.

## Reporting a vulnerability
**Please don't open a public issue for security problems.**
Use GitHub's private vulnerability reporting instead: open the repository's **Security** tab and choose **Report a vulnerability** ([direct link](https://github.com/grassfieldhq/grassfield-notes/security/advisories/new)). Please include:
- what the problem is and what an attacker could do with it,
- the steps or a sample file or repository needed to reproduce it,
- the version of Grassfield Notes and your operating system.

You should get an answer within a week. We'll keep you informed while we fix it, and credit you in the release notes if you'd like.

## Scope
Areas we especially want to hear about:
- **Sync**: content from a git repository (note files, attachment names, links) is untrusted. It must never make the app read or write outside its data folder.
- **Opening files**: Markdown files, pictures and PDFs opened or attached by the user, and the links and pictures they contain.
- **Exports**: generated PDF, DOCX, ePub, HTML, JPG and TextBundle files.
- **The sync token**: it must never appear on a command line or in a file.

## How the app protects you
These are the rules the code follows. The audit of 2026-11-08 and the changes it led to are described in [docs/SECURITY-AUDIT-2026-11-08.md](docs/SECURITY-AUDIT-2026-11-08.md).

- **Sync repository.** git runs with `core.symlinks=false`, so a link committed to the repository is checked out as a plain file. Every file read from the repository must be a regular file (checked with lstat, opened with `O_NOFOLLOW`) whose real path stays inside the repository. Every file the sync writes replaces its target with an atomic rename, so a link is never written through.
- **Sync token.** It is kept in the system keyring (the Secret Service on Linux, the Keychain on macOS, the Credential Manager on Windows), never in `library.json`. git receives it through `GIT_ASKPASS` (the app answers git's question itself, and only for the repository's own server), never in its arguments, and no git credential helper may store it. A token saved in plain text by version 1.0.0 is moved to the keyring at the next start.
- **Data folder.** The data folder is private to your user (mode 0700) and `library.json` is readable by you only (0600).
- **SSH host keys.** With an SSH address, an unknown server key is refused rather than accepted on first use. Before the first sync, check the server once in a terminal:

  ```
  ssh -T git@github.com
  ```

  Compare the fingerprint it shows with the one the service publishes (for GitHub: *GitHub's SSH key fingerprints* in the GitHub documentation) and answer `yes`; ssh then remembers it in `~/.ssh/known_hosts`. If the key is still unknown when you sync, the error message says so.
- **Links and attachments.** Web and mail links open only after you see the full address. A link to a file opens without asking only when it is a Markdown file or a picture in the same folder as the file; anything else is named to you first. Only pictures and PDFs (by name and by content, and not executable) are handed to your default apps; other files are shown in their folder instead.
- **Pictures in documents.** A picture path written in a Markdown file is only looked up inside that file's folder, and only regular picture files of bounded size are read. The same rule applies to exports and to *Add to my notes*.
- **Large or hostile files.** Styling runs in linear time and is capped per line; files over 1 MiB open read-only and without styling; named pipes and devices are never opened.
- **Saving Markdown files.** Files are saved atomically (temporary file, flush, rename) with their permissions kept, never through a link, and never over changes made by another program without asking you.
- **Text reading.** `tesseract` and `pdftotext` run on pictures and PDFs only, two at a time, with memory, CPU-time and time limits.

## Known design choices (not vulnerabilities)
- Your notes and attachments are stored **unencrypted** in your data folder (see the README). Protect that folder like any other personal file; use full-disk encryption for protection at rest.
- Use a token limited to a single repository with *Contents: read and write* only.
- Anyone with push access to your sync repository can change or delete your notes; that is what sync means. They cannot make the app touch files outside its data folder.
- Text reading and sync call the external programs `tesseract`, `pdftotext` and `git` installed on your computer.
