# ntui docs

User-facing documentation for the `ntui` library. Start here, then go deeper
as needed:

- [`getting-started.md`](getting-started.md) — install, build your first
  component, understand the render loop at a glance.
- [`hooks.md`](hooks.md) — every hook, its signature, and when to reach for it.
- [`architecture.md`](architecture.md) — the fiber tree, reconciler, layout,
  paint, and the two rendering backends (fullscreen vs. inline).
- [`superpowers/specs/2026-07-02-ntui-design.md`](superpowers/specs/2026-07-02-ntui-design.md) —
  the original design spec: full rationale for the fiber/reconciler/layout/paint
  pipeline and the `Backend` trait. `architecture.md` is the condensed,
  kept-current version of this; the spec is the historical record of *why*.
- [`superpowers/plans/2026-07-02-ntui.md`](superpowers/plans/2026-07-02-ntui.md) —
  the implementation plan the spec was built from.

For contributor-facing conventions (commands, invariants, testing patterns),
see [`CLAUDE.md`](../CLAUDE.md) at the repo root.
