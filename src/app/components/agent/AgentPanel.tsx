import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowUp,
  ChevronLeft,
  ChevronRight,
  CloudSun,
  Code,
  Gauge,
  Languages,
  LayoutGrid,
  List,
  Mail,
  MapPin,
  MessageSquare,
  PenLine,
  Mic,
  Plane,
  Copy,
  RotateCcw,
  X,
  Check,
  Square,
  SquarePen,
  VolumeX,
  History as HistoryIcon,
} from "lucide-react";
import {
  commands,
  events,
  type Reminder,
  type AgentChatList,
  type AgentHistoryMessage,
  type AgentNotchMode,
  type AgentNotchStatus,
  type AgentMessage,
  type AgentConfirm,
  type AgentReply,
  type AgentResult,
} from "@/bindings";
import { AgentMarkdown } from "../markdown/Markdown";
import { StreamingReply } from "./StreamingReply";
import { AgentCanvas, CANVAS_MIN } from "./canvas/AgentCanvas";
import { ResultCard, resultStartSize } from "./ResultCard";
import { canvasSpec } from "./canvas/library";
import { viewSummary, type ViewKind } from "./canvas/program";
import { AgentHistory } from "./AgentHistory";
import { AgentRecall } from "./AgentRecall";
import { AgentReminder, REMINDER_WIDTH } from "./AgentReminder";
import { AgentVoiceFade } from "./AgentVoiceFade";
import { CaptureWaveform } from "../../overlay/CaptureWaveform";
import { AgentWorkingLabel } from "./AgentWorkingLabel";

type Role = "user" | "assistant";
interface ChatMessage {
  id: string;
  role: Role;
  content: string;
  // A host-held extension/MCP call. Only the token displayed here can resume it.
  confirmAction?: AgentConfirm | null;
  // A generated view the reply carries (OpenUI Lang): a card in the chat that
  // opens it full in the notch.
  ui?: string | null;
  // Text the user asked to get back, changed (shown with Copy).
  result?: AgentResult | null;
  // The reply's spoken part, shown as its own message before `content`.
  said?: string | null;
}

/** What Copy copies for a reply: its result text when it has one (an
 * email's subject has its own copy button in the card). */
const copyTextOf = (m: ChatMessage) => m.result?.body ?? m.content;

/** Messages as shown: a reply's spoken part becomes its own message, so the
 * assistant reads as having sent two (the short spoken one, then the rest). */
const withSaid = (messages: ChatMessage[]): ChatMessage[] =>
  messages.flatMap((m) =>
    m.said?.trim()
      ? [
          { id: `${m.id}:said`, role: m.role, content: m.said },
          { ...m, said: null },
        ]
      : [m],
  );

const RESULT_ICON: Record<string, typeof MapPin> = {
  email: Mail,
  message: MessageSquare,
  rewrite: PenLine,
  list: List,
  code: Code,
  translation: Languages,
};

const VIEW_ICON: Record<ViewKind, typeof MapPin> = {
  weather: CloudSun,
  places: MapPin,
  journey: Plane,
  hero: Gauge,
  view: LayoutGrid,
};

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
/** The Recall shell width, in CSS px (`.agent-notch.is-fit` in agent.css). */
const RECALL_WIDTH = 380;
/** The chat (and chat list) shell, in CSS px. The window is larger (room for
 * a generated view's max, NOTCH_REPLY in agent.rs) and clipped to the shell. */
const CHAT_SIZE = { w: 600, h: 360 };
/** How far the user may drag the chat (AGENT_CHAT_MIN/MAX in
 * grain_commands.rs, which clamps again and keeps the size). */
const CHAT_MIN = { w: 440, h: 260 };
const CHAT_MAX = { w: 720, h: 600 };
/** Resize grips: which way each edge moves the size. The shell is centred
 * under the notch, so a side edge grows it on both sides (2x the drag). */
const CHAT_GRIPS = [
  { edge: "left", x: -1, y: 0 },
  { edge: "right", x: 1, y: 0 },
  { edge: "bottom", x: 0, y: 1 },
  { edge: "bottom-left", x: -1, y: 1 },
  { edge: "bottom-right", x: 1, y: 1 },
] as const;
/** Tab (Shift+Tab) walks the views in this order. */
const MODES: AgentNotchMode[] = ["recall", "chat"];
const MODE_LABEL = {
  recall: "agent.modeRecall",
  chat: "agent.modeChat",
} as const;
/** Hover time that opens a listening or working notch (FILL in
 * grain_notch_sliver.rs). */
const HOVER_OPEN_MS = 650;

const toPayload = (messages: ChatMessage[]): AgentMessage[] =>
  messages.map((m) => ({
    role: m.role,
    content: m.content,
    ui: m.ui ?? null,
    result: m.result ?? null,
    said: m.said ?? null,
  }));

const toChat = (messages: AgentHistoryMessage[]): ChatMessage[] =>
  messages.map((message, index) => ({
    id: `h${index}`,
    role: message.role === "assistant" ? "assistant" : "user",
    content: message.content,
    ui: message.ui ?? null,
    result: message.result ?? null,
    said: message.said ?? null,
  }));

/** Store what was said in a finished turn; a reply still awaiting approval
 * is stored once it resolves. */
