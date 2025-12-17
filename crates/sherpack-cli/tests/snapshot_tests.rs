//! Snapshot tests for error display formatting

use std::process::Command;
use std::fs;
use tempfile::TempDir;

/// Helper to run sherpack command and capture output
fn sherpack_output(args: &[&str]) -> (String, String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_sherpack"))
        .args(args)
        .output()
        .expect("Failed to execute sherpack");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    (stdout, stderr, success)
}

fn create_test_pack(templates: &[(&str, &str)]) -> TempDir {
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
  description: Test pack for snapshot tests
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
  pullPolicy: IfNotPresent
config:
  port: 8080
  host: localhost
"#,
    )
    .unwrap();

    // Create templates directory
    fs::create_dir(pack_path.join("templates")).unwrap();

    // Create template files
    for (name, content) in templates {
        fs::write(pack_path.join("templates").join(name), content).unwrap();
    }

    dir
}

/// Normalize output for snapshot comparison
/// Removes variable parts like timestamps, paths
#[allow(dead_code)]
fn normalize_output(output: &str) -> String {
    output
        .lines()
        .map(|line| {
            // Remove absolute paths
            if line.contains("/tmp/") || line.contains("/var/") {
                line.split("/tmp/").next().unwrap_or(line).to_string()
                    + "[TEMP_PATH]"
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

mod error_display_snapshots {
    use super::*;

    #[test]
    fn test_undefined_variable_error_display() {
        let pack = create_test_pack(&[
            ("deployment.yaml", "name: {{ values.undefined_key }}")
        ]);

        let (stdout, _stderr, success) = sherpack_output(&[
            "template",
            "test",
            pack.path().to_str().unwrap(),
        ]);

        assert!(!success);

        // Verify key parts of the error message
        assert!(stdout.contains("undefined"), "Should mention undefined variable");
        assert!(stdout.contains("error") || stdout.contains("✗"), "Should indicate error");
    }

    #[test]
    fn test_typo_value_error_has_suggestion() {
        let pack = create_test_pack(&[
            ("deployment.yaml", "name: {{ value.app.name }}")
        ]);

        let (stdout, _stderr, _success) = sherpack_output(&[
            "template",
            "test",
            pack.path().to_str().unwrap(),
        ]);

        // Should suggest "values" for "value" typo
        assert!(
            stdout.contains("values") && stdout.contains("Did you mean"),
            "Should suggest 'values' for 'value' typo. Output: {}",
            stdout
        );
    }

    #[test]
    fn test_unknown_filter_error_has_suggestion() {
        let pack = create_test_pack(&[
            ("deployment.yaml", "name: {{ values.app.name | toyml }}")
        ]);

        let (stdout, _stderr, _success) = sherpack_output(&[
            "template",
            "test",
            pack.path().to_str().unwrap(),
        ]);

        // Should suggest "toyaml" for "toyml" typo
        assert!(
            stdout.contains("toyaml"),
            "Should suggest 'toyaml' for 'toyml' typo. Output: {}",
            stdout
        );
    }

    #[test]
    fn test_missing_key_shows_available_keys() {
        let pack = create_test_pack(&[
            ("deployment.yaml", "repo: {{ values.image.repo }}")
        ]);

        let (stdout, _stderr, _success) = sherpack_output(&[
            "template",
            "test",
            pack.path().to_str().unwrap(),
        ]);

        // Should show available keys
        assert!(
            stdout.contains("repository") || stdout.contains("Available"),
            "Should show available keys. Output: {}",
            stdout
        );
    }

    #[test]
    fn test_multi_template_error_grouping() {
        let pack = create_test_pack(&[
            ("good.yaml", "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {{ release.name }}"),
            ("bad1.yaml", "error: {{ value.missing }}"),
            ("bad2.yaml", "error: {{ values.nonexistent }}"),
        ]);

        let (stdout, _stderr, _success) = sherpack_output(&[
            "lint",
            pack.path().to_str().unwrap(),
        ]);

        // Should show errors grouped
        assert!(
            stdout.contains("error") || stdout.contains("✗"),
            "Should indicate errors. Output: {}",
            stdout
        );
    }
}

mod validation_display_snapshots {
    use super::*;

    fn create_pack_with_schema() -> TempDir {
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
"#,
        )
        .unwrap();

        // Create schema
        fs::write(
            pack_path.join("values.schema.yaml"),
            r#"schemaVersion: sherpack/v1
title: Test Schema
properties:
  app:
    type: object
    properties:
      name:
        type: string
        required: true
      replicas:
        type: integer
        min: 1
        max: 10
"#,
        )
        .unwrap();

        // Create templates
        fs::create_dir(pack_path.join("templates")).unwrap();
        fs::write(
            pack_path.join("templates/deployment.yaml"),
            "name: {{ values.app.name }}\nreplicas: {{ values.app.replicas }}",
        )
        .unwrap();

        dir
    }

    #[test]
    fn test_validation_success_display() {
        let pack = create_pack_with_schema();

        let (stdout, _stderr, success) = sherpack_output(&[
            "validate",
            pack.path().to_str().unwrap(),
        ]);

        assert!(success, "Validation should succeed");
        assert!(
            stdout.contains("passed") || stdout.contains("✓"),
            "Should show success. Output: {}",
            stdout
        );
    }

    #[test]
    fn test_validation_error_display() {
        let pack = create_pack_with_schema();

        let (stdout, _stderr, success) = sherpack_output(&[
            "validate",
            pack.path().to_str().unwrap(),
            "--set",
            "app.replicas=999",
        ]);

        assert!(!success, "Validation should fail");
        assert!(
            stdout.contains("maximum") || stdout.contains("greater") || stdout.contains("10"),
            "Should show max validation error. Output: {}",
            stdout
        );
    }

    #[test]
    fn test_validation_json_format() {
        let pack = create_pack_with_schema();

        let (stdout, _stderr, _success) = sherpack_output(&[
            "validate",
            pack.path().to_str().unwrap(),
            "--set",
            "app.replicas=999",
            "--json",
        ]);

        // Parse as JSON and verify structure
        let json: serde_json::Value = serde_json::from_str(&stdout)
            .expect("Should be valid JSON");

        assert_eq!(json["valid"], false);
        assert!(json["errors"].as_array().unwrap().len() > 0);
        assert!(json["pack"]["name"].as_str().is_some());
    }
}

mod lint_display_snapshots {
    use super::*;

    #[test]
    fn test_lint_success_display() {
        let pack = create_test_pack(&[
            ("deployment.yaml", "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {{ release.name }}")
        ]);

        let (stdout, _stderr, success) = sherpack_output(&[
            "lint",
            pack.path().to_str().unwrap(),
        ]);

        assert!(success, "Lint should succeed for valid pack");
        assert!(
            stdout.contains("✓") || stdout.contains("passed"),
            "Should show success indicators. Output: {}",
            stdout
        );
    }

    #[test]
    fn test_lint_with_warnings() {
        // Create pack without values.yaml
        let dir = TempDir::new().unwrap();
        let pack_path = dir.path();

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

        fs::create_dir(pack_path.join("templates")).unwrap();
        fs::write(
            pack_path.join("templates/test.yaml"),
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: test",
        )
        .unwrap();

        let (stdout, _stderr, _success) = sherpack_output(&[
            "lint",
            pack_path.to_str().unwrap(),
        ]);

        // Should show warning about missing values.yaml
        assert!(
            stdout.contains("warning") || stdout.contains("⚠") || stdout.contains("optional"),
            "Should show warning for missing values.yaml. Output: {}",
            stdout
        );
    }
}
