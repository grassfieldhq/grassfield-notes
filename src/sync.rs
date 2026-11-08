//! Two-way sync of notes with a GitHub (or any git) repository, using the `git` program.
//!
//! Each note is one Markdown file `notes/<id>-<title>.md` with a small header holding its
//! dates and flags; attachments go to `attachments/<id>/`. The newest edit of a note wins.

use crate::model::{now, Attachment, Library, Note, SyncSettings};
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

fn git(dir: &Path, args: &[&str], token: &str) -> Result<String, String> {
    let out = crate::platform::command("git")
        .current_dir(dir)
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

fn read_remote(dir: &Path) -> BTreeMap<u64, Note> {
    let mut out = BTreeMap::new();
    if let Ok(rd) = std::fs::read_dir(dir.join("notes")) {
        for e in rd.flatten() {
            if let Ok(s) = std::fs::read_to_string(e.path()) {
                if let Some(n) = from_file(&s) {
                    out.insert(n.id, n);
                }
            }
        }
    }
    out
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
    for u in &upserts {
        for a in &u.attachments {
            // the other side has the newer note, so its pictures (and sketch data) win too
            let src = dir.join("attachments").join(u.id.to_string()).join(&a.name);
            let dst = Library::attachment_dir(u.id).join(&a.name);
            if src.exists() {
                let _ = std::fs::create_dir_all(Library::attachment_dir(u.id));
                let _ = std::fs::copy(&src, &dst);
                let side = crate::sketch::sidecar(&src);
                if side.exists() {
                    let _ = std::fs::copy(&side, crate::sketch::sidecar(&dst));
                }
            }
        }
    }
    // rewrite the repository content from the merged notes
    let _ = std::fs::remove_dir_all(dir.join("notes"));
    if let Err(e) = std::fs::create_dir_all(dir.join("notes")) {
        return fail(crate::i18n::trf("Écriture impossible : {e}", &[("e", &e as &dyn std::fmt::Display)]));
    }
    for n in &final_notes {
        let name = format!("{}-{}.md", n.id, slug(&n.title()));
        if let Err(e) = std::fs::write(dir.join("notes").join(name), to_file(n)) {
            return fail(crate::i18n::trf("Écriture impossible : {e}", &[("e", &e as &dyn std::fmt::Display)]));
        }
        for a in &n.attachments {
            let src = Library::attachment_dir(n.id).join(&a.name);
            let dst_dir = dir.join("attachments").join(n.id.to_string());
            if src.exists() {
                let _ = std::fs::create_dir_all(&dst_dir);
                let _ = std::fs::copy(&src, dst_dir.join(&a.name));
                let side = crate::sketch::sidecar(&src);
                if side.exists() {
                    let _ = std::fs::copy(&side, crate::sketch::sidecar(&dst_dir.join(&a.name)));
                }
            }
        }
    }
    // drop attachment folders of deleted notes
    let keep: BTreeSet<String> = final_notes.iter().map(|n| n.id.to_string()).collect();
    if let Ok(rd) = std::fs::read_dir(dir.join("attachments")) {
        for e in rd.flatten() {
            if !keep.contains(&e.file_name().to_string_lossy().to_string()) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let _ = std::fs::write(
        dir.join("README.md"),
        crate::i18n::tr("# Notes Grassfield\n\nCe dépôt est synchronisé par Grassfield Notes. Chaque note est un fichier Markdown du dossier `notes/`.\n"),
    );
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
