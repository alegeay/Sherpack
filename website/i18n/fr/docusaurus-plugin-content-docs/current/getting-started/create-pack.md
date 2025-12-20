---
id: create-pack
title: Créer un pack
sidebar_position: 3
---

# Créer un pack

Apprenez à créer un pack Sherpack complet.

## Scaffolding

```bash
sherpack create myapp
```

Ou avec un répertoire de sortie personnalisé :

```bash
sherpack create myapp -o ./packs
```

## Structure du pack

```
myapp/
├── Pack.yaml           # Métadonnées (obligatoire)
├── values.yaml         # Valeurs par défaut (obligatoire)
├── values.schema.yaml  # Schéma JSON (optionnel)
├── README.md           # Documentation (optionnel)
└── templates/          # Templates Jinja2 (obligatoire)
    ├── deployment.yaml
    ├── service.yaml
    ├── configmap.yaml
    ├── _helpers.tpl    # Templates partiels
    └── tests/          # Tests de hook
        └── test-connection.yaml
```

## Pack.yaml

Le fichier de métadonnées définit votre pack :

```yaml
apiVersion: sherpack/v1
kind: application  # ou 'library'
metadata:
  name: myapp
  version: 1.0.0
  description: Une application web moderne
  keywords:
    - web
    - api
  maintainers:
    - name: John Doe
      email: john@example.com
  sources:
    - https://github.com/example/myapp

# Dépendances optionnelles
dependencies:
  - name: redis
    version: ">=7.0.0"
    repository: https://charts.bitnami.com/bitnami
```

## values.yaml

Les valeurs par défaut de configuration :

```yaml
# Réplicas
replicaCount: 1

# Image
image:
  repository: myapp
  tag: latest
  pullPolicy: IfNotPresent

# Service
service:
  type: ClusterIP
  port: 80

# Ressources
resources:
  limits:
    cpu: 100m
    memory: 128Mi
  requests:
    cpu: 50m
    memory: 64Mi

# Ingress
ingress:
  enabled: false
  className: nginx
  hosts:
    - host: myapp.local
      paths:
        - path: /
          pathType: Prefix
```

## values.schema.yaml

Validez les valeurs avec un schéma :

```yaml
schemaVersion: "1.0"
title: Configuration MyApp
required:
  - image
  - service

properties:
  replicaCount:
    type: integer
    minimum: 1
    maximum: 100
    default: 1

  image:
    type: object
    required:
      - repository
    properties:
      repository:
        type: string
      tag:
        type: string
        default: latest
      pullPolicy:
        type: string
        enum: [Always, IfNotPresent, Never]
        default: IfNotPresent

  service:
    type: object
    properties:
      type:
        type: string
        enum: [ClusterIP, NodePort, LoadBalancer]
        default: ClusterIP
      port:
        type: integer
        minimum: 1
        maximum: 65535
        default: 80
```

## Templates

### Deployment

```yaml
# templates/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}-{{ pack.name }}
  labels:
    app.kubernetes.io/name: {{ pack.name }}
    app.kubernetes.io/instance: {{ release.name }}
    app.kubernetes.io/version: {{ pack.version | quote }}
spec:
  replicas: {{ values.replicaCount }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ pack.name }}
      app.kubernetes.io/instance: {{ release.name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {{ pack.name }}
        app.kubernetes.io/instance: {{ release.name }}
    spec:
      containers:
        - name: {{ pack.name }}
          image: "{{ values.image.repository }}:{{ values.image.tag }}"
          imagePullPolicy: {{ values.image.pullPolicy }}
          ports:
            - name: http
              containerPort: {{ values.service.port }}
          {% if values.resources %}
          resources:
            {{ values.resources | toyaml | indent(12) }}
          {% endif %}
```

### Helpers

```yaml
# templates/_helpers.tpl
{% macro fullname() %}
{{ release.name }}-{{ pack.name }}
{% endmacro %}

{% macro labels() %}
app.kubernetes.io/name: {{ pack.name }}
app.kubernetes.io/instance: {{ release.name }}
app.kubernetes.io/version: {{ pack.version | quote }}
app.kubernetes.io/managed-by: sherpack
{% endmacro %}

{% macro selectorLabels() %}
app.kubernetes.io/name: {{ pack.name }}
app.kubernetes.io/instance: {{ release.name }}
{% endmacro %}
```

## Tester le pack

```bash
# Linter
sherpack lint ./myapp

# Prévisualiser
sherpack template my-release ./myapp

# Valider le schéma
sherpack validate ./myapp -f custom-values.yaml

# Dry-run Kubernetes
sherpack install my-release ./myapp --dry-run
```

## Prochaines étapes

- [Templating](/docs/concepts/templating) - Syntaxe Jinja2 avancée
- [Validation de schéma](/docs/concepts/schema-validation) - Schémas JSON
- [Filtres](/docs/templating/filters) - Filtres disponibles
