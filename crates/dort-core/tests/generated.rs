mod support;

use dort_core::{
    Manifest,
    generated::{DestinationStatus, Preview},
    planner::{self, Request},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};
use support::TestHome;

const DECLARATION: &str = r#"
schema = 1

[[files]]
id = "settings"
mode = "template"
source = "recipes/settings"
target = "~/.config/editor/settings"
"#;

struct Fixture {
    home: TestHome,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = TestHome::new();
        let repo = home.repository("repo");
        fs::create_dir(repo.join("recipes")).unwrap();
        let fixture = Self { home, repo };
        fixture.manifest("");
        fixture
    }

    fn manifest(&self, extra: &str) {
        fs::write(self.repo.join("dort.toml"), format!("{DECLARATION}{extra}")).unwrap();
    }

    fn recipe(&self, bytes: impl AsRef<[u8]>) {
        fs::write(self.repo.join("recipes/settings"), bytes).unwrap();
    }

    fn preview(&self) -> dort_core::Result<Preview> {
        Preview::prepare(&self.home.host, &self.repo, "settings")
    }

    fn error(&self, expected: &str) -> String {
        let error = self.preview().unwrap_err().to_string();
        assert!(error.contains(expected), "{error:?} lacks {expected:?}");
        error
    }
}

#[test]
fn literal_render_uses_platform_source_and_leaves_missing_destination_alone() {
    let f = Fixture::new();
    f.manifest(
        r#"
inputs = { theme = "暗い", format = "{{ theme }}", empty = "" }
permissions = "0644"
overrides = { linux = "recipes/linux" }
"#,
    );
    f.recipe("base must not appear");
    let source = f.repo.join("recipes/linux");
    let template = "theme={{ theme }}\r\nformat=[{{ format }}]{{ empty }}\n{\"raw\":{}}\n";
    fs::write(&source, template).unwrap();
    let preview = f.preview().unwrap();
    assert_eq!(preview.source, source);
    assert_eq!(
        preview.destination,
        f.home.host.home.join(".config/editor/settings")
    );
    assert_eq!(preview.destination_status, DestinationStatus::Missing);
    assert_eq!(preview.permissions.as_deref(), Some("0644"));
    assert_eq!(
        preview.rendered.as_bytes(),
        "theme=暗い\r\nformat=[{{ theme }}]\n{\"raw\":{}}\n".as_bytes()
    );
    assert_eq!(
        preview
            .inputs
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["empty", "format", "theme"]
    );
    assert_eq!(
        serde_json::to_vec(&preview).unwrap(),
        serde_json::to_vec(&f.preview().unwrap()).unwrap()
    );
    assert_eq!(fs::read(&source).unwrap(), template.as_bytes());
    assert_eq!(fs::read_dir(&f.home.host.home).unwrap().count(), 0);
    let plan = planner::plan(planner::observe(&f.home.host, &f.repo, &[], Request::Apply).unwrap());
    assert!(plan.has_blockers());
    assert!(
        plan.files[0]
            .blockers
            .iter()
            .any(|b| b.contains("Template deployment is unsupported"))
    );
}

#[test]
fn destination_status_uses_only_metadata() {
    let f = Fixture::new();
    f.recipe("new configuration");
    let destination = f.preview().unwrap().destination;
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, b"password=cannot-appear\xff").unwrap();
    let preview = f.preview().unwrap();
    assert_eq!(preview.destination_status, DestinationStatus::Existing);
    assert!(
        !serde_json::to_string(&preview)
            .unwrap()
            .contains("cannot-appear")
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"password=cannot-appear\xff"
    );
    fs::remove_file(&destination).unwrap();
    symlink("not-present", &destination).unwrap();
    assert_eq!(
        f.preview().unwrap().destination_status,
        DestinationStatus::Blocked
    );
    fs::remove_file(&destination).unwrap();
    fs::remove_dir(destination.parent().unwrap()).unwrap();
    symlink(&f.repo, destination.parent().unwrap()).unwrap();
    assert_eq!(
        f.preview().unwrap().destination_status,
        DestinationStatus::Blocked
    );
    fs::write(
        f.repo.join("dort.toml"),
        DECLARATION.replace("~/.config/editor/settings", "~/.ssh/config"),
    )
    .unwrap();
    assert_eq!(
        f.preview().unwrap().destination_status,
        DestinationStatus::Blocked
    );
}

