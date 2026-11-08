//! Two-way sync of notes with a GitHub (or any git) repository, using the `git` program.
//!
//! Each note is one Markdown file `notes/<id>-<title>.md` with a small header holding its
//! dates and flags; attachments go to `attachments/<id>/`. The newest edit of a note wins.
//!
//! The repository is shared with other people and computers, so its content is untrusted:
//! - git runs with `core.symlinks=false`, so a committed link is checked out as a small
//!   plain file holding the link's text, never as a link;
//! - every file read from the repository is checked with lstat and opened with
//!   O_NOFOLLOW (only regular files are accepted), and its real path must stay inside the
//!   repository folder: a link left by an older checkout is never followed either;
//! - every file written into the repository or the library replaces its target with an
//!   atomic rename (a link at that name is replaced, never written through), and the
//!   folders written into are checked to be real folders first.

use crate::model::{now, Attachment, Library, Note, SyncSettings};
use crate::safefs::{self, Links};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

pub struct Merge {
    /// Notes added or changed on the other side.
    pub upserts: Vec<Note>,
    /// Notes deleted on the other side.
    pub deletes: Vec<u64>,
    pub synced_ids: BTreeSet<u64>,
    /// Each local note's modification time when the sync started.
    pub base_modified: HashMap<u64, i64>,
}

pub struct Outcome {
    pub ok: bool,
    pub message: String,
    pub merge: Option<Merge>,
}

pub struct Job {
    rx: Receiver<Outcome>,
}

impl Job {
    pub fn poll(&self) -> Option<Outcome> {
        self.rx.try_recv().ok()
    }
}

pub fn repo_dir() -> PathBuf {
    Library::data_dir().join("sync-repo")
}

fn auth_url(s: &SyncSettings) -> String {
    let url = s.repo_url.trim().to_string();
    let token = s.token.trim();
    if !token.is_empty() {
        if let Some(rest) = url.strip_prefix("https://") {
            return format!("https://x-access-token:{token}@{rest}");
        }
    }
    url
}

/// Largest note file read from the repository.
const MAX_NOTE_BYTES: u64 = 16 * 1024 * 1024;
/// Largest attachment copied between the repository and the library.
const MAX_ATTACHMENT_BYTES: u64 = 512 * 1024 * 1024;
/// Mode of files written by the sync (the data folder itself is private).
const FILE_MODE: u32 = 0o600;

fn git(dir: &Path, args: &[&str], token: &str) -> Result<String, String> {
    let out = crate::platform::command("git")
        .current_dir(dir)
        // a link committed to the repository is checked out as a plain file, never as a link
        .args(["-c", "core.symlinks=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new")
        .output()
        .map_err(|e| crate::i18n::trf("git est introuvable sur cet ordinateur ({e})", &[("e", &e as &dyn std::fmt::Display)]))?;
    let hide = |s: String| if token.is_empty() { s } else { s.replace(token, "•••") };
    let stdout = hide(String::from_utf8_lossy(&out.stdout).to_string());
    let stderr = hide(String::from_utf8_lossy(&out.stderr).to_string());
    if out.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.trim().is_empty() { stdout } else { stderr })
    }
}

fn slug(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    s.chars().take(48).collect()
}

pub fn to_file(n: &Note) -> String {
    let names: Vec<String> = n.attachments.iter().map(|a| a.name.replace('\n', " ")).collect();
    format!(
        "---\nid: {}\ncreated: {}\nmodified: {}\npinned: {}\narchived: {}\ntrashed: {}\nattachments: {}\n---\n{}",
        n.id,
        n.created,
        n.modified,
        n.pinned,
        n.archived,
        n.trashed,
        serde_json::to_string(&names).unwrap_or_else(|_| "[]".into()),
        n.text
    )
}

/// A plain file name (no folders, no "..") — repository content is never trusted to point elsewhere.
pub fn safe_file_name(name: &str) -> Option<String> {
    let base = std::path::Path::new(name).file_name()?.to_str()?;
    let ok = base == name && !base.is_empty() && base != "." && base != ".." && !base.contains(['/', '\\']);
    ok.then(|| base.to_string())
}

fn safe_branch(b: &str) -> bool {
    !b.is_empty()
        && !b.starts_with('-')
        && !b.contains("..")
        && b.chars().all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
}

