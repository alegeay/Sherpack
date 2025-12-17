//! Create command - scaffold a new pack

use console::style;
use miette::{IntoDiagnostic, Result, WrapErr};
use std::fs;
use std::path::Path;

pub fn run(name: &str, output: &Path) -> Result<()> {
    let pack_dir = output.join(name);

    // Check if directory exists
    if pack_dir.exists() {
        return Err(miette::miette!(
            "Directory {} already exists",
            pack_dir.display()
        ));
    }

    // Create directory structure
    fs::create_dir_all(&pack_dir)
        .into_diagnostic()
        .wrap_err("Failed to create pack directory")?;

    fs::create_dir_all(pack_dir.join("templates"))
        .into_diagnostic()
        .wrap_err("Failed to create templates directory")?;

    // Create Pack.yaml
    let pack_yaml = format!(
        r#"apiVersion: sherpack/v1
kind: application

metadata:
  name: {name}
  version: 0.1.0
  description: A Sherpack pack for {name}
  appVersion: "1.0.0"

engine:
  strict: true
"#
    );

    fs::write(pack_dir.join("Pack.yaml"), pack_yaml)
        .into_diagnostic()
        .wrap_err("Failed to write Pack.yaml")?;

    // Create values.yaml
    let values_yaml = format!(
        r#"# Default values for {name}

replicaCount: 1

image:
  repository: nginx
  tag: "latest"
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 80

resources: {{}}
  # limits:
  #   cpu: 100m
  #   memory: 128Mi
  # requests:
  #   cpu: 100m
  #   memory: 128Mi
"#
    );

    fs::write(pack_dir.join("values.yaml"), values_yaml)
        .into_diagnostic()
        .wrap_err("Failed to write values.yaml")?;

    // Create deployment.yaml template
    let deployment = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{{{ release.name }}}}-{name}
  labels:
    app.kubernetes.io/name: {name}
    app.kubernetes.io/instance: {{{{ release.name }}}}
    app.kubernetes.io/version: {{{{ pack.appVersion | default("unknown") }}}}
    app.kubernetes.io/managed-by: {{{{ release.service }}}}
spec:
  replicas: {{{{ values.replicaCount }}}}
  selector:
    matchLabels:
      app.kubernetes.io/name: {name}
      app.kubernetes.io/instance: {{{{ release.name }}}}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {name}
        app.kubernetes.io/instance: {{{{ release.name }}}}
    spec:
      containers:
        - name: {name}
          image: "{{{{ values.image.repository }}}}:{{{{ values.image.tag }}}}"
          imagePullPolicy: {{{{ values.image.pullPolicy }}}}
          ports:
            - name: http
              containerPort: 80
              protocol: TCP
          {{% if values.resources %}}
          resources:
            {{{{ values.resources | toyaml | nindent(12) }}}}
          {{% endif %}}
"#
    );

    fs::write(pack_dir.join("templates/deployment.yaml"), deployment)
        .into_diagnostic()
        .wrap_err("Failed to write deployment.yaml")?;

    // Create service.yaml template
    let service = format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {{{{ release.name }}}}-{name}
  labels:
    app.kubernetes.io/name: {name}
    app.kubernetes.io/instance: {{{{ release.name }}}}
spec:
  type: {{{{ values.service.type }}}}
  ports:
    - port: {{{{ values.service.port }}}}
      targetPort: http
      protocol: TCP
      name: http
  selector:
    app.kubernetes.io/name: {name}
    app.kubernetes.io/instance: {{{{ release.name }}}}
"#
    );

    fs::write(pack_dir.join("templates/service.yaml"), service)
        .into_diagnostic()
        .wrap_err("Failed to write service.yaml")?;

    // Create NOTES.txt
    let notes = format!(
        r#"Thank you for installing {{{{ pack.name }}}}.

Your release is named: {{{{ release.name }}}}

To get the application URL, run:
{{% if values.service.type == "NodePort" %}}
  export NODE_PORT=$(kubectl get --namespace {{{{ release.namespace }}}} -o jsonpath="{{{{.spec.ports[0].nodePort}}}}" services {{{{ release.name }}}}-{name})
  export NODE_IP=$(kubectl get nodes --namespace {{{{ release.namespace }}}} -o jsonpath="{{{{.items[0].status.addresses[0].address}}}}")
  echo http://$NODE_IP:$NODE_PORT
{{% elif values.service.type == "LoadBalancer" %}}
  export SERVICE_IP=$(kubectl get svc --namespace {{{{ release.namespace }}}} {{{{ release.name }}}}-{name} --template "{{{{{{ range (index .status.loadBalancer.ingress 0) }}}}}}{{{{{{.}}}}}}{{{{{{ end }}}}}}")
  echo http://$SERVICE_IP:{{{{ values.service.port }}}}
{{% else %}}
  kubectl --namespace {{{{ release.namespace }}}} port-forward svc/{{{{ release.name }}}}-{name} {{{{ values.service.port }}}}:{{{{ values.service.port }}}}
  echo "Visit http://127.0.0.1:{{{{ values.service.port }}}}"
{{% endif %}}
"#
    );

    fs::write(pack_dir.join("templates/NOTES.txt"), notes)
        .into_diagnostic()
        .wrap_err("Failed to write NOTES.txt")?;

    // Create .gitignore
    let gitignore = r#"# Sherpack
*.tgz
packs/
"#;

    fs::write(pack_dir.join(".gitignore"), gitignore)
        .into_diagnostic()
        .wrap_err("Failed to write .gitignore")?;

    println!(
        "{} Created pack {} at {}",
        style("✓").green().bold(),
        style(name).cyan(),
        style(pack_dir.display()).dim()
    );

    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {} to customize your pack",
        style("values.yaml").cyan()
    );
    println!(
        "  2. Edit templates in {}",
        style("templates/").cyan()
    );
    println!(
        "  3. Test with: {} template myrelease {}",
        style("sherpack").green(),
        pack_dir.display()
    );

    Ok(())
}
