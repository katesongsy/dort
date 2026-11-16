use super::{App, KeyCode, Target, click, key, navigation};

#[test]
fn menu_arrows_select_sections_and_destinations_without_dispatch() {
    for (w, h) in [(48, 14), (80, 24), (140, 40)] {
        let mut app = App::new(false);
        app.open_menu(0, w);
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
            assert_eq!(app.navigation.menu.unwrap().group, group);
            for selected in navigation::entry_order(group) {
                let menu = app.navigation.menu.unwrap();
                assert_eq!((menu.group, menu.selected), (group, selected));
                let screen = super::render(&app, w, h);
                assert!(screen.contains(navigation::GROUPS[group]));
                assert!(app.preview.is_none() && app.confirmation.is_none());
                key(&mut app, KeyCode::Right, w, h);
            }
            key(&mut app, KeyCode::Home, w, h);
            assert_eq!(app.navigation.menu.unwrap().selected, 0);
        }
        key(&mut app, KeyCode::Up, w, h);
        assert_eq!(app.navigation.menu.unwrap().group, 2);
        key(&mut app, KeyCode::Down, w, h);
        assert_eq!(
            app.navigation.menu.unwrap().group,
            navigation::GROUPS.len() - 1
        );
        key(&mut app, KeyCode::F(1), w, h);
        assert!(app.help.is_some());
        app.open_menu(2, w);
        assert!(app.help.is_some() && app.interaction_notice.is_some());
        assert_eq!(
            app.navigation.menu.unwrap().group,
            navigation::GROUPS.len() - 1
        );
    }
}