pub fn from_file(s: &str) -> Option<Note> {
    let rest = s.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let header = &rest[..end];
    let text = rest[end + 5..].to_string();
    let mut map = BTreeMap::new();
    for l in header.lines() {
        if let Some((k, v)) = l.split_once(':') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let num = |k: &str| map.get(k).and_then(|v| v.parse::<i64>().ok());
    let flag = |k: &str| map.get(k).map(|v| v == "true").unwrap_or(false);
    let names: Vec<String> = map
        .get("attachments")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or_default();
    Some(Note {
        id: map.get("id")?.parse().ok()?,
        created: num("created").unwrap_or(0),
        modified: num("modified").unwrap_or(0),
        pinned: flag("pinned"),
        archived: flag("archived"),
        trashed: flag("trashed"),
        attachments: names
            .into_iter()
            .filter_map(|name| safe_file_name(&name))
            .map(|name| Attachment { name, text: String::new(), read: false })
            .collect(),
        text,
    })
}

/// A regular file of the repository, given as a path relative to it, when it is safe to
/// read: every folder on the way is a real folder (no link), the file itself is a regular
/// file (lstat, no link), and its real path stays inside the repository.
fn repo_file(root: &Path, dir: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut p = dir.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        if safe_file_name(part).is_none() {
            return None;
        }
        p.push(part);
        let meta = std::fs::symlink_metadata(&p).ok()?;
        let last = i + 1 == parts.len();
        if (last && !meta.is_file()) || (!last && !meta.is_dir()) {
            return None;
        }
    }
    safefs::is_inside(root, &p).then_some(p)
}

pub(crate) fn read_remote(dir: &Path) -> BTreeMap<u64, Note> {
    let mut out = BTreeMap::new();
    let Ok(root) = dir.canonicalize() else { return out };
    if !safefs::is_real_dir(&dir.join("notes")) {
        return out;
    }
    if let Ok(rd) = std::fs::read_dir(dir.join("notes")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(p) = repo_file(&root, dir, &["notes", &name]) else { continue };
            let Ok(bytes) = safefs::read_regular(&p, Links::Refuse, MAX_NOTE_BYTES) else { continue };
            if let Some(n) = String::from_utf8(bytes).ok().as_deref().and_then(from_file) {
                out.insert(n.id, n);
            }
        }
    }
    out
}

/// Copy the attachments of notes that changed on the other side into the library.
pub(crate) fn bring_in_attachments(dir: &Path, upserts: &[Note]) {
    let Ok(root) = dir.canonicalize() else { return };
    for u in upserts {
        let id = u.id.to_string();
        for a in &u.attachments {
            // the other side has the newer note, so its pictures (and sketch data) win too
            let side = crate::sketch::sidecar(Path::new(&a.name)).to_string_lossy().to_string();
            for name in [a.name.as_str(), side.as_str()] {
                let Some(src) = repo_file(&root, dir, &["attachments", &id, name]) else { continue };
                let dst_dir = Library::attachment_dir(u.id);
                if std::fs::create_dir_all(&dst_dir).is_err() || !safefs::is_real_dir(&dst_dir) {
                    continue;
                }
                let _ = safefs::copy_regular(&src, Links::Refuse, &dst_dir.join(name), MAX_ATTACHMENT_BYTES, FILE_MODE);
            }
        }
    }
}

/// Write the merged notes and their attachments into the repository folder.
pub(crate) fn write_repo(dir: &Path, final_notes: &[Note]) -> std::io::Result<()> {
    // the folders written into are real folders: a link committed as "notes" or
    // "attachments" (or left by an older checkout) is removed, never followed
    let notes = dir.join("notes");
    safefs::remove_any(&notes)?;
    safefs::ensure_real_dir(&notes)?;
    let atts = dir.join("attachments");
    safefs::ensure_real_dir(&atts)?;
    for n in final_notes {
        let name = format!("{}-{}.md", n.id, slug(&n.title()));
        safefs::write_atomic(&notes.join(name), to_file(n).as_bytes(), FILE_MODE)?;
        if n.attachments.is_empty() {
            continue;
        }
        let dst_dir = atts.join(n.id.to_string());
        safefs::ensure_real_dir(&dst_dir)?;
        for a in &n.attachments {
            let Some(name) = safe_file_name(&a.name) else { continue };
            let src = Library::attachment_dir(n.id).join(&name);
            if safefs::open_regular(&src, Links::Refuse).is_err() {
                continue;
            }
            let _ = safefs::copy_regular(&src, Links::Refuse, &dst_dir.join(&name), MAX_ATTACHMENT_BYTES, FILE_MODE);
            let side = crate::sketch::sidecar(&src);
            if safefs::open_regular(&side, Links::Refuse).is_ok() {
                let _ = safefs::copy_regular(&side, Links::Refuse, &crate::sketch::sidecar(&dst_dir.join(&name)), MAX_ATTACHMENT_BYTES, FILE_MODE);
            }
        }
    }
    // drop attachment folders of deleted notes (links are removed, never followed)
    let keep: BTreeSet<String> = final_notes.iter().map(|n| n.id.to_string()).collect();
    for e in std::fs::read_dir(&atts)?.flatten() {
        if !keep.contains(&e.file_name().to_string_lossy().to_string()) {
            let _ = safefs::remove_any(&e.path());
        }
    }
    safefs::write_atomic(
        &dir.join("README.md"),
        crate::i18n::tr("# Notes Grassfield\n\nCe dépôt est synchronisé par Grassfield Notes. Chaque note est un fichier Markdown du dossier `notes/`.\n").as_bytes(),
        FILE_MODE,
    )
}

