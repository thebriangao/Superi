# Frontend CI contract

This CI-only browser entry proves that Superi's locked TypeScript and Vite toolchain can install from
the committed lockfile, perform strict type checking without emitting JavaScript, and create a
production bundle. It is not the deferred React application or Tauri desktop shell, and it contains
no authoritative project or editing behavior. It does consume the committed generated bindings at
`open/bindings/typescript/superi-api.ts`, including the canonical project command, replacement
event, editor resource, AI state, and transport-neutral client types.
The smoke consumer also constructs the typed API version negotiation request and consumes its
registered response type through the same method map used by every transport.
It also consumes the extension discovery query, replacement event, and registry resource through
the generated method, event, and resource maps.

Run the same gates as CI:

```bash
npm ci
npm run typecheck
npm run build
npm test
```

The final test reads the generated bundle, so `npm run build` must run before `npm test`. When the
real application enters the repository in Phase 3, this workflow must run the same independent
typecheck and production-build gates against that application rather than treating this contract as
application coverage.
