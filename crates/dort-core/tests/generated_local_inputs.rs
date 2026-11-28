mod support;

use dort_core::{
    Error, Manifest,
    generated::{Deployment, InputProvenance, Preview, Update, UpdateAction, local_inputs},
    local::{RepositoryRegistry, repository_state_path},
    manifest_edit::{Change, FileChange, Preview as Edit},
    state::WriterLock,
    template::VALUE_LIMIT,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};
use support::TestHome;

const TEMPLATE: &str = "{{ local.editor }}";

fn file(id: &str, text: &str, names: &str) -> String {
    format!(
        "\n[[files]]\nid = '{id}'\nmode = 'template'\ntarget = '~/{id}'\ntemplate = '''{text}'''\nlocal_inputs = [{names}]\n"
    )
}

struct Fixture {
    home: TestHome,
    repo: PathBuf,
    state: PathBuf,
}

impl Fixture {
    fn new(text: &str, names: &str) -> Self {
        let home = TestHome::new();
        let repo = home.repository("repo");
        let id = RepositoryRegistry::register(
            &home.host,
            &WriterLock::acquire(&home.host).unwrap(),
            &repo,
        )
        .unwrap();
        let state = repository_state_path(&home.host, &id)
            .unwrap()
            .join("generated-inputs");
        let f = Self { home, repo, state };
        f.write(text, names);
        f
    }

    fn write(&self, text: &str, names: &str) {
        fs::write(
            self.repo.join("dort.toml"),
            format!("schema = 1\n{}", file("settings", text, names)),
        )
        .unwrap();
    }

    fn preview(&self) -> dort_core::Result<Preview> {
        Preview::prepare(&self.home.host, &self.repo, "settings")
    }

    fn setting(&self, name: &str, value: Option<&str>) -> dort_core::Result<local_inputs::Preview> {
        local_inputs::Preview::prepare(&self.home.host, &self.repo, "settings", name, value)
    }

    fn set(&self, value: &str) {
        self.setting("editor", Some(value))
            .unwrap()
            .approve()
            .execute()
            .unwrap();
    }

    fn rows(&self, root: &Path, id: &str) -> Vec<(String, Option<String>)> {
        local_inputs::inspect(&self.home.host, root, id)
            .unwrap()
            .into_iter()
            .map(|row| (row.name, row.value))
            .collect()
    }

    fn error<T>(result: dort_core::Result<T>, message: &str) -> String {
        let Err(Error::Invalid(error)) = result else {
            panic!("expected error: {message}")
        };
        assert!(error.contains(message), "{error}");
        error
    }
}

#[test]
fn only_declared_names_resolve_in_the_exact_registered_repository_and_mapping() {
    let f = Fixture::new(
        "{{ inputs.editor }}/{{ local.editor }}/{{ host.os }}",
        "'editor', 'optional'",
    );
    let path = f.repo.join("dort.toml");
    let text = format!(
        "{}inputs = {{ editor = 'common' }}\nhost_inputs = ['os']\n{}",
        fs::read_to_string(&path).unwrap(),
        file("other", TEMPLATE, "'editor'")
    );
    fs::write(&path, &text).unwrap();
    Fixture::error(f.preview(), "then preview again: editor");
    f.set("${EDITOR}/{{ inputs.editor }}");
    let p = f.preview().unwrap();
    assert_eq!(p.rendered, "common/${EDITOR}/{{ inputs.editor }}/linux");
    assert_eq!(
        p.resolved_inputs["local.editor"].provenance,
        InputProvenance::MachineLocal
    );
    assert_eq!(
        p.resolved_inputs["inputs.editor"].provenance,
        InputProvenance::Manifest
    );
    assert!(matches!(
        p.resolved_inputs["host.os"].provenance,
        InputProvenance::Host(_)
    ));
    assert!(!p.resolved_inputs.contains_key("local.optional"));
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert_eq!(f.rows(&f.repo, "other"), [("editor".into(), None)]);

    let other = f.home.repository("other-repo");
    fs::write(other.join("dort.toml"), &text).unwrap();
    Fixture::error(
        Preview::prepare(&f.home.host, &other, "settings"),
        "Register this repository first",
    );
    Fixture::error(
        local_inputs::inspect(&f.home.host, &other, "settings"),
        "Register this repository",
    );
    RepositoryRegistry::register(
        &f.home.host,
        &WriterLock::acquire(&f.home.host).unwrap(),
        &other,
    )
    .unwrap();
    assert_eq!(f.rows(&other, "settings")[0], ("editor".into(), None));

    f.write(TEMPLATE, "'optional'");
    Fixture::error(
        f.preview(),
        "Declare these names in the generated file editor",
    );
    Fixture::error(f.setting("editor", Some("other")), "Declare the input");
    f.write(TEMPLATE, "'editor'");
    assert_eq!(
        f.preview().unwrap().rendered,
        "${EDITOR}/{{ inputs.editor }}"
    );
}

