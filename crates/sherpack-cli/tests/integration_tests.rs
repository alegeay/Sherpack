//! Integration tests for CLI commands

use std::process::Command;

/// Helper to run sherpack command
fn sherpack(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sherpack"))
        .args(args)
        .output()
        .expect("Failed to execute sherpack")
}

/// Get the fixtures path
fn fixtures_path() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures")
}

mod validate_command {
    use super::*;

    #[test]
    fn test_validate_valid_pack() {
        let output = sherpack(&["validate", &format!("{}/demo-pack", fixtures_path())]);

        assert!(output.status.success(), "Expected success for valid pack");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Validation passed"));
    }

    #[test]
    fn test_validate_with_invalid_values() {
        let output = sherpack(&[
            "validate",
            &format!("{}/demo-pack", fixtures_path()),
            "--set",
            "app.replicas=999",
        ]);

        assert!(!output.status.success(), "Expected failure for invalid values");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("greater than") || stdout.contains("maximum"));
    }

    #[test]
    fn test_validate_json_output() {
        let output = sherpack(&[
            "validate",
            &format!("{}/demo-pack", fixtures_path()),
            "--json",
        ]);

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should be valid JSON
        let json: serde_json::Value = serde_json::from_str(&stdout)
            .expect("Output should be valid JSON");

        assert!(json.get("valid").is_some());
        assert!(json.get("pack").is_some());
    }

    #[test]
    fn test_validate_json_output_with_errors() {
        let output = sherpack(&[
            "validate",
            &format!("{}/demo-pack", fixtures_path()),
            "--set",
            "app.replicas=-1",
            "--json",
        ]);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value = serde_json::from_str(&stdout)
            .expect("Output should be valid JSON");

        assert_eq!(json["valid"], false);
        assert!(json["errors"].as_array().unwrap().len() > 0);
    }

    #[test]
    fn test_validate_verbose() {
        let output = sherpack(&[
            "validate",
            &format!("{}/demo-pack", fixtures_path()),
            "-v",
        ]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Applied defaults") || stdout.contains("Loaded"));
    }
}

mod lint_command {
    use super::*;

    #[test]
    fn test_lint_valid_pack() {
        let output = sherpack(&["lint", &format!("{}/simple-pack", fixtures_path())]);

        // simple-pack doesn't have schema, so just check basic linting
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Pack.yaml is valid"));
    }

    #[test]
    fn test_lint_with_schema() {
        let output = sherpack(&["lint", &format!("{}/demo-pack", fixtures_path())]);

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Pack.yaml is valid"));
        assert!(stdout.contains("values.schema.yaml is valid") || stdout.contains("schema"));
    }

    #[test]
    fn test_lint_skip_schema() {
        let output = sherpack(&[
            "lint",
            &format!("{}/demo-pack", fixtures_path()),
            "--skip-schema",
        ]);

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should not mention schema validation when skipped
        assert!(!stdout.contains("Validating values against schema"));
    }
}

mod template_command {
    use super::*;

    #[test]
    fn test_template_basic() {
        let output = sherpack(&[
            "template",
            "myrelease",
            &format!("{}/simple-pack", fixtures_path()),
        ]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("apiVersion"));
    }

    #[test]
    fn test_template_with_values() {
        let output = sherpack(&[
            "template",
            "myrelease",
            &format!("{}/demo-pack", fixtures_path()),
            "--set",
            "app.name=customapp",
        ]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("customapp"));
    }

    #[test]
    fn test_template_show_only() {
        let output = sherpack(&[
            "template",
            "myrelease",
            &format!("{}/demo-pack", fixtures_path()),
            "-s",
            "deployment",
        ]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("deployment"));
        // Should not contain other templates
        assert!(!stdout.contains("# Source: configmap.yaml"));
    }

