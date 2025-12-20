---
id: tutorial
title: Tutoriel
sidebar_position: 3
---

# Tutoriel Sherpack

Apprenez Sherpack en construisant un pack d'application web complet de A à Z.

## Aperçu

Dans ce tutoriel, vous apprendrez à :
- Créer une structure de pack
- Écrire des templates Jinja2
- Configurer les valeurs et les schémas
- Déployer sur Kubernetes
- Gérer les releases

**Durée** : ~20 minutes

---

## Partie 1 : Créer votre pack

### Initialiser le pack

```bash
sherpack create webapp
cd webapp
```

Cela crée la structure de base :

```
webapp/
├── Pack.yaml         # Métadonnées
├── values.yaml       # Valeurs par défaut
└── templates/
    └── deployment.yaml
```

### Définir les métadonnées

Éditez `Pack.yaml` :

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: webapp
  version: 1.0.0
  description: Une application web exemple
  appVersion: "1.0"
```

---

## Partie 2 : Configurer les valeurs

Éditez `values.yaml` :

```yaml
# Application
app:
  name: webapp
  replicas: 2

# Image
image:
  repository: nginx
  tag: "1.25-alpine"
  pullPolicy: IfNotPresent

# Service
service:
  type: ClusterIP
  port: 80

# Ressources
resources:
  limits:
    cpu: "200m"
    memory: "128Mi"
  requests:
    cpu: "100m"
    memory: "64Mi"

# Fonctionnalités optionnelles
ingress:
  enabled: false
  host: ""
```

---

## Partie 3 : Écrire les templates

### Deployment

Remplacez `templates/deployment.yaml` :

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app: {{ values.app.name }}
spec:
  replicas: {{ values.app.replicas }}
  selector:
    matchLabels:
      app: {{ values.app.name }}
      instance: {{ release.name }}
  template:
    metadata:
      labels:
        app: {{ values.app.name }}
        instance: {{ release.name }}
    spec:
      containers:
        - name: {{ values.app.name }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          imagePullPolicy: {{ values.image.pullPolicy }}
          ports:
            - containerPort: 80
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```

### Service

Créez `templates/service.yaml` :

```yaml
apiVersion: v1
kind: Service
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
spec:
  type: {{ values.service.type }}
  ports:
    - port: {{ values.service.port }}
      targetPort: 80
  selector:
    app: {{ values.app.name }}
    instance: {{ release.name }}
```

### Ingress optionnel

Créez `templates/ingress.yaml` :

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
spec:
  rules:
    - host: {{ values.ingress.host }}
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: {{ release.name }}
                port:
                  number: {{ values.service.port }}
{% endif %}
```

---

## Partie 4 : Tester votre pack

### Valider la structure

```bash
sherpack lint .
```

Sortie attendue :
```
✓ Pack.yaml is valid
✓ values.yaml is valid
✓ Templates render successfully
```

### Prévisualiser la sortie

```bash
# Rendre avec les valeurs par défaut
sherpack template myapp .

# Rendre avec des valeurs personnalisées
sherpack template myapp . --set app.replicas=5
```

### Tester avec des valeurs personnalisées

Créez `values-prod.yaml` :

```yaml
app:
  replicas: 5

image:
  tag: "1.25.3"

service:
  type: LoadBalancer

ingress:
  enabled: true
  host: webapp.example.com
```

```bash
sherpack template myapp . -f values-prod.yaml
```

---

## Partie 5 : Déployer sur Kubernetes

### Installer

```bash
# Installation basique
sherpack install myapp . -n default

# Avec les valeurs de production
sherpack install myapp . -n production -f values-prod.yaml --create-namespace

# Avec attente et atomique
sherpack install myapp . --wait --atomic --timeout 120
```

### Vérifier le statut

```bash
# Lister les releases
sherpack list

# Vérifier le statut
sherpack status myapp

# Voir l'historique
sherpack history myapp
```

### Mettre à niveau

```bash
# Mise à niveau avec nouvelles valeurs
sherpack upgrade myapp . --set image.tag=1.26-alpine

# Prévisualiser les changements d'abord
sherpack upgrade myapp . --set image.tag=1.26-alpine --diff --dry-run
```

### Rollback

```bash
# Voir l'historique
sherpack history myapp

# Rollback vers la révision 1
sherpack rollback myapp 1
```

### Désinstaller

```bash
sherpack uninstall myapp
```

---

## Partie 6 : Ajouter la validation de schéma

Créez `values.schema.yaml` :

```yaml
$schema: http://json-schema.org/draft-07/schema#
type: object
required:
  - app
  - image

properties:
  app:
    type: object
    properties:
      name:
        type: string
        default: webapp
      replicas:
        type: integer
        minimum: 1
        maximum: 100
        default: 2

  image:
    type: object
    required:
      - repository
    properties:
      repository:
        type: string
      tag:
        type: string
        default: "latest"
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
        default: 80
```

Maintenant validez :

```bash
sherpack validate .
```

---

## Partie 7 : Créer des macros d'aide

Créez `templates/_helpers.tpl` :

```jinja
{% macro labels() %}
app.kubernetes.io/name: {{ values.app.name }}
app.kubernetes.io/instance: {{ release.name }}
app.kubernetes.io/version: {{ pack.version }}
app.kubernetes.io/managed-by: sherpack
{% endmacro %}

{% macro selectorLabels() %}
app.kubernetes.io/name: {{ values.app.name }}
app.kubernetes.io/instance: {{ release.name }}
{% endmacro %}
```

Utilisez dans les templates :

```yaml
{% from "_helpers.tpl" import labels, selectorLabels %}

apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  labels:
    {{ labels() | indent(4) }}
spec:
  selector:
    matchLabels:
      {{ selectorLabels() | indent(6) }}
```

---

## Partie 8 : Packager et distribuer

### Créer l'archive

```bash
sherpack package .
# Crée : webapp-1.0.0.tar.gz
```

### Signer l'archive

```bash
# Générer les clés (une seule fois)
sherpack keygen

# Signer
sherpack sign webapp-1.0.0.tar.gz -k ~/.sherpack/keys/sherpack.key
```

### Pousser vers un registry

```bash
sherpack push webapp-1.0.0.tar.gz oci://registry.example.com/packs/webapp:1.0.0
```

---

## Résumé

Vous avez appris à :

| Tâche | Commande |
|-------|----------|
| Créer un pack | `sherpack create <nom>` |
| Valider | `sherpack lint` / `sherpack validate` |
| Prévisualiser | `sherpack template <release> <pack>` |
| Installer | `sherpack install <release> <pack>` |
| Mettre à niveau | `sherpack upgrade <release> <pack>` |
| Rollback | `sherpack rollback <release> <rév>` |
| Désinstaller | `sherpack uninstall <release>` |
| Packager | `sherpack package <pack>` |

## Prochaines étapes

- Explorez la [Référence des filtres](/docs/templating/filters) pour tous les filtres et fonctions
- Apprenez la [Gestion des CRD](/docs/kubernetes/crd-handling) pour les opérations Kubernetes avancées
- Consultez la [Référence CLI](/docs/cli-reference) pour toutes les commandes
- Découvrez la [Conversion depuis Helm](/docs/cli-reference#convert)
