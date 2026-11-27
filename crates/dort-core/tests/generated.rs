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
source = "templates/settings"
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
        fs::create_dir(repo.join("templates")).unwrap();
        let fixture = Self { home, repo };
        fixture.manifest("");
        fixture
    }

    fn manifest(&self, extra: &str) {
        fs::write(self.repo.join("dort.toml"), format!("{DECLARATION}{extra}")).unwrap();
    }

    fn template(&self, bytes: impl AsRef<[u8]>) {
        fs::write(self.repo.join("templates/settings"), bytes).unwrap();
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
overrides = { linux = "templates/linux" }
"#,
    );
    f.template("base must not appear");
    let source = f.repo.join("templates/linux");
    let template = "theme={{ theme }}\r\nformat=[{{ format }}]{{ empty }}\n{\"raw\":{}}\n";
    fs::write(&source, template).unwrap();
    let preview = f.preview().unwrap();
    assert_eq!(preview.source.as_ref(), Some(&source));
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
    f.template("new configuration");
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
    f.template("{{ absent }}");
    assert!(f.error("Missing input absent").contains("dort.toml"));
    for expression in [
        "{{ env.HOME }}",
        "{{ name | unsafe }}",
        "{{ unsafe\x1b[31m }}",
        "{{ unsafe",
        "{{ {{ unsafe }} }}",
    ] {
        f.template(expression);
        assert!(!f.error("Use {{ name }}").contains("unsafe"));
    }
    f.template(b"invalid\xffUTF-8");
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
    f.template("é".repeat(32 * 1024));
    assert_eq!(f.preview().unwrap().rendered.len(), 64 * 1024);
    f.template("é".repeat(32 * 1024 + 1));
    f.error("exceeds 64 KiB");
    let value = "é".repeat(8 * 1024);
    f.manifest(&format!("inputs = {{ v = '{value}x' }}"));
    f.template("{{ v }}");
    f.error("exceeds 16 KiB");
    f.manifest(&format!("inputs = {{ v = '{value}' }}"));
    f.template("{{ v }}".repeat(8));
    assert_eq!(f.preview().unwrap().rendered.len(), 128 * 1024);
    f.template(format!("{}x", "{{ v }}".repeat(8)));
    f.error("exceeds 128 KiB");
}

#[test]
fn unsafe_and_sensitive_sources_never_render() {
    let f = Fixture::new();
    f.error("source is missing");
    let source = f.repo.join("templates/settings");
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
    f.template("private configuration");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o4700)).unwrap();
    f.error("special permission bits");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    f.template("password=never-show-this");
    assert!(!f.error("may contain secrets").contains("never-show-this"));
    fs::write(
        f.repo.join("dort.toml"),
        DECLARATION.replace("templates/settings", ".ssh/config"),
    )
    .unwrap();
    f.error("sensitive path");
    fs::remove_dir_all(f.repo.join("templates")).unwrap();
    symlink(&f.home.host.home, f.repo.join("templates")).unwrap();
    f.manifest("");
    f.error("Cannot inspect template source safely");
}

