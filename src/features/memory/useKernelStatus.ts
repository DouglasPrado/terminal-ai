import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { type KernelStatus } from "../../lib/ipc";
import { useMemoryStore } from "../../stores/memory";

/**
 * Subscribes to kernel status once, for the whole app.
 *
 * Mounting the chip in several places is safe: the `subscribed` flag means only the first mount
 * opens the event listener, and nothing here polls. The backend pushes `memory-kernel-status` on
 * change only, and `get_memory_kernel_status` reads a cache in Rust — so N memory views cost the
 * kernel nothing (Principle IV, SC-020).
 */
export function useKernelStatus() {
  const status = useMemoryStore((s) => s.status);
  const setStatus = useMemoryStore((s) => s.setStatus);
  const ensureSubscribed = useMemoryStore((s) => s.ensureSubscribed);
  const refresh = useMemoryStore((s) => s.refresh);

  useEffect(() => {
    // Intentionally never torn down: the subscription belongs to the app, not to whichever view
    // happened to mount first. Unlistening on unmount would leave later views with stale status.
    ensureSubscribed(() => {
      void refresh();
      void listen<KernelStatus>("memory-kernel-status", (event) => setStatus(event.payload));
    });
  }, [ensureSubscribed, refresh, setStatus]);

  return status;
}
