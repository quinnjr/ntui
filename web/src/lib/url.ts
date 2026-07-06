// GitHub Pages serves this project from a subpath (`/ntui/`), configured as
// `base` in astro.config.mjs. Any internal, root-relative link must go
// through this helper instead of a hardcoded `/...` string, or it will 404
// once deployed.
export function withBase(path: string): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, ''); // e.g. "/ntui" (trailing slash normalized off)
  const cleanPath = path.replace(/^\//, '');
  return cleanPath ? `${base}/${cleanPath}` : `${base}/`;
}