#[test]
fn secret_sources_block_path_overlap_and_aliases_including_inactive_overrides() {
    let f = Fixture::new();
    f.template("not recognizable by a keyword");
    for secret_source in [
        "templates/settings",
        "templates",
        "templates/settings/nested",
    ] {
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
    symlink("templates/settings", f.repo.join("alias")).unwrap();
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

fn inline(template: &str, extra: &str) -> String {
    let declaration = DECLARATION.replace(
        "source = \"templates/settings\"",
        &format!("template = {}", toml::Value::String(template.into())),
    );
    format!("{declaration}{extra}")
}

#[test]
fn inline_template_needs_no_source_and_uses_the_same_render_rules() {
    let f = Fixture::new();
    let save = |template: &str, extra: &str| {
        fs::write(f.repo.join("dort.toml"), inline(template, extra)).unwrap()
    };
    save(
        "[{{ z }}]\n{{ a }}{{\tz }}\n",
        "inputs = { a = '', z = ' x ' }",
    );
    let preview = f.preview().unwrap();
    assert_eq!(preview.source, None);
    assert_eq!(preview.rendered, "[ x ]\n x \n");
    let plan = planner::plan(planner::observe(&f.home.host, &f.repo, &[], Request::Apply).unwrap());
    assert!(plan.has_blockers() && plan.files[0].effects.is_empty());
    assert!(plan.inputs.files[0].source.is_none());

    for (template, extra, error) in [
        ("{{ env.HOME }}", "", "Use {{ name }}"),
        ("{{ missing }}", "", "Missing input missing"),
        ("password=never-show", "", "may contain secrets"),
        (
            "{{ v }}",
            "inputs = { v = 'Bearer never-show' }",
            "may contain secrets",
        ),
        (
            "{{ key }} = never-show",
            "inputs = { key = 'password' }",
            "may contain secrets",
        ),
    ] {
        save(template, extra);
        assert!(!f.error(error).contains("never-show"));
    }
    let large_value = "é".repeat(8 * 1024);
    save(&"é".repeat(32 * 1024), "");
    assert_eq!(f.preview().unwrap().rendered.len(), 64 * 1024);
    save(&"é".repeat(32 * 1024 + 1), "");
    f.error("exceeds 64 KiB");
    save("{{ v }}", &format!("inputs = {{ v = '{large_value}x' }}"));
    f.error("exceeds 16 KiB");
    save(
        &"{{ v }}".repeat(8),
        &format!("inputs = {{ v = '{large_value}' }}"),
    );
    assert_eq!(f.preview().unwrap().rendered.len(), 128 * 1024);
    save(
        &"{{ v }}".repeat(9),
        &format!("inputs = {{ v = '{large_value}' }}"),
    );
    f.error("exceeds 128 KiB");
}

#[test]
fn inline_template_and_repository_source_are_distinct_persisted_forms() {
    for invalid in [
        inline("hello", "source = 'file'"),
        inline("hello", "overrides = { linux = 'file' }"),
        inline("hello", "").replace(r#"mode = "template""#, r#"mode = "link""#),
        inline("hello", "").replace(r#"mode = "template""#, r#"mode = "secret""#),
        DECLARATION.replace("source = \"templates/settings\"", ""),
        DECLARATION.replace("templates/settings", ""),
    ] {
        assert!(Manifest::parse(&invalid).is_err(), "{invalid}");
    }
    for mode in ["link", "secret", "template"] {
        let parsed = Manifest::parse(
            &DECLARATION.replace(r#"mode = "template""#, &format!("mode = {mode:?}")),
        )
        .unwrap();
        let encoded = toml::to_string(&parsed).unwrap();
        assert!(encoded.contains("source = \"templates/settings\""));
        assert!(!encoded.contains("template ="));
        assert_eq!(Manifest::parse(&encoded).unwrap().files, parsed.files);
    }
}

#[test]
fn add_generated_template_after_existing_file_mappings() {
    use dort_core::{
        local::RepositoryRegistry,
        manifest_edit::{Change, FileChange, Preview as EditPreview},
        state::WriterLock,
    };
    let f = Fixture::new();
    RepositoryRegistry::register(
        &f.home.host,
        &WriterLock::acquire(&f.home.host).unwrap(),
        &f.repo,
    )
    .unwrap();
    let path = f.repo.join("dort.toml");
    let original = r#"schema = 1

[profiles]
available = ["work"]

[[files]]
id = "shell"
mode = "link"
source = "files/shell"
target = "~/.shell"
overrides = { linux = "files/shell-linux" }

[[files]]
id = "editor"
mode = "link"
source = "files/editor"
target = "~/.editor"
"#;
    fs::write(&path, original).unwrap();
    let mapping = Manifest::parse(&inline("theme={{ theme }}", "inputs = { theme = 'dark' }"))
        .unwrap()
        .files
        .remove(0);
    let edit = EditPreview::prepare(
        &f.home.host,
        &f.repo,
        &Change::File(Box::new(FileChange {
            before: None,
            mapping: mapping.clone(),
        })),
    )
    .unwrap();
    assert_eq!(edit.generated().unwrap().rendered, "theme=dark");
    edit.approve().execute(&f.home.host).unwrap();
    let mut expected = Manifest::parse(original).unwrap().files;
    expected.push(mapping);
    assert_eq!(
        Manifest::parse(&fs::read_to_string(path).unwrap())
            .unwrap()
            .files,
        expected
    );
}

#[test]
fn generated_edit_previews_the_replacement_and_preserves_other_declarations() {
    use dort_core::{
        local::RepositoryRegistry,
        manifest_edit::{Change, FileChange, Preview as EditPreview},
        state::WriterLock,
    };
    let f = Fixture::new();
    RepositoryRegistry::register(
        &f.home.host,
        &WriterLock::acquire(&f.home.host).unwrap(),
        &f.repo,
    )
    .unwrap();
    let path = f.repo.join("dort.toml");
    let original = inline(
        "theme={{ theme }}",
        r#"
profiles = ["work"]
requires = ["editor"]
permissions = "0600"
[files.inputs]
theme = "dark"

[profiles]
available = ["work"]

# Keep this note.
[[tools]]
id = "editor"
binary = "editor"
install = { provider = "brew", package = "editor" }
"#,
    );
    fs::write(&path, &original).unwrap();
    let destination = f.preview().unwrap().destination;
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, "existing HOME file").unwrap();
    let before = Manifest::parse(&original).unwrap().files.remove(0);
    let mut mapping = before.clone();
    mapping.template = Some("new text\r\n  ".into());
    mapping.inputs.clear();
    let edit = EditPreview::prepare(
        &f.home.host,
        &f.repo,
        &Change::File(Box::new(FileChange {
            before: Some(before),
            mapping: mapping.clone(),
        })),
    )
    .unwrap();
    let reviewed = edit.generated().unwrap().rendered.clone();
    assert_eq!(reviewed, "new text\r\n  ");
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let backup = edit.approve().execute(&f.home.host).unwrap().unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), original);
    let stored = fs::read_to_string(path).unwrap();
    assert!(stored.contains("# Keep this note."));
    let manifest = Manifest::parse(&stored).unwrap();
    assert_eq!(manifest.files, [mapping]);
    assert_eq!(manifest.tools[0].id, "editor");
    assert!(!stored.contains("[files.inputs]"));
    assert_eq!(f.preview().unwrap().rendered, reviewed);
    assert_eq!(
        fs::read_to_string(destination).unwrap(),
        "existing HOME file"
    );
}

#[test]
fn editing_legacy_recipe_writes_only_template_and_preserves_comments() {
    use dort_core::{
        local::RepositoryRegistry,
        manifest_edit::{Change, FileChange, Preview as EditPreview},
        state::WriterLock,
    };
    let f = Fixture::new();
    RepositoryRegistry::register(
        &f.home.host,
        &WriterLock::acquire(&f.home.host).unwrap(),
        &f.repo,
    )
    .unwrap();
    let path = f.repo.join("dort.toml");
    let original = inline("theme={{ theme }}", "inputs = { theme = 'dark' }")
        .replacen("\ntemplate =", "\n# Template note\nrecipe =", 1)
        .replacen("\ntarget =", " # Keep inline\ntarget =", 1);
    fs::write(&path, &original).unwrap();
    assert_eq!(f.preview().unwrap().rendered, "theme=dark");
    assert!(
        Manifest::parse(&original.replace("\ninputs =", "\ntemplate = 'duplicate'\ninputs ="))
            .is_err()
    );
    let before = Manifest::parse(&original).unwrap().files.remove(0);
    for content in ["theme={{ theme }}", "new={{ theme }}"] {
        fs::write(&path, &original).unwrap();
        let mut mapping = before.clone();
        mapping.inputs.insert("theme".into(), "light".into());
        mapping.template = Some(content.into());
        EditPreview::prepare(
            &f.home.host,
            &f.repo,
            &Change::File(Box::new(FileChange {
                before: Some(before.clone()),
                mapping: mapping.clone(),
            })),
        )
        .unwrap()
        .approve()
        .execute(&f.home.host)
        .unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# Template note\ntemplate ="), "{saved}");
        assert!(saved.contains(" # Keep inline"), "{saved}");
        assert!(!saved.contains("\nrecipe ="), "{saved}");
        assert!(!saved.contains("\nsyntax ="), "{saved}");
        assert_eq!(Manifest::parse(&saved).unwrap().files, [mapping]);
        assert_eq!(
            f.preview().unwrap().rendered,
            content.replace("{{ theme }}", "light")
        );
    }
}

#[test]
fn jinja_uses_explicit_namespaces_and_reports_values_and_provenance() {
    use dort_core::{
        generated::{HostInput, InputProvenance, ResolvedInput, input_names_for},
        template::Dialect,
    };
    let f = Fixture::new();
    f.manifest(
        r#"
syntax = "jinja"
host_inputs = ["os", "arch"]
inputs = { title = "míxed <&>", host = "manifest host", spare = "unused" }
"#,
    );
    let text = concat!(
        "{% if host.os == 'linux' and host.user is not defined %}",
        "{% for item in [inputs.title, host.arch, inputs.host] %}",
        "{{ loop.index }}={{ item }};{% endfor %}",
        "{% endif %}\n",
    );
    f.template(text);
    let p = f.preview().unwrap();
    assert_eq!(p.syntax, Dialect::Jinja);
    assert_eq!(p.rendered, "1=míxed <&>;2=x86_64;3=manifest host;\n");
    assert_eq!(p.resolved_inputs.len(), 5);
    assert_eq!(
        p.resolved_inputs["host.os"],
        ResolvedInput {
            value: "linux".into(),
            provenance: InputProvenance::Host(HostInput::Os),
        }
    );
    assert_eq!(
        p.resolved_inputs["inputs.spare"],
        ResolvedInput {
            value: "unused".into(),
            provenance: InputProvenance::Manifest,
        }
    );
    assert_eq!(
        input_names_for("settings", Dialect::Jinja, text).unwrap(),
        [
            "host.arch",
            "host.os",
            "host.user",
            "inputs.host",
            "inputs.title"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    );
    assert_eq!(
        serde_json::to_value(&p).unwrap()["resolved_inputs"]["host.os"],
        serde_json::json!({
            "value": "linux", "provenance": { "host": "os" }
        })
    );
}

#[test]
fn literal_and_jinja_never_fall_back_or_read_undeclared_values() {
    let f = Fixture::new();
    let source = "{% if false %}{{ theme }}{% endif %}\r\n{# literal comment #}";
    f.manifest("inputs = { theme = 'dark' }\n");
    f.template(source);
    let legacy = f.preview().unwrap();
    assert_eq!(legacy.rendered, source.replace("{{ theme }}", "dark"));
    assert_eq!(legacy.resolved_inputs["theme"].value, "dark");
    f.template("{{ host.os }}");
    f.error("Use {{ name }}");

    f.manifest("syntax = 'jinja'\ninputs = { theme = 'dark' }\n");
    for text in [
        "{{ theme }}",
        "{{ host.os }}",
        "{{ local.theme }}",
        "{{ inputs.absent }}",
    ] {
        f.template(text);
        f.error("Supply the missing input");
    }
    f.template(
        "{% if host.os is defined or local.theme is defined %}wrong{% endif %}{{ inputs.theme }}",
    );
    let p = f.preview().unwrap();
    assert_eq!(p.rendered, "dark");
    assert_eq!(p.resolved_inputs.len(), 1);

    f.template("{% if false %}{% include 'do-not-echo' %}{% endif %}");
    assert!(
        !f.error("Use substitutions, conditions")
            .contains("do-not-echo")
    );
    f.template("{{ inputs['theme'] }}");
    f.error("Use substitutions, conditions");
}

#[test]
fn host_identity_uses_the_os_and_paths_use_resolved_dort_directories() {
    use dort_core::context::HostContext;
    use std::process::Command;
    const CHILD: &str = "DORT_HOST_IDENTITY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "host_identity_uses_the_os_and_paths_use_resolved_dort_directories",
            ])
            .env(CHILD, "1")
            .env("USER", "forged-user")
            .env("LOGNAME", "forged-logname")
            .env("HOSTNAME", "forged-hostname")
            .env("ComputerName", "forged-computer-name")
            .env("HOME", "/never-use-this-home")
            .env("XDG_CONFIG_HOME", "/never-use-this-config")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }
    let f = Fixture::new();
    f.manifest("syntax = 'jinja'\nhost_inputs = ['user','hostname','home','config_home']\n");
    f.template("{{ host.user }}\n{{ host.hostname }}\n{{ host.home }}\n{{ host.config_home }}\n");
    let command = |name, args: &[&str]| {
        let output = Command::new(name).args(args).output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    let account = command("id", &["-un"]);
    let hostname = command("hostname", &[]);
    for config_home in [None, Some(f.home.host.home.join("xdg-config"))] {
        let host = HostContext::new(
            f.home.host.home.clone(),
            config_home.clone(),
            None,
            f.home.host.platform,
            f.home.host.architecture,
        )
        .unwrap();
        let p = Preview::prepare(&host, &f.repo, "settings").unwrap();
        let expected_config_home = config_home.unwrap_or_else(|| host.home.join(".config"));
        assert_eq!(
            p.rendered,
            format!(
                "{account}{hostname}{}\n{}\n",
                host.home.display(),
                expected_config_home.display()
            )
        );
    }
}

#[test]
fn unavailable_host_input_names_the_field_and_next_action() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let mut f = Fixture::new();
    f.home.host.config_dir = f
        .home
        .host
        .home
        .join(OsStr::from_bytes(b"private-marker\xff"))
        .join("dort");
    f.manifest("syntax = 'jinja'\nhost_inputs = ['config_home']\n");
    f.template("{{ host.config_home }}");
    assert!(
        !f.error("Cannot read host.config_home as text. Use a UTF-8 directory path.")
            .contains("private-marker")
    );
    f.manifest("syntax = 'jinja'\n");
    f.template("literal without host inputs");
    assert!(f.preview().is_ok());
}