#[test]
fn empty_is_defined_but_unset_and_inactive_inputs_can_be_omitted() {
    let f = Fixture::new(
        "{% if local.editor is defined %}[{{ local.editor }}]{% else %}absent{% endif %}{% if false %}{{ local.later }}{% endif %}",
        "'editor', 'later'",
    );
    assert_eq!(f.preview().unwrap().rendered, "absent");
    assert!(!f.state.exists());
    let set = f.setting("editor", Some("")).unwrap();
    assert_eq!(set.review().name, "editor");
    assert_eq!(set.review().previous, None);
    assert_eq!(set.review().proposed, Some(""));
    assert!(!f.state.exists());
    assert!(set.approve().execute().unwrap());
    assert_eq!(f.preview().unwrap().rendered, "[]");
    assert_eq!(
        f.rows(&f.repo, "settings"),
        [
            ("editor".into(), Some(String::new())),
            ("later".into(), None)
        ]
    );
    let clear = f.setting("editor", None).unwrap();
    assert_eq!(clear.review().previous, Some(""));
    assert_eq!(clear.review().proposed, None);
    assert!(clear.approve().execute().unwrap());
    assert_eq!(f.preview().unwrap().rendered, "absent");
    assert!(
        !f.setting("editor", None)
            .unwrap()
            .approve()
            .execute()
            .unwrap()
    );
}

#[test]
fn manifest_review_can_save_a_new_declaration_before_setting_its_local_value() {
    let f = Fixture::new(
        "{% if inputs.optional is defined %}{{ inputs.optional }}{% endif %}\n{{ local.editor }}",
        "'editor'",
    );
    let path = f.repo.join("dort.toml");
    let text = fs::read_to_string(&path).unwrap();
    let mapping = Manifest::parse(&text).unwrap().files.remove(0);
    assert_eq!(
        Manifest::parse(&toml::to_string(&Manifest::parse(&text).unwrap()).unwrap())
            .unwrap()
            .files[0],
        mapping
    );
    fs::write(&path, "schema = 1\n").unwrap();
    Fixture::error(
        f.setting("editor", Some("vim")),
        "No file with that ID in dort.toml",
    );
    let review = Edit::prepare(
        &f.home.host,
        &f.repo,
        &Change::File(Box::new(FileChange {
            before: None,
            mapping,
        })),
    )
    .unwrap();
    assert!(review.generated().is_none());
    assert_eq!(review.local_input_setup().unwrap().names, ["editor"]);
    assert!(review.human().contains("Save the declaration only"));
    review.approve().execute(&f.home.host).unwrap();
    assert!(!f.state.exists());
    f.set("vim");
    let before = Manifest::parse(&fs::read_to_string(&path).unwrap())
        .unwrap()
        .files
        .remove(0);
    let mut mapping = before.clone();
    mapping.template = Some("editor={{ local.editor }}".into());
    mapping
        .local_inputs
        .as_mut()
        .unwrap()
        .push("optional".into());
    let review = Edit::prepare(
        &f.home.host,
        &f.repo,
        &Change::File(Box::new(FileChange {
            before: Some(before),
            mapping,
        })),
    )
    .unwrap();
    assert!(review.local_input_setup().is_none());
    assert_eq!(review.generated().unwrap().rendered, "editor=vim");
    review.approve().execute(&f.home.host).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("vim"));
    assert_eq!(
        f.rows(&f.repo, "settings"),
        [
            ("editor".into(), Some("vim".into())),
            ("optional".into(), None)
        ]
    );
}

#[test]
fn settings_approvals_stale_on_an_intervening_setting_or_declaration_edit() {
    let f = Fixture::new(TEMPLATE, "'editor', 'theme'");
    let first = f.setting("editor", Some("vim")).unwrap().approve();
    f.setting("theme", Some("dark"))
        .unwrap()
        .approve()
        .execute()
        .unwrap();
    Fixture::error(first.execute(), "Open a new review");
    assert_eq!(
        f.rows(&f.repo, "settings"),
        [
            ("editor".into(), None),
            ("theme".into(), Some("dark".into()))
        ]
    );
    let stale = f.setting("editor", Some("vim")).unwrap().approve();
    f.write(TEMPLATE, "'editor'");
    Fixture::error(stale.execute(), "Open a new review");
    let locked = f.setting("editor", Some("vim")).unwrap().approve();
    let _lock = WriterLock::acquire(&f.home.host).unwrap();
    assert!(matches!(locked.execute(), Err(Error::Locked)));
}

