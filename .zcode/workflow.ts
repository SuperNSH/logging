// Plan for Logging in this repo. Phases run in order for each area:
// implement, audit, unit tests, device test, vision, smoke.
// Gates on every area: cargo fmt --check, cargo clippy -D warnings, cargo test.

export type Phase = "implement" | "audit" | "unit-tests" | "device-test" | "vision" | "smoke";

export interface Area {
  name: string;
  phases: Phase[];
  status: Record<Phase, "not-started" | "done" | "blocked">;
}

export const workflow = {
  repo: "SuperNSH/logging",
  gates: [
    "cargo fmt --all -- --check",
    "cargo clippy --all-targets --all-features -- -D warnings",
    "cargo test --all-features --no-fail-fast",
  ],
  areas: [
    {
      name: "logging",
      phases: ["implement", "audit", "unit-tests", "device-test", "vision", "smoke"],
      status: {
        "implement": "done",
        "audit": "done",
        "unit-tests": "done",
        // CI has no device; the desktop runners exercise the same std::fs paths.
        "device-test": "blocked",
        // No vision rig exists in CI; screenshots of the demo are not compared.
        "vision": "blocked",
        // The demo app's UI test exercises the library end to end on every push.
        "smoke": "done",
      },
    },
  ] satisfies Area[],
};