/// Decide what changes on each side. Pure function, easy to test.
pub fn merge(
    local: &[Note],
    remote: &BTreeMap<u64, Note>,
    synced: &BTreeSet<u64>,
    remote_exists: bool,
    last_sync: i64,
) -> (Vec<Note>, Vec<u64>, Vec<Note>) {
    let local_map: HashMap<u64, &Note> = local.iter().map(|n| (n.id, n)).collect();
    let mut upserts = Vec::new();
    let mut deletes = Vec::new();
    for (id, r) in remote {
        match local_map.get(id) {
            Some(l) if r.modified > l.modified => upserts.push(r.clone()),
            Some(_) => {}
            None if synced.contains(id) => {} // deleted here: the file will be removed
            None => upserts.push(r.clone()),
        }
    }
    for n in local {
        if remote_exists && !remote.contains_key(&n.id) && synced.contains(&n.id) && n.modified <= last_sync {
            deletes.push(n.id); // deleted on the other side and not edited here since
        }
    }
    // final set written to the repository
    let mut final_notes: BTreeMap<u64, Note> = local.iter().map(|n| (n.id, n.clone())).collect();
    for u in &upserts {
        let entry = final_notes.entry(u.id).or_insert_with(|| u.clone());
        let mut merged = u.clone();
        for a in &entry.attachments {
            if !merged.attachments.iter().any(|b| b.name == a.name) {
                merged.attachments.push(a.clone());
            }
        }
        *entry = merged;
    }
    for d in &deletes {
        final_notes.remove(d);
    }
    (upserts, deletes, final_notes.into_values().collect())
}

