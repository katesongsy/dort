use dort_core::{
    model::Provider,
    tools::{VersionMatch, VersionPolicy},
};

#[test]
fn provider_policies_are_explicit_and_enforceable() {
    for provider in [Provider::Brew, Provider::Npm] {
        assert_eq!(
            VersionPolicy::parse(provider, None).unwrap(),
            VersionPolicy::ProviderDefault {}
        );
        assert_eq!(
            VersionPolicy::parse(provider, Some("latest")).unwrap(),
            VersionPolicy::Latest {}
        );
        for invalid in ["", "22", "22.1", "^1.2.3", "01.2.3", "PRIVATE\u{1b}"] {
            assert!(VersionPolicy::parse(provider, Some(invalid)).is_err());
        }
    }
    assert!(VersionPolicy::parse(Provider::Brew, Some("1.2.3")).is_err());
    assert!(VersionPolicy::parse(Provider::Npm, Some("1.2.3")).is_ok());
}

#[test]
fn provider_current_never_requests_an_upgrade() {
    for provider in [Provider::Brew, Provider::Npm] {
        for requested in [None, Some("latest")] {
            let policy = VersionPolicy::parse(provider, requested).unwrap();
            for observed in ["1.0.0", "0.1", "1:2.3-4~build"] {
                assert_eq!(
                    policy.matches_version(Some(observed)),
                    VersionMatch::Satisfied
                );
            }
        }
    }
}

#[test]
fn absent_malformed_or_oversized_observations_stay_unverifiable() {
    for policy in [
        VersionPolicy::ProviderDefault {},
        VersionPolicy::Latest {},
        VersionPolicy::Exact {
            version: "1.2.3".into(),
        },
    ] {
        for observed in [
            None,
            Some(""),
            Some("PRIVATE\u{1b}"),
            Some("/path"),
            Some("two words"),
            Some(&"a".repeat(257)),
        ] {
            assert_eq!(policy.matches_version(observed), VersionMatch::Unverifiable);
        }
    }
}

#[test]
fn exact_versions_include_prerelease_and_build_metadata() {
    let policy = VersionPolicy::parse(Provider::Npm, Some("1.2.3-rc.1+build.2")).unwrap();
    assert_eq!(
        policy.matches_version(Some("1.2.3-rc.1+build.2")),
        VersionMatch::Satisfied
    );
    for other in ["1.2.3", "1.2.3-rc.2+build.2", "1.2.3-rc.1+build.3"] {
        assert_eq!(policy.matches_version(Some(other)), VersionMatch::Mismatch);
    }
    assert_eq!(
        policy.matches_version(Some("1.2")),
        VersionMatch::Unverifiable
    );
}

#[test]
fn typed_policy_metadata_roundtrips_without_becoming_approval() {
    for policy in [
        VersionPolicy::ProviderDefault {},
        VersionPolicy::Latest {},
        VersionPolicy::Exact {
            version: "1.2.3".into(),
        },
    ] {
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(
            serde_json::from_str::<VersionPolicy>(&json).unwrap(),
            policy
        );
        assert!(!json.contains("approval"));
    }
    assert!(
        serde_json::from_str::<VersionPolicy>(
            r#"{"kind":"exact","version":"1.2.3","approval":true}"#
        )
        .is_err()
    );
}
