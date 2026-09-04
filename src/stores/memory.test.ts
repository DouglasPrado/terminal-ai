import { beforeEach, describe, expect, it, vi } from "vitest";
import { describeKernel, useMemoryStore } from "./memory";
import type { KernelStatus } from "../lib/ipc";

function status(overrides: Partial<KernelStatus> = {}): KernelStatus {
  return {
    state: "ready",
    owned: true,
    serverUrl: "http://127.0.0.1:49374",
    versionMatchesPin: true,
    hasToken: false,
    pendingMigration: 0,
    hybridSearch: false,
    lastCheckedAt: "2026-09-03T00:00:00Z",
    ...overrides,
  };
}

describe("memory kernel store", () => {
  beforeEach(() => {
    useMemoryStore.setState({ status: undefined, subscribed: false });
  });

  it("subscribes exactly once no matter how many views ask", () => {
    // SC-020 and Principle IV: the kernel is watched once for the whole app. Before this store
    // existed, memory state was component-local, so every mounted panel would have started its
    // own poll.
    const start = vi.fn();
    const results = Array.from({ length: 5 }, () =>
      useMemoryStore.getState().ensureSubscribed(start),
    );

    expect(start).toHaveBeenCalledTimes(1);
    expect(results).toEqual([true, false, false, false, false]);
  });

  it("keeps one shared snapshot", () => {
    useMemoryStore.getState().setStatus(status({ pages: 42 }));
    expect(useMemoryStore.getState().status?.pages).toBe(42);
  });

  it("tells an attached server apart from one we started", () => {
    // The distinction the whole ownership rule rests on: the UI must never offer to stop a server
    // the user was already running.
    expect(describeKernel(status({ state: "attached", owned: false })).label).toContain("externo");
    expect(describeKernel(status({ state: "ready", owned: true })).label).not.toContain("externo");
  });

  it("describes every failure state as something the user can act on", () => {
    for (const state of ["notInstalled", "portConflict", "failed", "degraded"] as const) {
      const described = describeKernel(status({ state }));
      expect(described.tone === "bad" || described.tone === "warn").toBe(true);
      expect(described.label.length).toBeGreaterThan(0);
    }
  });

  it("shows a neutral state while the kernel is still coming up", () => {
    // A starting kernel is not an error, and rendering it as one would make a normal boot look
    // broken for the first few seconds.
    for (const state of ["probing", "starting"] as const) {
      expect(describeKernel(status({ state })).tone).toBe("idle");
    }
    expect(describeKernel(undefined).tone).toBe("idle");
  });
});
