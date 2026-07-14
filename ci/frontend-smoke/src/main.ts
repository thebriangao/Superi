type FrontendGate = "strict-types" | "production-bundle";

interface FrontendContract {
  readonly product: "Superi";
  readonly status: "ready";
  readonly gates: readonly FrontendGate[];
}

const contract = {
  product: "Superi",
  status: "ready",
  gates: ["strict-types", "production-bundle"],
} satisfies FrontendContract;

const root = document.querySelector("#contract-root");
if (!(root instanceof HTMLElement)) {
  throw new Error("frontend contract root is missing");
}

const heading = document.createElement("h1");
heading.textContent = `${contract.product} frontend contract`;

const summary = document.createElement("p");
summary.textContent = `${contract.status}: ${contract.gates.join(", ")}`;

root.replaceChildren(heading, summary);
