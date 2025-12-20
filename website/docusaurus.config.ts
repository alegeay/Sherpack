import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'Sherpack',
  tagline: 'A blazingly fast Kubernetes package manager with Jinja2 templating',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  // GitHub Pages URL
  url: 'https://alegeay.github.io',
  baseUrl: '/Sherpack/',

  // GitHub pages deployment config
  organizationName: 'alegeay',
  projectName: 'Sherpack',
  trailingSlash: false,

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',

  // i18n configuration for French and English
  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'fr'],
    localeConfigs: {
      en: {
        label: 'English',
        htmlLang: 'en-US',
      },
      fr: {
        label: 'Français',
        htmlLang: 'fr-FR',
      },
    },
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/alegeay/sherpack/tree/main/website/',
          routeBasePath: 'docs',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themes: [
    [
      require.resolve('@easyops-cn/docusaurus-search-local'),
      {
        hashed: true,
        language: ['en', 'fr'],
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
        docsRouteBasePath: '/docs',
        indexBlog: false,
      },
    ],
  ],

  themeConfig: {
    image: 'img/sherpack-social-card.png',
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Sherpack',
      logo: {
        alt: 'Sherpack Logo',
        src: 'img/logo.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'left',
          label: 'Documentation',
        },
        {
          href: '/docs/cli-reference',
          label: 'CLI Reference',
          position: 'left',
        },
        {
          type: 'localeDropdown',
          position: 'right',
        },
        {
          href: 'https://github.com/alegeay/sherpack',
          label: 'GitHub',
          position: 'right',
          className: 'header-github-link',
          'aria-label': 'GitHub repository',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Documentation',
          items: [
            {
              label: 'Getting Started',
              to: '/docs/getting-started/installation',
            },
            {
              label: 'CLI Reference',
              to: '/docs/cli-reference',
            },
            {
              label: 'Architecture',
              to: '/docs/architecture',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'GitHub Discussions',
              href: 'https://github.com/alegeay/sherpack/discussions',
            },
            {
              label: 'Report Issues',
              href: 'https://github.com/alegeay/sherpack/issues',
            },
          ],
        },
        {
          title: 'More',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/alegeay/sherpack',
            },
            {
              label: 'Releases',
              href: 'https://github.com/alegeay/sherpack/releases',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Sherpack. Built with Docusaurus.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['bash', 'yaml', 'rust', 'toml', 'json'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
