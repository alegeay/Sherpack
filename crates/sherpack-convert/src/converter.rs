//! Main converter logic
//!
//! Orchestrates the conversion of a Helm chart to a Sherpack pack.
//!
//! # Design Philosophy
//!
//! This converter follows Jinja2's explicit import philosophy:
//! - Macros must be imported before use
//! - Auto-detects all macros defined in helpers files
//! - Generates minimal import statements with only used macros

use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::chart::HelmChart;
use crate::error::{ConversionWarning, ConvertError, Result, WarningCategory, WarningSeverity};
use crate::parser;
use crate::transformer::Transformer;

/// Options for the converter
#[derive(Debug, Clone, Default)]
pub struct ConvertOptions {
    /// Overwrite existing output directory
    pub force: bool,
    /// Only show what would be converted
    pub dry_run: bool,
    /// Verbose output
    pub verbose: bool,
}

/// Result of a conversion
#[derive(Debug)]
pub struct ConversionResult {
    /// Files that were converted
    pub converted_files: Vec<PathBuf>,
    /// Files that were copied as-is
    pub copied_files: Vec<PathBuf>,
    /// Files that were skipped
    pub skipped_files: Vec<PathBuf>,
    /// Warnings generated during conversion
    pub warnings: Vec<ConversionWarning>,
}

