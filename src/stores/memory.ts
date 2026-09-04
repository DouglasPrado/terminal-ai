import { create } from "zustand";
import { ipc, type KernelStatus } from "../lib/ipc";

interface MemoryState {
  status?: KernelStatus;
  /** Set once the app has subscribed. Guards against N panels each starting a poll. */
  subscribed: boolean;
  setStatus: (status: KernelStatus) => void;
  /**
   * Start the subscription, at most once per app lifetime.
   *
   * Returns whether it actually started, so the caller knows whether it owns the teardown. The
   * idempotency lives here rather than in the hook so it can be tested without a DOM — and because
   * "one subscription for the whole app" is a property of the state, not of any component.
   */
  ensureSubscribed: (start: () => void) => boolean;
  refresh: () => Promise<void>;
}

/**
 * Kernel state lives here, not in the components.
 *
 * The memory panel used to keep everything in local `useState`, which was survivable when memory
 * was a local table. It is not survivable now: every mounted view would poll the kernel on its own,
 * which is exactly the per-card polling Principle IV forbids. One store, one subscription, one
 * snapshot — `get_memory_kernel_status` reads a cache in Rust and performs no network call.
 */
export const useMemoryStore = create<MemoryState>((set, get) => ({
  subscribed: false,
  setStatus: (status) => set({ status }),
  ensureSubscribed: (start) => {
    if (get().subscribed) return false;
    set({ subscribed: true });
    start();
    return true;
  },
  refresh: async () => {
    // Deliberately not guarded by `subscribed`: an explicit refresh after the user starts or stops
    // the kernel should reflect immediately rather than waiting for the next pushed event.
    const { status } = await ipc.getMemoryKernelStatus();
    if (get().status?.lastCheckedAt !== status.lastCheckedAt) {
      set({ status });
    }
  },
}));

/** Human wording for a kernel state. Kept beside the store so it cannot drift per component. */
export function describeKernel(status: KernelStatus | undefined): {
  label: string;
  tone: "ok" | "warn" | "bad" | "idle";
} {
  if (!status) return { label: "verificando…", tone: "idle" };
  switch (status.state) {
    case "ready":
      return { label: "memória ativa", tone: "ok" };
    case "attached":
      return { label: "memória ativa (servidor externo)", tone: "ok" };
    case "probing":
    case "starting":
      return { label: "iniciando memória…", tone: "idle" };
    case "degraded":
      return { label: "memória instável", tone: "warn" };
    case "notInstalled":
      return { label: "memória não instalada", tone: "bad" };
    case "portConflict":
      return { label: "porta ocupada", tone: "bad" };
    case "failed":
      return { label: "memória indisponível", tone: "bad" };
    default:
      return { label: "memória indisponível", tone: "bad" };
  }
}
