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

mod package_command {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_package_creates_archive() {
        let temp = TempDir::new().unwrap();
        let output_path = temp.path().join("test.tar.gz");

        let output = sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            output_path.to_str().unwrap(),
        ]);

        assert!(output.status.success(), "Package command should succeed");
        assert!(output_path.exists(), "Archive should be created");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Packaging"));
        assert!(stdout.contains("Created"));
    }

    #[test]
    fn test_package_default_output_name() {
        let temp = TempDir::new().unwrap();

        // Copy demo-pack to temp dir so we can package it there
        let pack_dir = temp.path().join("demo-pack");
        fs::create_dir_all(&pack_dir).unwrap();

        // Copy Pack.yaml
        fs::copy(
            format!("{}/demo-pack/Pack.yaml", fixtures_path()),
            pack_dir.join("Pack.yaml"),
        ).unwrap();

        // Copy values.yaml
        fs::copy(
            format!("{}/demo-pack/values.yaml", fixtures_path()),
            pack_dir.join("values.yaml"),
        ).unwrap();

        // Copy templates
        let templates_src = format!("{}/demo-pack/templates", fixtures_path());
        let templates_dst = pack_dir.join("templates");
        fs::create_dir_all(&templates_dst).unwrap();
        for entry in fs::read_dir(&templates_src).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), templates_dst.join(entry.file_name())).unwrap();
        }

        let output = sherpack(&["package", pack_dir.to_str().unwrap()]);

        assert!(output.status.success());

        // Default name should be {name}-{version}.tar.gz
        let expected_archive = pack_dir.join("demo-pack-1.0.0.tar.gz");
        assert!(expected_archive.exists(), "Archive with default name should exist");
    }

    #[test]
    fn test_package_shows_contents() {
        let temp = TempDir::new().unwrap();
        let output_path = temp.path().join("test.tar.gz");

        let output = sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            output_path.to_str().unwrap(),
        ]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should list contents
        assert!(stdout.contains("Contents:"));
        assert!(stdout.contains("Pack.yaml"));
        assert!(stdout.contains("values.yaml"));
        assert!(stdout.contains("Digest:"));
    }
}

mod inspect_command {
    use super::*;
    use tempfile::TempDir;

    fn create_test_archive() -> (TempDir, std::path::PathBuf) {
        let temp = TempDir::new().unwrap();
        let archive_path = temp.path().join("test.tar.gz");

        let output = sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);
        assert!(output.status.success());

        (temp, archive_path)
    }

    #[test]
    fn test_inspect_shows_contents() {
        let (_temp, archive_path) = create_test_archive();

        let output = sherpack(&["inspect", archive_path.to_str().unwrap()]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(stdout.contains("Archive"));
        assert!(stdout.contains("demo-pack"));
        assert!(stdout.contains("Files:"));
        assert!(stdout.contains("MANIFEST"));
        assert!(stdout.contains("Pack.yaml"));
    }

    #[test]
    fn test_inspect_with_manifest_flag() {
        let (_temp, archive_path) = create_test_archive();

        let output = sherpack(&["inspect", archive_path.to_str().unwrap(), "--manifest"]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should output raw manifest
        assert!(stdout.contains("sherpack-manifest-version:"));
        assert!(stdout.contains("[files]"));
        assert!(stdout.contains("[digest]"));
    }

    #[test]
    fn test_inspect_with_checksums() {
        let (_temp, archive_path) = create_test_archive();

        let output = sherpack(&["inspect", archive_path.to_str().unwrap(), "--checksums"]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should show sha256 checksums
        assert!(stdout.contains("sha256:"));
    }
}

mod verify_command {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_verify_valid_archive() {
        let temp = TempDir::new().unwrap();
        let archive_path = temp.path().join("test.tar.gz");

        // Create archive
        let output = sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);
        assert!(output.status.success());

        // Verify archive
        let output = sherpack(&["verify", archive_path.to_str().unwrap()]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(stdout.contains("Integrity check:"));
        assert!(stdout.contains("[OK]"));
        assert!(stdout.contains("All file checksums match"));
    }

    #[test]
    fn test_verify_without_signature_skips() {
        let temp = TempDir::new().unwrap();
        let archive_path = temp.path().join("test.tar.gz");

        // Create archive (no signature)
        sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);

        let output = sherpack(&["verify", archive_path.to_str().unwrap()]);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(stdout.contains("[SKIP]"));
        assert!(stdout.contains("No signature file"));
    }

    #[test]
    fn test_verify_require_signature_fails_without_sig() {
        let temp = TempDir::new().unwrap();
        let archive_path = temp.path().join("test.tar.gz");

        // Create archive (no signature)
        sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);

        let output = sherpack(&[
            "verify",
            archive_path.to_str().unwrap(),
            "--require-signature",
        ]);

        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("[FAIL]") || stdout.contains("No signature"));
    }
}