function remember(user: string | null, reply: AgentReply | null) {
  const turn: AgentMessage[] = [];
  if (user?.trim()) turn.push({ role: "user", content: user });
  if ((reply?.text.trim() || reply?.ui) && !reply.confirm_action)
    turn.push({
      role: "assistant",
      content: reply.text,
      ui: reply.ui,
      result: reply.result,
      said: reply.said,
    });
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
  // EXPERIMENT: the streaming reply, shown until the finished one lands.
  const [streamText, setStreamText] = useState("");
  // The streaming reply's finished spoken part, once more follows it.
  const [streamSaid, setStreamSaid] = useState("");
  useEffect(() => {
    if (busy) return;
    setStreamText("");
    setStreamSaid("");
  }, [busy]);
  const [error, setError] = useState<string | null>(null);
  const [flashedId, setFlashedId] = useState<string | null>(null);
  // Talk-back: the reply being read aloud, by speech id (null = silent).
  const [speechId, setSpeechId] = useState<number | null>(null);
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
  // The chat list or a ringing reminder replaces the chat or Recall pane
  // while it is open.
  const [view, setView] = useState<"main" | "history" | "reminder">("main");
  // Ringing reminders (oldest due first). A hand-opened notch shows each one
  // once per open; Back leaves it ringing for the next open, Done ends it.
  const [ringing, setRinging] = useState<Reminder[]>([]);
  const [handOpened, setHandOpened] = useState(false);
  const remindedRef = useRef(new Set<string>());
  // For the window key handler (Escape on a reminder counts as Done).
  const finishReminderRef = useRef<(id: string) => void>(() => {});
  const [chatList, setChatList] = useState<AgentChatList | null>(null);
  // Earlier turns of the one running chat (this session's turns are below).
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const historyRef = useRef<ChatMessage[]>([]);
  // The height Recall asks for (AgentRecall owns its layout numbers).
  const [recallHeight, setRecallHeight] = useState(200);
  // The height the reminder view asks for (AgentReminder measures itself).
  const [reminderHeight, setReminderHeight] = useState(220);
  // A Recall device menu floats below the shell; lift the click clip for it.
  const [recallMenu, setRecallMenu] = useState(false);
  // The generated view open full over the chat (a new reply's view opens on
  // its own; a card in the chat reopens an older one), the shell size it asks
  // for, and its composer's draft.
  const [shownUi, setShownUi] = useState<string | null>(null);
  const shownUiRef = useRef<string | null>(null);
  shownUiRef.current = shownUi;
  // Open a view (or, with none, go back to the chat); set once `unclipThen`
  // exists, read by the runs defined before it.
  const presentUiRef = useRef<(ui: string | null | undefined) => void>(
    () => {},
  );
  const [canvasSize, setCanvasSize] = useState(CANVAS_MIN);
  // The result card open over the chat (changed text with Copy), like a
  // view: a reply's result opens on its own, its card in the chat reopens it.
  const [shownResult, setShownResult] = useState<AgentResult | null>(null);
  const shownResultRef = useRef<AgentResult | null>(null);
  shownResultRef.current = shownResult;
  const presentResultRef = useRef<(result: AgentResult) => void>(() => {});
  const [resultSize, setResultSize] = useState({ w: 560, h: 170 });
  /** Open what a reply carries: its result card, its view, or the chat. */
  const presentReply = useCallback(
    (reply: AgentReply) =>
      reply.result
        ? presentResultRef.current(reply.result)
        : presentUiRef.current(reply.ui),
    [],
  );
  // The chat's size: the user's last drag, kept by the host. While a drag
  // runs the window is unclipped and the shell follows the pointer.
  const [chatSize, setChatSize] = useState(CHAT_SIZE);
  // Dictation into the open chat's composer (see startVoice).
  const [voice, setVoice] = useState<"off" | "listening" | "transcribing">(
    "off",
  );
  const voiceRef = useRef(voice);
  const voiceToggleRef = useRef<() => void>(() => {});
  const summonListeningRef = useRef(false);
  const voiceSendRef = useRef<() => void>(() => {});
  // Shrunk to the listening notch by Escape: what the capture state was, put
  // back when the chat reopens. `null` while open.
  const collapsedRef = useRef<Pick<
    AgentNotchStatus,
    "opened" | "listening"
  > | null>(null);
  const [resizing, setResizing] = useState(false);
  useEffect(() => {
    void commands
      .agentChatSize()
      .then((size) => setChatSize({ w: size.width, h: size.height }))
      .catch(console.error);
  }, []);
  const [canvasDraft, setCanvasDraft] = useState(false);
  const canvasComposerRef = useRef<HTMLTextAreaElement>(null);
  const currentStartRef = useRef<HTMLDivElement>(null);
  // The fitted shell (Recall or a reminder) the window is clipped to: `null`
  // unclipped, `undefined` not yet known (the host may still hold the compact
  // clip). See the shape effect.
  const clipRef = useRef<{ w: number; h: number } | null | undefined>(
    undefined,
  );
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
  // The user pressed Stop during the current run: its cancelled error is
  // shown as "Stopped", not as a failure.
  const stoppedRef = useRef(false);
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
    // Reopened after Escape shrank it to the listening notch.
    const saved = collapsedRef.current;
    if (saved) {
      collapsedRef.current = null;
      setCapture((current) => current && { ...current, ...saved });
    }
  }, []);

  /** Escape shrank the open chat to the listening notch (the host already
   * clipped it): show the compact listening state; the recording goes on. */
  const collapse = useCallback(() => {
    revealedRef.current = false;
    setRevealed(false);
    setGrown(false);
    setHoverOpening(false);
    // The host holds the compact clip now; the shape effect starts over.
    clipRef.current = undefined;
    setCapture((current) => {
      if (!current) return current;
      collapsedRef.current ??= {
        opened: current.opened,
        listening: current.listening,
      };
      // Not "opened": hover and click reopen it, as for a summon.
      return { ...current, opened: false, listening: true };
    });
  }, []);

  /** The hand-opened notch is on screen: grow from the sliver to full size. */
  const open = useCallback(() => {
    setEntering(false);
    reveal();
  }, [reveal]);

  /** A failed run's message; a run the user stopped just says so. */
  const runError = useCallback(
    (message: string | null | undefined) =>
      stoppedRef.current ? t("agent.stopped") : message || t("agent.error"),
    [t],
  );

  /** Stop the running request (the Stop button in the composer). */
  const stop = useCallback(() => {
    if (!busyRef.current) return;
    stoppedRef.current = true;
    void commands.agentStop().catch(console.error);
  }, []);

  /** Run the FIRST instruction (or a retry of it) — compact stage. */
  const runFirst = useCallback(
    async (instruction: string, record = true) => {
      // The panel may already be busy while speech is being transcribed. Seed
      // visible state now: updating instructionRef or busy=true alone need not
      // trigger another render before the model finishes.
      setMessages([{ id: "u0", role: "user", content: instruction }]);
      setBusy(true);
      setError(null);
      stoppedRef.current = false;
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
          canvasSpec(),
        );
        if (res.status === "ok") {
          setFirstReply(res.data);
          // Its view opens with it (before the reveal, so the shell grows
          // straight to the view's size).
          presentReply(res.data);
          if (!record) remember(null, res.data);
        } else {
          setError(runError(res.error));
        }
      } catch (e) {
        setError(runError(e instanceof Error ? e.message : null));
      } finally {
        setBusy(false);
      }
    },
    [runError, presentReply],
  );

  /** Run the conversation with its existing turns. */
  const runConversation = useCallback(
    async (history: ChatMessage[]) => {
      setBusy(true);
      setError(null);
      stoppedRef.current = false;
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
          canvasSpec(),
        );
        if (res.status === "ok") {
          const reply = res.data;
          // A result or a view opens; a reply without one shows in the chat.
          presentReply(reply);
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
              ui: reply.ui,
              result: reply.result,
              said: reply.said,
            },
          ]);
        } else {
          presentUiRef.current(null);
          setError(runError(res.error));
        }
      } catch (e) {
        presentUiRef.current(null);
        setError(runError(e instanceof Error ? e.message : null));
      } finally {
        setBusy(false);
      }
    },
    [runError, presentReply],
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
      firstRunStartedRef.current = true;
      contextRef.current = await commands.agentGetContext().catch(() => null);
      instructionRef.current = instruction.text.trim();
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
    let speakingGeneration: number | null = null;
    // Ids only grow, so a start that arrives after its own end is ignored.
    let lastEndedSpeech = 0;
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
      // Compact spoken reply: the notch stays small with the speaking bar.
      win
        .listen<number>("agent-speaking", ({ payload }) => {
          speakingGeneration = payload;
          setCapture((current) =>
            current?.generation === payload
              ? { ...current, working: false, speaking: true }
              : current,
          );
        })
        .then(track),
      // EXPERIMENT: the reply so far, while it streams (empty clears it).
      win
        .listen<{ said: string; text: string }>(
          "agent-stream",
          ({ payload }) => {
            setStreamSaid(payload.said);
            setStreamText(payload.text);
          },
        )
        .then(track),
      // Talk-back started or ended: the reply's Stop button follows it.
      win
        .listen<{ id: number; active: boolean }>(
          "agent-speech",
          ({ payload }) => {
            if (payload.active) {
              if (payload.id > lastEndedSpeech) setSpeechId(payload.id);
            } else {
              lastEndedSpeech = Math.max(lastEndedSpeech, payload.id);
              setSpeechId((current) =>
                current === payload.id ? null : current,
              );
            }
          },
        )
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
      // Alt+A while the chat is open: dictate into the composer.
      win
        .listen("agent-panel-voice", () => voiceToggleRef.current())
        .then(track),
      // Escape (or the host) shrank the chat to the listening notch.
      win.listen("agent-collapse", () => collapse()).then(track),
      // Enter or Alt+A on that notch: the host reopened it; send the words.
      win
        .listen("agent-panel-voice-send", () => voiceSendRef.current())
        .then(track),
      win
        .listen<string>("agent-panel-dictation", ({ payload }) => {
          const dictated = payload.trim();
          if (!dictated || !revealedRef.current) return;
          const canvas = modeRef.current === "chat" && !!shownUiRef.current;
          const input = canvas
            ? canvasComposerRef.current
            : composerRef.current;
          if (!input) return;
          const separator = input.value && !/\s$/.test(input.value) ? " " : "";
          input.value += separator + dictated;
          resizeComposer(input);
          if (canvas) setCanvasDraft(true);
          else setHasDraft(true);
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
        // A hand-opened notch keeps its mode; a summon is a chat.
        if (memory && status.opened) setMode(memory.mode ?? "chat");
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
        setHandOpened(status.opened);
        if (status.opened && status.shown) open();
        generationRef.current = status.generation;
        setCapture({
          ...status,
          listening:
            status.listening || listeningGeneration === status.generation,
          working: status.working || workingGeneration === status.generation,
          speaking: status.speaking || speakingGeneration === status.generation,
        });
        // Opened while the voice is still talking: offer its Stop button.
        if (status.voice !== null && status.voice > lastEndedSpeech)
          setSpeechId((current) => current ?? status.voice);
        await startFirstIfQueued();
      })
      .catch(console.error);
    return () => {
      dead = true;
      unlisten.forEach((release) => release());
    };
  }, [reveal, open, collapse, startFirstIfQueued, loadHistory, win]);

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
  // The blue sweep follows the voice: a streamed reply starts talking while
  // the turn still works, and the voice wins over the working look.
  const voiceSpeaking = !!capture?.speaking || (!revealed && speechId !== null);
  const openEarly = useCallback(() => {
    setHoverOpening(false);
    // Shrunk by Escape while listening: reopen the same chat.
    if (collapsedRef.current)
      void commands.agentReopenListening().catch(console.error);
    else if (capture)
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
      // Tab (or Shift+Tab) walks Recall and Chat; on a reminder it moves
      // between its controls.
      if (
        event.key === "Tab" &&
        view !== "reminder" &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.metaKey
      ) {
        event.preventDefault();
        const at = MODES.indexOf(modeRef.current);
        const step = event.shiftKey ? MODES.length - 1 : 1;
        switchModeRef.current(MODES[(at + step) % MODES.length]);
        return;
      }
      if (event.key !== "Escape") return;
      event.preventDefault();
      // Still listening: shrink to the listening notch, the voice goes on.
      if (voiceRef.current === "listening" || summonListeningRef.current) {
        void commands.agentCollapseToListening().catch(console.error);
        return;
      }
      // The chat list and an open view step back to the chat first; a
      // reminder counts as done.
      if (view === "history") setView("main");
      else if (
        view === "main" &&
        modeRef.current === "chat" &&
        (shownUiRef.current || shownResultRef.current)
      )
        presentUiRef.current(null);
      else if (view === "reminder" && ringing[0])
        finishReminderRef.current(ringing[0].id);
      else dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [revealed, dismiss, view, ringing]);

  // Clip the window to a fitted shell (Recall or a reminder) so its
  // transparent rest never eats clicks. Pixels outside the clip are not
  // refreshed while it holds: lifting or widening it shows whatever was drawn
  // there when it was applied. So the clip only ever tightens once the shell
  // has SETTLED (painted transparent outside it), and widens before the shell
  // grows, never after.
  const reminderShown = view === "reminder" && ringing.length > 0;
  // A generated view open over the chat.
  const uiShown =
    mode === "chat" && view === "main" && !reminderShown && !!shownUi;
  // A result card open over the chat (never together with a view).
  const resultShown =
    mode === "chat" && view === "main" && !reminderShown && !!shownResult;
  // Either one hides the chat behind it.
  const overChat = uiShown || resultShown;
  // The shell every view settles to: Recall, a reminder, a generated view and
  // a result card fit their content; the chat and the chat list keep a fixed
  // size. The window is larger than all but a view's max, so it is clipped to
  // the shell.
  const shell = reminderShown
    ? { w: REMINDER_WIDTH, h: reminderHeight }
    : view === "main" && mode === "recall"
      ? { w: RECALL_WIDTH, h: recallHeight }
      : uiShown
        ? canvasSize
        : resultShown
          ? resultSize
          : chatSize;
  // The chat (or the chat list) is what the user may resize.
  const chatSized = shell === chatSize;
  const shellW = shell.w;
  const shellH = shell.h;
  useEffect(() => {
    if (!revealed || closing) return;
    // A drag lifted the clip; the size it settles on is clipped after.
    if (resizing) return;
    // An open Recall device menu floats below the shell: no clip under it.
    const fit =
      mode === "recall" && recallMenu && view === "main"
        ? null
        : { w: shellW, h: shellH };
    if (!fit) {
      if (clipRef.current !== null) {
        clipRef.current = null;
        void commands.agentNotchShape(null).catch(console.error);
      }
      return;
    }
    const apply = (clip: { w: number; h: number }) => {
      clipRef.current = clip;
      void commands
        .agentNotchShape({ width: clip.w, height: clip.h })
        .catch(console.error);
    };
    const held = clipRef.current;
    if (held === undefined || (held && fit.w >= held.w && fit.h >= held.h)) {
      // Growing (or the first clip): widen now.
      apply(fit);
      return;
    }
    // Growing one way and shrinking the other: widen to cover both first.
    if (held && (fit.w > held.w || fit.h > held.h))
      apply({ w: Math.max(fit.w, held.w), h: Math.max(fit.h, held.h) });
    // From unclipped, or shrinking: tighten once the shell has settled.
    return whenSettled(
      shellRef.current,
      Math.min(fit.w, window.innerWidth),
      Math.min(fit.h, window.innerHeight),
      () => apply(fit),
    );
  }, [revealed, closing, mode, recallMenu, view, shellW, shellH, resizing]);

  /** Drag a grip: lift the clip (pixels outside it are not repainted), let
   * the shell follow the pointer, then keep the size it settles on. A
   * double-click puts the default size back. */
  const startChatResize = (
    event: React.PointerEvent<HTMLElement>,
    grip: (typeof CHAT_GRIPS)[number],
  ) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const start = { x: event.clientX, y: event.clientY, ...chatSize };
    let size = { w: chatSize.w, h: chatSize.h };
    clipRef.current = null;
    void commands.agentNotchShape(null).catch(console.error);
    setResizing(true);
    const move = (e: PointerEvent) => {
      size = {
        w: Math.round(
          Math.max(
            CHAT_MIN.w,
            Math.min(
              Math.min(CHAT_MAX.w, window.innerWidth),
              start.w + grip.x * 2 * (e.clientX - start.x),
            ),
          ),
        ),
        h: Math.round(
          Math.max(
            CHAT_MIN.h,
            Math.min(
              Math.min(CHAT_MAX.h, window.innerHeight),
              start.h + grip.y * (e.clientY - start.y),
            ),
          ),
        ),
      };
      setChatSize(size);
    };
    const end = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
      setResizing(false);
      void commands
        .setAgentChatSize(size.w, size.h)
        .then((kept) => setChatSize({ w: kept.width, h: kept.height }))
        .catch(console.error);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  };

  const resetChatSize = (event: React.MouseEvent) => {
    event.stopPropagation();
    unclipThen(() => setChatSize(CHAT_SIZE));
    void commands
      .setAgentChatSize(CHAT_SIZE.w, CHAT_SIZE.h)
      .catch(console.error);
  };

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

  /** Focus the chat's composer, or the open view's. An open result card
   * focuses its own Copy button. */
  const focusChat = useCallback(
    () =>
      requestAnimationFrame(() => {
        if (shownResultRef.current) return;
        (shownUiRef.current
          ? canvasComposerRef.current
          : composerRef.current
        )?.focus();
      }),
    [],
  );

  // Opening a view (or going back to the chat, `null`) closes a result card.
  presentUiRef.current = (ui) => {
    const next = ui ?? null;
    if (next === shownUiRef.current && !shownResultRef.current) return;
    shownUiRef.current = next;
    shownResultRef.current = null;
    const change = () => {
      setShownUi(next);
      setShownResult(null);
      if (revealedRef.current) focusChat();
    };
    // Before the reveal nothing is clipped yet: the shell grows to the view.
    if (revealedRef.current) unclipThen(change);
    else change();
  };

  presentResultRef.current = (result) => {
    if (result === shownResultRef.current) return;
    const opening = !shownResultRef.current;
    shownResultRef.current = result;
    shownUiRef.current = null;
    const change = () => {
      // A newly opened card starts at its layout's size until it measures.
      if (opening) setResultSize(resultStartSize(result));
      setShownUi(null);
      setShownResult(result);
    };
    if (revealedRef.current) unclipThen(change);
    else change();
  };

  /** Copy a result's text, with its formatted version when given (the
   * paste target picks); Copy in the card then closes the notch. */
  const copyResult = useCallback(
    (text: string, html?: string) =>
      (html
        ? commands.agentCopyFormatted(html, text)
        : commands.agentCopy(text)
      )
        .then((res) => res.status === "ok")
        .catch(() => false),
    [],
  );

  const switchMode = (next: AgentNotchMode) =>
    unclipThen(() => {
      setView("main");
      if (next === mode) return;
      // An open device menu goes with Recall.
      setRecallMenu(false);
      setMode(next);
      void commands.agentNotchSetMode(next).catch(console.error);
      if (next === "chat") focusChat();
      else (document.activeElement as HTMLElement | null)?.blur();
    });
  // For the window key handler, which outlives renders.
  const switchModeRef = useRef(switchMode);
  switchModeRef.current = switchMode;
  const modeRef = useRef(mode);
  modeRef.current = mode;

  // Ringing reminders: read on open and whenever the list changes.
  useEffect(() => {
    let dead = false;
    const load = () =>
      void commands
        .remindersList()
        .then((result) => {
          if (dead || result.status !== "ok") return;
          setRinging(
            result.data
              .filter((r) => r.status === "ringing")
              .sort(
                (a, b) =>
                  a.date.localeCompare(b.date) ||
                  (a.time ?? "").localeCompare(b.time ?? ""),
              ),
          );
        })
        .catch(console.error);
    load();
    let release: (() => void) | undefined;
    void events.remindersChanged
      .listen(load)
      .then((stop) => {
        if (dead) stop();
        else release = stop;
      })
      .catch(console.error);
    return () => {
      dead = true;
      release?.();
    };
  }, []);

  // A hand-opened notch shows any reminder it has not shown yet (a summon's
  // reply is never replaced). With none left, the view steps back.
  useEffect(() => {
    if (!revealed || closing) return;
    if (view === "reminder" && ringing.length === 0) {
      setView("main");
      return;
    }
    if (!handOpened) return;
    const fresh = ringing.filter((r) => !remindedRef.current.has(r.id));
    if (!fresh.length) return;
    fresh.forEach((r) => remindedRef.current.add(r.id));
    if (view !== "reminder") unclipThen(() => setView("reminder"));
  }, [revealed, closing, handOpened, ringing, view, unclipThen]);

  /** Leave the reminder view; the reminder stays ringing for the next open. */
  const leaveReminder = useCallback(
    () =>
      // The chat pane is larger than the fitted reminder: unclip first.
      unclipThen(() => {
        setView("main");
        if (modeRef.current === "chat") focusChat();
      }),
    [unclipThen, focusChat],
  );

  /** Take `id` off the notch; the next one shows, or the view steps back. */
  const dropReminder = useCallback(
    (id: string) => {
      const rest = ringing.filter((r) => r.id !== id);
      setRinging(rest);
      if (!rest.length) leaveReminder();
    },
    [ringing, leaveReminder],
  );

  /** Done: the reminder is finished. */
  const finishReminder = useCallback(
    (id: string) => {
      dropReminder(id);
      void commands.remindersSetDone(id, true).catch(console.error);
    },
    [dropReminder],
  );
  finishReminderRef.current = finishReminder;

  /** Snooze: rings again in `minutes`, even while this notch stays open. */
  const snoozeReminder = useCallback(
    (id: string, minutes: number) => {
      dropReminder(id);
      remindedRef.current.delete(id);
      void commands.remindersSnooze(id, minutes).catch(console.error);
    },
    [dropReminder],
  );

  /** Show `earlier` as the chat, with no turn of this open on top of it. */
  const showChat = useCallback((earlier: ChatMessage[]) => {
    instructionRef.current = "";
    shownUiRef.current = null;
    setShownUi(null);
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
        if (reply?.text || reply?.ui) {
          seed.push({
            id: rid(),
            role: "assistant",
            content: reply.text,
            confirmAction: reply.confirm_action,
            ui: reply.ui,
            result: reply.result,
            said: reply.said,
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

  /** Start dictating into the composer (its mic, or Alt+A while open). */
  const startVoice = useCallback(async () => {
    if (voiceRef.current !== "off" || busyRef.current) return;
    // A generated view is open: dictate into the chat behind it.
    if (shownUiRef.current || shownResultRef.current)
      presentUiRef.current(null);
    voiceRef.current = "listening";
    setVoice("listening");
    setError(null);
    composerRef.current?.focus();
    const res = await commands.agentPanelVoiceStart().catch((e: unknown) => ({
      status: "error" as const,
      error: String(e),
    }));
    if (res.status === "error" && voiceRef.current === "listening") {
      voiceRef.current = "off";
      setVoice("off");
      setError(res.error);
    }
  }, []);

  /** Stop dictating. `send`: straight into the chat; otherwise the words
   * land in the composer to edit. */
  const finishVoice = useCallback(
    async (send: boolean) => {
      if (voiceRef.current !== "listening") return;
      voiceRef.current = "transcribing";
      setVoice("transcribing");
      const res = await commands
        .agentPanelVoiceStop(true)
        .catch((e: unknown) => ({
          status: "error" as const,
          error: String(e),
        }));
      voiceRef.current = "off";
      setVoice("off");
      if (res.status === "error") {
        setError(res.error);
        return;
      }
      const text = res.data?.trim();
      if (!text) return;
      const input = composerRef.current;
      const draft = input?.value.trim();
      if (send && !draft && !busyRef.current) {
        void runFollowup(text);
        return;
      }
      if (!input) return;
      input.value = draft ? `${draft} ${text}` : text;
      resizeComposer(input);
      setHasDraft(true);
      input.focus();
      if (send) sendDraft();
    },
    [runFollowup, sendDraft],
  );

  // Enter or Alt+A on the listening notch: the host reopened the chat.
  voiceSendRef.current = () => void finishVoice(true);
  // Alt+A while the chat is open: start, or finish and send.
  voiceToggleRef.current = () => {
    if (voiceRef.current === "listening") void finishVoice(true);
    else if (voiceRef.current === "off") void startVoice();
  };

  // The microphone never outlives the notch (the host also releases it).
  useEffect(
    () => () => {
      if (voiceRef.current === "listening")
        void commands.agentPanelVoiceStop(false).catch(console.error);
    },
    [],
  );

  const onComposerKey = useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (voiceRef.current !== "off") {
        // Dictating: Enter sends what was said; Escape shrinks the chat to
        // the listening notch and the voice goes on (Escape there drops it).
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          if (voiceRef.current === "listening")
            void commands.agentCollapseToListening().catch(console.error);
        } else if (event.key === "Enter" && !event.nativeEvent.isComposing) {
          event.preventDefault();
          void finishVoice(true);
        }
        return;
      }
      if (
        event.key !== "Enter" ||
        event.shiftKey ||
        event.nativeEvent.isComposing
      )
        return;
      event.preventDefault();
      // Opened while the summon still listens: Enter sends that recording.
      if (summonListeningRef.current) {
        void commands.agentInputSubmit().catch(console.error);
        return;
      }
      sendDraft();
    },
    [sendDraft, finishVoice],
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
        if (shownUiRef.current) canvasComposerRef.current?.focus();
        else composerRef.current?.focus();
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
                  ui: reply.ui,
                  result: reply.result,
                  said: reply.said,
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

  // Opened while the summon still listens: its recording shows as a live
  // waveform in the composer, and Enter (or send) sends it.
  const summonListening =
    voice === "off" &&
    busy &&
    !messages.length &&
    !!capture?.listening &&
    !capture.working;
  summonListeningRef.current = summonListening;
  const voiceShown = voice !== "off" || summonListening;
  // Selected source text is separate from the submitted message below.
  const quoteText = contextRef.current?.trim() || "";
  const thread: ChatMessage[] = hasFollowups
    ? messages
    : [
        ...messages,
        ...(!busy && (firstReply?.text || firstReply?.ui)
          ? [
              {
                id: "a0",
                role: "assistant" as const,
                content: firstReply.text,
                confirmAction: firstReply.confirm_action,
                ui: firstReply.ui,
                result: firstReply.result,
                said: firstReply.said,
              },
            ]
          : []),
      ];
  const canRetry = !hasFollowups && !busy && !firstReply?.confirm_action;
  // Talk-back reads the newest reply; its Stop button sits on that reply.
  const speakingId =
    speechId === null
      ? null
      : ([...thread, ...(thread.length ? [] : history)]
          .reverse()
          .find((m) => m.role === "assistant" && m.content.trim())?.id ?? null);
  const stopSpeech = useCallback(() => {
    setSpeechId(null);
    void commands.agentStopSpeech().catch(console.error);
  }, []);
  const listing = revealed && view === "history";
  const recall = revealed && mode === "recall" && view === "main";
  const reminding = revealed && reminderShown;
  // An open view is mounted only while it shows (the renderer goes with it).
  // A summon's view is mounted before the reveal, as the chat's reply is.
  const uiMounted = uiShown && (revealed || !busy);
  const resultMounted = resultShown && (revealed || !busy);
  // Recall, a reminder and a view settle into a shell fitted to their content.
  const fitted = recall || reminding || (revealed && overChat);
  const canClear = !busy && (history.length > 0 || thread.length > 0);

  /** A reply's view, as a card that opens it full. */
  const renderViewCard = (ui: string) => {
    const { kind, title } = viewSummary(ui);
    const Icon = VIEW_ICON[kind];
    return (
      <button
        type="button"
        className="agc-ui-card"
        onClick={() => presentUiRef.current(ui)}
      >
        <span className="agc-ui-card-icon" aria-hidden>
          <Icon size={17} strokeWidth={1.8} />
        </span>
        <span className="agc-ui-card-text">
          <span className="agc-ui-card-title">
            {title ?? t("agent.viewOpen")}
          </span>
          <span className="agc-ui-card-kind">
            {t(`agent.viewKind.${kind}`)}
          </span>
        </span>
        <ChevronRight className="agc-ui-card-go" size={16} aria-hidden />
      </button>
    );
  };

  /** A reply's result, as a card that reopens it with Copy. */
  const renderResultCard = (result: AgentResult) => {
    const Icon = RESULT_ICON[result.kind] ?? PenLine;
    const title =
      result.subject || result.body.split("\n").find((line) => line.trim());
    return (
      <button
        type="button"
        className="agc-ui-card"
        onClick={() => presentResultRef.current(result)}
      >
        <span className="agc-ui-card-icon" aria-hidden>
          <Icon size={17} strokeWidth={1.8} />
        </span>
        <span className="agc-ui-card-text">
          <span className="agc-ui-card-title">
            {title?.trim() || t("agent.resultOpen")}
          </span>
          <span className="agc-ui-card-kind">
            {t(`agent.resultKind.${result.kind}`, {
              defaultValue: t("agent.resultKind.rewrite"),
            })}
          </span>
        </span>
        <ChevronRight className="agc-ui-card-go" size={16} aria-hidden />
      </button>
    );
  };

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
        className={`agent-notch${entering ? " is-entering" : ""}${revealed ? " is-reply" : ""}${fitted ? " is-fit" : ""}${closing ? " is-closing" : ""}${resizing ? " is-resizing" : ""}`}
        data-opened={capture?.opened ? "" : undefined}
        style={
          {
            "--chat-w": `${chatSize.w}px`,
            "--chat-h": `${chatSize.h}px`,
            "--recall-w": `${RECALL_WIDTH}px`,
            "--recall-h": `${recallHeight}px`,
            "--fit-w": `${shellW}px`,
            "--fit-h": `${shellH}px`,
          } as React.CSSProperties
        }
        data-phase={
          revealed
            ? "reply"
            : voiceSpeaking
              ? "speaking"
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
            active={
              !!capture?.listening &&
              !capture.working &&
              !voiceSpeaking &&
              !closing
            }
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
                {capture?.working && !voiceSpeaking ? (
                  <AgentWorkingLabel />
                ) : (
                  <span role="status">
                    {t(
                      voiceSpeaking
                        ? "agent.notchSpeaking"
                        : capture?.listening
                          ? "agent.notchListening"
                          : "agent.notchStarting",
                    )}
                  </span>
                )}
              </span>
            )}
            <span className="agent-notch-brand" aria-hidden={!revealed}>
              {reminding ? (
                // Back to Chat or Recall; the reminder keeps ringing.
                <button
                  type="button"
                  className="agent-notch-back"
                  aria-label={t("agent.reminderBackTo", {
                    mode: t(MODE_LABEL[mode]),
                  })}
                  onClick={leaveReminder}
                >
                  <ChevronLeft size={14} strokeWidth={2.2} />
                  <span>{t(MODE_LABEL[mode])}</span>
                </button>
              ) : overChat && revealed ? (
                // Back from the open view to the chat it belongs to.
                <button
                  type="button"
                  className="agent-notch-back"
                  aria-label={t("agent.viewBack")}
                  onClick={() => presentUiRef.current(null)}
                >
                  <ChevronLeft size={14} strokeWidth={2.2} />
                  <span>{t("agent.modeChat")}</span>
                </button>
              ) : (
                <span
                  className="agent-notch-modes"
                  role="tablist"
                  aria-label={t("agent.brand")}
                >
                  {MODES.map((option) => (
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
                      {t(MODE_LABEL[option])}
                    </button>
                  ))}
                </span>
              )}
            </span>
          </span>
          <span className="agent-notch-spacer" />
          {reminding && ringing.length > 1 && (
            <span className="agent-notch-count">
              {t("agent.reminderCount", { index: 1, count: ringing.length })}
            </span>
          )}
          {revealed &&
            ((mode === "chat" && !reminding && !overChat) || listing) && (
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
          {revealed &&
            ((mode === "chat" && !reminding && !overChat) || listing) && (
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

        {reminding && (
          <div className="agent-notch-reminder">
            <AgentReminder
              key={ringing[0].id}
              reminder={ringing[0]}
              onDone={finishReminder}
              onSnooze={snoozeReminder}
              onHeightChange={setReminderHeight}
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

        {uiMounted && shownUi && (
          <div className="agent-notch-canvas-pane" aria-hidden={!revealed}>
            <AgentCanvas
              ui={shownUi}
              busy={busy}
              composerRef={canvasComposerRef}
              hasDraft={canvasDraft}
              onDraft={setCanvasDraft}
              onAsk={(request) => void runFollowup(request)}
              onStop={stop}
              onSize={setCanvasSize}
            />
          </div>
        )}

        {resultMounted && shownResult && (
          <div className="agent-notch-result-pane" aria-hidden={!revealed}>
            <ResultCard
              result={shownResult}
              busy={busy}
              onCopy={copyResult}
              onDone={dismiss}
              onAsk={(request) => void runFollowup(request)}
              onSize={setResultSize}
            />
          </div>
        )}

        {/* Stays mounted (hidden) under Recall, the chat list and an open
            view, so a draft and the scroll position survive switching. */}
        {(revealed ||
          (mode === "chat" && !busy && (!!firstReply || !!error))) && (
          <div
            className={`agent-notch-reply${view !== "main" || mode !== "chat" || overChat ? " is-away" : ""}`}
            aria-hidden={
              !revealed || view !== "main" || mode !== "chat" || overChat
            }
          >
            <div className="agc-c-scroll">
              {withSaid(history).map((m) => (
                <div
                  key={m.id}
                  className={`agc-c-turn is-${m.role}${speakingId === m.id ? " is-speaking" : ""}`}
                >
                  {m.role === "user" ? (
                    <div className="agc-c-user">{m.content}</div>
                  ) : (
                    <>
                      {m.content.trim() && (
                        <div className="agc-c-answer">
                          <AgentMarkdown markdown={m.content} />
                        </div>
                      )}
                      {m.ui && renderViewCard(m.ui)}
                      {m.result && renderResultCard(m.result)}
                      <div className="agc-c-tools">
                        {speakingId === m.id && (
                          <button
                            type="button"
                            className="agc-c-tool agc-c-stop-speech"
                            onClick={stopSpeech}
                            aria-label={t("agent.stopSpeaking")}
                            title={t("agent.stopSpeaking")}
                          >
                            <VolumeX size={13} />
                          </button>
                        )}
                        <button
                          type="button"
                          className={`agc-c-tool ${flashedId === m.id ? "is-flash" : ""}`}
                          onClick={() => copyOne(m.id, copyTextOf(m))}
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

              {withSaid(thread).map((m) => (
                <div
                  key={m.id}
                  className={`agc-c-turn is-${m.role}${speakingId === m.id ? " is-speaking" : ""}`}
                >
                  {m.role === "user" ? (
                    <div className="agc-c-user">{m.content}</div>
                  ) : (
                    <>
                      {m.content.trim() && (
                        <div className="agc-c-answer">
                          <AgentMarkdown markdown={m.content} />
                        </div>
                      )}
                      {m.ui && renderViewCard(m.ui)}
                      {m.result && renderResultCard(m.result)}
                      {m.confirmAction && renderConfirmAction(m.confirmAction)}
                      {m.content.trim() && (
                        <div className="agc-c-tools">
                          {speakingId === m.id && (
                            <button
                              type="button"
                              className="agc-c-tool agc-c-stop-speech"
                              onClick={stopSpeech}
                              aria-label={t("agent.stopSpeaking")}
                              title={t("agent.stopSpeaking")}
                            >
                              <VolumeX size={13} />
                            </button>
                          )}
                          <button
                            type="button"
                            className={`agc-c-tool ${flashedId === m.id ? "is-flash" : ""}`}
                            onClick={() => copyOne(m.id, copyTextOf(m))}
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

              {busy && !summonListening && streamSaid && (
                // The spoken part, done: its own message above the rest.
                <div className="agc-c-turn is-assistant">
                  <div className="agc-c-answer">
                    <AgentMarkdown markdown={streamSaid} />
                  </div>
                </div>
              )}
              {busy && !summonListening && streamText && (
                <StreamingReply
                  // The rest starts a new message: its reveal starts over.
                  key={streamSaid ? "rest" : "said"}
                  text={streamText}
                  capAtStart={!hasFollowups}
                />
              )}
              {busy && !summonListening && !streamText && (
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
              <div className={`agc-c-field${voiceShown ? " is-voice" : ""}`}>
                <button
                  type="button"
                  className={`agc-c-mic${voiceShown ? " is-on" : ""}`}
                  disabled={
                    voice === "transcribing" ||
                    summonListening ||
                    (busy && voice === "off")
                  }
                  onClick={() =>
                    voice === "listening"
                      ? void finishVoice(false)
                      : void startVoice()
                  }
                  aria-pressed={voiceShown}
                  aria-label={t("agent.voiceDictate")}
                  title={t("agent.voiceDictate")}
                >
                  <Mic size={15} strokeWidth={2} />
                </button>
                {voiceShown && (
                  <div className="agc-c-voice" role="status">
                    {voice === "transcribing" ? (
                      <span className="agc-c-voice-hint is-working">
                        {t("agent.voiceTranscribing")}
                      </span>
                    ) : (
                      <>
                        <CaptureWaveform ready session={0} />
                        <span className="agc-c-voice-hint">
                          {t("agent.voiceSendHint")}
                        </span>
                      </>
                    )}
                  </div>
                )}
                <textarea
                  ref={composerRef}
                  rows={1}
                  className="agc-c-textarea"
                  aria-label={t("agent.placeholderAsk")}
                  placeholder={voiceShown ? "" : t("agent.placeholderAsk")}
                  readOnly={voiceShown}
                  onInput={(event) => {
                    resizeComposer(event.currentTarget);
                    setHasDraft(!!event.currentTarget.value.trim());
                  }}
                  onKeyDown={onComposerKey}
                />
                {voice === "listening" || summonListening ? (
                  <button
                    type="button"
                    className="agc-c-send"
                    onClick={() =>
                      summonListening
                        ? void commands.agentInputSubmit().catch(console.error)
                        : void finishVoice(true)
                    }
                    aria-label={t("agent.send")}
                    title={t("agent.send")}
                  >
                    <ArrowUp size={15} strokeWidth={2.25} />
                  </button>
                ) : busy ? (
                  <button
                    type="button"
                    className="agc-c-send agc-c-stop"
                    onClick={stop}
                    aria-label={t("agent.stop")}
                    title={t("agent.stop")}
                  >
                    <Square size={10} fill="currentColor" strokeWidth={0} />
                  </button>
                ) : (
                  <button
                    type="button"
                    className="agc-c-send"
                    disabled={!hasDraft}
                    onClick={sendDraft}
                    aria-label={t("agent.send")}
                    title={t("agent.send")}
                  >
                    <ArrowUp size={15} strokeWidth={2.25} />
                  </button>
                )}
              </div>
            </div>
          </div>
        )}
        {revealed &&
          !closing &&
          chatSized &&
          CHAT_GRIPS.map((grip) => (
            <span
              key={grip.edge}
              className={`agent-chat-grip is-${grip.edge}`}
              aria-hidden
              onPointerDown={(event) => startChatResize(event, grip)}
              onClick={(event) => event.stopPropagation()}
              onDoubleClick={resetChatSize}
            />
          ))}
      </section>
    </div>
  );
}
