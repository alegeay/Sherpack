import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    'intro',
    {
      type: 'category',
      label: 'Getting Started',
      collapsed: false,
      items: [
        'getting-started/installation',
        'getting-started/quick-start',
        'getting-started/create-pack',
      ],
    },
    {
      type: 'category',
      label: 'Core Concepts',
      items: [
        'concepts/pack-structure',
        'concepts/values',
        'concepts/templating',
        'concepts/schema-validation',
      ],
    },
    {
      type: 'category',
      label: 'Templating',
      items: [
        'templating/context-variables',
        'templating/filters',
        'templating/functions',
        'templating/control-structures',
      ],
    },
    {
      type: 'category',
      label: 'Kubernetes',
      items: [
        'kubernetes/install-upgrade',
        'kubernetes/rollback-uninstall',
        'kubernetes/hooks',
        'kubernetes/health-checks',
        'kubernetes/storage-drivers',
        'kubernetes/crd-handling',
      ],
    },
    {
      type: 'category',
      label: 'Packaging',
      items: [
        'packaging/create-archive',
        'packaging/signing',
        'packaging/verification',
      ],
    },
    {
      type: 'category',
      label: 'Repositories',
      items: [
        'repositories/configuration',
        'repositories/search-pull',
        'repositories/oci-push',
        'repositories/dependencies',
      ],
    },
    'cli-reference',
    'architecture',
  ],
};

export default sidebars;
