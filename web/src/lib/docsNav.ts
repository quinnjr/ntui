export type NavItem = {
  slug: string;
  title: string;
  description: string;
};

// Explicit order + titles for the sidebar; the markdown files themselves
// (in ../../docs) stay plain, frontmatter-free docs readable straight from
// GitHub.
export const DOCS_NAV: NavItem[] = [
  {
    slug: 'getting-started',
    title: 'Getting Started',
    description: 'Install ntui, build your first component, understand the render loop.',
  },
  {
    slug: 'primitives',
    title: 'Primitives',
    description: 'The five element kinds every component lowers to: View, Text, Fragment, Provider, Component.',
  },
  {
    slug: 'hooks',
    title: 'Hooks',
    description: 'Every hook, its signature, and when to reach for it.',
  },
  {
    slug: 'widgets',
    title: 'Widgets',
    description: 'The first-party ntui::widgets layer: themed, focusable components built from the five primitives.',
  },
  {
    slug: 'architecture',
    title: 'Architecture',
    description: 'The fiber tree, reconciler, layout, paint, and the two rendering backends.',
  },
];
