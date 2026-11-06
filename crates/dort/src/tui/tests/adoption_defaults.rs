use super::{batch_ui::Editor, shell::AdoptionDraft};
use std::{collections::BTreeSet, path::Path};

#[test]
fn single_and_batch_defaults_preserve_home_relative_paths_not_machine_home_names() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    for name in ["home-one", "different-home"] {
        let home = root.join(name);
        std::fs::create_dir(&home).unwrap();
        let paths: BTreeSet<_> = [
            ".zshrc",
            ".config/nvim/init.lua",
            ".config/other/init.lua",
            ".config/a space/設定.lua",
        ]
        .into_iter()
        .map(|p| home.join(p))
        .collect();
        let mut batch = Editor::new(&paths, &home);
        let ids: BTreeSet<_> = batch.drafts.iter().map(|d| &d.id).collect();
        assert_eq!(ids.len(), paths.len());
        for (path, draft) in paths.iter().zip(&batch.drafts) {
            let mut single = AdoptionDraft::new();
            single.suggest(path, &home);
            assert_eq!(single.target, path.to_str().unwrap());
            assert_eq!(single.source, draft.source);
            assert_eq!(single.id, draft.id);
            assert_eq!(
                Path::new(&single.source),
                Path::new("files").join(path.strip_prefix(&home).unwrap())
            );
            assert!(!single.source.contains(name));
        }
        assert!(batch.drafts.iter().any(|d| d.source == "files/.zshrc"));
        assert!(batch.drafts.iter().any(|d| d.id == "config-nvim-init-lua"));
        // User edits survive reopening; only newly marked files receive defaults.
        batch.drafts[0].source = "custom/layout".into();
        batch.drafts[0].id = "custom-id".into();
        let mut expanded = paths.clone();
        expanded.insert(home.join(".config/zz/new.lua"));
        batch.sync(&expanded, &home);
        assert_eq!(batch.drafts[0].source, "custom/layout");
        assert_eq!(batch.drafts[0].id, "custom-id");
        assert!(
            batch
                .drafts
                .iter()
                .any(|d| d.source == "files/.config/zz/new.lua")
        );
    }
}
