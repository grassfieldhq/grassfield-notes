use crate::markdown::{layout_job, MdStyle};
use crate::model::*;
use crate::theme;

#[test]
fn tags_are_found_and_headings_ignored() {
    let t = parse_tags("# Titre\n\nTexte #idées et #travail/projets, pas #1 ni a#b\n```\n#code\n```");
    let v: Vec<_> = t.into_iter().collect();
    assert_eq!(v, vec!["idées".to_string(), "travail/projets".to_string()]);
}

#[test]
fn title_and_preview() {
    let n = Note {
        id: 1,
        text: "\n# **Bonjour**\nligne *deux*\n- [ ] tâche".into(),
        created: 0,
        modified: 0,
        pinned: false,
        archived: false,
        trashed: false,
        attachments: vec![],
    };
    assert_eq!(n.title(), "Bonjour");
    assert_eq!(n.preview(), "ligne deux tâche");
    assert!(n.has_open_todo());
    assert_eq!(n.todo_counts(), (0, 1));
}

#[test]
fn layout_covers_every_byte() {
    let style = MdStyle {
        images: Default::default(),
        hide_markup: false,
        theme: theme::get(0),
        size: 16.0,
        font: EditorFont::Sans,
        dim_markup: true,
        spacing: 1.0,
    };
    for text in [
        "",
        "# Titre\n\n**gras** *it* ~~b~~ ==h== `c` [[lien]] [a](http://x) #tag é\n- [ ] a\n- [x] b\n> q\n1. un\n---\n```\ncode\n```\n",
        "**non fermé *aussi `code",
        "émoji 🌿 et accents àéè #tag/ok",
    ] {
        let job = layout_job(text, &style, 500.0);
        let mut pos = 0;
        for s in &job.sections {
            assert_eq!(s.byte_range.start.0, pos, "gap in {text:?}");
            assert!(s.byte_range.end.0 >= s.byte_range.start.0);
            pos = s.byte_range.end.0;
        }
        assert_eq!(pos, text.len());
    }
}

#[test]
fn library_filters_and_rename() {
    let mut lib = Library::sample();
    assert!(lib.count(&Filter::All) >= 5);
    assert_eq!(lib.count(&Filter::Pinned), 1);
    assert!(lib.count(&Filter::Tag("travail".into())) >= 2);
    lib.rename_tag("travail", "boulot");
    assert_eq!(lib.count(&Filter::Tag("travail".into())), 0);
    assert!(lib.count(&Filter::Tag("boulot/projets".into())) >= 1);
    let found = lib.visible(&Filter::All, "#maison");
    assert!(found.len() >= 2);
    let id = found[0];
    lib.get_mut(id).unwrap().trashed = true;
    assert_eq!(lib.count(&Filter::Trash), 1);
    assert!(!lib.visible(&Filter::All, "").contains(&id));
}

