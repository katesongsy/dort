use super::*;
#[test]
fn explicit_cross_catalog_prerequisites_are_fresh_and_never_installed() {
    for missing in [false, true] {
        let f = Fixture::new();
        fs::write(f.repo.join("dort.toml"),MANIFEST.replace("binary='demo'","binary='demo'\nrequires=['prereq']")+"\n[[tools]]\nid='prereq'\nbinary='prereq'\n[tools.install]\nubuntu={provider='mise',package='prereq',version='1.2.3'}\n").unwrap();
        let mise = f.program("mise");
        fs::write(&mise,"#!/bin/sh\nprintf mise >> \"$HOME/calls\"\nif [ -f \"$HOME/prereq-missing\" ]; then printf '{}'; else printf '{\"prereq\":[{\"version\":\"1.2.3\",\"installed\":true}]}'; fi\n").unwrap();
        fs::set_permissions(&mise, fs::Permissions::from_mode(0o700)).unwrap();
        let prereq = inspection::Preview::new(
            Query::MiseInstalled,
            mise.clone(),
            f.home.host.home.clone(),
            f.request().search_dirs,
        )
        .unwrap()
        .approve()
        .inspect();
        let target = f.report();
        let mut scope = f.scope();
        scope.catalogs.push(npm::Catalog {
            query: Query::MiseInstalled,
            executable: mise,
            search_dirs: f.request().search_dirs,
            npm_binding: None,
            tools: BTreeSet::from(["prereq".into()]),
        });
        let mut req = f.request();
        req.bound_tools.insert("prereq".into());
        let p = apt::prepare(&f.home.host, req, scope, &[&target, &prereq]).unwrap();
        if missing {
            f.flag("prereq-missing");
            assert!(f.execute(p).is_err());
            assert!(!f.calls().contains("sudo "));
        } else {
            assert_eq!(
                f.execute(p).unwrap().outcome.unwrap().package,
                PackageState::ProviderCurrentPresent
            );
        }
        assert!(!f.calls().contains("mise install"));
    }
}
#[test]
fn malformed_file_journal_blocks_before_candidate_but_not_dpkg_closure() {
    let f = Fixture::new();
    let p = f.preview();
    let local = dort_core::local::LocalConfig::load(&f.home.host).unwrap();
    let id = local.repositories.keys().next().unwrap();
    let parent = dort_core::local::repository_state_path(&f.home.host, id)
        .unwrap()
        .join("transactions");
    let path = parent.join("a".repeat(64));
    fs::create_dir_all(&path).unwrap();
    for d in [
        path,
        parent.clone(),
        parent.parent().unwrap().to_owned(),
        f.home.host.state_dir.join("repositories"),
    ] {
        fs::set_permissions(d, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let calls = f.calls();
    assert!(f.execute(p).is_err());
    assert_eq!(calls, f.calls());
    let root = f.home.host.state_dir.join("tools-attempts");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let id = "b".repeat(64);
    fs::create_dir(root.join(&id)).unwrap();
    fs::set_permissions(root.join(&id), fs::Permissions::from_mode(0o700)).unwrap();
    let mut req = close_request(id);
    req.apt = true;
    req.executable = Some(f.program("dpkg-query"));
    req.search_dirs = f.request().search_dirs;
    let p = install::prepare_close(&f.home.host, req).unwrap();
    assert!(
        install::close(&f.home.host, &f.lock, p.approve())
            .unwrap()
            .explicitly_closed
    );
    assert!(!f.calls().contains("cache ") && !f.calls().contains("sudo "));
}
#[test]
fn candidate_source_drift_and_outcome_persistence_failure_do_not_retry() {
    let f = Fixture::new();
    let p = f.preview();
    fs::write(
        f.program("apt-cache"),
        format!("{CACHE}\nprintf '\\n# changed\\n' >> \"$0\"\n"),
    )
    .unwrap();
    // Bind the deliberately mutating fake as the explicit reviewed program.
    drop(p);
    let p = f.preview();
    assert!(f.execute(p).is_err());
    assert!(!f.calls().contains("sudo "));
    assert!(!f.home.host.state_dir.join("tools-attempts").exists());

    let f = Fixture::new();
    let p = f.preview();
    let result = install::execute(&f.home.host, &f.lock, p.approve(), |e| {
        if e == Event::PostInspection {
            let attempts = install::diagnose(&f.home.host).unwrap();
            let path = f
                .home
                .host
                .state_dir
                .join("tools-attempts")
                .join(&attempts[0].id)
                .join("outcome.json");
            fs::create_dir(&path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    });
    assert!(result.is_err());
    assert_eq!(f.calls().matches("sudo ").count(), 1);
    assert!(install::diagnose(&f.home.host).unwrap()[0].unresolved);
}
#[test]
fn apt_crash_child() {
    let Some(home) = std::env::var_os("DORT_APT_CRASH_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let host = dort_core::context::HostContext::new(
        home.clone(),
        None,
        None,
        Platform::Ubuntu,
        dort_core::platform::Architecture::X86_64,
    )
    .unwrap();
    let req = install::Request {
        repository: home.parent().unwrap().join("repo"),
        profiles: vec![],
        tool: "demo".into(),
        executable: home.join("bin/dpkg-query"),
        search_dirs: vec![home.join("bin")],
        tool_search_dirs: vec![home.join("lookup")],
        bound_tools: BTreeSet::from(["demo".into()]),
    };
    let scope = apt::Scope {
        apt_cache: home.join("bin/apt-cache"),
        apt_get: home.join("bin/apt-get"),
        sudo: home.join("bin/sudo"),
        policy: VersionPolicy::Latest {},
        catalogs: vec![npm::Catalog {
            query: Query::DpkgPackages,
            executable: req.executable.clone(),
            search_dirs: req.search_dirs.clone(),
            npm_binding: None,
            tools: req.bound_tools.clone(),
        }],
    };
    let report = inspection::Preview::new(
        Query::DpkgPackages,
        req.executable.clone(),
        home,
        req.search_dirs.clone(),
    )
    .unwrap()
    .approve()
    .inspect();
    let p = apt::prepare(&host, req, scope, &[&report]).unwrap();
    let lock = WriterLock::acquire(&host).unwrap();
    let stage = std::env::var("DORT_APT_CRASH_STAGE").unwrap();
    install::execute(&host, &lock, p.approve(), |e| {
        if format!("{e:?}") == stage {
            std::process::exit(86);
        }
    })
    .unwrap();
    panic!("crash boundary not reached");
}
#[test]
fn process_exit_boundaries_leave_honest_unresolved_or_durable_outcomes_without_replay() {
    for stage in [
        "IntentDurable",
        "Installing",
        "PostInspection",
        "OutcomeDurable",
    ] {
        let f = Fixture::new();
        drop(f.lock);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "boundaries::apt_crash_child", "--test-threads=1"])
            .env_clear()
            .env("DORT_APT_CRASH_HOME", &f.home.host.home)
            .env("DORT_APT_CRASH_STAGE", stage)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(86),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let before = fs::read_to_string(f.home.host.home.join("calls")).unwrap();
        let attempts = install::diagnose(&f.home.host).unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].unresolved, stage != "OutcomeDurable");
        assert_eq!(
            before,
            fs::read_to_string(f.home.host.home.join("calls")).unwrap()
        );
        if stage != "OutcomeDurable" {
            let lock = WriterLock::acquire(&f.home.host).unwrap();
            let p = install::prepare_close(&f.home.host, close_request(attempts[0].id.clone()))
                .unwrap();
            assert!(
                install::close(&f.home.host, &lock, p.approve())
                    .unwrap()
                    .explicitly_closed
            );
        }
    }
}
