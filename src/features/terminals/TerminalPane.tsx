import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { BrainCircuit } from "lucide-react";
import { Button } from "../../components/Button";
import { ipc } from "../../lib/ipc";

export function TerminalPane({
  sessionId,
  active,
  onReady,
}: {
  sessionId: string;
  active?: boolean;
  onReady?: () => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const [selection, setSelection] = useState("");
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const terminal = new Terminal({
      allowProposedApi: true,
      convertEol: false,
      cursorBlink: true,
      cursorStyle: "block",
      // Option must COMPOSE accented characters (ç, á, ã, ê…) on a macOS/ABNT keyboard rather than
      // act as Meta — otherwise ⌥C and the dead-key accents never reach the PTY. Trade-off: no
      // Option-as-Meta (readline Alt+B/F/D) in the terminal; can be promoted to a setting if needed.
      macOptionIsMeta: false,
      // xterm renders to canvas/WebGL and cannot resolve CSS vars — pass a concrete stack.
      fontFamily:
        '"SF Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, "Cascadia Code", "JetBrains Mono", Consolas, monospace',
      fontSize: 14,
      fontWeight: 400,
      fontWeightBold: 700,
      lineHeight: 1.2,
      letterSpacing: 0,
      scrollback: 10_000,
      rightClickSelectsWord: false,
      // Full 16-color ANSI palette so agent TUIs (Claude/Codex/OpenCode) render in real colors,
      // not washed-out white. Brand-aligned but balanced for a native-terminal feel.
      theme: {
        background: "#08070d",
        foreground: "#e2e0ea",
        cursor: "#e879f9",
        cursorAccent: "#08070d",
        selectionBackground: "#e879f93d",
        black: "#171425",
        red: "#ff5c57",
        green: "#34d399",
        yellow: "#facc15",
        blue: "#60a5fa",
        magenta: "#e879f9",
        cyan: "#22d3ee",
        white: "#e5e5e5",
        brightBlack: "#655e80",
        brightRed: "#ff8f87",
        brightGreen: "#6ee7b7",
        brightYellow: "#fde047",
        brightBlue: "#93c5fd",
        brightMagenta: "#f0abfc",
        brightCyan: "#67e8f9",
        brightWhite: "#fafafa",
      },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(new SearchAddon());
    terminal.loadAddon(new Unicode11Addon());
    terminal.loadAddon(
      new WebLinksAddon((event, uri) => {
        event.preventDefault();
        if (window.confirm(`Open this external URL in your browser?\n\n${uri}`)) {
          const opened = window.open(uri, "_blank", "noopener,noreferrer");
          if (opened) opened.opener = null;
        }
      }),
    );
    terminal.unicode.activeVersion = "11";
    terminal.open(container);
    try {
      terminal.loadAddon(new WebglAddon());
    } catch {
      /* canvas renderer remains available */
    }
    terminalRef.current = terminal;
    fit.fit();
    const input = terminal.onData((data) => void ipc.writeInput(sessionId, data));
    const resize = terminal.onResize(
      ({ cols, rows }) => void ipc.resizeSession(sessionId, cols, rows),
    );
    // Dynamic terminal titles (T068): xterm parses OSC 0/2 title sequences from the PTY stream;
    // surface them (sanitized/bounded) so the pane header reflects the running program / cwd.
    const title = terminal.onTitleChange((value) =>
      window.dispatchEvent(
        new CustomEvent("terminal-title", {
          detail: {
            sessionId,
            title: [...value]
              .filter(
                (character) => character.charCodeAt(0) >= 32 && character.charCodeAt(0) !== 127,
              )
              .join("")
              .slice(0, 120),
          },
        }),
      ),
    );
    const selectionChange = terminal.onSelectionChange(() => setSelection(terminal.getSelection()));
    // Ordered, de-duplicated output (T065): buffer live chunks by `seq` until the scrollback
    // backfill is written, then flush them in order — so live output is never written before the
    // history (which would duplicate/reorder lines), and any duplicate delivery is dropped.
    const writeBase64 = (data: string) =>
      terminal.write(Uint8Array.from(atob(data), (char) => char.charCodeAt(0)));
    let ready = false;
    let lastSeq = -1;
    const pending: Array<{ seq: number; bytes: string }> = [];
    const writeChunk = (seq: number, bytes: string) => {
      if (seq <= lastSeq) return;
      lastSeq = seq;
      writeBase64(bytes);
    };
    const output = (event: Event) => {
      const chunk = (event as CustomEvent<{ seq: number; bytes: string }>).detail;
      if (ready) writeChunk(chunk.seq, chunk.bytes);
      else pending.push(chunk);
    };
    window.addEventListener(`terminal-output:${sessionId}`, output);
    const observer = new ResizeObserver(() => {
      fit.fit();
    });
    observer.observe(container);
    void ipc
      .getScrollback(sessionId)
      .then((result) => writeBase64(result.data))
      .catch(() => {})
      .finally(() => {
        pending.sort((a, b) => a.seq - b.seq);
        for (const chunk of pending) writeChunk(chunk.seq, chunk.bytes);
        pending.length = 0;
        ready = true;
      });
    onReady?.();
    return () => {
      window.removeEventListener(`terminal-output:${sessionId}`, output);
      observer.disconnect();
      input.dispose();
      resize.dispose();
      title.dispose();
      selectionChange.dispose();
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [sessionId, onReady]);
  // Give this pane real keyboard focus when it becomes active (e.g. via Cmd/Ctrl+Arrow), so
  // typing lands in the newly-selected terminal, not the previously-focused one (FR-031).
  useEffect(() => {
    if (active) terminalRef.current?.focus();
  }, [active]);
  return (
    <div className="relative h-full w-full">
      <div
        ref={containerRef}
        className="h-full w-full bg-app px-1.5 py-1"
        aria-label="Interactive terminal"
      />
      {selection && (
        <Button
          variant="accent"
          size="sm"
          className="absolute bottom-3 right-4 z-20"
          onClick={() =>
            window.dispatchEvent(
              new CustomEvent("terminal-memory-selection", {
                detail: { sessionId, text: selection },
              }),
            )
          }
        >
          <BrainCircuit size={13} /> Salvar na memória
        </Button>
      )}
    </div>
  );
}