mod keygen_command {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_keygen_creates_keys() {
        let temp = TempDir::new().unwrap();
        let key_dir = temp.path();

        let output = sherpack(&[
            "keygen",
            "-o",
            key_dir.to_str().unwrap(),
            "--no-password",
        ]);

        assert!(output.status.success(), "Keygen should succeed");

        let secret_key = key_dir.join("sherpack.key");
        let public_key = key_dir.join("sherpack.pub");

        assert!(secret_key.exists(), "Secret key should be created");
        assert!(public_key.exists(), "Public key should be created");

        // Check key contents look valid (minisign format)
        let sk_content = fs::read_to_string(&secret_key).unwrap();
        let pk_content = fs::read_to_string(&public_key).unwrap();

        assert!(sk_content.contains("secret key"), "Secret key should have 'secret key' comment");
        assert!(pk_content.contains("public key"), "Public key should have 'public key' comment");
    }

    #[test]
    fn test_keygen_fails_if_exists() {
        let temp = TempDir::new().unwrap();
        let key_dir = temp.path();

        // First keygen
        sherpack(&["keygen", "-o", key_dir.to_str().unwrap(), "--no-password"]);

        // Second keygen should fail
        let output = sherpack(&["keygen", "-o", key_dir.to_str().unwrap(), "--no-password"]);

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}{}", stdout, stderr);
        assert!(combined.contains("already exist") || combined.contains("--force"));
    }

    #[test]
    fn test_keygen_force_overwrites() {
        let temp = TempDir::new().unwrap();
        let key_dir = temp.path();

        // First keygen
        sherpack(&["keygen", "-o", key_dir.to_str().unwrap(), "--no-password"]);

        // Get original key content
        let original_pk = fs::read_to_string(key_dir.join("sherpack.pub")).unwrap();

        // Second keygen with --force
        let output = sherpack(&[
            "keygen",
            "-o",
            key_dir.to_str().unwrap(),
            "--no-password",
            "--force",
        ]);

        assert!(output.status.success());

        // Key should be different (new keypair)
        let new_pk = fs::read_to_string(key_dir.join("sherpack.pub")).unwrap();
        assert_ne!(original_pk, new_pk, "New keypair should be generated");
    }
}

mod sign_and_verify_command {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sign_creates_signature() {
        let temp = TempDir::new().unwrap();
        let key_dir = temp.path().join("keys");
        let archive_path = temp.path().join("test.tar.gz");

        // Generate keys
        sherpack(&["keygen", "-o", key_dir.to_str().unwrap(), "--no-password"]);

        // Create archive
        sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);

        // Sign archive
        let output = sherpack(&[
            "sign",
            archive_path.to_str().unwrap(),
            "-k",
            key_dir.join("sherpack.key").to_str().unwrap(),
        ]);

        assert!(output.status.success(), "Sign should succeed");