pub(crate) fn run(settings: SyncSettings, notes: Vec<Note>, synced: BTreeSet<u64>) -> Outcome {
    let fail = |m: String| Outcome { ok: false, message: m, merge: None };
    if settings.repo_url.trim().is_empty() {
        return fail(crate::i18n::tr("Indiquez d'abord l'adresse du dépôt GitHub.").into());
    }
    let token = settings.token.trim().to_string();
    let branch = if settings.branch.trim().is_empty() { "main".to_string() } else { settings.branch.trim().to_string() };
    if !safe_branch(&branch) {
        return fail(crate::i18n::trf("Nom de branche invalide : {0}", &[("0", &branch)]));
    }
    let dir = repo_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return fail(crate::i18n::trf("Dossier de synchronisation impossible à créer : {e}", &[("e", &e as &dyn std::fmt::Display)]));
    }
    if !dir.join(".git").exists() {
        if let Err(e) = git(&dir, &["init", "-q"], &token) {
            return fail(crate::i18n::trf("git init a échoué : {e}", &[("e", &e as &dyn std::fmt::Display)]));
        }
    }
    let _ = git(&dir, &["config", "user.name", "Grassfield Notes"], &token);
    let _ = git(&dir, &["config", "user.email", "grassfield-notes@localhost"], &token);
    let url = auth_url(&settings);
    // "--" keeps an address or branch from ever being read as a git option
    let remote_exists = match git(&dir, &["fetch", "-q", "--", &url, &branch], &token) {
        Ok(_) => {
            if let Err(e) = git(&dir, &["checkout", "-q", "-B", &branch, "FETCH_HEAD"], &token) {
                return fail(crate::i18n::trf("Lecture du dépôt impossible : {e}", &[("e", &e as &dyn std::fmt::Display)]));
            }
            let _ = git(&dir, &["reset", "-q", "--hard", "FETCH_HEAD"], &token);
            true
        }
        Err(e) if e.contains("couldn't find remote ref") || e.contains("Couldn't find remote ref") => {
            let _ = git(&dir, &["checkout", "-q", "-B", &branch], &token);
            false // empty repository: first push
        }
        Err(e) => return fail(crate::i18n::trf("Connexion au dépôt impossible : {0}", &[("0", &e.trim() as &dyn std::fmt::Display)])),
    };
    let remote = read_remote(&dir);
    let (upserts, deletes, final_notes) = merge(&notes, &remote, &synced, remote_exists, settings.last_sync);

    // bring attachments of incoming notes into the library folder
    bring_in_attachments(&dir, &upserts);
    // rewrite the repository content from the merged notes
    if let Err(e) = write_repo(&dir, &final_notes) {
        return fail(crate::i18n::trf("Écriture impossible : {e}", &[("e", &e as &dyn std::fmt::Display)]));
    }
    if let Err(e) = git(&dir, &["add", "-A"], &token) {
        return fail(crate::i18n::trf("git add a échoué : {e}", &[("e", &e as &dyn std::fmt::Display)]));
    }
    let changed = git(&dir, &["status", "--porcelain"], &token).map(|s| !s.trim().is_empty()).unwrap_or(true);
    if changed {
        let msg = crate::i18n::trf("Synchronisation Grassfield Notes — {0}", &[("0", &chrono::Local::now().format("%d/%m/%Y %H:%M") as &dyn std::fmt::Display)]);
        if let Err(e) = git(&dir, &["commit", "-q", "-m", &msg], &token) {
            return fail(crate::i18n::trf("git commit a échoué : {e}", &[("e", &e as &dyn std::fmt::Display)]));
        }
    }
    if changed || !remote_exists {
        let refspec = format!("HEAD:refs/heads/{branch}");
        if let Err(e) = git(&dir, &["push", "-q", "--", &url, &refspec], &token) {
            return fail(crate::i18n::trf("Envoi vers GitHub impossible : {0}", &[("0", &e.trim() as &dyn std::fmt::Display)]));
        }
    }
    let base_modified = notes.iter().map(|n| (n.id, n.modified)).collect();
    let message = crate::i18n::trf(
        "Synchronisé : {0} reçue(s), {1} supprimée(s), {2} note(s) dans le dépôt",
        &[("0", &upserts.len()), ("1", &deletes.len()), ("2", &final_notes.len())],
    );
    Outcome {
        ok: true,
        message,
        merge: Some(Merge { upserts, deletes, synced_ids: final_notes.iter().map(|n| n.id).collect(), base_modified }),
    }
}

/// Start a sync in the background.
pub fn start(settings: SyncSettings, notes: Vec<Note>, synced: BTreeSet<u64>, ctx: eframe::egui::Context) -> Job {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let out = run(settings, notes, synced);
        let _ = tx.send(out);
        ctx.request_repaint();
    });
    Job { rx }
}

/// Apply a finished sync to the library, keeping edits made while it ran.
pub fn apply(lib: &mut Library, m: Merge) -> usize {
    let mut changes = 0;
    for u in m.upserts {
        let base = m.base_modified.get(&u.id).copied();
        match lib.get_mut(u.id) {
            Some(n) if Some(n.modified) == base => {
                let mut attachments = u.attachments.clone();
                for a in &n.attachments {
                    if let Some(b) = attachments.iter_mut().find(|b| b.name == a.name) {
                        *b = a.clone(); // keep text already read here
                    } else {
                        attachments.push(a.clone());
                    }
                }
                *n = Note { attachments, ..u };
                changes += 1;
            }
            Some(_) => {} // edited during the sync: the next sync sends it
            None => {
                lib.next_id = lib.next_id.max(u.id);
                lib.notes.push(u);
                changes += 1;
            }
        }
    }
    for d in m.deletes {
        let base = m.base_modified.get(&d).copied();
        let before = lib.notes.len();
        lib.notes.retain(|n| !(n.id == d && Some(n.modified) == base));
        changes += before - lib.notes.len();
    }
    lib.synced_ids = m.synced_ids;
    lib.settings.sync.last_sync = now();
    changes
}
