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

## Scope and known design choices
Areas we especially want to hear about:
- **Sync**: content from a git repository (note files, attachment names) is treated as untrusted and must never write outside the app's data folder.
- **Opening files**: Markdown files, pictures and PDFs opened or attached by the user.
- **Exports**: generated PDF, DOCX, ePub, HTML and ZIP files.

Known design choices (not vulnerabilities):
- Your notes and the optional GitHub access token are stored **unencrypted** in your local data folder (see the README). Protect that folder like any other personal file, and use a token limited to a single repository.
- Text reading and sync call the external programs `tesseract`, `pdftotext` and `git` installed on your computer.
