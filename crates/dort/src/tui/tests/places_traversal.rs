use super::{App, KeyCode, Target, click, key, navigation};

#[test]
fn mouse_changes_places_groups_without_opening_navigation_through_other_dialogs() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.open_menu(0, w);
        let groups = super::render(&app, w, h);
        assert!(groups.contains("Session ▸"));
        assert!(!groups.contains("\\u{25b8}"));
        for group in 0..navigation::GROUPS.len() {
            assert!(
                click(
                    &mut app,
                    w,
                    h,
                    |t| matches!(t, Target::MenuGroup(index) if index == group)
                )
                .is_none()
            );
            let menu = app.navigation.menu.unwrap();
            assert_eq!(menu.group, group);
            assert!(menu.expanded);
            assert!(app.preview.is_none() && app.confirmation.is_none());
            key(&mut app, KeyCode::Left, w, h);
            assert!(!app.navigation.menu.unwrap().expanded);
        }
        key(&mut app, KeyCode::F(1), w, h);
        assert!(app.help.is_some());
        app.open_menu(2, w);
        assert!(app.help.is_some() && app.interaction_notice.is_some());
        let rendered = super::render(&app, w, h);
        assert!(
            rendered.contains("Dialog open:"),
            "{w}: restriction must be visible"
        );
        assert_eq!(
            app.navigation.menu.unwrap().group,
            navigation::GROUPS.len() - 1
        );
    }
}
