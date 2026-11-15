//! Test projection of actual view actions; never a runtime input/permission source.
pub fn keys(app: &super::app::App) -> Vec<(&'static str, &'static str)> {
    super::tests::render(app, 80, 24);
    super::workbench::actions(app)
        .into_iter()
        .map(|a| (a.key, a.label))
        .collect()
}
