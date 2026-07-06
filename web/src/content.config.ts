import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';

// The markdown files have no frontmatter (they're plain docs meant to also
// read well straight from GitHub), so no schema is needed here — titles and
// ordering live in `src/lib/docsNav.ts`.
const docs = defineCollection({
  loader: glob({ pattern: '*.md', base: '../docs' }),
});

export const collections = { docs };
