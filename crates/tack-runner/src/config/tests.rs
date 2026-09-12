use super::*;

#[test]
fn configuration_precedence_is_defaults_file_environment_then_cli() {
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                api_base_url = "https://file.invalid"
                runner_id = "file"
                state_dir = "file-state"
                enrollment_credential = "file-secret"
            "#,
        ),
        environment: ConfigOverrides {
            api_base_url: Some("https://environment.invalid".into()),
            runner_id: Some("environment".into()),
            state_dir: Some(PathBuf::from("environment-state")),
            enrollment_credential: Some(EnrollmentCredential::new("environment-secret")),
            providers: BTreeMap::new(),
        },
        command_line: ConfigOverrides {
            api_base_url: Some("https://cli.invalid".into()),
            runner_id: Some("cli".into()),
            state_dir: Some(PathBuf::from("cli-state")),
            enrollment_credential: Some(EnrollmentCredential::new("cli-secret")),
            providers: BTreeMap::new(),
        },
    })
    .expect("configuration should load");

    assert_eq!(config.api_base_url, "https://cli.invalid");
    assert_eq!(config.runner_id, "cli");
    assert_eq!(config.state_dir, PathBuf::from("cli-state"));
    assert_eq!(config.enrollment_credential.unwrap().expose(), "cli-secret");
}

#[test]
fn credentials_are_redacted_from_debug_and_missing_error() {
    let secret = "enrollment-credential-must-not-appear";
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        command_line: ConfigOverrides {
            enrollment_credential: Some(EnrollmentCredential::new(secret)),
            ..ConfigOverrides::default()
        },
        ..RunnerConfigSources::default()
    })
    .expect("configuration should load");

    assert!(!format!("{config:?}").contains(secret));
    assert!(!format!("{:?}", config.enrollment_credential).contains(secret));
    let missing = RunnerConfig::defaults()
        .require_enrollment_credential()
        .expect_err("credential is absent");
    assert!(!missing.to_string().contains(secret));
}

#[test]
fn invalid_toml_does_not_echo_credential_like_source_text() {
    let secret = "do-not-echo-this-credential";
    let error = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some("enrollment_credential = [do-not-echo-this-credential"),
        ..RunnerConfigSources::default()
    })
    .expect_err("invalid TOML should fail");

    assert_eq!(error, ConfigError::Invalid);
    assert!(!error.to_string().contains(secret));
}

#[test]
fn provider_defaults_to_disabled_with_the_expected_secret_name() {
    let config = RunnerConfig::defaults();
    let provider = config
        .providers
        .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("default provider entry present");
    assert!(!provider.enabled);
    assert_eq!(provider.secret, DEFAULT_VERCEL_AI_GATEWAY_SECRET);
}

/// Mirrors the test above for the second known provider — both are
/// seeded disabled, independently of each other.
#[test]
fn the_anthropic_provider_also_defaults_to_disabled_with_the_expected_secret_name() {
    let config = RunnerConfig::defaults();
    let provider = config
        .providers
        .get(ANTHROPIC_CONFIG_KEY)
        .expect("default provider entry present");
    assert!(!provider.enabled);
    assert_eq!(provider.secret, DEFAULT_ANTHROPIC_SECRET);
}

/// Enabling one known provider through the environment must not
/// disturb the other's own (disabled) default — proves the two
/// providers' config overrides are independent, keyed maps rather than
/// one override clobbering the whole `providers` map.
#[test]
fn enabling_one_known_provider_does_not_affect_the_others_default() {
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        environment: ConfigOverrides {
            providers: BTreeMap::from([(
                ANTHROPIC_CONFIG_KEY.to_owned(),
                ProviderOverride {
                    enabled: Some(true),
                    secret: Some("anthropic-secret".to_owned()),
                },
            )]),
            ..ConfigOverrides::default()
        },
        ..RunnerConfigSources::default()
    })
    .expect("configuration should load");

    let anthropic = config
        .providers
        .get(ANTHROPIC_CONFIG_KEY)
        .expect("anthropic entry present");
    assert!(anthropic.enabled);
    assert_eq!(anthropic.secret, "anthropic-secret");

    let vercel = config
        .providers
        .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("vercel entry still present at its default");
    assert!(!vercel.enabled);
    assert_eq!(vercel.secret, DEFAULT_VERCEL_AI_GATEWAY_SECRET);
}

/// Mirrors `configuration_precedence_is_defaults_file_environment_then_cli`,
/// but for a provider entry specifically: proves the field-level merge
/// (environment overrides only `secret`, `enabled` still comes from the
/// file) rather than one override replacing the whole entry.
#[test]
fn provider_config_precedence_is_defaults_file_environment_then_cli() {
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [provider.vercel_ai_gateway]
                enabled = true
                secret = "file-secret"
            "#,
        ),
        environment: ConfigOverrides {
            providers: BTreeMap::from([(
                VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
                ProviderOverride {
                    enabled: None,
                    secret: Some("environment-secret".to_owned()),
                },
            )]),
            ..ConfigOverrides::default()
        },
        command_line: ConfigOverrides::default(),
    })
    .expect("configuration should load");

    let provider = config
        .providers
        .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("provider entry present");
    assert!(
        provider.enabled,
        "the file enabled it and nothing later touched that field"
    );
    assert_eq!(
        provider.secret, "environment-secret",
        "environment overrides the file's secret"
    );
}

#[test]
fn unknown_field_inside_a_provider_table_is_rejected() {
    let error = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [provider.vercel_ai_gateway]
                enabled = true
                bogus = "nope"
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect_err("an unknown field inside a provider table must be rejected");
    assert_eq!(error, ConfigError::Invalid);
}

/// A provider name this build does not recognize must not fail
/// configuration loading — it simply sits unused, and the real
/// `vercel_ai_gateway` entry stays at its (disabled) default.
#[test]
fn an_unrecognized_provider_name_does_not_fail_loading_or_affect_the_known_one() {
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [provider.some_future_gateway]
                enabled = true
                secret = "irrelevant"
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect("an unrecognized provider name must not fail configuration loading");

    assert!(
        config
            .providers
            .get("some_future_gateway")
            .expect("the entry is still recorded")
            .enabled
    );
    let vercel = config
        .providers
        .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("the known provider's default entry is untouched");
    assert!(!vercel.enabled);
}
