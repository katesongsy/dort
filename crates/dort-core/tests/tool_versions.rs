use dort_core::{
    Manifest,
    model::Provider,
    tools::{UnsupportedVersionPolicy, VersionMatch, VersionPolicy},
};

fn legacy_support(provider: Provider, requested: Option<&str>) -> bool {
    let Some(value) = requested else { return true };
    match provider {
        Provider::Brew | Provider::Apt => value == "latest",
        Provider::Npm => value == "latest" || semver::Version::parse(value).is_ok(),
        Provider::Mise => {
            value == "latest"
                || semver::Version::parse(value).is_ok()
                || (value.split('.').count() <= 2
                    && value.split('.').all(|part| {
                        !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
                    }))
        }
    }
}

#[test]
fn typed_policies_preserve_existing_manifest_acceptance_and_error_text() {
    let values: Vec<String> = [
        "latest",
        "1.2.3",
        "1.2.3-alpha.1+build.9",
        "0",
        "24",
        "24.10",
        "01",
        "01.02",
        "",
        "latest ",
        "v1.2.3",
        "^24",
        "24.*",
        ">=24",
        "1.2.3.4",
        ".1",
        "1.",
        "-1",
        "01.2.3",
        "24\n",
        "24;command",
        "1.2.3 secret",
        "24/1",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(["9".repeat(100)])
    .collect();
    for provider in [Provider::Brew, Provider::Apt, Provider::Mise, Provider::Npm] {
        for requested in
            std::iter::once(None).chain(values.iter().map(|value| Some(value.as_str())))
        {
            let expected = legacy_support(provider, requested);
            assert_eq!(VersionPolicy::parse(provider, requested).is_ok(), expected);
            let mut manifest = Manifest::parse(include_str!("../examples/dort.toml")).unwrap();
            for tool in &mut manifest.tools {
                for install in tool
                    .install
                    .values_mut()
                    .filter(|spec| spec.provider == provider)
                {
                    install.version = requested.map(str::to_owned);
                }
            }
            let result = manifest.validate();
            assert_eq!(result.is_ok(), expected);
            if !expected {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains(&UnsupportedVersionPolicy.to_string())
                );
            }
        }
    }
    assert_eq!(
        UnsupportedVersionPolicy.to_string(),
        "provider cannot enforce this version policy"
    );
}

#[test]
fn provider_default_and_latest_do_not_turn_installed_versions_into_upgrade_requests() {
    for provider in [Provider::Brew, Provider::Apt, Provider::Mise, Provider::Npm] {
        for request in [None, Some("latest")] {
            let policy = VersionPolicy::parse(provider, request).unwrap();
            for installed in [
                "1.0.0",
                "0.0.1",
                "2:2.43.0-1ubuntu7.3",
                "HEAD-a1b2",
                "24.1.0_2",
            ] {
                assert_eq!(
                    policy.matches_version(Some(installed)),
                    VersionMatch::Satisfied
                );
            }
        }
    }
}

#[test]
fn absent_malformed_or_oversized_observations_stay_unverifiable() {
    let oversized = "1".repeat(257);
    let policies = [
        VersionPolicy::parse(Provider::Apt, None).unwrap(),
        VersionPolicy::parse(Provider::Brew, Some("latest")).unwrap(),
        VersionPolicy::parse(Provider::Npm, Some("1.2.3")).unwrap(),
        VersionPolicy::parse(Provider::Mise, Some("24.1")).unwrap(),
    ];
    for policy in policies {
        for installed in [
            None,
            Some(""),
            Some("1.2.3\n"),
            Some("\u{1b}[31m1.2.3"),
            Some("1.2.3 value"),
            Some("版本"),
            Some(oversized.as_str()),
        ] {
            assert_eq!(
                policy.matches_version(installed),
                VersionMatch::Unverifiable
            );
        }
    }
}

#[test]
fn exact_versions_include_prerelease_and_build_metadata() {
    for provider in [Provider::Mise, Provider::Npm] {
        let policy = VersionPolicy::parse(provider, Some("1.2.3-alpha.1+build.9")).unwrap();
        assert_eq!(
            policy.matches_version(Some("1.2.3-alpha.1+build.9")),
            VersionMatch::Satisfied
        );
        for other in [
            "1.2.3-alpha.1+build.10",
            "1.2.3-alpha.2+build.9",
            "1.2.3",
            "1.2.4",
        ] {
            assert_eq!(policy.matches_version(Some(other)), VersionMatch::Mismatch);
        }
        for unverifiable in ["v1.2.3", "2:1.2.3-ubuntu1", "HEAD"] {
            assert_eq!(
                policy.matches_version(Some(unverifiable)),
                VersionMatch::Unverifiable
            );
        }
    }
}

#[test]
fn mise_major_and_minor_policies_are_prefixes_not_caret_requirements() {
    let major = VersionPolicy::parse(Provider::Mise, Some("24")).unwrap();
    let minor = VersionPolicy::parse(Provider::Mise, Some("24.1")).unwrap();
    for installed in ["24.1.0", "24.1.9+build.1"] {
        assert_eq!(
            major.matches_version(Some(installed)),
            VersionMatch::Satisfied
        );
        assert_eq!(
            minor.matches_version(Some(installed)),
            VersionMatch::Satisfied
        );
    }
    assert_eq!(
        major.matches_version(Some("24.9.0")),
        VersionMatch::Satisfied
    );
    assert_eq!(
        minor.matches_version(Some("24.9.0")),
        VersionMatch::Mismatch
    );
    for installed in ["25.0.0", "23.9.9", "24.1.0-rc.1"] {
        assert_eq!(
            major.matches_version(Some(installed)),
            VersionMatch::Mismatch
        );
        assert_eq!(
            minor.matches_version(Some(installed)),
            VersionMatch::Mismatch
        );
    }
}

#[test]
fn legacy_noncanonical_prefixes_are_accepted_but_never_guessed_at() {
    for request in ["01".to_owned(), "24.01".to_owned(), "9".repeat(100)] {
        let policy = VersionPolicy::parse(Provider::Mise, Some(&request)).unwrap();
        assert_eq!(
            policy.matches_version(Some("24.1.0")),
            VersionMatch::Unverifiable
        );
    }
    let zero = VersionPolicy::parse(Provider::Mise, Some("0.0")).unwrap();
    assert_eq!(zero.matches_version(Some("0.0.1")), VersionMatch::Satisfied);
    assert_eq!(zero.matches_version(Some("0.1.0")), VersionMatch::Mismatch);
}

#[test]
fn typed_policy_metadata_roundtrips_without_becoming_approval() {
    for request in [
        None,
        Some("latest"),
        Some("24"),
        Some("24.1"),
        Some("24.1.0+build.1"),
    ] {
        let policy = VersionPolicy::parse(Provider::Mise, request).unwrap();
        let encoded = serde_json::to_vec(&policy).unwrap();
        let decoded: VersionPolicy = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(policy, decoded);
    }
    assert!(
        serde_json::from_str::<VersionPolicy>(r#"{"kind":"latest","command":"not-an-approval"}"#)
            .is_err()
    );
    let malformed: VersionPolicy =
        serde_json::from_str(r#"{"kind":"exact","version":"not-a-version"}"#).unwrap();
    assert_eq!(
        malformed.matches_version(Some("1.2.3")),
        VersionMatch::Unverifiable
    );
}