#[test]
fn every_generated_action_rechecks_local_values_even_if_rendered_bytes_are_unchanged() {
    let f = Fixture::new("initial", "'editor'");
    let deploy = || Deployment::prepare(&f.home.host, None, "settings", &[]).unwrap();
    let stale = deploy().approve();
    f.set("");
    Fixture::error(stale.execute(), "Open a new review");
    assert!(!f.home.host.home.join("settings").exists());
    assert_eq!(deploy().review().resolved_inputs["local.editor"].value, "");
    let dest = deploy().approve().execute().unwrap().destination;
    for action in [
        UpdateAction::Update,
        UpdateAction::Replace,
        UpdateAction::Recreate,
    ] {
        f.set("vim");
        f.write("new output", "'editor'");
        match action {
            UpdateAction::Update => (),
            UpdateAction::Replace => fs::write(&dest, "local edit").unwrap(),
            UpdateAction::Recreate => fs::remove_file(&dest).unwrap(),
        }
        let before = fs::read(&dest).ok();
        let prepare =
            || Update::prepare_action(&f.home.host, None, "settings", &[], action).unwrap();
        let pending = prepare();
        assert_eq!(
            pending.review().resolved_inputs["local.editor"].value,
            "vim"
        );
        let stale = pending.approve();
        f.set("helix");
        assert_eq!(prepare().review().proposed, "new output");
        Fixture::error(stale.execute(), "Open a new review");
        assert_eq!(fs::read(&dest).ok(), before);
    }
}

#[test]
fn bounds_and_no_follow_reads_reject_unsafe_private_material_without_leaking_values() {
    let f = Fixture::new(TEMPLATE, "'editor'");
    const SECRET: &str = "token=this-must-stay-private";
    for (value, message) in [
        (SECRET, "non-secret"),
        ("unsafe\0literal", "control characters"),
        (&"a".repeat(VALUE_LIMIT + 1), "16 KiB"),
    ] {
        let error = Fixture::error(f.setting("editor", Some(value)), message);
        assert!(!error.contains(value));
    }
    assert!(!f.state.exists());
    f.set("safe-local-value");
    f.write(
        "{% for item in local.editor %}{{ item }}{% endfor %}",
        "'editor'",
    );
    assert!(!Fixture::error(f.preview(), "Check input types").contains("safe-local-value"));
    f.write(TEMPLATE, "'editor'");
    let slot = fs::read_dir(&f.state)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        fs::metadata(&f.state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&slot).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let original = fs::read_to_string(&slot).unwrap();
    fs::write(&slot, original.replace("safe-local-value", SECRET)).unwrap();
    assert!(!Fixture::error(f.preview(), "non-secret").contains(SECRET));
    fs::write(&slot, format!("unparseable {SECRET}")).unwrap();
    assert!(!Fixture::error(f.preview(), "Cannot read or save").contains(SECRET));
    fs::write(&slot, "x".repeat(1024 * 1024 + 1)).unwrap();
    Fixture::error(f.preview(), "Cannot read or save");
    fs::write(&slot, &original).unwrap();
    fs::set_permissions(&slot, fs::Permissions::from_mode(0o644)).unwrap();
    Fixture::error(f.preview(), "Cannot read or save");
    fs::set_permissions(&slot, fs::Permissions::from_mode(0o600)).unwrap();
    let external = f.repo.join("outside");
    fs::hard_link(&slot, &external).unwrap();
    Fixture::error(f.preview(), "Cannot read or save");
    fs::remove_file(&external).unwrap();
    let approval = f.setting("editor", Some("after-review")).unwrap().approve();
    fs::rename(&slot, &external).unwrap();
    symlink(&external, &slot).unwrap();
    Fixture::error(f.preview(), "Cannot read or save");
    Fixture::error(approval.execute(), "Open a new review");
    assert_eq!(fs::read_to_string(external).unwrap(), original);
}

#[test]
fn combined_host_manifest_and_local_context_is_bounded() {
    let f = Fixture::new(TEMPLATE, "'editor'");
    let path = f.repo.join("dort.toml");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("host_inputs = ['os']\n[files.inputs]\n");
    let max = "x".repeat(VALUE_LIMIT);
    for i in 0..15 {
        text.push_str(&format!("i{i} = '{max}'\n"));
    }
    fs::write(path, text).unwrap();
    Fixture::error(f.setting("editor", Some(&max)), "256 KiB");
    f.set("small");
    assert_eq!(f.preview().unwrap().rendered, "small");
}

#[test]
fn local_declarations_reject_wrong_modes_duplicates_and_invalid_names() {
    for (text, message) in [
        (
            file("settings", "plain", "'editor'").replace("mode = 'template'", "mode = 'link'"),
            "requires template mode",
        ),
        (
            file("settings", "plain", "'editor', 'editor'"),
            "list each local input once",
        ),
        (
            file("settings", "plain", "'not.a.name'"),
            "invalid local input name",
        ),
    ] {
        assert!(
            Manifest::parse(&format!("schema = 1\n{text}"))
                .unwrap_err()
                .to_string()
                .contains(message)
        );
    }
}
