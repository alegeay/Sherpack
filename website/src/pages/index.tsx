import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/docs">
            Get Started
          </Link>
          <Link
            className="button button--outline button--secondary button--lg"
            style={{marginLeft: '1rem'}}
            to="/docs/cli-reference">
            CLI Reference
          </Link>
        </div>
      </div>
    </header>
  );
}

type FeatureItem = {
  title: string;
  description: ReactNode;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Jinja2 Templating',
    description: (
      <>
        Familiar Python-like syntax with <code>{'{{ }}'}</code> and <code>{'{% %}'}</code>.
        No more fighting with Go templates. If you know Jinja2 or Ansible, you're ready.
      </>
    ),
  },
  {
    title: 'Full Kubernetes Lifecycle',
    description: (
      <>
        Install, upgrade, rollback, and uninstall with a single tool.
        Hooks, health checks, and automatic rollback on failure.
      </>
    ),
  },
  {
    title: 'Schema Validation',
    description: (
      <>
        Validate configuration with JSON Schema before deployment.
        Helpful error messages with suggestions for typos.
      </>
    ),
  },
  {
    title: 'Package Signing',
    description: (
      <>
        Cryptographic signatures with Minisign for supply chain security.
        Verify package integrity before installation.
      </>
    ),
  },
  {
    title: 'Repository Support',
    description: (
      <>
        HTTP and OCI registry support. Push to Docker Hub, GHCR, or any OCI-compliant registry.
        Dependency management with lock files.
      </>
    ),
  },
  {
    title: 'Blazingly Fast',
    description: (
      <>
        Written in Rust with zero runtime dependencies.
        ~5MB binary vs ~50MB for Helm.
      </>
    ),
  },
];

function Feature({title, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center padding-horiz--md padding-vert--lg">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

function HomepageFeatures(): ReactNode {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title="Kubernetes Package Manager"
      description="A blazingly fast Kubernetes package manager with Jinja2 templating">
      <HomepageHeader />
      <main>
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