    #[test]
    fn test_template_skip_schema() {
        // Test that --skip-schema allows invalid values
        let output = sherpack(&[
            "template",
            "myrelease",
            &format!("{}/demo-pack", fixtures_path()),
            "--set",
            "app.replicas=999",
            "--skip-schema",
            "-s",
            "deployment",
        ]);

        // Should succeed despite invalid value because schema is skipped
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("replicas: 999"));
    }

    #[test]
    fn test_template_schema_validation_blocks_invalid() {
        let output = sherpack(&[
            "template",
            "myrelease",
            &format!("{}/demo-pack", fixtures_path()),
            "--set",
            "app.replicas=999",
        ]);

        // Should fail because schema validation catches the error
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            combined.contains("validation") || combined.contains("maximum") || combined.contains("greater"),
            "Expected validation error message"
        );
    }

    #[test]
    fn test_template_show_values() {
        let output = sherpack(&[
            "template",
            "myrelease",
            &format!("{}/demo-pack", fixtures_path()),
            "--show-values",
            "-s",
            "deployment",
        ]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("# Computed Values"));
    }
}

mod show_command {
    use super::*;

    #[test]
    fn test_show_pack() {
        let output = sherpack(&["show", &format!("{}/demo-pack", fixtures_path())]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("demo-pack"));
    }

    #[test]
    fn test_show_all() {
        let output = sherpack(&[
            "show",
            &format!("{}/demo-pack", fixtures_path()),
            "--all",
        ]);

        assert!(output.status.success());
    }
}

mod error_messages {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_pack_with_error(error_template: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let pack_path = dir.path();

        // Create Pack.yaml
        fs::write(
            pack_path.join("Pack.yaml"),
            r#"apiVersion: sherpack/v1
kind: application
metadata:
  name: test-pack
  version: 1.0.0
"#,
        )
        .unwrap();

        // Create values.yaml
        fs::write(
            pack_path.join("values.yaml"),
            r#"app:
  name: myapp
  replicas: 3
image:
  repository: nginx
  tag: latest
"#,
        )
        .unwrap();

        // Create templates directory and error template
        fs::create_dir(pack_path.join("templates")).unwrap();
        fs::write(pack_path.join("templates/test.yaml"), error_template).unwrap();

        dir
    }

    #[test]
    fn test_error_message_typo_value_vs_values() {
        let pack = create_test_pack_with_error(
            "name: {{ value.app.name }}"
        );

        let output = sherpack(&[
            "template",
            "test",
            pack.path().to_str().unwrap(),
        ]);

        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should suggest "values" instead of "value"
        assert!(
            stdout.contains("values") && stdout.contains("Did you mean"),
            "Expected suggestion for 'value' -> 'values' typo. Got: {}",
            stdout
        );
    }

    #[test]
    fn test_error_message_unknown_filter() {
        let pack = create_test_pack_with_error(
            "name: {{ values.app.name | toyml }}"
        );

        let output = sherpack(&[
            "template",
            "test",
            pack.path().to_str().unwrap(),
        ]);

        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should suggest "toyaml" for "toyml"
        assert!(
            stdout.contains("toyaml") || stdout.contains("Did you mean"),
            "Expected suggestion for 'toyml' -> 'toyaml'. Got: {}",
            stdout
        );
    }

    #[test]
    fn test_error_message_undefined_nested_key() {
        let pack = create_test_pack_with_error(
            "repo: {{ values.image.repo }}"
        );

        let output = sherpack(&[
            "template",
            "test",
            pack.path().to_str().unwrap(),
        ]);

        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should show available keys
        assert!(
            stdout.contains("repository") || stdout.contains("Available"),
            "Expected available keys in error. Got: {}",
            stdout
        );
    }

    #[test]
    fn test_multi_error_collection() {
        let pack = create_test_pack_with_error(
            r#"# Multiple errors
error1: {{ value.app.name }}
error2: {{ values.undefined.key }}
"#
        );

        let output = sherpack(&[
            "lint",
            pack.path().to_str().unwrap(),
        ]);

        // lint should show multiple errors
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should contain at least one error indication
        assert!(
            stdout.contains("error") || stdout.contains("✗"),
            "Expected error indication. Got: {}",
            stdout
        );
    }
}
