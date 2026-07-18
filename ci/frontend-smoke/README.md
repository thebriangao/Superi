# Frontend CI contract

This retained browser fixture proves that Superi's locked TypeScript and Vite compatibility surface
can consume the generated public API bindings independently of the desktop shell. It is not the
production React application or Tauri desktop shell, and it contains no authoritative project or
editing behavior. It consumes the committed generated bindings at
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

The final test reads the generated bundle, so `npm run build` must run before `npm test`. Blocking
frontend CI now runs those gates against `/app`; this fixture remains a focused compatibility test
for the generated public contract.
