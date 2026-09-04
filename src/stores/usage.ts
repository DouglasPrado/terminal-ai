import { create } from "zustand";
import type { UsageSnapshot } from "../lib/ipc";

interface UsageState {
  snapshot?: UsageSnapshot;
  setSnapshot: (snapshot: UsageSnapshot) => void;
}
export const useUsageStore = create<UsageState>((set) => ({
  setSnapshot: (snapshot) => set({ snapshot }),
}));