        let sig_path = temp.path().join("test.tar.gz.minisig");
        assert!(sig_path.exists(), "Signature file should be created");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Signing"));
        assert!(stdout.contains("Created"));
        assert!(stdout.contains("Trusted comment"));
    }

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let temp = TempDir::new().unwrap();
        let key_dir = temp.path().join("keys");
        let archive_path = temp.path().join("test.tar.gz");

        // Generate keys
        sherpack(&["keygen", "-o", key_dir.to_str().unwrap(), "--no-password"]);

        // Create archive
        sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);

        // Sign archive
        sherpack(&[
            "sign",
            archive_path.to_str().unwrap(),
            "-k",
            key_dir.join("sherpack.key").to_str().unwrap(),
        ]);

        // Verify with signature
        let output = sherpack(&[
            "verify",
            archive_path.to_str().unwrap(),
            "-k",
            key_dir.join("sherpack.pub").to_str().unwrap(),
        ]);

        assert!(output.status.success(), "Verify should succeed");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(stdout.contains("Integrity check:"));
        assert!(stdout.contains("Signature check:"));
        assert!(stdout.contains("[OK]"));
        assert!(stdout.contains("Signature valid"));
        assert!(stdout.contains("verified successfully"));
    }

    #[test]
    fn test_verify_fails_with_wrong_key() {
        let temp = TempDir::new().unwrap();
        let key_dir1 = temp.path().join("keys1");
        let key_dir2 = temp.path().join("keys2");
        let archive_path = temp.path().join("test.tar.gz");

        // Generate two different keypairs
        sherpack(&["keygen", "-o", key_dir1.to_str().unwrap(), "--no-password"]);
        sherpack(&["keygen", "-o", key_dir2.to_str().unwrap(), "--no-password"]);

        // Create and sign with first keypair
        sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);
        sherpack(&[
            "sign",
            archive_path.to_str().unwrap(),
            "-k",
            key_dir1.join("sherpack.key").to_str().unwrap(),
        ]);

        // Verify with second keypair (should fail)
        let output = sherpack(&[
            "verify",
            archive_path.to_str().unwrap(),
            "-k",
            key_dir2.join("sherpack.pub").to_str().unwrap(),
        ]);

        assert!(!output.status.success(), "Verify should fail with wrong key");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("[FAIL]") || stdout.contains("failed"));
    }
}

// ============================================================================
// Phase 5: Repository, Search, and Dependency Management Tests
// ============================================================================

mod repo_command {
    use super::*;

    #[test]
    fn test_repo_list_empty() {
        // Without HOME set to temp dir, this may find existing config
        // Just verify the command runs without crashing
        let output = sherpack(&["repo", "list"]);

        // Should succeed even with no repos (or existing config)
        assert!(output.status.success(), "repo list should succeed");
    }

    #[test]
    fn test_repo_list_with_auth_flag() {
        let output = sherpack(&["repo", "list", "--auth"]);

        // Should succeed and show auth status
        assert!(output.status.success(), "repo list --auth should succeed");
    }

    #[test]
    fn test_repo_add_invalid_url() {
        let output = sherpack(&["repo", "add", "invalid", "not-a-url"]);

        // Should fail with validation error
        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            combined.contains("Invalid") || combined.contains("URL") || combined.contains("http"),
            "Expected URL validation error. Got: {}", combined
        );
    }

    #[test]
    fn test_repo_remove_nonexistent() {
        let output = sherpack(&["repo", "remove", "nonexistent-repo-xyz"]);

        // Should fail - repo doesn't exist
        assert!(!output.status.success());
    }

    #[test]
    fn test_repo_update_nonexistent() {
        let output = sherpack(&["repo", "update", "nonexistent-repo-xyz"]);

        // Should fail - repo doesn't exist
        assert!(!output.status.success());
    }
}

mod search_command {
    use super::*;

    #[test]
    fn test_search_no_repos() {
        // Search with no repos configured should handle gracefully
        let output = sherpack(&["search", "nginx"]);

        // May succeed with no results or fail gracefully
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        // Should not crash, either shows no results or repo error
        assert!(
            combined.contains("No") || combined.contains("repository") || output.status.success(),
            "Search should handle no repos gracefully. Got: {}", combined
        );
    }

