use super::{Fixture, KeyCode, fs, key};

fn provider(app: &mut super::App, field: usize, row: usize) {
    app.tools_view.declaration.focus = field;
    assert!(key(app, KeyCode::Char(' ')).is_none());
    assert!(app.choice.is_some());
    assert!(key(app, KeyCode::Home).is_none());
    for _ in 0..row {
        assert!(key(app, KeyCode::Down).is_none());
    }
    assert!(key(app, KeyCode::Enter).is_none());
    assert!(app.choice.is_none());
    assert_eq!(app.tools_view.declaration.focus, field);
}

#[test]
fn platform_choices_retain_independent_even_empty_drafts_without_execution() {
    let f = Fixture::new();
    let mut app = f.app(false);
    let original = fs::read(f.repo.join("dort.toml")).unwrap();
    app.tools_view.declaration.fields[5].clear();
    provider(&mut app, 4, 2); // macOS mise, not a provider query.
    assert_eq!(app.tools_view.declaration.fields[4], "mise");
    app.tools_view.declaration.fields[5] = "node".into();
    app.tools_view.declaration.fields[6] = "22".into();
    provider(&mut app, 4, 3);
    assert_eq!(app.tools_view.declaration.fields[4], "npm");
    assert!(app.tools_view.declaration.controls().contains(&7));
    app.tools_view.declaration.fields[5] = "example-cli".into();
    app.tools_view.declaration.fields[7] = "node-runtime".into();
    provider(&mut app, 4, 1);
    assert_eq!(app.tools_view.declaration.fields[4], "brew");
    assert_eq!(app.tools_view.declaration.fields[5], "");
    assert_eq!(app.tools_view.declaration.fields[9], "demo");
    provider(&mut app, 4, 2);
    assert_eq!(&app.tools_view.declaration.fields[5..7], &["node", "22"]);
    provider(&mut app, 4, 3);
    assert_eq!(app.tools_view.declaration.fields[5], "example-cli");
    assert_eq!(app.tools_view.declaration.fields[7], "node-runtime");
    let fields = app.tools_view.declaration.fields.clone();
    assert!(key(&mut app, KeyCode::F(3)).is_none());
    assert!(app.tools_view.declaration.details);
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(key(&mut app, KeyCode::Esc).is_none());
    assert!(app.tools_view.declaration_open && !app.tools_view.declaration.details);
    assert_eq!(app.tools_view.declaration.fields, fields);
    assert!(key(&mut app, KeyCode::F(4)).is_none());
    assert!(app.tools_view.declaration.controls().contains(&3));
    assert!(key(&mut app, KeyCode::F(4)).is_none());
    assert!(!app.tools_view.declaration.controls().contains(&3));
    assert!(app.preview.is_none() && app.confirmation.is_none() && app.receipt.is_none());
    assert_eq!(fs::read(f.repo.join("dort.toml")).unwrap(), original);
}
