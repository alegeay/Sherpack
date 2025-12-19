import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import Translate, {translate} from '@docusaurus/Translate';

import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <div className={styles.heroContent}>
          <div className={styles.heroTag}>
            <span className={styles.tagIcon}>⚡</span>
            <Translate id="homepage.hero.tag">Written in Rust</Translate>
          </div>
          <Heading as="h1" className="hero__title">
            {siteConfig.title}
          </Heading>
          <p className="hero__subtitle">
            <Translate id="homepage.hero.subtitle">
              A blazingly fast Kubernetes package manager with Jinja2 templating
            </Translate>
          </p>
          <div className={styles.heroStats}>
            <div className={styles.stat}>
              <span className={styles.statValue}>~5MB</span>
              <span className={styles.statLabel}>
                <Translate id="homepage.stats.binary">Binary size</Translate>
              </span>
            </div>
            <div className={styles.statDivider} />
            <div className={styles.stat}>
              <span className={styles.statValue}>282+</span>
              <span className={styles.statLabel}>
                <Translate id="homepage.stats.tests">Tests passing</Translate>
              </span>
            </div>
            <div className={styles.statDivider} />
            <div className={styles.stat}>
              <span className={styles.statValue}>0</span>
              <span className={styles.statLabel}>
                <Translate id="homepage.stats.runtime">Runtime deps</Translate>
              </span>
            </div>
          </div>
          <div className={styles.buttons}>
            <Link
              className="button button--primary button--lg"
              to="/docs/getting-started/installation">
              <Translate id="homepage.hero.getStarted">Get Started</Translate>
            </Link>
            <Link
              className="button button--outline button--lg"
              to="/docs/cli-reference">
              <Translate id="homepage.hero.cliReference">CLI Reference</Translate>
            </Link>
          </div>
          <div className={styles.codePreview}>
            <div className={styles.codeHeader}>
              <span className={styles.codeDot} style={{background: '#ff5f57'}} />
              <span className={styles.codeDot} style={{background: '#febc2e'}} />
              <span className={styles.codeDot} style={{background: '#28c840'}} />
              <span className={styles.codeTitle}>Terminal</span>
            </div>
            <pre className={styles.codeContent}>
              <code>
                <span className={styles.codePrompt}>$</span> sherpack install myapp ./pack -n production{'\n'}
                <span className={styles.codeOutput}>✓ Loaded pack myapp v1.0.0{'\n'}</span>
                <span className={styles.codeOutput}>✓ Validated values against schema{'\n'}</span>
                <span className={styles.codeOutput}>✓ Rendered 5 templates{'\n'}</span>
                <span className={styles.codeOutput}>✓ Applied 12 resources{'\n'}</span>
                <span className={styles.codeSuccess}>✓ Release myapp deployed successfully</span>
              </code>
            </pre>
          </div>
        </div>
      </div>
    </header>
  );
}

type FeatureItem = {
  icon: string;
  title: ReactNode;
  description: ReactNode;
};

const FeatureList: FeatureItem[] = [
  {
    icon: '🎨',
    title: <Translate id="homepage.features.jinja2.title">Jinja2 Templating</Translate>,
    description: (
      <>
        <Translate id="homepage.features.jinja2.description">
          Familiar Python-like syntax. No more fighting with Go templates.
          If you know Jinja2 or Ansible, you're ready to go.
        </Translate>
      </>
    ),
  },
  {
    icon: '☸️',
    title: <Translate id="homepage.features.kubernetes.title">Full Kubernetes Lifecycle</Translate>,
    description: (
      <Translate id="homepage.features.kubernetes.description">
        Install, upgrade, rollback, and uninstall with a single tool.
        Hooks, health checks, and automatic rollback on failure.
      </Translate>
    ),
  },
  {
    icon: '🔒',
    title: <Translate id="homepage.features.schema.title">Schema Validation</Translate>,
    description: (
      <Translate id="homepage.features.schema.description">
        Validate configuration with JSON Schema before deployment.
        Helpful error messages with suggestions for typos and missing fields.
      </Translate>
    ),
  },
  {
    icon: '✍️',
    title: <Translate id="homepage.features.signing.title">Package Signing</Translate>,
    description: (
      <Translate id="homepage.features.signing.description">
        Cryptographic signatures with Minisign for supply chain security.
        Verify package integrity before installation.
      </Translate>
    ),
  },
  {
    icon: '📦',
    title: <Translate id="homepage.features.repository.title">Repository Support</Translate>,
    description: (
      <Translate id="homepage.features.repository.description">
        HTTP and OCI registry support. Push to Docker Hub, GHCR, or any OCI-compliant registry.
        Full dependency management with lock files.
      </Translate>
    ),
  },
  {
    icon: '🚀',
    title: <Translate id="homepage.features.fast.title">Blazingly Fast</Translate>,
    description: (
      <Translate id="homepage.features.fast.description">
        Written in Rust with zero runtime dependencies.
        ~5MB binary vs ~50MB for Helm. Instant startup time.
      </Translate>
    ),
  },
];

function Feature({icon, title, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className={styles.featureCard}>
        <div className={styles.featureIcon}>{icon}</div>
        <Heading as="h3" className={styles.featureTitle}>{title}</Heading>
        <p className={styles.featureDescription}>{description}</p>
      </div>
    </div>
  );
}

function HomepageFeatures(): ReactNode {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className={styles.featuresHeader}>
          <Heading as="h2">
            <Translate id="homepage.features.heading">Why Sherpack?</Translate>
          </Heading>
          <p>
            <Translate id="homepage.features.subheading">
              Everything you need to manage Kubernetes applications, without the complexity.
            </Translate>
          </p>
        </div>
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}

function ComparisonSection(): ReactNode {
  return (
    <section className={styles.comparison}>
      <div className="container">
        <div className={styles.comparisonHeader}>
          <Heading as="h2">
            <Translate id="homepage.comparison.heading">Sherpack vs Helm</Translate>
          </Heading>
        </div>
        <div className={styles.comparisonGrid}>
          <div className={styles.comparisonCard}>
            <div className={styles.comparisonLabel}>Helm</div>
            <pre className={styles.comparisonCode}>
              <code>{`{{- if .Values.enabled }}
{{- range $key, $value := .Values.items }}
  {{ $key }}: {{ $value | quote }}
{{- end }}
{{- end }}`}</code>
            </pre>
          </div>
          <div className={styles.comparisonCard + ' ' + styles.comparisonCardHighlight}>
            <div className={styles.comparisonLabel}>Sherpack</div>
            <pre className={styles.comparisonCode}>
              <code>{`{% if values.enabled %}
{% for key, value in values.items %}
  {{ key }}: {{ value | quote }}
{% endfor %}
{% endif %}`}</code>
            </pre>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  return (
    <Layout
      title={translate({id: 'homepage.title', message: 'Kubernetes Package Manager'})}
      description={translate({id: 'homepage.description', message: 'A blazingly fast Kubernetes package manager with Jinja2 templating'})}>
      <HomepageHeader />
      <main>
        <HomepageFeatures />
        <ComparisonSection />
      </main>
    </Layout>
  );
}
