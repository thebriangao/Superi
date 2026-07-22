import {
  DESKTOP_PLATFORMS,
  PLATFORM_SEMANTIC_CONTRACTS,
  desktopPlatformLabel,
  platformSemanticSnapshot,
} from "./platform-parity.ts";

const SNAPSHOTS = DESKTOP_PLATFORMS.map(platformSemanticSnapshot);

export function PlatformParityPanel() {
  return (
    <section aria-labelledby="platform-parity-title">
      <header>
        <p className="eyebrow">Cross-platform contract</p>
        <h4 id="platform-parity-title">Application semantic parity</h4>
      </header>
      <p className="explanation">
        macOS, Windows, and Linux expose the same project and editor semantics.
        Native adapters may report different availability, but they cannot
        reinterpret commands, state, timing, channels, routing, or results.
      </p>
      <dl className="lifecycle-details">
        <div>
          <dt>Platforms</dt>
          <dd>
            {SNAPSHOTS.map(({ platform }) =>
              desktopPlatformLabel(platform),
            ).join(", ")}
          </dd>
        </div>
        <div>
          <dt>Contract</dt>
          <dd>{SNAPSHOTS[0].contract_id}</dd>
        </div>
        <div>
          <dt>Coverage</dt>
          <dd>{PLATFORM_SEMANTIC_CONTRACTS.length} shared semantic domains</dd>
        </div>
      </dl>
      <ul aria-label="Shared desktop semantic domains">
        {PLATFORM_SEMANTIC_CONTRACTS.map((contract) => (
          <li key={contract.domain}>
            <strong>{contract.domain}</strong>: {contract.invariant}
          </li>
        ))}
      </ul>
    </section>
  );
}