#[test]
fn every_export_format_writes_a_file() {
    let lib = Library::sample();
    let dir = std::env::temp_dir().join(format!("gf-export-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for n in lib.notes.iter().take(2) {
        for f in crate::export::ALL {
            let p = crate::export::export_note(n, f, &dir).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            assert!(p.exists(), "{f:?} missing");
            if p.is_file() {
                let bytes = std::fs::read(&p).unwrap();
                assert!(bytes.len() > 50, "{f:?} too small");
                match f {
                    crate::export::Format::Pdf => assert!(bytes.starts_with(b"%PDF-")),
                    crate::export::Format::Docx | crate::export::Format::Epub => assert!(bytes.starts_with(b"PK\x03\x04")),
                    crate::export::Format::Jpg => assert!(bytes.starts_with(&[0xFF, 0xD8])),
                    crate::export::Format::Rtf => assert!(bytes.starts_with(b"{\\rtf1")),
                    _ => {}
                }
            } else {
                assert!(p.join("text.markdown").exists());
            }
        }
    }
    println!("exported to {}", dir.display());
}

#[test]
fn sync_file_roundtrip_and_merge() {
    let lib = Library::sample();
    let n = &lib.notes[0];
    let back = crate::sync::from_file(&crate::sync::to_file(n)).unwrap();
    assert_eq!(back.id, n.id);
    assert_eq!(back.text, n.text);
    assert_eq!(back.pinned, n.pinned);
    // remote has a newer version of note 0 and a brand-new note; note 1 was deleted remotely
    let mut remote = std::collections::BTreeMap::new();
    let mut newer = n.clone();
    newer.text.push_str("\nmodifié ailleurs");
    newer.modified += 100;
    remote.insert(newer.id, newer.clone());
    let mut fresh = n.clone();
    fresh.id = 999;
    remote.insert(999, fresh);
    for other in lib.notes.iter().skip(2) {
        remote.insert(other.id, other.clone());
    }
    let synced: std::collections::BTreeSet<u64> = lib.notes.iter().map(|n| n.id).collect();
    let (ups, dels, fin) = crate::sync::merge(&lib.notes, &remote, &synced, true, now() + 10);
    assert!(ups.iter().any(|u| u.id == n.id && u.text.ends_with("ailleurs")));
    assert!(ups.iter().any(|u| u.id == 999));
    assert_eq!(dels, vec![lib.notes[1].id]);
    assert!(!fin.iter().any(|f| f.id == lib.notes[1].id));
}

/// Two "computers" (two data folders) syncing through a local bare git repository.
#[test]
fn sync_between_two_computers() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let base = std::env::temp_dir().join(format!("gf-sync-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let remote = base.join("remote.git");
    assert!(std::process::Command::new("git").args(["init", "-q", "--bare"]).arg(&remote).status().unwrap().success());
    let settings = SyncSettings {
        // a plain folder path works as a git remote on every system
        repo_url: remote.display().to_string(),
        branch: "main".into(),
        ..Default::default()
    };
    let with_data = |name: &str| std::env::set_var("GRASSFIELD_DATA", base.join(name).join("library.json"));

    // computer A pushes its sample notes
    with_data("a");
    let mut a = Library::sample();
    let out = crate::sync::run(settings.clone(), a.notes.clone(), a.synced_ids.clone());
    assert!(out.ok, "{}", out.message);
    crate::sync::apply(&mut a, out.merge.unwrap());
    assert_eq!(a.synced_ids.len(), a.notes.len());

    // computer B starts empty and receives them
    with_data("b");
    let mut b = Library::default();
    let out = crate::sync::run(settings.clone(), b.notes.clone(), b.synced_ids.clone());
    assert!(out.ok, "{}", out.message);
    crate::sync::apply(&mut b, out.merge.unwrap());
    assert_eq!(b.notes.len(), a.notes.len());

    // B edits a note and adds one; A gets both
    let first = b.notes[0].id;
    b.get_mut(first).unwrap().text.push_str("\nAjouté sur B");
    b.get_mut(first).unwrap().modified += 50;
    b.new_note("# Note de B".into());
    let out = crate::sync::run(settings.clone(), b.notes.clone(), b.synced_ids.clone());
    assert!(out.ok, "{}", out.message);
    crate::sync::apply(&mut b, out.merge.unwrap());

    with_data("a");
    let out = crate::sync::run(settings.clone(), a.notes.clone(), a.synced_ids.clone());
    assert!(out.ok, "{}", out.message);
    crate::sync::apply(&mut a, out.merge.unwrap());
    assert!(a.get(first).unwrap().text.ends_with("Ajouté sur B"));
    assert!(a.notes.iter().any(|n| n.title() == "Note de B"));
    std::env::remove_var("GRASSFIELD_DATA");
}

#[test]
fn text_is_read_from_images_and_pdfs() {
    let tools = crate::attach::tools();
    let lib = Library::sample();
    let dir = std::env::temp_dir().join(format!("gf-ocr-test-{}", std::process::id()));
    let n = &lib.notes[1]; // "Idées de jardin"
    if tools.pdf {
        let p = crate::export::export_note(n, crate::export::Format::Pdf, &dir).unwrap();
        let t = crate::attach::read_text(&p);
        assert!(t.contains("jardin"), "pdf text: {t}");
    }
    if tools.ocr {
        let p = crate::export::export_note(n, crate::export::Format::Jpg, &dir).unwrap();
        let t = crate::attach::read_text(&p).to_lowercase();
        assert!(t.contains("printemps") || t.contains("prairie"), "ocr text: {t}");
    }
}

#[test]
fn translations_are_complete_and_fill_placeholders() {
    use crate::i18n::{self, Lang};
    assert!(i18n::missing().is_empty(), "{:?}", i18n::missing());
    assert_eq!(i18n::tr_in(Lang::En, "Corbeille"), "Trash");
    assert_eq!(i18n::trf_in(Lang::En, "{n} note(s) importée(s)", &[("n", &3)]), "3 note(s) imported");
    assert_eq!(i18n::tr_in(Lang::De, "Préférences"), "Einstellungen");
    assert_eq!(i18n::tr_in(Lang::Fr, "Corbeille"), "Corbeille");
    // old libraries (no language saved) stay French
    let s: Settings = serde_json::from_str("{\"theme\":0,\"font_size\":16.0,\"line_width\":720.0,\"line_spacing\":1.0,\"sort\":\"Modified\",\"show_previews\":true,\"editor_font\":\"Sans\",\"dim_markup\":true,\"sidebar_width\":230.0}").unwrap();
    assert_eq!(s.language, Some(Lang::Fr));
}

#[test]
fn sketches_draw_and_reload() {
    use crate::sketch::{self, Sketch, Stroke, Tool};
    let dir = std::env::temp_dir().join(format!("gf-sketch-test-{}", std::process::id()));
    let png = dir.join("croquis.png");
    let mut sk = Sketch::default();
    sk.strokes.push(Stroke { tool: Tool::Pen, color: [255, 0, 0], width: 10.0, points: vec![[100.0, 100.0], [300.0, 100.0]] });
    sk.strokes.push(Stroke { tool: Tool::Marker, color: [0, 0, 255], width: 30.0, points: vec![[100.0, 300.0], [300.0, 300.0]] });
    sketch::save(&sk, &png).unwrap();
    let img = image::open(&png).unwrap().to_rgba8();
    assert_eq!(img.get_pixel(200, 100).0[..3], [255, 0, 0]); // pen: solid
    let m = img.get_pixel(200, 300).0;
    assert!(m[2] == 255 && m[0] > 100 && m[0] < 200, "marker should be see-through: {m:?}"); // highlighter: translucent
    assert_eq!(img.get_pixel(600, 600).0[..3], [255, 255, 255]);
    assert_eq!(sketch::load(&png).unwrap(), sk); // stays editable
    assert!(sketch::hits(&sk.strokes[0], [200.0, 104.0], 1.0));
    assert!(!sketch::hits(&sk.strokes[0], [200.0, 200.0], 1.0));
    assert_eq!(crate::model::image_ref("![](croquis.png)"), Some("croquis.png"));
    assert_eq!(crate::model::image_ref("texte ![](a.png) suite"), None);
}

#[test]
fn exports_include_pictures() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("GRASSFIELD_DATA", std::env::temp_dir().join(format!("gf-pic-export-{}", std::process::id())).join("library.json"));
    let mut lib = Library::sample();
    let id = lib.notes[0].id;
    let dir = Library::attachment_dir(id);
    std::fs::create_dir_all(&dir).unwrap();
    image::RgbImage::from_pixel(400, 200, image::Rgb([200, 40, 40])).save(dir.join("photo.png")).unwrap();
    lib.get_mut(id).unwrap().text.push_str("\n![](photo.png)\n");
    let out = std::env::temp_dir().join(format!("gf-pic-out-{}", std::process::id()));
    let n = lib.get(id).unwrap();
    let pdf = std::fs::read(crate::export::export_note(n, crate::export::Format::Pdf, &out).unwrap()).unwrap();
    assert!(pdf.windows(10).any(|w| w == b"/DCTDecode"), "picture embedded in PDF");
    let html = std::fs::read_to_string(crate::export::export_note(n, crate::export::Format::Html, &out).unwrap()).unwrap();
    assert!(html.contains("data:image/png;base64,"));
    let epub = std::fs::read(crate::export::export_note(n, crate::export::Format::Epub, &out).unwrap()).unwrap();
    assert!(epub.windows(17).any(|w| w == b"OEBPS/images/img0"));
    let jpg = crate::export::export_note(n, crate::export::Format::Jpg, &out).unwrap();
    let img = image::open(jpg).unwrap().to_rgb8();
    assert!(img.pixels().any(|p| p.0[0] > 180 && p.0[1] < 80 && p.0[2] < 80), "picture drawn in JPG");
    std::env::remove_var("GRASSFIELD_DATA");
}

/// Tests that point GRASSFIELD_DATA somewhere take turns.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());


#[test]
fn sync_never_trusts_paths_from_the_repository() {
    use crate::sync::safe_file_name;
    assert_eq!(safe_file_name("photo.png").as_deref(), Some("photo.png"));
    assert_eq!(safe_file_name("../../.bashrc"), None);
    assert_eq!(safe_file_name("a/b.png"), None);
    assert_eq!(safe_file_name(".."), None);
    let evil = "---\nid: 7\ncreated: 0\nmodified: 0\npinned: false\narchived: false\ntrashed: false\nattachments: [\"../../evil.sh\", \"ok.png\"]\n---\ntexte";
    let n = crate::sync::from_file(evil).unwrap();
    assert_eq!(n.attachments.len(), 1);
    assert_eq!(n.attachments[0].name, "ok.png");
}

#[test]
fn renaming_unicode_tags_never_panics() {
    // "İ" is two bytes but lowercases to "i̇" (three bytes): offsets must not be mixed up
    assert_eq!(tag_match_len("İİ", "i̇i̇"), Some("İİ".len()));
    assert_eq!(tag_match_len("İİ/sous", "i̇i̇"), Some("İİ".len()));
    assert_eq!(tag_match_len("İİx", "i̇i̇"), None);
    assert_eq!(tag_match_len("travail", ""), None);
    let mut lib = Library::default();
    let id = lib.new_note("# T\n\n#İİ/€ et #İİ/sous et #autre".into());
    let tags = lib.get(id).unwrap().tags();
    assert!(tags.contains("i̇i̇"), "{tags:?}");
    lib.rename_tag("i̇i̇", "neuf");
    assert_eq!(lib.get(id).unwrap().text, "# T\n\n#neuf/€ et #neuf/sous et #autre");
    lib.delete_tag("neuf");
    assert_eq!(lib.get(id).unwrap().text, "# T\n\n/€ et  et #autre");
}

#[test]
fn note_ids_never_overflow_or_repeat() {
    let mut lib = Library::default();
    // a synced note can carry the largest id there is
    lib.notes.push(Note { id: u64::MAX, text: String::new(), created: 0, modified: 0, pinned: false, archived: false, trashed: false, attachments: vec![] });
    lib.next_id = u64::MAX;
    let a = lib.new_note("a".into());
    let b = lib.new_note("b".into());
    assert!(a != u64::MAX && b != u64::MAX && a != b && a != 0 && b != 0);
}

fn git_in(dir: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn note_file(id: u64, attachments: &str, text: &str) -> String {
    format!("---\nid: {id}\ncreated: 1\nmodified: 9999999999\npinned: false\narchived: false\ntrashed: false\nattachments: {attachments}\n---\n{text}")
}

/// A collaborator with push access commits links pointing at the victim's files.
/// Syncing must never read through them, never write through them, and never let
/// a name with ".." leave the repository.
#[cfg(unix)]
#[test]
fn sync_never_follows_links_from_a_hostile_repository() {
    use std::os::unix::fs::symlink;
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let base = std::env::temp_dir().join(format!("gf-hostile-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // the victim's own files (a stand-in home folder, never the real one)
    let home = base.join("home");
    std::fs::create_dir_all(home.join("outside/keep")).unwrap();
    let bashrc = home.join(".bashrc");
    // shaped like a note: if a link to it were followed, a note would appear
    let bashrc_text = note_file(77, "[]", "shell startup file");
    std::fs::write(&bashrc, &bashrc_text).unwrap();
    let secret = home.join("secret.txt");
    std::fs::write(&secret, "TOP SECRET").unwrap();
    std::fs::write(home.join("outside/keep/file"), "keep me").unwrap();

    // the attacker's repository
    let remote = base.join("remote.git");
    git_in(&base, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    let work = base.join("attacker");
    git_in(&base, &["init", "-q", work.to_str().unwrap()]);
    std::fs::create_dir_all(work.join("notes")).unwrap();
    std::fs::create_dir_all(work.join("attachments/5")).unwrap();
    symlink(&bashrc, work.join("notes/1-evil.md")).unwrap();
    std::fs::write(work.join("notes/5-ok.md"), note_file(5, r#"["photo.png", "../../.bashrc"]"#, "# Ok")).unwrap();
    symlink(&secret, work.join("attachments/5/photo.png")).unwrap();
    std::fs::write(work.join("notes/6-dir.md"), note_file(6, r#"["x.png"]"#, "# Dir")).unwrap();
    symlink(home.join("outside"), work.join("attachments/6")).unwrap();
    symlink(&bashrc, work.join("README.md")).unwrap();
    git_in(&work, &["add", "-A"]);
    git_in(&work, &["commit", "-q", "-m", "evil"]);
    git_in(&work, &["push", "-q", remote.to_str().unwrap(), "HEAD:refs/heads/main"]);

    // the victim syncs, with one local note that has an attachment to send
    std::env::set_var("GRASSFIELD_DATA", base.join("victim").join("library.json"));
    let mut lib = Library::default();
    let mine = lib.new_note("# Mine".into());
    std::fs::create_dir_all(Library::attachment_dir(mine)).unwrap();
    std::fs::write(Library::attachment_dir(mine).join("a.png"), "picture").unwrap();
    lib.get_mut(mine).unwrap().attachments.push(Attachment { name: "a.png".into(), text: String::new(), read: true });
    let settings = SyncSettings { repo_url: remote.display().to_string(), branch: "main".into(), ..Default::default() };
    let out = crate::sync::run(settings.clone(), lib.notes.clone(), lib.synced_ids.clone());
    assert!(out.ok, "{}", out.message);
    crate::sync::apply(&mut lib, out.merge.unwrap());

    assert_eq!(std::fs::read_to_string(&bashrc).unwrap(), bashrc_text, "the startup file was not written through");
    assert!(lib.get(77).is_none(), "a linked note was read");
    assert!(lib.get(5).is_some());
    assert_eq!(lib.get(5).unwrap().attachments.len(), 1, "a name with .. was kept");
    let pulled = std::fs::read(Library::attachment_dir(5).join("photo.png")).unwrap_or_default();
    assert_ne!(pulled, b"TOP SECRET", "a local file was pulled into the notes");
    let outside: Vec<_> = std::fs::read_dir(home.join("outside")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(outside.len(), 1, "something was written into a linked folder: {outside:?}");
    assert_eq!(std::fs::read_to_string(home.join("outside/keep/file")).unwrap(), "keep me");

    // second round: the whole "attachments" folder becomes a link to the victim's files
    git_in(&work, &["fetch", "-q", remote.to_str().unwrap(), "main"]);
    git_in(&work, &["reset", "-q", "--hard", "FETCH_HEAD"]);
    git_in(&work, &["rm", "-q", "-r", "attachments"]);
    symlink(home.join("outside"), work.join("attachments")).unwrap();
    git_in(&work, &["add", "-A"]);
    git_in(&work, &["commit", "-q", "-m", "evil 2"]);
    git_in(&work, &["push", "-q", remote.to_str().unwrap(), "HEAD:refs/heads/main"]);
    let out = crate::sync::run(settings, lib.notes.clone(), lib.synced_ids.clone());
    assert!(out.ok, "{}", out.message);
    assert_eq!(std::fs::read_to_string(home.join("outside/keep/file")).unwrap(), "keep me", "a linked folder was emptied");
    assert!(!home.join("outside").join(mine.to_string()).exists(), "an attachment was written into a linked folder");
    assert_eq!(std::fs::read_to_string(&bashrc).unwrap(), bashrc_text);
    std::env::remove_var("GRASSFIELD_DATA");
}

/// Links already present in the local copy (left by a version without the git setting)
/// are refused as well: lstat, O_NOFOLLOW and the real-path check.
#[cfg(unix)]
#[test]
fn links_already_in_the_local_copy_are_refused() {
    use std::os::unix::fs::symlink;
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let base = std::env::temp_dir().join(format!("gf-hostile-copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("note.md"), note_file(42, "[]", "outside")).unwrap();
    std::fs::write(outside.join("secret.png"), "TOP SECRET").unwrap();
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("notes")).unwrap();
    std::fs::create_dir_all(repo.join("attachments/3")).unwrap();
    symlink(outside.join("note.md"), repo.join("notes/42-x.md")).unwrap();
    std::fs::write(repo.join("notes/3-ok.md"), note_file(3, r#"["p.png", "d.png"]"#, "ok")).unwrap();
    symlink(outside.join("secret.png"), repo.join("attachments/3/p.png")).unwrap();
    // a folder link on the way
    symlink(&outside, repo.join("attachments/3/sub")).unwrap();
    let remote = crate::sync::read_remote(&repo);
    assert_eq!(remote.keys().copied().collect::<Vec<_>>(), vec![3]);

    std::env::set_var("GRASSFIELD_DATA", base.join("data").join("library.json"));
    let notes: Vec<_> = remote.into_values().collect();
    crate::sync::bring_in_attachments(&repo, &notes);
    assert!(!Library::attachment_dir(3).join("p.png").exists(), "a linked attachment was copied");

    // writing never goes through a linked "notes" folder or README
    std::fs::remove_dir_all(repo.join("notes")).unwrap();
    symlink(&outside, repo.join("notes")).unwrap();
    symlink(outside.join("note.md"), repo.join("README.md")).unwrap();
    crate::sync::write_repo(&repo, &notes).unwrap();
    assert!(safefs_is_real_dir(&repo.join("notes")));
    assert_eq!(std::fs::read_to_string(outside.join("note.md")).unwrap(), note_file(42, "[]", "outside"));
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 2);
    std::env::remove_var("GRASSFIELD_DATA");
}

fn safefs_is_real_dir(p: &std::path::Path) -> bool {
    std::fs::symlink_metadata(p).map(|m| m.is_dir()).unwrap_or(false)
}