impl ConversionResult {
    fn new() -> Self {
        Self {
            converted_files: Vec::new(),
            copied_files: Vec::new(),
            skipped_files: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Convert a Helm chart to a Sherpack pack
pub struct Converter {
    options: ConvertOptions,
}

impl Converter {
    pub fn new(options: ConvertOptions) -> Self {
        Self { options }
    }

    /// Convert a Helm chart directory to a Sherpack pack
    pub fn convert(&self, chart_path: &Path, output_path: &Path) -> Result<ConversionResult> {
        let mut result = ConversionResult::new();

        // Validate input
        if !chart_path.exists() {
            return Err(ConvertError::DirectoryNotFound(chart_path.to_path_buf()));
        }

        // Check for Chart.yaml
        let chart_yaml_path = chart_path.join("Chart.yaml");
        if !chart_yaml_path.exists() {
            return Err(ConvertError::NotAChart("Chart.yaml".to_string()));
        }

        // Check output directory
        if output_path.exists() && !self.options.force {
            return Err(ConvertError::OutputExists(output_path.to_path_buf()));
        }

        // Parse Chart.yaml
        let chart_content = fs::read_to_string(&chart_yaml_path)?;
        let chart = HelmChart::parse(&chart_content)?;
        let chart_name = chart.name.clone();

        if !self.options.dry_run {
            // Create output directory
            fs::create_dir_all(output_path)?;
        }

        // Convert Chart.yaml -> Pack.yaml
        let pack = chart.to_sherpack();
        let pack_yaml = pack.to_yaml()?;

        if !self.options.dry_run {
            let pack_path = output_path.join("Pack.yaml");
            fs::write(&pack_path, &pack_yaml)?;
            result.converted_files.push(pack_path);
        } else {
            result.converted_files.push(output_path.join("Pack.yaml"));
        }

        // Copy values.yaml
        let values_path = chart_path.join("values.yaml");
        if values_path.exists() {
            if !self.options.dry_run {
                let dest = output_path.join("values.yaml");
                fs::copy(&values_path, &dest)?;
                result.copied_files.push(dest);
            } else {
                result.copied_files.push(output_path.join("values.yaml"));
            }
        }

        // Copy values.schema.json -> values.schema.yaml (or keep as JSON)
        let schema_json = chart_path.join("values.schema.json");
        if schema_json.exists() {
            if !self.options.dry_run {
                let dest = output_path.join("values.schema.json");
                fs::copy(&schema_json, &dest)?;
                result.copied_files.push(dest);
            } else {
                result
                    .copied_files
                    .push(output_path.join("values.schema.json"));
            }
        }

        // Convert templates directory
        let templates_dir = chart_path.join("templates");
        if templates_dir.exists() {
            self.convert_templates_dir(
                &templates_dir,
                &output_path.join("templates"),
                &chart_name,
                &mut result,
            )?;
        }

        // Convert charts/ -> packs/ (subcharts)
        let charts_dir = chart_path.join("charts");
        if charts_dir.exists() {
            self.convert_subcharts(&charts_dir, &output_path.join("packs"), &mut result)?;
        }

        // Copy other files (README, LICENSE, etc.)
        self.copy_extra_files(chart_path, output_path, &mut result)?;

        Ok(result)
    }

    fn convert_templates_dir(
        &self,
        src_dir: &Path,
        dest_dir: &Path,
        chart_name: &str,
        result: &mut ConversionResult,
    ) -> Result<()> {
        if !self.options.dry_run {
            fs::create_dir_all(dest_dir)?;
        }

        // Three-pass conversion for proper macro import handling
        // Pass 1: Convert helpers files and collect macro definitions per file
        let mut macro_sources: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut defined_macros: HashSet<String> = HashSet::new();
        let mut helper_files: Vec<(PathBuf, String, String)> = Vec::new(); // (dest_path, dest_name, converted_content)

        for entry in WalkDir::new(src_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name.starts_with('_') && file_name.ends_with(".tpl") {
                let content = fs::read_to_string(path)?;
                let rel_path = path.strip_prefix(src_dir).unwrap_or(path);
                let dest_path = self.get_dest_path(dest_dir, rel_path);
                let dest_name = dest_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("_helpers.j2")
                    .to_string();

                match self.convert_helpers(&content, chart_name, &dest_path) {
                    Ok((converted, warnings)) => {
                        // Extract macro names from converted content and track their source file
                        let macros = extract_macro_definitions(&converted);
                        for macro_name in &macros {
                            macro_sources.insert(macro_name.clone(), dest_name.clone());
                        }
                        defined_macros.extend(macros);

                        // Store for pass 2 processing
                        helper_files.push((dest_path.clone(), dest_name, converted));
                        result.converted_files.push(dest_path);
                        result.warnings.extend(warnings);
                    }
                    Err(e) => {
                        result.warnings.push(ConversionWarning {
                            severity: WarningSeverity::Error,
                            category: WarningCategory::Syntax,
                            file: path.to_path_buf(),
                            line: None,
                            pattern: "template parse".to_string(),
                            message: format!("Failed to convert: {}", e),
                            suggestion: Some("Manual conversion may be required".to_string()),
                            doc_link: None,
                        });
                        result.skipped_files.push(path.to_path_buf());
                    }
                }
            }
        }

        // Pass 2: Add cross-imports to helper files and write them
        for (dest_path, this_file, converted) in &helper_files {
            // Find macros used in this helper that are defined in OTHER helper files
            let used_macros = find_used_macros(converted, &defined_macros);

            // Group by source file, excluding macros from this file
            let mut imports_by_file: std::collections::HashMap<&str, Vec<&str>> =
                std::collections::HashMap::new();
            for macro_name in &used_macros {
                if let Some(source_file) = macro_sources.get(macro_name) {
                    if source_file != this_file {
                        imports_by_file
                            .entry(source_file.as_str())
                            .or_default()
                            .push(macro_name.as_str());
                    }
                }
            }

            // Generate import statements
            let final_content = if !imports_by_file.is_empty() {
                let mut import_statements = String::new();
                let mut sorted_files: Vec<&&str> = imports_by_file.keys().collect();
                sorted_files.sort();

                for file in sorted_files {
                    let mut macro_list: Vec<&str> = imports_by_file[*file].clone();
                    macro_list.sort();
                    import_statements.push_str(&format!(
                        "{{%- from \"{}\" import {} -%}}\n",
                        file,
                        macro_list.join(", ")
                    ));
                }
                format!("{}{}", import_statements, converted)
            } else {
                converted.clone()
            };

            if !self.options.dry_run {
                fs::create_dir_all(dest_path.parent().unwrap_or(dest_dir))?;
                fs::write(dest_path, &final_content)?;
            }
        }

        // Pass 3: Convert regular templates with macro awareness
        for entry in WalkDir::new(src_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if path.is_dir() {
                let rel_path = path.strip_prefix(src_dir).unwrap_or(path);
                let dest = dest_dir.join(rel_path);
                if !self.options.dry_run {
                    fs::create_dir_all(&dest)?;
                }
                continue;
            }

            let rel_path = path.strip_prefix(src_dir).unwrap_or(path);
            let dest_path = self.get_dest_path(dest_dir, rel_path);
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip helpers (already processed)
            if file_name.starts_with('_') && file_name.ends_with(".tpl") {
                continue;
            }

            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    result.warnings.push(ConversionWarning {
                        severity: WarningSeverity::Error,
                        category: WarningCategory::Syntax,
                        file: path.to_path_buf(),
                        line: None,
                        pattern: "file read".to_string(),
                        message: format!("Failed to read file: {}", e),
                        suggestion: None,
                        doc_link: None,
                    });
                    result.skipped_files.push(path.to_path_buf());
                    continue;
                }
            };

            // NOTES.txt needs conversion too - it often contains Go templates
            // Don't skip it anymore

            // Convert template files
            if content.contains("{{") {
                match self.convert_template_with_macros(
                    &content,
                    chart_name,
                    &dest_path,
                    &defined_macros,
                    &macro_sources,
                ) {
                    Ok((converted, warnings)) => {
                        if !self.options.dry_run {
                            fs::write(&dest_path, &converted)?;
                        }
                        result.converted_files.push(dest_path.clone());
                        result.warnings.extend(warnings);
                    }
                    Err(e) => {
                        result.warnings.push(ConversionWarning {
                            severity: WarningSeverity::Error,
                            category: WarningCategory::Syntax,
                            file: path.to_path_buf(),
                            line: None,
                            pattern: "template parse".to_string(),
                            message: format!("Failed to convert: {}", e),
                            suggestion: Some("Manual conversion may be required".to_string()),
                            doc_link: None,
                        });
                        if !self.options.dry_run {
                            fs::write(&dest_path, &content)?;
                        }
                        result.skipped_files.push(path.to_path_buf());
                    }
                }
            } else {
                if !self.options.dry_run {
                    fs::write(&dest_path, &content)?;
                }
                result.copied_files.push(dest_path);
            }
        }

        Ok(())
    }

    /// Convert a template with awareness of defined macros
    fn convert_template_with_macros(
        &self,
        content: &str,
        chart_name: &str,
        dest_path: &Path,
        defined_macros: &HashSet<String>,
        macro_sources: &std::collections::HashMap<String, String>,
    ) -> Result<(String, Vec<ConversionWarning>)> {
        let ast = parser::parse(content)?;
        let mut transformer = Transformer::new().with_chart_prefix(chart_name);
        let converted = transformer.transform(&ast);

        // Find macros used in the converted template
        let used_macros = find_used_macros(&converted, defined_macros);

        // Generate import statements grouped by source file
        let final_content = if !used_macros.is_empty() && !macro_sources.is_empty() {
            // Group macros by their source file
            let mut imports_by_file: std::collections::HashMap<&str, Vec<&str>> =
                std::collections::HashMap::new();
            for macro_name in &used_macros {
                if let Some(source_file) = macro_sources.get(macro_name) {
                    imports_by_file
                        .entry(source_file.as_str())
                        .or_default()
                        .push(macro_name.as_str());
                }
            }

            // Generate import statements for each file
            let mut import_statements = String::new();
            let mut sorted_files: Vec<&&str> = imports_by_file.keys().collect();
            sorted_files.sort();

            for file in sorted_files {
                let mut macro_list: Vec<&str> = imports_by_file[*file].clone();
                macro_list.sort();
                import_statements.push_str(&format!(
                    "{{%- from \"{}\" import {} -%}}\n",
                    file,
                    macro_list.join(", ")
                ));
            }
            format!("{}{}", import_statements, converted)
        } else {
            converted
        };

        // Convert transformer warnings
        let warnings = self.collect_warnings(&transformer, dest_path, &final_content);

        Ok((final_content, warnings))
    }

    fn convert_template(
        &self,
        content: &str,
        chart_name: &str,
        dest_path: &Path,
    ) -> Result<(String, Vec<ConversionWarning>)> {
        // For backwards compatibility, use empty macro set
        self.convert_template_with_macros(
            content,
            chart_name,
            dest_path,
            &HashSet::new(),
            &std::collections::HashMap::new(),
        )
    }

    /// Collect and convert transformer warnings
    fn collect_warnings(
        &self,
        transformer: &Transformer,
        dest_path: &Path,
        final_content: &str,
    ) -> Vec<ConversionWarning> {
        let mut warnings: Vec<ConversionWarning> = transformer
            .warnings()
            .iter()
            .map(|w| {
                let category = match w.severity {
                    crate::transformer::WarningSeverity::Info => WarningCategory::Syntax,
                    crate::transformer::WarningSeverity::Warning => WarningCategory::Syntax,
                    crate::transformer::WarningSeverity::Unsupported => {
                        WarningCategory::UnsupportedFeature
                    }
                };
                let severity = match w.severity {
                    crate::transformer::WarningSeverity::Info => WarningSeverity::Info,
                    crate::transformer::WarningSeverity::Warning => WarningSeverity::Warning,
                    crate::transformer::WarningSeverity::Unsupported => {
                        WarningSeverity::Unsupported
                    }
                };
                ConversionWarning {
                    severity,
                    category,
                    file: dest_path.to_path_buf(),
                    line: None,
                    pattern: w.pattern.clone(),
                    message: w.message.clone(),
                    suggestion: w.suggestion.clone(),
                    doc_link: w.doc_link.clone(),
                }
            })
            .collect();

        // Check for __UNSUPPORTED_ markers
        if final_content.contains("__UNSUPPORTED_FILES__") {
            warnings.push(ConversionWarning::unsupported(
                dest_path.to_path_buf(),
                ".Files.*",
                "Embed file content in values.yaml or use ConfigMap/Secret resources",
            ));
        }

        if final_content.contains("__UNSUPPORTED_GENCA__") {
            warnings.push(ConversionWarning::security(
                dest_path.to_path_buf(),
                "genCA",
                "'genCA' generates certificates in templates - this is insecure",
                "Use cert-manager for certificate management",
            ));
        }

        warnings
    }

    fn convert_helpers(
        &self,
        content: &str,
        chart_name: &str,
        dest_path: &Path,
    ) -> Result<(String, Vec<ConversionWarning>)> {
        // Helpers files are just templates with define blocks
        self.convert_template(content, chart_name, dest_path)
    }

    fn get_dest_path(&self, dest_dir: &Path, rel_path: &Path) -> PathBuf {
        let file_name = rel_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Rename _helpers.tpl -> _macros.j2
        let new_name = if file_name.starts_with('_') && file_name.ends_with(".tpl") {
            let base = file_name
                .strip_prefix('_')
                .unwrap_or(file_name)
                .strip_suffix(".tpl")
                .unwrap_or(file_name);
            format!("_{}.j2", base)
        } else {
            file_name.to_string()
        };

        if let Some(parent) = rel_path.parent() {
            dest_dir.join(parent).join(new_name)
        } else {
            dest_dir.join(new_name)
        }
    }

    fn convert_subcharts(
        &self,
        charts_dir: &Path,
        packs_dir: &Path,
        result: &mut ConversionResult,
    ) -> Result<()> {
        if !charts_dir.exists() {
            return Ok(());
        }

        if !self.options.dry_run {
            fs::create_dir_all(packs_dir)?;
        }

        for entry in fs::read_dir(charts_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let subchart_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                let dest = packs_dir.join(subchart_name);

                // Recursively convert subchart
                match self.convert(&path, &dest) {
                    Ok(sub_result) => {
                        result.converted_files.extend(sub_result.converted_files);
                        result.copied_files.extend(sub_result.copied_files);
                        result.skipped_files.extend(sub_result.skipped_files);
                        result.warnings.extend(sub_result.warnings);
                    }
                    Err(e) => {
                        result.warnings.push(ConversionWarning {
                            severity: WarningSeverity::Error,
                            category: WarningCategory::Syntax,
                            file: path.clone(),
                            line: None,
                            pattern: "subchart".to_string(),
                            message: format!("Failed to convert subchart: {}", e),
                            suggestion: None,
                            doc_link: None,
                        });
                        result.skipped_files.push(path);
                    }
                }
            } else if path.extension().map(|e| e == "tgz").unwrap_or(false) {
                // Packaged subchart - just copy for now
                if !self.options.dry_run {
                    let dest = packs_dir.join(path.file_name().unwrap());
                    fs::copy(&path, &dest)?;
                    result.copied_files.push(dest);
                } else {
                    result.copied_files.push(path);
                }
            }
        }

        Ok(())
    }

