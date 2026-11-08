import { ArrowClockwise, X } from "@phosphor-icons/react";
import { Channel } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal as XTerm } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
import { native } from "./api";
import { HostKeyPrompt, type HostKeyRequest } from "./HostKeyPrompt";
import type { Forward, Host, Settings } from "./model";
import "@xterm/xterm/css/xterm.css";
export interface Session {
  fingerprints?: string[];
  id: string;
  host: Host;
  forward?: Forward;
}
type Output =
  | { type: "data"; data: number[] }
  | { type: "exit"; code: number }
  | { type: "trustchanged" }
  | { type: "hostkey"; request: HostKeyRequest }
  | { type: "status"; status: string };
export function TerminalPane({
  session,
  active,
  focusRequest,
  settings,
  onClose,
  onOS,
  onTrustChanged,
}: {
  session: Session;
  active: boolean;
  focusRequest: number;
  settings: Settings;
  onClose: () => void;
  onOS: (id: string, os: string) => void;
  onTrustChanged: () => void;
}) {
  const container = useRef<HTMLDivElement>(null);
  const activeRef = useRef(active);
  activeRef.current = active;
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const osRef = useRef(onOS);
  osRef.current = onOS;
  const trustRef = useRef(onTrustChanged);
  trustRef.current = onTrustChanged;
  const [attempt, setAttempt] = useState(0);
  const [status, setStatus] = useState("Connecting");
  const [error, setError] = useState("");
  const [failed, setFailed] = useState(false);
  const [hostKey, setHostKey] = useState<HostKeyRequest | null>(null);
  const hostKeyRef = useRef<HostKeyRequest | null>(null);
  const terminalRef = useRef<XTerm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  useEffect(() => {
    if (!container.current) return;
    setStatus(attempt ? "Reconnecting" : "Connecting");
    setFailed(false);
    setError("");
    setHostKey(null);
    hostKeyRef.current = null;
    let disposed = false;
    let ended = false;
    let sessionId: string | undefined;
    const term = new XTerm({
      cursorBlink: true,
      macOptionIsMeta: settingsRef.current.optionAsMeta,
      fontFamily: settingsRef.current.font,
      fontSize: settingsRef.current.fontSize,
      lineHeight: settingsRef.current.lineHeight,
      theme: terminalTheme(settingsRef.current, container.current),
    });
    terminalRef.current = term;
    const fit = new FitAddon();
    fitRef.current = fit;
    term.loadAddon(fit);
    term.open(container.current);
    fit.fit();
    for (const fingerprint of session.fingerprints ?? [])
      term.writeln(`Synced fingerprint reference: ${fingerprint}`);
    if (session.fingerprints?.length)
      term.writeln(
        "Compare this reference with SSH’s fingerprint before accepting a new host key.\r\n",
      );
    const input = term.onData((data) => {
      if (sessionId && !hostKeyRef.current)
        void native("write_session", { id: sessionId, data }).catch((err) => {
          setError(String(err));
        });
    });
    const bell = term.onBell(() => {
      if (settingsRef.current.bell) {
        const ctx = new AudioContext();
        const oscillator = ctx.createOscillator();
        const gain = ctx.createGain();
        gain.gain.value = 0.08;
        oscillator.connect(gain);
        gain.connect(ctx.destination);
        oscillator.frequency.value = 700;
        oscillator.start();
        oscillator.stop(ctx.currentTime + 0.12);
        oscillator.onended = () => {
          void ctx.close();
        };
      }
    });
    term.parser.registerOscHandler(777, (data) => {
      if (data.startsWith("tabius-os="))
        osRef.current(session.host.id, data.slice(10));
      return true;
    });
    const channel = new Channel<Output>();
    channel.onmessage = (message) => {
      if (disposed) return;
      if (message.type === "data") term.write(new Uint8Array(message.data));
      else if (message.type === "trustchanged") trustRef.current();
      else if (message.type === "hostkey") {
        hostKeyRef.current = message.request;
        setHostKey(message.request);
        setStatus("Awaiting host verification");
        term.options.disableStdin = true;
      } else if (message.type === "status") {
        setStatus(message.status);
        if (message.status === "Reconnecting" || message.status === "Running")
          setFailed(false);
      } else {
        hostKeyRef.current = null;
        setHostKey(null);
        ended = true;
        setFailed(message.code !== 0);
        setStatus("Disconnected");
        term.writeln(
          `\r\n\x1b[90mSession ended (exit ${message.code}).\x1b[0m`,
        );
      }
    };
    void native<string>("start_session", {
      id: session.id,
      host: session.host,
      settings: settingsRef.current,
      forward: session.forward ?? null,
      rows: term.rows,
      cols: term.cols,
      onOutput: channel,
    })
      .then((id) => {
        if (disposed) {
          void native("close_session", { id });
          return;
        }
        sessionId = id;
        if (!ended && !hostKeyRef.current) {
          setStatus("Running");
          if (activeRef.current) term.focus();
        }
      })
      .catch((err) => {
        if (!disposed) {
          setStatus("Failed");
          setFailed(true);
          setError(String(err));
        }
      });
    const resize = term.onResize(({ rows, cols }) => {
      if (sessionId)
        void native("resize_session", { id: sessionId, rows, cols }).catch(
          () => {},
        );
    });
    const observer = new ResizeObserver(() => fit.fit());
    observer.observe(container.current);
    return () => {
      disposed = true;
      observer.disconnect();
      input.dispose();
      resize.dispose();
      bell.dispose();
      term.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      if (sessionId)
        void native("close_session", { id: sessionId }).catch(() => {});
    };
  }, [session, attempt]);
  useEffect(() => {
    if (!focusRequest || !activeRef.current) return;
    const frame = requestAnimationFrame(() => {
      fitRef.current?.fit();
      terminalRef.current?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [focusRequest]);
  useEffect(() => {
    if (terminalRef.current && container.current) {
      const options = terminalRef.current.options;
      options.fontSize = settings.fontSize;
      options.fontFamily = settings.font;
      options.lineHeight = settings.lineHeight;
      options.macOptionIsMeta = settings.optionAsMeta;
      options.theme = terminalTheme(settings, container.current);
      const frame = requestAnimationFrame(() => fitRef.current?.fit());
      return () => cancelAnimationFrame(frame);
    }
  }, [settings]);
  return (
    <div className="terminal-pane">
      <div className="terminal-toolbar">
        <span className={`status-dot ${status === "Running" ? "live" : ""}`} />
        <strong>
          {session.host.username ? `${session.host.username}@` : ""}
          {session.host.address}
        </strong>
        <span>
          {session.forward ? `${session.forward.kind} forwarding` : status}
        </span>
        <div className="spacer" />
        {failed && (
          <button
            type="button"
            onClick={() => {
              setError("");
              setFailed(false);
              setAttempt((value) => value + 1);
            }}
          >
            <ArrowClockwise size={16} /> Reconnect
          </button>
        )}
        <button
          type="button"
          className="icon-button"
          aria-label="Close terminal"
          onClick={onClose}
        >
          <X size={16} />
        </button>
      </div>
      {error && <div className="inline-error">{error}</div>}
      {hostKey && active && (
        <HostKeyPrompt
          key={hostKey.id}
          request={hostKey}
          sessionId={session.id}
          references={session.fingerprints ?? []}
          onAbort={onClose}
          onAnswered={() => {
            if (hostKeyRef.current?.id !== hostKey.id) return;
            hostKeyRef.current = null;
            setHostKey(null);
            setStatus("Running");
            if (terminalRef.current) {
              terminalRef.current.options.disableStdin = false;
              terminalRef.current.focus();
            }
          }}
        />
      )}
      <div
        className={`terminal-surface ${settings.theme.toLowerCase()}`}
        ref={container}
      />
    </div>
  );
}
function terminalTheme(settings: Settings, surface: HTMLElement) {
  const themes: Record<string, { foreground: string; cursor: string }> = {
    Graphite: {
      foreground: "#d4dedb",
      cursor: "#84d5b3",
    },
    Midnight: {
      foreground: "#d5deef",
      cursor: "#8cb9ff",
    },
    Paper: { foreground: "#313b36", cursor: "#2d735b" },
  };
  return {
    ...themes[settings.theme],
    // Match the canvas to its CSS surface, including padding and theme previews.
    background: getComputedStyle(surface).backgroundColor,
    ...(settings.highlighting
      ? {
          red: "#e68a89",
          green: "#8ac5a1",
          yellow: "#d6be89",
          blue: "#8cafe0",
          magenta: "#b9a2cf",
          cyan: "#86c7c4",
        }
      : {
          red: "#c9cecb",
          green: "#c9cecb",
          yellow: "#c9cecb",
          blue: "#c9cecb",
          magenta: "#c9cecb",
          cyan: "#c9cecb",
        }),
  };
}
