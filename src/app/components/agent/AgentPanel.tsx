import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowUp,
  Copy,
  RotateCcw,
  X,
  Check,
  SquarePen,
  History as HistoryIcon,
} from "lucide-react";
import {
  commands,
  type AgentChatList,
  type AgentHistoryMessage,
  type AgentNotchMode,
  type AgentNotchStatus,
  type AgentMessage,
  type AgentConfirm,
  type AgentReply,
} from "@/bindings";
import { AgentMarkdown } from "../markdown/Markdown";
import { AgentHistory } from "./AgentHistory";
import { AgentRecall } from "./AgentRecall";
import { AgentVoiceFade } from "./AgentVoiceFade";
import { AgentWorkingLabel } from "./AgentWorkingLabel";

type Role = "user" | "assistant";
interface ChatMessage {
  id: string;
  role: Role;
  content: string;
  // A host-held extension/MCP call. Only the token displayed here can resume it.
  confirmAction?: AgentConfirm | null;
}

const rid = () => `${Date.now()}-${Math.random().toString(36).slice(2)}`;
function resizeComposer(input: HTMLTextAreaElement) {
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 76)}px`;
}

/** Call `done` once `shell` has sat at `width` x `height` CSS px for a few
 * frames (its transition finished and painted), or after a cap. Returns a
 * cancel. */
function whenSettled(
  shell: HTMLElement | null,
  width: number,
  height: number,
  done: () => void,
): () => void {
  const start = performance.now();
  let still = 0;
  let frame = 0;
  const tick = () => {
    const rect = shell?.getBoundingClientRect();
    const there =
      !!rect &&
      Math.abs(rect.width - width) < 1 &&
      Math.abs(rect.height - height) < 1;
    still = there ? still + 1 : 0;
    if (still >= 3 || performance.now() - start > 1500) done();
    else frame = requestAnimationFrame(tick);
  };
  frame = requestAnimationFrame(tick);
  return () => cancelAnimationFrame(frame);
}

/** Earlier turns sent with each request, so the chat continues. */
const HISTORY_CONTEXT = 12;
/** The Recall shell width, in CSS px (`.agent-notch.is-recall` in agent.css). */
const RECALL_WIDTH = 380;
/** Hover time that opens a listening or working notch (FILL in
 * grain_notch_sliver.rs). */
const HOVER_OPEN_MS = 650;

const toPayload = (messages: ChatMessage[]): AgentMessage[] =>
  messages.map((m) => ({ role: m.role, content: m.content }));

const toChat = (messages: AgentHistoryMessage[]): ChatMessage[] =>
  messages.map((message, index) => ({
    id: `h${index}`,
    role: message.role === "assistant" ? "assistant" : "user",
    content: message.content,
  }));

/** Store what was said in a finished turn; a reply still awaiting approval
 * is stored once it resolves. */
function remember(user: string | null, reply: AgentReply | null) {
  const turn: AgentMessage[] = [];
  if (user?.trim()) turn.push({ role: "user", content: user });
  if (reply?.text.trim() && !reply.confirm_action)
    turn.push({ role: "assistant", content: reply.text });
  if (turn.length) void commands.agentHistoryAppend(turn).catch(console.error);
}

export function AgentPanel() {
  const { t } = useTranslation();
  // ONE handle for the component's life. `getCurrentWindow()` mints a fresh
  // object on every call, so calling it inline made `win` a new identity each
  // render — every effect that depends on it tore down and re-subscribed
  // constantly, and `listen()` is async, so any subscription still in flight
  // when its own cleanup ran survived forever. Those orphans stacked up and
  // each one handled the same event again (one dictation pasted N times).
  const win = useMemo(() => getCurrentWindow(), []);

  // The first reply can be retried until the conversation has a follow-up.
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [firstReply, setFirstReply] = useState<AgentReply | null>(null);
  const [hasFollowups, setHasFollowups] = useState(false);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [flashedId, setFlashedId] = useState<string | null>(null);
  const [quoteOpen, setQuoteOpen] = useState(false);
  const [revealed, setRevealed] = useState(false);
  // True once the shell has finished growing; the compact status (and its
  // phrase timer) stays mounted until then so it can fade out with the growth.
  const [grown, setGrown] = useState(false);
  const [hasDraft, setHasDraft] = useState(false);
  // Retracting into the screen edge; the window closes when that ends.
  const [closing, setClosing] = useState(false);
  // The shell first paints as the resting sliver and grows out of it.
  const [entering, setEntering] = useState(true);
  const [mode, setMode] = useState<AgentNotchMode>("chat");
  // The chat list replaces the chat or Recall pane while it is open.
  const [view, setView] = useState<"main" | "history">("main");
  const [chatList, setChatList] = useState<AgentChatList | null>(null);
  // Earlier turns of the one running chat (this session's turns are below).
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const historyRef = useRef<ChatMessage[]>([]);
  // The height Recall asks for (AgentRecall owns its layout numbers).
  const [recallHeight, setRecallHeight] = useState(200);
  // A Recall device menu floats below the shell; lift the click clip for it.
  const [recallMenu, setRecallMenu] = useState(false);
  const currentStartRef = useRef<HTMLDivElement>(null);
  // The Recall height the window is clipped to: `null` unclipped, `undefined`
  // not yet known (the host may still hold the compact clip). See the shape
  // effect.
  const clipRef = useRef<number | null | undefined>(undefined);
  const shellRef = useRef<HTMLElement>(null);
  historyRef.current = history;
  const revealedRef = useRef(false);
  const [capture, setCapture] = useState<AgentNotchStatus | null>(null);
  const generationRef = useRef<number | null>(null);
  const contextRef = useRef<string | null>(null);
  const instructionRef = useRef<string>("");
  const messagesRef = useRef<ChatMessage[]>([]);
  const firstReplyRef = useRef<AgentReply | null>(null);
  const hasFollowupsRef = useRef(false);
  const busyRef = useRef(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const flashTimer = useRef<number | undefined>(undefined);
  // Guards the first LLM run so the mount-take and the `agent-instruction`
  // event (whichever wins the race) only trigger it once.
  const firstRunStartedRef = useRef(false);
  const firstTurnGenerationRef = useRef<number | null>(null);
  // A chat turn sent from an earlier open is still running on the host.
  const backgroundRef = useRef(false);
  messagesRef.current = messages;
  firstReplyRef.current = firstReply;
  hasFollowupsRef.current = hasFollowups;
  busyRef.current = busy;

  /** Load the one running chat (the host records every turn). */
  const loadHistory = useCallback(async () => {
    const memory = await commands.agentHistory().catch(() => null);
    if (memory) setHistory(toChat(memory.messages));
    return memory;
  }, []);

  const reveal = useCallback(() => {
    revealedRef.current = true;
    setRevealed(true);
  }, []);

  /** The hand-opened notch is on screen: grow from the sliver to full size. */
  const open = useCallback(() => {
    setEntering(false);
    reveal();
  }, [reveal]);

  /** Run the FIRST instruction (or a retry of it) — compact stage. */
  const runFirst = useCallback(
    async (instruction: string, record = true) => {
      // The panel may already be busy while speech is being transcribed. Seed
      // visible state now: updating instructionRef or busy=true alone need not
      // trigger another render before the model finishes.
      setMessages([{ id: "u0", role: "user", content: instruction }]);
      setBusy(true);
      setError(null);
      try {
        const payload: AgentMessage[] = [
          ...toPayload(historyRef.current.slice(-HISTORY_CONTEXT)),
          { role: "user", content: instruction },
        ];
        const res = await commands.agentRun(
          payload,
          contextRef.current,
          revealedRef.current ? null : firstTurnGenerationRef.current,
          // The host records the turn itself, so it survives a close mid-run.
          // A retry asks again: only its new reply is stored.
          record,
        );
        if (res.status === "ok") {
          setFirstReply(res.data);
          if (!record) remember(null, res.data);
        } else {
          setError(res.error || t("agent.error"));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : t("agent.error"));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

  /** Run the conversation with its existing turns. */
  const runConversation = useCallback(
    async (history: ChatMessage[]) => {
      setBusy(true);
      setError(null);
      try {
        const payload: AgentMessage[] = [
          ...toPayload(historyRef.current.slice(-HISTORY_CONTEXT)),
          ...toPayload(history),
        ];
        const res = await commands.agentRun(
          payload,
          contextRef.current,
          null,
          true,
        );
        if (res.status === "ok") {
          const reply = res.data;
          setMessages((prev) => [
            // Any new turn after a pending action either resolved or cancelled
            // that host token. Keep historical text, but never leave a stale
            // approval button active in an older message.
            ...prev.map((message) =>
              message.confirmAction
                ? { ...message, confirmAction: null }
                : message,
            ),
            {
              id: rid(),
              role: "assistant",
              content: reply.text,
              confirmAction: reply.confirm_action,
            },
          ]);
        } else {
          setError(res.error || t("agent.error"));
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : t("agent.error"));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

  const retry = useCallback(() => {
    if (
      busyRef.current ||
      hasFollowupsRef.current ||
      !instructionRef.current ||
      firstReplyRef.current?.confirm_action
    )
      return;
    void runFirst(instructionRef.current, false);
  }, [runFirst]);

  /** Take the queued first instruction and run it — guarded so the mount-take
   * and the `agent-instruction` event fire it at most once. */
  const startFirstIfQueued = useCallback(async () => {
    const generation = generationRef.current;
    if (generation === null || firstRunStartedRef.current) return;
    let instruction: Awaited<ReturnType<typeof commands.agentTakeInstruction>> =
      null;
    try {
      instruction = await commands.agentTakeInstruction(generation);
    } catch {
      /* nothing queued yet */
    }
    if (instruction?.text.trim() && !firstRunStartedRef.current) {
      firstTurnGenerationRef.current = instruction.generation;
      if (instruction.is_error) {
        firstRunStartedRef.current = true;
        setError(instruction.text);
        setBusy(false);
        return;
      }
      instructionRef.current = instruction.text.trim();
      firstRunStartedRef.current = true;
      contextRef.current = await commands.agentGetContext().catch(() => null);
      await runFirst(instructionRef.current);
    }
  }, [runFirst]);

  useEffect(() => {
    let dead = false;
    const unlisten: Array<() => void> = [];
    const track = (release: () => void) => {
      if (dead) release();
      else unlisten.push(release);
    };
    let listeningGeneration: number | null = null;
    let workingGeneration: number | null = null;
    void Promise.all([
      win
        .listen<number>("agent-listening", ({ payload }) => {
          listeningGeneration = payload;
          setCapture((current) =>
            current?.generation === payload
              ? { ...current, listening: true }
              : current,
          );
        })
        .then(track),
      win
        .listen<number>("agent-working", ({ payload }) => {
          workingGeneration = payload;
          setCapture((current) =>
            current?.generation === payload
              ? { ...current, working: true }
              : current,
          );
        })
        .then(track),
      // A turn left running by an earlier open finished: show its reply.
      win
        .listen("agent-chat-done", () => {
          if (!backgroundRef.current) return;
          backgroundRef.current = false;
          void loadHistory().then(() => {
            if (!dead) setBusy(false);
          });
        })
        .then(track),
      win
        .listen("agent-instruction", () => void startFirstIfQueued())
        .then(track),
      // The host times the close itself (NOTCH_CLOSE); this only animates it.
      win.listen("agent-dismiss", () => setClosing(true)).then(track),
      win
        .listen("agent-open", () => {
          requestAnimationFrame(() => {
            if (!dead) open();
          });
        })
        .then(track),
      win
        .listen("agent-reveal", () => {
          // Give the WebView a compact paint after the native bounds change.
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              if (!dead) reveal();
            });
          });
        })
        .then(track),
      win
        .listen<string>("agent-panel-dictation", ({ payload }) => {
          const dictated = payload.trim();
          if (!dictated || !revealedRef.current) return;
          const input = composerRef.current;
          if (!input) return;
          const separator = input.value && !/\s$/.test(input.value) ? " " : "";
          input.value += separator + dictated;
          resizeComposer(input);
          setHasDraft(true);
          input.focus();
        })
        .then(track),
    ])
      .then(async () => {
        const status = await commands.agentNotchStatus();
        if (dead) return;
        if (!status.opened) {
          // Summoned: the window is already on screen as the sliver; grow now.
          requestAnimationFrame(() => {
            if (!dead) setEntering(false);
          });
        } else {
          // Hand-opened: no first turn is coming. The composer is free unless
          // a turn sent before the last close is still running.
          backgroundRef.current = status.chat_busy;
          setBusy(status.chat_busy);
        }
        // The chat continues: load it before the first turn is sent.
        const memory = await loadHistory();
        if (dead) return;
        if (memory) {
          // A summon is always a chat; a hand-opened notch keeps its mode.
          if (status.opened) setMode(memory.mode ?? "chat");
        }
        if (backgroundRef.current) {
          // The turn may have ended before this window was listening.
          const now = await commands.agentNotchStatus().catch(() => null);
          if (dead) return;
          if (now && !now.chat_busy && backgroundRef.current) {
            backgroundRef.current = false;
            await loadHistory();
            if (dead) return;
            setBusy(false);
          }
        }
        if (status.opened && status.shown) open();
        generationRef.current = status.generation;
        setCapture({
          ...status,
          listening:
            status.listening || listeningGeneration === status.generation,
          working: status.working || workingGeneration === status.generation,
        });
        await startFirstIfQueued();
      })
      .catch(console.error);
    return () => {
      dead = true;
      unlisten.forEach((release) => release());
    };
  }, [reveal, open, startFirstIfQueued, loadHistory, win]);

  // Reveal only after reply/error state has committed. The host verifies that
  // this first turn still owns the waiting pill, so Stop cannot reopen a card.
  // Opened early, the call only ends the host's wait (once).
  const presentedRef = useRef<number | null>(null);
  useEffect(() => {
    const generation = firstTurnGenerationRef.current;
    if (
      !busy &&
      generation !== null &&
      presentedRef.current !== generation &&
      (firstReply || error)
    ) {
      presentedRef.current = generation;
      void commands.agentPresentReply(generation).catch(console.error);
    }
  }, [busy, error, firstReply]);

  // Hovering the notch while it listens or works fills a bar, as the resting
  // sliver does; when it is full (or on a click) the chat opens early.
  const [hoverOpening, setHoverOpening] = useState(false);
  const canOpenEarly = !revealed && !closing && !!capture && !capture.opened;
  const openEarly = useCallback(() => {
    setHoverOpening(false);
    if (capture)
      void commands.agentOpenSummon(capture.generation).catch(console.error);
  }, [capture]);
  useEffect(() => {
    if (!hoverOpening || !canOpenEarly) return;
    const timer = window.setTimeout(openEarly, HOVER_OPEN_MS);
    return () => window.clearTimeout(timer);
  }, [hoverOpening, canOpenEarly, openEarly]);

  /** Close the reply the same way the host closes it: retract, then close. */
  const dismiss = useCallback(() => {
    setClosing(true);
    // Matches --notch-close in agent.css (and NOTCH_CLOSE in agent.rs).
    window.setTimeout(() => void win.close(), 320);
  }, [win]);

  useEffect(() => {
    if (!revealed) return;
    // Matches --notch-grow in agent.css (and NOTCH_GROW in agent.rs).
    const timer = window.setTimeout(() => setGrown(true), 460);
    return () => window.clearTimeout(timer);
  }, [revealed]);

  useEffect(() => {
    if (!revealed || mode !== "chat" || view !== "main") return;
    // The first reply reads from its request down; later turns follow the end.
    const start = !hasFollowups ? currentStartRef.current : null;
    if (start) start.scrollIntoView({ block: "start" });
    else endRef.current?.scrollIntoView({ block: "end" });
  }, [messages, history, hasFollowups, busy, revealed, mode, view]);

  // A hand-opened notch has no global Escape (it may stay open for a whole
  // meeting); its own keydown closes it. A summon's Escape is the host's.
  useEffect(() => {
    if (!revealed) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      // Tab (or Shift+Tab) flips between Recall and Chat.
      if (
        event.key === "Tab" &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.metaKey
      ) {
        event.preventDefault();
        switchModeRef.current(modeRef.current === "chat" ? "recall" : "chat");
        return;
      }
      if (event.key !== "Escape") return;
      event.preventDefault();
      // The chat list steps back to the chat first.
      if (view === "history") setView("main");
      else dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [revealed, dismiss, view]);

  // Clip the window to the Recall shell so its transparent rest never eats
  // clicks. Pixels outside the clip are not refreshed while it holds: lifting
  // or widening it shows whatever was drawn there when it was applied. So the
  // clip only ever tightens once the shell has SETTLED (painted transparent
  // outside it), and widens before the shell grows, never after.
  useEffect(() => {
    if (!revealed || closing) return;
    const clip = mode === "recall" && !recallMenu && view !== "history";
    if (!clip) {
      if (clipRef.current !== null) {
        clipRef.current = null;
        void commands.agentNotchShape(null).catch(console.error);
      }
      return;
    }
    const apply = () => {
      clipRef.current = recallHeight;
      void commands
        .agentNotchShape({ width: RECALL_WIDTH, height: recallHeight })
        .catch(console.error);
    };
    const held = clipRef.current;
    if (held !== null && (held === undefined || recallHeight >= held)) {
      // Growing within Recall (or the first clip): widen now.
      apply();
      return;
    }
    // From unclipped, or shrinking: tighten once the shell has settled.
    return whenSettled(
      shellRef.current,
      Math.min(RECALL_WIDTH, window.innerWidth),
      Math.min(recallHeight, window.innerHeight),
      apply,
    );
  }, [revealed, closing, mode, recallHeight, recallMenu, view]);

  /** Apply a change that may grow the shell. While the window is clipped to
   * Recall, lift the clip first and wait until it is gone: a growth started
   * under the old clip is cut off for a frame or two (the glitch). */
  const unclipThen = useCallback((change: () => void) => {
    if (clipRef.current === null) {
      change();
      return;
    }
    clipRef.current = null;
    void commands
      .agentNotchShape(null)
      .catch(console.error)
      .finally(() => requestAnimationFrame(change));
  }, []);

  const switchMode = (next: AgentNotchMode) =>
    unclipThen(() => {
      setView("main");
      if (next === mode) return;
      // An open device menu goes with Recall.
      setRecallMenu(false);
      setMode(next);
      void commands.agentNotchSetMode(next).catch(console.error);
      if (next === "chat")
        requestAnimationFrame(() => composerRef.current?.focus());
      else (document.activeElement as HTMLElement | null)?.blur();
    });
  // For the window key handler, which outlives renders.
  const switchModeRef = useRef(switchMode);
  switchModeRef.current = switchMode;
  const modeRef = useRef(mode);
  modeRef.current = mode;

  /** Show `earlier` as the chat, with no turn of this open on top of it. */
  const showChat = useCallback((earlier: ChatMessage[]) => {
    instructionRef.current = "";
    setHistory(earlier);
    setMessages([]);
    setFirstReply(null);
    setHasFollowups(true);
    setError(null);
  }, []);

  /** Start a new chat; earlier ones stay in the history. */
  const newChat = useCallback(() => {
    if (busyRef.current) return;
    void commands.agentHistoryNew().catch(console.error);
    showChat([]);
    setView("main");
    setMode("chat");
    void commands.agentNotchSetMode("chat").catch(console.error);
    requestAnimationFrame(() => composerRef.current?.focus());
  }, [showChat]);

  const toggleHistory = useCallback(() => {
    if (view === "history") {
      setView("main");
      return;
    }
    void commands.agentHistoryList().then(setChatList).catch(console.error);
    unclipThen(() => setView("history"));
  }, [view, unclipThen]);

  /** Continue an earlier chat. */
  const openChat = useCallback(
    async (id: string) => {
      if (busyRef.current) return;
      if (id !== chatList?.current) {
        const res = await commands.agentHistoryOpen(id);
        if (res.status !== "ok") {
          setChatList((list) =>
            list
              ? { ...list, chats: list.chats.filter((c) => c.id !== id) }
              : list,
          );
          return;
        }
        showChat(toChat(res.data.messages));
      }
      setView("main");
      setMode("chat");
      void commands.agentNotchSetMode("chat").catch(console.error);
      requestAnimationFrame(() => composerRef.current?.focus());
    },
    [chatList, showChat],
  );

  const deleteChat = useCallback(
    async (id: string) => {
      if (busyRef.current) return;
      const res = await commands.agentHistoryDelete(id);
      if (res.status !== "ok") return;
      if (id === chatList?.current) showChat([]);
      setChatList((list) =>
        list
          ? {
              current: list.current === id ? null : list.current,
              chats: list.chats.filter((c) => c.id !== id),
            }
          : list,
      );
    },
    [chatList, showChat],
  );

  const runFollowup = useCallback(
    async (text: string) => {
      if (!text.trim() || busyRef.current) return;
      let base = messagesRef.current;
      if (!hasFollowupsRef.current) {
        const seed: ChatMessage[] = [];
        if (instructionRef.current) {
          seed.push({
            id: rid(),
            role: "user",
            content: instructionRef.current,
          });
        }
        const reply = firstReplyRef.current;
        if (reply?.text) {
          seed.push({
            id: rid(),
            role: "assistant",
            content: reply.text,
            confirmAction: reply.confirm_action,
          });
        }
        base = seed;
        setHasFollowups(true);
      }
      const next: ChatMessage[] = [
        ...base,
        { id: rid(), role: "user", content: text },
      ];
      setMessages(next);
      await runConversation(next);
    },
    [runConversation],
  );

  /** Send the composer's draft as a follow-up (Enter or the send button). */
  const sendDraft = useCallback(() => {
    const input = composerRef.current;
    const text = input?.value.trim();
    if (!input || !text || busyRef.current) return;
    input.value = "";
    resizeComposer(input);
    setHasDraft(false);
    void runFollowup(text);
  }, [runFollowup]);

  const onComposerKey = useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (
        event.key !== "Enter" ||
        event.shiftKey ||
        event.nativeEvent.isComposing
      )
        return;
      event.preventDefault();
      sendDraft();
    },
    [sendDraft],
  );

  /** Copy one specific answer (the per-reply copy affordance). Only the copied
   * reply's icon flashes — the flash is keyed by turn id, not global. */
  const copyOne = useCallback((id: string, text: string) => {
    if (!text.trim()) return;
    commands
      .agentCopy(text)
      .then((result) => {
        if (result.status !== "ok") return;
        setFlashedId(id);
        window.clearTimeout(flashTimer.current);
        flashTimer.current = window.setTimeout(() => setFlashedId(null), 1600);
      })
      .catch(() => {});
  }, []);

  useEffect(() => () => window.clearTimeout(flashTimer.current), []);

  useEffect(() => {
    if (!revealed) return;
    const focusComposer = () => {
      if (document.hasFocus() && document.activeElement === document.body) {
        composerRef.current?.focus();
      }
    };
    focusComposer();
    window.addEventListener("focus", focusComposer);
    return () => window.removeEventListener("focus", focusComposer);
  }, [revealed]);

  /** Resume exactly the host-held action shown in this reply. The model is not
   * called again and cannot substitute a provider, tool, or argument. */
  const resolveAction = useCallback(
    async (pending: AgentConfirm, approve: boolean) => {
      if (busyRef.current) return;
      setBusy(true);
      setError(null);
      try {
        const response = await commands.agentConfirmAction(
          pending.token,
          approve,
        );
        if (response.status !== "ok") {
          throw new Error(response.error || t("agent.error"));
        }
        const reply = response.data;
        remember(null, reply);
        setFirstReply((current) =>
          current?.confirm_action?.token === pending.token ? reply : current,
        );
        setMessages((current) =>
          current.map((message) =>
            message.confirmAction?.token === pending.token
              ? {
                  ...message,
                  content: reply.text,
                  confirmAction: reply.confirm_action,
                }
              : message,
          ),
        );
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : t("agent.error"));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

  const renderConfirmAction = (pending: AgentConfirm) => (
    <div className="agc-evidence agc-confirm-action">
      <span className="agc-confirm-q">{pending.summary}</span>
      <div className="agc-confirm-actions">
        <button
          type="button"
          className="agc-action-btn"
          disabled={busy}
          onClick={() => void resolveAction(pending, true)}
        >
          {t("agent.confirm")}
        </button>
        <button
          type="button"
          className="agc-cancel-btn"
          disabled={busy}
          onClick={() => void resolveAction(pending, false)}
        >
          {t("tray.cancel")}
        </button>
      </div>
    </div>
  );

  // Selected source text is separate from the submitted message below.
  const quoteText = contextRef.current?.trim() || "";
  const thread: ChatMessage[] = hasFollowups
    ? messages
    : [
        ...messages,
        ...(!busy && firstReply?.text
          ? [
              {
                id: "a0",
                role: "assistant" as const,
                content: firstReply.text,
                confirmAction: firstReply.confirm_action,
              },
            ]
          : []),
      ];
  const canRetry = !hasFollowups && !busy && !firstReply?.confirm_action;
  const listing = revealed && view === "history";
  const recall = revealed && mode === "recall" && !listing;
  const canClear = !busy && (history.length > 0 || thread.length > 0);

  return (
    <div
      className="agent-panel-root"
      onPointerEnter={() => canOpenEarly && setHoverOpening(true)}
      onPointerLeave={() => setHoverOpening(false)}
    >
      <section
        onClick={() => canOpenEarly && openEarly()}
        data-hover={canOpenEarly && hoverOpening ? "" : undefined}
        ref={shellRef}
        className={`agent-notch${entering ? " is-entering" : ""}${revealed ? " is-reply" : ""}${recall ? " is-recall" : ""}${closing ? " is-closing" : ""}`}
        data-opened={capture?.opened ? "" : undefined}
        style={
          {
            "--recall-w": `${RECALL_WIDTH}px`,
            "--recall-h": `${recallHeight}px`,
          } as React.CSSProperties
        }
        data-phase={
          revealed
            ? "reply"
            : capture?.working
              ? "working"
              : capture?.listening
                ? "listening"
                : "starting"
        }
        aria-label={t("agent.brand")}
      >
        {!revealed && (
          <AgentVoiceFade
            active={!!capture?.listening && !capture.working && !closing}
          />
        )}
        {!revealed && <span className="agent-notch-work" aria-hidden />}
        {canOpenEarly && <span className="agent-notch-fill" aria-hidden />}
        {/* One header for both states: the compact status crossfades into the
            brand while the shell grows, so nothing pops in or out. */}
        <header className="agent-notch-header">
          <span className="agent-notch-label">
            {!grown && !capture?.opened && (
              <span className="agent-notch-status" aria-hidden={revealed}>
                {capture?.working ? (
                  <AgentWorkingLabel />
                ) : (
                  <span role="status">
                    {t(
                      capture?.listening
                        ? "agent.notchListening"
                        : "agent.notchStarting",
                    )}
                  </span>
                )}
              </span>
            )}
            <span className="agent-notch-brand" aria-hidden={!revealed}>
              <span
                className="agent-notch-modes"
                role="tablist"
                aria-label={t("agent.brand")}
              >
                {(["recall", "chat"] as const).map((option) => (
                  <button
                    key={option}
                    type="button"
                    role="tab"
                    className="agent-notch-mode"
                    aria-selected={mode === option}
                    tabIndex={revealed ? 0 : -1}
                    title={t("agent.modeSwitchHint")}
                    onClick={() => switchMode(option)}
                  >
                    {t(
                      option === "chat" ? "agent.modeChat" : "agent.modeRecall",
                    )}
                  </button>
                ))}
              </span>
            </span>
          </span>
          <span className="agent-notch-spacer" />
          {revealed && (mode === "chat" || listing) && (
            <button
              type="button"
              className="agent-notch-control agent-notch-history"
              aria-pressed={listing}
              title={t("agent.history")}
              onClick={toggleHistory}
            >
              <HistoryIcon size={13} />
              <span>{t("agent.history")}</span>
            </button>
          )}
          {revealed && (mode === "chat" || listing) && (
            <button
              type="button"
              className="agent-notch-control agent-notch-close"
              title={t("agent.newChat")}
              aria-label={t("agent.newChat")}
              disabled={!canClear}
              onClick={newChat}
            >
              <SquarePen size={13} />
            </button>
          )}
          <button
            type="button"
            className="agent-notch-control agent-notch-close"
            title={revealed ? t("agent.notchClose") : t("agent.notchCancel")}
            aria-label={
              revealed ? t("agent.notchClose") : t("agent.notchCancel")
            }
            disabled={!revealed && !capture}
            onClick={(event) => {
              // Not the shell's click-to-open.
              event.stopPropagation();
              if (closing) return;
              if (revealed) dismiss();
              else if (capture)
                void commands.agentCancelNotch(capture.generation);
            }}
          >
            <X size={14} />
          </button>
        </header>

        {/* Mounted (hidden, clipped by the compact shell) as soon as the first
            reply is known, so reveal is only a class change: mounting the
            reply in the reveal frame stalled the start of the growth. */}
        {listing && (
          <div className="agent-notch-chats">
            <AgentHistory
              chats={chatList?.chats ?? null}
              current={chatList?.current ?? null}
              busy={busy}
              onOpen={(id) => void openChat(id)}
              onDelete={(id) => void deleteChat(id)}
            />
          </div>
        )}

        {recall && (
          <div className="agent-notch-recall">
            <AgentRecall
              onHeightChange={setRecallHeight}
              onMenuChange={setRecallMenu}
            />
          </div>
        )}

        {/* Stays mounted (hidden) under Recall and the chat list, so a draft
            and the scroll position survive switching. */}
        {(revealed ||
          (mode === "chat" && !busy && (!!firstReply || !!error))) && (
          <div
            className={`agent-notch-reply${listing || mode !== "chat" ? " is-away" : ""}`}
            aria-hidden={!revealed || listing || mode !== "chat"}
          >
            <div className="agc-c-scroll">
              {history.map((m) => (
                <div key={m.id} className={`agc-c-turn is-${m.role}`}>
                  {m.role === "user" ? (
                    <div className="agc-c-user">{m.content}</div>
                  ) : (
                    <>
                      <div className="agc-c-answer">
                        <AgentMarkdown markdown={m.content} />
                      </div>
                      <div className="agc-c-tools">
                        <button
                          type="button"
                          className={`agc-c-tool ${flashedId === m.id ? "is-flash" : ""}`}
                          onClick={() => copyOne(m.id, m.content)}
                          aria-label={t("agent.copyReply")}
                          title={t("agent.copyReply")}
                        >
                          {flashedId === m.id ? (
                            <Check size={13} />
                          ) : (
                            <Copy size={13} />
                          )}
                        </button>
                      </div>
                    </>
                  )}
                </div>
              ))}
              <div
                ref={currentStartRef}
                className="agc-c-mark"
                aria-hidden="true"
              />
              {quoteText && !hasFollowups && (
                <button
                  className={`agc-c-prompt ${quoteOpen ? "is-open" : ""}`}
                  onClick={() => setQuoteOpen((v) => !v)}
                  type="button"
                  aria-expanded={quoteOpen}
                >
                  {quoteText}
                </button>
              )}

              {thread.map((m) => (
                <div key={m.id} className={`agc-c-turn is-${m.role}`}>
                  {m.role === "user" ? (
                    <div className="agc-c-user">{m.content}</div>
                  ) : (
                    <>
                      <div className="agc-c-answer">
                        <AgentMarkdown markdown={m.content} />
                      </div>
                      {m.confirmAction && renderConfirmAction(m.confirmAction)}
                      {m.content.trim() && (
                        <div className="agc-c-tools">
                          <button
                            type="button"
                            className={`agc-c-tool ${flashedId === m.id ? "is-flash" : ""}`}
                            onClick={() => copyOne(m.id, m.content)}
                            aria-label={t("agent.copyReply")}
                            title={t("agent.copyReply")}
                          >
                            {flashedId === m.id ? (
                              <Check size={13} />
                            ) : (
                              <Copy size={13} />
                            )}
                          </button>
                          {canRetry && m.id === "a0" && (
                            <button
                              type="button"
                              className="agc-c-tool"
                              onClick={retry}
                              aria-label={t("agent.retry")}
                              title={t("agent.retry")}
                            >
                              <RotateCcw size={13} />
                            </button>
                          )}
                        </div>
                      )}
                    </>
                  )}
                </div>
              ))}

              {busy &&
                !thread.length &&
                capture?.listening &&
                !capture.working && (
                  <div className="agc-c-pending" role="status">
                    {t("agent.notchListening")}
                  </div>
                )}
              {busy && (
                <div className="agent-typing" aria-hidden="true">
                  <span />
                  <span />
                  <span />
                </div>
              )}
              {error && <div className="agc-error">{error}</div>}
              <div ref={endRef} />
            </div>

            <div className="agc-c-compose">
              <div className="agc-c-field">
                <textarea
                  ref={composerRef}
                  rows={1}
                  className="agc-c-textarea"
                  aria-label={t("agent.placeholderAsk")}
                  placeholder={t("agent.placeholderAsk")}
                  onInput={(event) => {
                    resizeComposer(event.currentTarget);
                    setHasDraft(!!event.currentTarget.value.trim());
                  }}
                  onKeyDown={onComposerKey}
                />
                <button
                  type="button"
                  className="agc-c-send"
                  disabled={!hasDraft || busy}
                  onClick={sendDraft}
                  aria-label={t("agent.send")}
                  title={t("agent.send")}
                >
                  <ArrowUp size={15} strokeWidth={2.25} />
                </button>
              </div>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
