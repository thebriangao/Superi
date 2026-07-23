# Public API client contract

This transport-neutral TypeScript fixture proves that a strict external client can consume Superi's
committed generated public API binding independently of the native desktop application. It contains
no project authority, editing behavior, browser runtime, renderer, or privileged host integration.

The contract imports `open/bindings/typescript/superi-api.ts` and exercises the version negotiation,
project command, playback command, command log, replacement event, editor resource, extension
registry, AI state, and transport-neutral client types.

Run the same gates as continuous integration:

```bash
npm ci
npm run typecheck
npm test
```