#[test]
fn errors_guide_missing_inputs_and_reject_expressions_without_echoing_text() {
    let f = Fixture::new();
    f.recipe("{{ absent }}");
    assert!(f.error("Missing input absent").contains("dort.toml"));
    for expression in [
        "{{ env.HOME }}",
        "{{ name | unsafe }}",
        "{{ unsafe\x1b[31m }}",
        "{{ unsafe",
        "{{ {{ unsafe }} }}",
    ] {
        f.recipe(expression);
        assert!(!f.error("Use {{ name }}").contains("unsafe"));
    }
    f.recipe(b"invalid\xffUTF-8");
    f.error("must be UTF-8");
    f.manifest("inputs = { a = 42 }\n");
    f.error("Use literal strings");
    fs::write(
        f.repo.join("dort.toml"),
        DECLARATION.replace("template", "link"),
    )
    .unwrap();
    f.error("requires a file with mode");
    assert!(
        !serde_json::to_string(&Manifest::parse(DECLARATION).unwrap())
            .unwrap()
            .contains("inputs")
    );
    assert!(
        Manifest::parse(&format!(
            "{}inputs = {{ a = 'x' }}",
            DECLARATION.replace("template", "link")
        ))
        .is_err()
    );
}

#[test]
fn limits_count_utf8_bytes_and_allow_the_exact_boundary() {
    let f = Fixture::new();
    f.recipe("é".repeat(32 * 1024));
    assert_eq!(f.preview().unwrap().rendered.len(), 64 * 1024);
    f.recipe("é".repeat(32 * 1024 + 1));
    f.error("exceeds 64 KiB");
    let value = "é".repeat(8 * 1024);
    f.manifest(&format!("inputs = {{ v = '{value}x' }}"));
    f.recipe("{{ v }}");
    f.error("exceeds 16 KiB");
    f.manifest(&format!("inputs = {{ v = '{value}' }}"));
    f.recipe("{{ v }}".repeat(8));
    assert_eq!(f.preview().unwrap().rendered.len(), 128 * 1024);
    f.recipe(format!("{}x", "{{ v }}".repeat(8)));
    f.error("exceeds 128 KiB");
}

#[test]
fn unsafe_and_sensitive_sources_never_render() {
    let f = Fixture::new();
    f.error("source is missing");
    let source = f.repo.join("recipes/settings");
    let alternate = f.repo.join("other");
    fs::write(&alternate, "private configuration").unwrap();
    symlink(&alternate, &source).unwrap();
    f.error("regular file without links");
    fs::remove_file(&source).unwrap();
    fs::hard_link(&alternate, &source).unwrap();
    f.error("regular file without links");
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    f.error("regular file without links");
    fs::remove_dir(&source).unwrap();
    f.recipe("private configuration");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o4700)).unwrap();
    f.error("special permission bits");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    f.recipe("password=never-show-this");
    assert!(!f.error("may contain secrets").contains("never-show-this"));
    fs::write(
        f.repo.join("dort.toml"),
        DECLARATION.replace("recipes/settings", ".ssh/config"),
    )
    .unwrap();
    f.error("sensitive path");
    fs::remove_dir_all(f.repo.join("recipes")).unwrap();
    symlink(&f.home.host.home, f.repo.join("recipes")).unwrap();
    f.manifest("");
    f.error("Cannot inspect template source safely");
}

#[test]
fn secret_sources_block_path_overlap_and_aliases_including_inactive_overrides() {
    let f = Fixture::new();
    f.recipe("not recognizable by a keyword");
    for secret_source in ["recipes/settings", "recipes", "recipes/settings/nested"] {
        f.manifest(&format!(
            r#"
[profiles]
available = ["work"]
[[files]]
id = "private"
mode = "secret"
source = "otherwise-unrelated"
profiles = ["work"]
target = "~/private-settings"
overrides = {{ macos = "{secret_source}" }}
"#
        ));
        f.error("overlaps a declared secret source");
    }
    symlink("recipes/settings", f.repo.join("alias")).unwrap();
    f.manifest(
        r#"
[[files]]
id = "private"
mode = "secret"
source = "alias"
target = "~/private-settings"
"#,
    );
    f.error("Cannot rule out a secret source alias");
}
