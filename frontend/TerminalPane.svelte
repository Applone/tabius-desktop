<script lang="ts">
  import { Channel } from "@tauri-apps/api/core";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Terminal as XTerm } from "@xterm/xterm";
  import { ArrowClockwise, X } from "phosphor-svelte";
  import { native } from "./api";
  import { terminalFontFamily } from "./fonts";
  import HostKeyPrompt from "./HostKeyPrompt.svelte";
  import type { HostKeyRequest, Settings } from "./model";
  import { bannerSystem, terminalControl } from "./terminal-input";
  import "@xterm/xterm/css/xterm.css";
  import { untrack } from "svelte";
  import type { Session } from "./model";

  type Output =
    | { type: "data"; data: number[] }
    | { type: "exit"; code: number; manual: boolean }
    | { type: "trustchanged" }
    | { type: "hostkey"; request: HostKeyRequest }
    | { type: "status"; status: string };
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
    const background = getComputedStyle(surface).backgroundColor;
    return {
      ...themes[settings.theme],
      background,
      // Powerline prompts use ANSI black as the background of dark segments.
      ...(settings.theme !== "Paper" && { black: background }),
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

  let {
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
  } = $props();

  let container: HTMLDivElement | undefined = $state();
  let attempt = $state(0);
  let status = $state("Connecting");
  let error = $state("");
  let failed = $state(false);
  let hostKey = $state.raw<HostKeyRequest | null>(null);
  let terminalRef: XTerm | null = null;
  let fitRef: FitAddon | null = null;
  function start(session: Session, attempt: number) {
    const surface = container;
    if (!surface) return;
    status = attempt ? "Reconnecting" : "Connecting";
    failed = false;
    error = "";
    hostKey = null;
    let disposed = false;
    let ended = false;
    let sessionId: string | undefined;
    let banner = "";
    let bannerDone = false;
    const bannerDecoder = new TextDecoder();
    const term = new XTerm({
      cursorBlink: true,
      macOptionIsMeta: settings.optionAsMeta,
      fontFamily: terminalFontFamily(settings.font),
      fontSize: (settings.fontSize * 96) / 72,
      lineHeight: settings.lineHeight,
      theme: terminalTheme(settings, surface),
    });
    terminalRef = term;
    const fit = new FitAddon();
    fitRef = fit;
    term.loadAddon(fit);
    term.open(surface);
    // The WebGL renderer draws Powerline separators to the cell edges.
    let renderer: WebglAddon | undefined;
    try {
      renderer = new WebglAddon();
      renderer.onContextLoss(() => renderer?.dispose());
      term.loadAddon(renderer);
    } catch {
      // Keep the terminal usable when the webview cannot provide WebGL.
      renderer?.dispose();
    }
    term.attachCustomKeyEventHandler((event) => {
      if (
        event.type === "keydown" &&
        !event.isComposing &&
        !event.getModifierState("AltGraph")
      ) {
        if (
          event.code === "KeyV" &&
          event.ctrlKey &&
          event.shiftKey &&
          !event.altKey &&
          !event.metaKey
        ) {
          event.preventDefault();
          if (!event.repeat && !hostKey)
            void readText()
              .then((text) => {
                if (text && !disposed && !ended && !hostKey) term.paste(text);
              })
              .catch((err) => {
                if (!disposed) error = `Could not paste: ${String(err)}`;
              });
          return false;
        }
        const control = terminalControl(event);
        if (
          control !== undefined &&
          (!event.altKey ||
            settings.optionAsMeta ||
            !/Mac/.test(navigator.platform))
        ) {
          event.preventDefault();
          if (sessionId && !hostKey)
            void native("write_session", {
              id: sessionId,
              data: control,
            }).catch((err) => (error = String(err)));
          return false;
        }
      }
      if (
        event.type !== "keydown" ||
        event.code !== "KeyC" ||
        !event.ctrlKey ||
        !event.shiftKey ||
        event.altKey ||
        event.metaKey ||
        event.isComposing ||
        event.getModifierState("AltGraph")
      )
        return true;
      event.preventDefault();
      if (term.hasSelection() && !event.repeat) {
        if (navigator.clipboard)
          void navigator.clipboard
            .writeText(term.getSelection())
            .catch((err) => {
              if (!disposed)
                error = `Could not copy terminal selection: ${String(err)}`;
            });
        else
          error = "Could not copy terminal selection: clipboard unavailable.";
      }
      return false;
    });
    fit.fit();
    for (const fingerprint of session.fingerprints ?? [])
      term.writeln(`Synced fingerprint reference: ${fingerprint}`);
    if (session.fingerprints?.length)
      term.writeln(
        "Compare this reference with SSH’s fingerprint before accepting a new host key.\r\n",
      );
    const input = term.onData((data) => {
      bannerDone = true;
      if (sessionId && !hostKey)
        void native("write_session", { id: sessionId, data }).catch((err) => {
          error = String(err);
        });
    });
    const bell = term.onBell(() => {
      if (settings.bell) {
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
      if (settings.detectOS && data.startsWith("tabius-os=")) {
        bannerDone = true;
        onOS(session.host.id, data.slice(10));
      }
      return true;
    });
    const channel = new Channel<Output>();
    channel.onmessage = (message) => {
      if (disposed) return;
      if (message.type === "data") {
        const bytes = new Uint8Array(message.data);
        if (!bannerDone && settings.detectOS) {
          banner += bannerDecoder.decode(bytes.slice(0, 8192 - banner.length), {
            stream: true,
          });
          const system = bannerSystem(banner);
          if (system) {
            onOS(session.host.id, system);
            bannerDone = true;
          }
          if (banner.length >= 8192) bannerDone = true;
        }
        term.write(bytes);
      } else if (message.type === "trustchanged") onTrustChanged();
      else if (message.type === "hostkey") {
        hostKey = message.request;
        status = "Awaiting host verification";
        term.options.disableStdin = true;
      } else if (message.type === "status") {
        status = message.status;
        if (message.status === "Reconnecting" || message.status === "Running")
          failed = false;
      } else {
        ended = true;
        if (message.manual) {
          onClose();
          return;
        }
        hostKey = null;
        failed = message.code !== 0;
        status = "Disconnected";
        term.writeln(
          `\r\n\x1b[90mSession ended (exit ${message.code}).\x1b[0m`,
        );
      }
    };
    void native<string>("start_session", {
      id: session.id,
      host: session.host,
      settings: settings,
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
        if (!ended && !hostKey) {
          status = "Running";
          if (active) term.focus();
        }
      })
      .catch((err) => {
        if (!disposed) {
          status = "Failed";
          failed = true;
          error = String(err);
        }
      });
    const resize = term.onResize(({ rows, cols }) => {
      if (sessionId)
        void native("resize_session", { id: sessionId, rows, cols }).catch(
          () => {},
        );
    });
    const observer = new ResizeObserver(() => fit.fit());
    observer.observe(surface);
    return () => {
      disposed = true;
      observer.disconnect();
      input.dispose();
      resize.dispose();
      bell.dispose();
      term.dispose();
      terminalRef = null;
      fitRef = null;
      if (sessionId)
        void native("close_session", { id: sessionId }).catch(() => {});
    };
  }
  $effect(() => {
    const currentSession = session;
    const currentAttempt = attempt;
    return untrack(() => start(currentSession, currentAttempt));
  });
  $effect(() => {
    if (!focusRequest || !untrack(() => active)) return;
    const frame = requestAnimationFrame(() => {
      fitRef?.fit();
      terminalRef?.focus();
    });
    return () => cancelAnimationFrame(frame);
  });
  $effect(() => {
    if (terminalRef && container) {
      const options = terminalRef.options;
      options.fontSize = (settings.fontSize * 96) / 72;
      options.fontFamily = terminalFontFamily(settings.font);
      options.lineHeight = settings.lineHeight;
      options.macOptionIsMeta = settings.optionAsMeta;
      options.theme = terminalTheme(settings, container);
      const frame = requestAnimationFrame(() => fitRef?.fit());
      return () => cancelAnimationFrame(frame);
    }
  });
</script>

<div class="terminal-pane">
  <div class="terminal-toolbar">
    <span class={`status-dot ${status === "Running" ? "live" : ""}`}></span
    ><strong
      >{session.host.username ? `${session.host.username}@` : ""}{session.host
        .address}</strong
    ><span
      >{session.forward ? `${session.forward.kind} forwarding` : status}</span
    >
    <div class="spacer"></div>
    {#if failed}
      <button
        type="button"
        onclick={() => {
          error = "";
          failed = false;
          ++attempt;
        }}
      >
        <ArrowClockwise size={16} aria-hidden="true" />
        Reconnect
      </button>
    {/if}
    <button
      type="button"
      class="icon-button"
      aria-label="Close terminal"
      onclick={onClose}
    >
      <X size={16} aria-hidden="true" />
    </button>
  </div>
  {#if error}
    <div class="inline-error">{error}</div>
  {/if}
  {#if hostKey && active}
    {@const request = hostKey}
    {#key request.id}
      <HostKeyPrompt
        {request}
        sessionId={session.id}
        references={session.fingerprints ?? []}
        onAbort={onClose}
        onAnswered={() => {
          if (hostKey?.id !== request.id) return;
          hostKey = null;
          status = "Running";
          if (terminalRef) {
            terminalRef.options.disableStdin = false;
            terminalRef.focus();
          }
        }}
      />
    {/key}
  {/if}
  <div
    class={`terminal-surface ${settings.theme.toLowerCase()}`}
    bind:this={container}
  ></div>
</div>
