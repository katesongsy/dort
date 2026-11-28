use super::*;
use crate::{
    local::{LocalConfig, RepositoryRegistry},
    platform::{Architecture, Platform},
};
use rustix::io::Errno;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

const SOURCE: &str = "theme = dark\nfont = mono\n";
const MANIFEST: &str = "schema = 1\n\n[[files]]\nid = 'settings'\nmode = 'link'\nsource = 'source'\ntarget = '~/app/file'\n";

struct Fixture {
    host: HostContext,
    repo: PathBuf,
    dest: PathBuf,
}

impl Fixture {
    fn new(root: &Path) -> Self {
        let host = HostContext::new(
            root.join("home"),
            None,
            None,
            Platform::Linux,
            Architecture::X86_64,
        )
        .unwrap();
        let repo = root.join("repo");
        let dest = host.home.join("app/file");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(repo.join("dort.toml"), MANIFEST).unwrap();
        {
            let lock = WriterLock::acquire(&host).unwrap();
            RepositoryRegistry::register(&host, &lock, &repo).unwrap();
        }
        fs::write(repo.join("source"), SOURCE).unwrap();
        fs::set_permissions(repo.join("source"), fs::Permissions::from_mode(0o640)).unwrap();
        symlink(repo.join("source"), &dest).unwrap();
        Self { host, repo, dest }
    }

    fn template(&self) -> PathBuf {
        self.repo.join("templates/settings.jinja")
    }

    fn approve(&self) -> ConvertApproval {
        Convert::prepare(&self.host, None, "settings", &[])
            .unwrap()
            .approve()
    }

    fn manifest(&self) -> ManagedFile {
        LocalConfig::load(&self.host).unwrap();
        crate::Manifest::parse(&fs::read_to_string(self.repo.join("dort.toml")).unwrap())
            .unwrap()
            .files
            .into_iter()
            .find(|file| file.id == "settings")
            .unwrap()
    }

    fn slot(&self) -> Slot {
        Slot::new(&self.host, &self.repository_id(), "settings", &self.dest).unwrap()
    }

    fn repository_id(&self) -> String {
        LocalConfig::load(&self.host)
            .unwrap()
            .repository_id(&self.repo)
            .unwrap()
            .to_owned()
    }

    fn assert_converted(&self) {
        assert_eq!(fs::read(self.template()).unwrap(), SOURCE.as_bytes());
        assert_eq!(mode(&self.template()), 0o644);
        assert!(fs::symlink_metadata(&self.dest).unwrap().is_file());
        assert_eq!(fs::read(&self.dest).unwrap(), SOURCE.as_bytes());
        assert_eq!(mode(&self.dest), 0o640);
        let mapping = self.manifest();
        assert_eq!(mapping.mode, FileMode::Template);
        assert_eq!(
            mapping.source.as_deref(),
            Some(Path::new("templates/settings.jinja"))
        );
        assert_eq!(mapping.permissions.as_deref(), Some("0640"));
        assert_eq!(
            self.slot().load().unwrap().unwrap().retained.as_deref(),
            Some(self.repo.join("source").as_path())
        );
        assert!(pending_conversion(&self.host).unwrap().is_none());
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

fn converted(outcome: ConvertOutcome) -> Converted {
    match outcome {
        ConvertOutcome::Converted(converted) => converted,
        other => panic!("expected a conversion, got {other:?}"),
    }
}

#[test]
fn a_reviewed_conversion_keeps_the_source_bytes_and_reports_the_retained_link() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let review = Convert::prepare(&f.host, None, "settings", &[]).unwrap();
    let review = review.review();
    assert_eq!(review.before, SOURCE);
    assert_eq!(review.after, SOURCE);
    assert_eq!(review.retained, f.repo.join("source"));
    assert_eq!(review.template, Path::new("templates/settings.jinja"));
    assert_eq!(review.folder, Some(Path::new("templates")));
    assert_eq!(review.permissions, "0640");
    assert_eq!(review.template_permissions, "0644");

    let result = converted(f.approve().execute().unwrap());
    assert_eq!(result.destination, f.dest);
    assert_eq!(result.template, f.template());
    assert_eq!(result.retained, f.repo.join("source"));
    assert_eq!(result.permissions, "0640");
    f.assert_converted();
}

#[test]
fn a_source_with_template_syntax_refuses_and_names_the_line() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    fs::write(f.repo.join("source"), "theme = dark\nfont = {{ name }}\n").unwrap();
    let error = Convert::prepare(&f.host, None, "settings", &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("Line 2"), "{error}");
    assert!(error.contains("{{"), "{error}");
    assert!(fs::symlink_metadata(&f.dest).unwrap().is_symlink());
    assert!(!f.repo.join("templates").exists());
}

#[test]
fn each_partial_boundary_resumes_twice_and_converges() {
    for (boundary, done) in [
        (Boundary::Folder, 0usize),
        (Boundary::TemplateFile, 0),
        (Boundary::Exchanged, 1),
        (Boundary::Mapping, 2),
    ] {
        let root = tempfile::tempdir().unwrap();
        let f = Fixture::new(&root.path().canonicalize().unwrap());
        let outcome = f
            .approve()
            .execute_at(|at| {
                if at == boundary {
                    Err(Errno::IO)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let ConvertOutcome::NeedsRecovery { completed, .. } = outcome else {
            panic!("{boundary:?} did not need recovery: {outcome:?}");
        };
        assert_eq!(completed.len(), done, "{boundary:?}");
        assert!(pending_conversion(&f.host).unwrap().is_some());

        converted(recover_conversion(&f.host).unwrap().unwrap());
        assert!(
            recover_conversion(&f.host).unwrap().is_none(),
            "{boundary:?} left a second recovery"
        );
        f.assert_converted();
    }
}

#[test]
fn a_changed_source_refuses_the_stale_approval_and_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let approval = f.approve();
    fs::write(f.repo.join("source"), "theme = light\n").unwrap();
    let error = approval.execute().unwrap_err().to_string();
    assert!(error.contains("review changed"), "{error}");
    assert!(fs::symlink_metadata(&f.dest).unwrap().is_symlink());
    assert!(!f.repo.join("templates").exists());
    assert_eq!(f.manifest().mode, FileMode::Link);
    assert!(pending_conversion(&f.host).unwrap().is_none());
}

#[test]
fn a_foreign_or_unsafe_link_refuses() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    let other = f.repo.join("other");
    fs::write(&other, "elsewhere\n").unwrap();
    fs::remove_file(&f.dest).unwrap();
    symlink(&other, &f.dest).unwrap();
    let error = Convert::prepare(&f.host, None, "settings", &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("points somewhere else"), "{error}");

    fs::remove_file(&f.dest).unwrap();
    fs::write(&f.dest, "unmanaged\n").unwrap();
    let error = Convert::prepare(&f.host, None, "settings", &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("is not the managed symlink"), "{error}");
}

#[test]
fn a_template_path_used_by_another_mapping_refuses() {
    let root = tempfile::tempdir().unwrap();
    let f = Fixture::new(&root.path().canonicalize().unwrap());
    fs::create_dir_all(f.repo.join("templates")).unwrap();
    fs::write(f.repo.join("templates/settings.jinja"), "shared\n").unwrap();
    fs::write(
        f.repo.join("dort.toml"),
        format!(
            "{MANIFEST}\n[[files]]\nid = 'other'\nmode = 'template'\nsource = 'templates/settings.jinja'\ntarget = '~/app/other'\n"
        ),
    )
    .unwrap();
    let error = Convert::prepare(&f.host, None, "settings", &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("already used by another mapping"), "{error}");
}
