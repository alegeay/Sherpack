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
  url: 'https://music-soul1-1.github.io',
  baseUrl: '/sherpack/',

  // GitHub pages deployment config
  organizationName: 'music-soul1-1',
  projectName: 'sherpack',
  trailingSlash: false,

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/music-soul1-1/sherpack/tree/main/website/',
          routeBasePath: 'docs',
        },
        blog: false, // Disable blog
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
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
          href: 'https://github.com/music-soul1-1/sherpack',
          label: 'GitHub',
          position: 'right',
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
          title: 'More',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/music-soul1-1/sherpack',
            },
            {
              label: 'Issues',
              href: 'https://github.com/music-soul1-1/sherpack/issues',
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
    algolia: undefined, // Disable Algolia for now
  } satisfies Preset.ThemeConfig,
};

export default config;