    fn copy_extra_files(
        &self,
        src_dir: &Path,
        dest_dir: &Path,
        result: &mut ConversionResult,
    ) -> Result<()> {
        let extra_files = ["README.md", "LICENSE", "CHANGELOG.md", ".helmignore"];

        for file in &extra_files {
            let src = src_dir.join(file);
            if src.exists() {
                let dest = if *file == ".helmignore" {
                    dest_dir.join(".sherpackignore")
                } else {
                    dest_dir.join(file)
                };

                if !self.options.dry_run {
                    fs::copy(&src, &dest)?;
                }
                result.copied_files.push(dest);
            }
        }

        Ok(())
    }
}

// =============================================================================
// Macro Detection Helpers
// =============================================================================

/// Extract macro definitions from Jinja2 content
///
/// Finds all `{%- macro name() %}` patterns and returns the macro names.
/// This is the Jinja2 way - explicit definitions enable explicit imports.
fn extract_macro_definitions(content: &str) -> HashSet<String> {
    let re = Regex::new(r"\{%-?\s*macro\s+(\w+)\s*\(").expect("valid regex");
    re.captures_iter(content)
        .filter_map(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Find which macros from `defined` are used in the content
///
/// Scans for `macroName()` patterns that match defined macros.
/// Returns only the macros that are actually used.
fn find_used_macros(content: &str, defined: &HashSet<String>) -> HashSet<String> {
    let mut used = HashSet::new();

    for macro_name in defined {
        // Look for macro calls: macroName() with possible whitespace
        let pattern = format!(r"\b{}\s*\(\s*\)", regex::escape(macro_name));
        if let Ok(re) = Regex::new(&pattern) {
            if re.is_match(content) {
                used.insert(macro_name.clone());
            }
        }
    }

    used
}

// =============================================================================
// Public API
// =============================================================================

/// Quick convert function
pub fn convert(chart_path: &Path, output_path: &Path) -> Result<ConversionResult> {
    let converter = Converter::new(ConvertOptions::default());
    converter.convert(chart_path, output_path)
}

/// Convert with options
pub fn convert_with_options(
    chart_path: &Path,
    output_path: &Path,
    options: ConvertOptions,
) -> Result<ConversionResult> {
    let converter = Converter::new(options);
    converter.convert(chart_path, output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_chart(dir: &Path) {
        fs::create_dir_all(dir.join("templates")).unwrap();

        fs::write(
            dir.join("Chart.yaml"),
            r#"
apiVersion: v2
name: test-app
version: 1.0.0
description: A test application
"#,
        )
        .unwrap();

        fs::write(
            dir.join("values.yaml"),
            r#"
replicaCount: 1
image:
  repository: nginx
  tag: latest
"#,
        )
        .unwrap();

        fs::write(
            dir.join("templates/deployment.yaml"),
            r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .Release.Name }}
spec:
  replicas: {{ .Values.replicaCount }}
  template:
    spec:
      containers:
        - name: {{ .Chart.Name }}
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag }}"
"#,
        )
        .unwrap();

        fs::write(
            dir.join("templates/_helpers.tpl"),
            r#"
{{- define "test-app.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" }}
{{- end }}
"#,
        )
        .unwrap();
    }

    #[test]
    fn test_convert_simple_chart() {
        let chart_dir = TempDir::new().unwrap();
        let output_base = TempDir::new().unwrap();
        let output_dir = output_base.path().join("output");

        create_test_chart(chart_dir.path());

        let result = convert(chart_dir.path(), &output_dir).unwrap();

        assert!(!result.converted_files.is_empty());
        assert!(output_dir.join("Pack.yaml").exists());
        assert!(output_dir.join("values.yaml").exists());
        assert!(output_dir.join("templates").exists());
    }

    #[test]
    fn test_convert_deployment() {
        let chart_dir = TempDir::new().unwrap();
        let output_base = TempDir::new().unwrap();
        let output_dir = output_base.path().join("output");

        create_test_chart(chart_dir.path());

        convert(chart_dir.path(), &output_dir).unwrap();

        let deployment = fs::read_to_string(output_dir.join("templates/deployment.yaml")).unwrap();

        assert!(deployment.contains("release.name"));
        assert!(deployment.contains("values.replicaCount"));
        assert!(deployment.contains("pack.name"));
        assert!(deployment.contains("values.image.repository"));
    }

    #[test]
    fn test_convert_helpers() {
        let chart_dir = TempDir::new().unwrap();
        let output_base = TempDir::new().unwrap();
        let output_dir = output_base.path().join("output");

        create_test_chart(chart_dir.path());

        convert(chart_dir.path(), &output_dir).unwrap();

        let helpers_path = output_dir.join("templates/_helpers.j2");
        assert!(helpers_path.exists());

        let helpers = fs::read_to_string(&helpers_path).unwrap();
        assert!(helpers.contains("macro"));
        assert!(helpers.contains("endmacro"));
    }

    #[test]
    fn test_dry_run() {
        let chart_dir = TempDir::new().unwrap();
        let output_base = TempDir::new().unwrap();
        let output_dir = output_base.path().join("output");

        create_test_chart(chart_dir.path());

        let options = ConvertOptions {
            dry_run: true,
            ..Default::default()
        };

        let result = convert_with_options(chart_dir.path(), &output_dir, options).unwrap();

        // Should report files but not create them
        assert!(!result.converted_files.is_empty());
        assert!(!output_dir.join("Pack.yaml").exists());
    }

    #[test]
    fn test_force_overwrite() {
        let chart_dir = TempDir::new().unwrap();
        let output_base = TempDir::new().unwrap();
        let output_dir = output_base.path().join("output");

        create_test_chart(chart_dir.path());

        // First conversion
        convert(chart_dir.path(), &output_dir).unwrap();

        // Second conversion without force should fail
        let err = convert(chart_dir.path(), &output_dir);
        assert!(err.is_err());

        // With force should succeed
        let options = ConvertOptions {
            force: true,
            ..Default::default()
        };

        let result = convert_with_options(chart_dir.path(), &output_dir, options);
        assert!(result.is_ok());
    }
}
