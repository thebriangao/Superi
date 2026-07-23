# Superi editorial contracts

This package preserves the exact fail-closed timeline, caption, nesting, multicam, retime,
transition, clip-projection, playback, and editorial-feedback planners that were useful in the
removed web presentation.

These modules are external behavioral contract fixtures. Rust engine, timeline, graph, project,
audio, and public API crates remain canonical. A native interface may use these tests as migration
evidence, but it may not treat this package as a second authored-state owner.

Run:

```sh
npm test
npm run typecheck
```