    #[test]
    fn test_search_json_output() {
        let output = sherpack(&["search", "nginx", "--json"]);

        // With no repos configured, search outputs a message (not JSON)
        // This is expected behavior - JSON output only works when repos exist
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("No") || stdout.contains("repository") || stdout.is_empty(),
            "Should handle no repos gracefully. Got: {}", stdout
        );
    }

    #[test]
    fn test_search_specific_repo() {
        let output = sherpack(&["search", "nginx", "--repo", "nonexistent"]);

        // Should fail - repo doesn't exist
        assert!(!output.status.success());
    }
}

mod pull_command {
    use super::*;

    #[test]
    fn test_pull_invalid_reference() {
        let output = sherpack(&["pull", "invalid-reference-no-repo"]);

        // Should fail with validation error about format
        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            combined.contains("Invalid") || combined.contains("format") || combined.contains("reference"),
            "Expected format error. Got: {}", combined
        );
    }

    #[test]
    fn test_pull_nonexistent_repo() {
        let output = sherpack(&["pull", "nonexistent-repo/nginx:1.0.0"]);

        // Should fail - repo doesn't exist
        assert!(!output.status.success());
    }

    #[test]
    fn test_pull_oci_invalid_format() {
        let output = sherpack(&["pull", "oci://invalid"]);

        // Should fail with OCI reference error
        assert!(!output.status.success());
    }
}

mod push_command {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_push_missing_archive() {
        let output = sherpack(&["push", "/nonexistent/archive.tar.gz", "oci://registry/repo:tag"]);

        // Should fail - file doesn't exist
        assert!(!output.status.success());
    }

    #[test]
    fn test_push_invalid_destination() {
        let temp = TempDir::new().unwrap();
        let archive_path = temp.path().join("test.tar.gz");

        // Create a valid archive first
        let _ = sherpack(&[
            "package",
            &format!("{}/demo-pack", fixtures_path()),
            "-o",
            archive_path.to_str().unwrap(),
        ]);

        // Push with invalid destination
        let output = sherpack(&[
            "push",
            archive_path.to_str().unwrap(),
            "not-oci-format",
        ]);

        // Should fail - invalid destination format
        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            combined.contains("oci://") || combined.contains("Invalid") || combined.contains("format"),
            "Expected OCI format error. Got: {}", combined
        );
    }
}

mod dependency_command {
    use super::*;

    #[test]
    fn test_dependency_list_pack() {
        let output = sherpack(&[
            "dependency",
            "list",
            &format!("{}/demo-pack", fixtures_path()),
        ]);

        // demo-pack has no dependencies, should succeed with no deps message
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("No dependencies") || stdout.contains("dependencies"),
            "Should mention dependencies. Got: {}", stdout
        );
    }

    #[test]
    fn test_dependency_tree_pack() {
        let output = sherpack(&[
            "dependency",
            "tree",
            &format!("{}/demo-pack", fixtures_path()),
        ]);

        // Should succeed and show tree (empty for no deps)
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("demo-pack") || stdout.contains("Dependencies"),
            "Should show pack name. Got: {}", stdout
        );
    }

    #[test]
    fn test_dependency_list_nonexistent() {
        let output = sherpack(&[
            "dependency",
            "list",
            "/nonexistent/pack",
        ]);

        // Should fail - pack doesn't exist
        assert!(!output.status.success());
    }

    #[test]
    fn test_dependency_update_no_deps() {
        let output = sherpack(&[
            "dependency",
            "update",
            &format!("{}/demo-pack", fixtures_path()),
        ]);

        // demo-pack has no dependencies, should succeed
        assert!(output.status.success());
    }

    #[test]
    fn test_dependency_build_requires_lockfile() {
        let output = sherpack(&[
            "dependency",
            "build",
            &format!("{}/demo-pack", fixtures_path()),
        ]);

        // demo-pack has no Pack.lock.yaml, so build fails with helpful message
        assert!(!output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            combined.contains("Pack.lock") || combined.contains("update"),
            "Should mention lock file requirement. Got: {}", combined
        );
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