#[test]
fn template_options_are_strictly_scoped() {
    let f = Fixture::new();
    for (mode, options, expected) in [
        ("link", "syntax = 'jinja'", "syntax requires template mode"),
        (
            "secret",
            "host_inputs = []",
            "host_inputs requires template mode",
        ),
        (
            "template",
            "host_inputs = ['os']",
            "host_inputs requires syntax",
        ),
        (
            "template",
            "syntax = 'literal'\nhost_inputs = []",
            "host_inputs requires syntax",
        ),
        (
            "template",
            "syntax = 'jinja'\nhost_inputs = ['os','os']",
            "list each host input once",
        ),
    ] {
        let text = format!(
            "{}\n{options}\n",
            DECLARATION.replace("\"template\"", &format!("\"{mode}\""))
        );
        let error = Manifest::parse(&text).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
    for options in [
        "syntax = 'other'",
        "syntax = 'jinja'\nhost_inputs = ['env']",
        "syntax = 'jinja'\ninputs = { value = ['a'] }",
    ] {
        f.manifest(options);
        f.error("Check syntax, host_inputs and local_inputs");
    }
}

#[test]
fn jinja_uses_the_same_source_value_and_output_limits() {
    let f = Fixture::new();
    f.manifest("syntax = 'jinja'\n");
    f.template("x".repeat(64 * 1024 + 1));
    f.error("exceeds 64 KiB");
    let value = "x".repeat(16 * 1024);
    f.manifest(&format!(
        "syntax = 'jinja'\ninputs = {{ v = '{value}x' }}\n"
    ));
    f.error("exceeds 16 KiB");
    f.manifest(&format!("syntax = 'jinja'\ninputs = {{ v = '{value}' }}\n"));
    f.template("{{ inputs.v }}".repeat(8));
    assert_eq!(f.preview().unwrap().rendered.len(), 128 * 1024);
    f.template("{{ inputs.v }}".repeat(9));
    f.error("Keep the rendered text within 128 KiB");
}

#[test]
fn pending_jinja_edit_previews_inputs_and_persists_explicit_options() {
    use dort_core::{
        generated::{HostInput, InputProvenance},
        local::RepositoryRegistry,
        manifest_edit::{Change, FileChange, Preview as EditPreview},
        state::WriterLock,
        template::Dialect,
    };
    let f = Fixture::new();
    RepositoryRegistry::register(
        &f.home.host,
        &WriterLock::acquire(&f.home.host).unwrap(),
        &f.repo,
    )
    .unwrap();
    let original = inline("theme={{ theme }}", "inputs = { theme = 'dark' }");
    let path = f.repo.join("dort.toml");
    fs::write(&path, &original).unwrap();
    let before = Manifest::parse(&original).unwrap().files.remove(0);
    let mut mapping = before.clone();
    mapping.template = Some("{{ host.os }} theme={{ inputs.theme }}".into());
    mapping.syntax = Some(Dialect::Jinja);
    mapping.host_inputs = Some([HostInput::Os].into_iter().collect());
    mapping.inputs.insert("theme".into(), "light".into());
    let edit = EditPreview::prepare(
        &f.home.host,
        &f.repo,
        &Change::File(Box::new(FileChange {
            before: Some(before),
            mapping: mapping.clone(),
        })),
    )
    .unwrap();
    let p = edit.generated().unwrap();
    assert_eq!(p.rendered, "linux theme=light");
    assert_eq!(
        p.resolved_inputs["host.os"].provenance,
        InputProvenance::Host(HostInput::Os)
    );
    assert_eq!(p.resolved_inputs["inputs.theme"].value, "light");
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    edit.approve().execute(&f.home.host).unwrap();
    assert_eq!(
        Manifest::parse(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .files,
        [mapping]
    );
    assert_eq!(f.preview().unwrap().rendered, "linux theme=light");

    let before = Manifest::parse(&fs::read_to_string(&path).unwrap())
        .unwrap()
        .files
        .remove(0);
    let mut mapping = before.clone();
    mapping.template = Some("literal theme={{ theme }}".into());
    mapping.syntax = None;
    mapping.host_inputs = None;
    let edit = EditPreview::prepare(
        &f.home.host,
        &f.repo,
        &Change::File(Box::new(FileChange {
            before: Some(before),
            mapping,
        })),
    )
    .unwrap();
    assert_eq!(edit.generated().unwrap().rendered, "literal theme=light");
    edit.approve().execute(&f.home.host).unwrap();
    let stored = fs::read_to_string(path).unwrap();
    assert!(!stored.contains("syntax"));
    assert!(!stored.contains("host_inputs"));
}

#[test]
fn literal_templates_keep_large_unreferenced_manifest_inputs_compatible() {
    let f = Fixture::new();
    let value = "x".repeat(16 * 1024);
    let mut extra = String::from("[files.inputs]\nwinner = 'visible'\n");
    for i in 0..17 {
        extra.push_str(&format!("unused_{i} = '{value}'\n"));
    }
    f.manifest(&extra);
    f.template("{{ winner }}");
    assert_eq!(f.preview().unwrap().rendered, "visible");
}
