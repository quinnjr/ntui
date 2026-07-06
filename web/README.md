# ntui-docs

The `ntui` documentation site: [Astro](https://astro.build) + [Tailwind CSS](https://tailwindcss.com), reading content straight from [`../docs`](../docs) via an Astro content collection (see `src/content.config.ts`) so the docs stay single-sourced between GitHub and the deployed site.

```bash
pnpm install
pnpm dev      # http://localhost:4321/ntui/
pnpm build    # → dist/
pnpm preview  # serve the production build locally
pnpm astro check  # typecheck .astro files
```

Deployed to GitHub Pages at <https://quinnjr.github.io/ntui/> by
[`.github/workflows/docs.yml`](../.github/workflows/docs.yml) on every push to
`main` that touches `web/` or `docs/`.

## Adding a doc page

1. Add the markdown file to [`../docs`](../docs) (plain markdown, no
   frontmatter — it should also read well straight from GitHub).
2. Register it in `src/lib/docsNav.ts` (slug, title, description) — this
   drives the sidebar, prev/next links, and page metadata.

## Structure

- `src/content.config.ts` — the `docs` content collection, loaded from
  `../docs/*.md`.
- `src/lib/docsNav.ts` — sidebar ordering, titles, and per-page descriptions.
- `src/lib/url.ts` — `withBase()`, required for every internal root-relative
  link since the site is served from the `/ntui` GitHub Pages subpath.
- `src/components/Seo.astro` — title/description/canonical, Open Graph,
  Twitter card, and JSON-LD (SoftwareApplication + FAQPage on the homepage;
  TechArticle + BreadcrumbList on docs pages).
- `src/layouts/DocsLayout.astro` — shared shell (header, sidebar, footer).
- `src/pages/index.astro` — landing page.
- `src/pages/docs/[...slug].astro` — renders each `docs` collection entry
  listed in `docsNav.ts`.
- `public/robots.txt`, `public/llms.txt` — crawler/LLM-agent discoverability;
  `@astrojs/sitemap` generates `sitemap-index.xml` at build time.
