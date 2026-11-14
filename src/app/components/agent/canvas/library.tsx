// The Canvas component library (generative UI experiment, see
// docs/Generative UI/PLAN.md). The model writes OpenUI Lang against these
// components; the same definitions generate the prompt that describes them, so
// what the model is told and what renders can never drift apart.
//
// Look: calm and premium (the approved Apple-style mockups). Designed views
// (Weather, Places, Journey, Hero) carry the design; the model only fills in
// their data. Colour is never decoration: it comes from meaning (weather,
// status, trends), mostly chosen here rather than by the model.
import "./devtoolsOff";
import {
  action,
  createLibrary,
  defineComponent,
  useFormName,
  useIsStreaming,
  useStateField,
  useTriggerAction,
  FormNameContext,
} from "@openuidev/react-lang";
import { z } from "zod/v4";
import {
  AlertTriangle,
  ArrowDownRight,
  ArrowRight,
  ArrowUpRight,
  Bus,
  Calendar,
  Car,
  Check,
  Clock,
  Cloud,
  CloudFog,
  CloudLightning,
  CloudRain,
  CloudSnow,
  CloudSun,
  DollarSign,
  Home,
  Info,
  Link,
  Mail,
  MapPin,
  Moon,
  Mountain,
  Phone,
  Plane,
  Star,
  Sun,
  Thermometer,
  TrainFront,
  Users,
  Wind,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { AgentMarkdown } from "../../markdown/Markdown";

/** Only https images; anything else (or a broken link) renders nothing. */
const safeImage = (src: unknown): string | null =>
  typeof src === "string" && /^https:\/\/\S+$/i.test(src.trim())
    ? src.trim()
    : null;

const ICONS = {
  sun: Sun,
  "partly-cloudy": CloudSun,
  cloud: Cloud,
  rain: CloudRain,
  storm: CloudLightning,
  snow: CloudSnow,
  fog: CloudFog,
  wind: Wind,
  moon: Moon,
  thermometer: Thermometer,
  "map-pin": MapPin,
  mountain: Mountain,
  calendar: Calendar,
  clock: Clock,
  star: Star,
  check: Check,
  alert: AlertTriangle,
  info: Info,
  car: Car,
  plane: Plane,
  train: TrainFront,
  bus: Bus,
  home: Home,
  phone: Phone,
  mail: Mail,
  link: Link,
  dollar: DollarSign,
  users: Users,
  "arrow-right": ArrowRight,
} satisfies Record<string, LucideIcon>;
type IconName = keyof typeof ICONS;
const iconNames = Object.keys(ICONS) as [IconName, ...IconName[]];
const icon = z.enum(iconNames);

/** Weather glyphs colour themselves; everything else stays neutral. */
const ICON_TONE: Partial<Record<IconName, string>> = {
  sun: "amber",
  "partly-cloudy": "amber",
  rain: "blue",
  storm: "purple",
  snow: "ice",
  moon: "moon",
};

/** Meaning-only colour the model may pick: status and the odd icon. */
const STATUS = ["good", "warn", "bad", "muted"] as const;
const status = z.enum(STATUS).optional();
const ICON_TONES = [
  "neutral",
  "blue",
  "amber",
  "green",
  "red",
  "purple",
] as const;

function Glyph({
  name,
  size,
  tone,
  className = "",
}: {
  name: IconName;
  size: number;
  tone?: string;
  className?: string;
}) {
  const Svg = ICONS[name] ?? Info;
  const hue = tone && tone !== "neutral" ? tone : ICON_TONE[name];
  return (
    <Svg
      className={`gui-icon${hue ? ` hue-${hue}` : ""}${className ? ` ${className}` : ""}`}
      size={size}
      strokeWidth={1.7}
      aria-hidden
    />
  );
}

/** A remote image that hides itself when it cannot load. */
function Photo({
  src,
  className,
  alt = "",
}: {
  src: unknown;
  className: string;
  alt?: string;
}) {
  const [broken, setBroken] = useState(false);
  const url = broken ? null : safeImage(src);
  if (!url) return null;
  return (
    <img
      className={className}
      src={url}
      alt={alt}
      referrerPolicy="no-referrer"
      decoding="async"
      onError={() => setBroken(true)}
    />
  );
}

/** Runs a button or row's OpenUI action. With none, the click sends its
 * label as the next request (OpenUI's rule, stated in the prompt). */
function useRun(label: string, run: unknown) {
  const trigger = useTriggerAction();
  const form = useFormName();
  const streaming = useIsStreaming();
  return {
    disabled: streaming,
    onClick: () =>
      void trigger(label, form, run as Parameters<typeof trigger>[2]),
  };
}

// ── Designed views ──────────────────────────────────────────────────────

const Hour = defineComponent({
  name: "Hour",
  description:
    'One column of the Weather hourly strip: time ("Now", "2 pm"), icon, temp ("26°").',
  props: z.object({ time: z.string(), icon, temp: z.string() }),
  component: () => null,
});

/** The temperature line over the hourly strip, scaled to its own range. */
function TempLine({ temps }: { temps: number[] }) {
  if (temps.length < 2 || temps.some(Number.isNaN)) return null;
  const max = Math.max(...temps);
  const min = Math.min(...temps);
  const pts = temps.map((t, i) => ({
    x: (i + 0.5) * 100,
    y: 6 + ((max - t) / (max - min || 1)) * 24,
  }));
  let d = `M ${pts[0].x} ${pts[0].y}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const mx = (pts[i].x + pts[i + 1].x) / 2;
    const my = (pts[i].y + pts[i + 1].y) / 2;
    d += ` Q ${pts[i].x} ${pts[i].y} ${mx} ${my}`;
  }
  d += ` L ${pts[pts.length - 1].x} ${pts[pts.length - 1].y}`;
  return (
    <svg
      className="gui-wx-line"
      viewBox={`0 0 ${temps.length * 100} 36`}
      preserveAspectRatio="none"
      aria-hidden
    >
      <path d={d} vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

const Weather = defineComponent({
  name: "Weather",
  description:
    'Current weather. place ("Pune"), temp ("24°"), condition (short, "Light rain until 3 pm"), icon, high/low ("29°"), hours: up to 6 Hour columns starting with "Now", updated (optional, "Updated now").',
  props: z.object({
    place: z.string(),
    temp: z.string(),
    condition: z.string(),
    icon,
    high: z.string().optional(),
    low: z.string().optional(),
    hours: z.array(Hour.ref).optional(),
    updated: z.string().optional(),
  }),
  component: ({ props }) => {
    const hours = ((props.hours ?? []) as unknown[])
      .map((node) => (node as { props?: Record<string, string> })?.props)
      .filter((hour): hour is Record<string, string> => !!hour?.time)
      .slice(0, 6);
    return (
      <div className="gui-view gui-wx">
        <div className="gui-wx-top gui-t3">
          <span>{props.place}</span>
          {props.updated && <span>{props.updated}</span>}
        </div>
        <div className="gui-wx-hero">
          <div>
            <div className="gui-wx-temp">{props.temp}</div>
            <div className="gui-wx-cond">{props.condition}</div>
            {(props.high || props.low) && (
              <div className="gui-t2 gui-wx-hl">
                {props.high && <span>H {props.high}</span>}
                {props.low && <span>L {props.low}</span>}
              </div>
            )}
          </div>
          <Glyph name={props.icon} size={40} className="gui-wx-glyph" />
        </div>
        {hours.length > 0 && (
          <>
            <div className="gui-hairline" />
            <TempLine temps={hours.map((h) => parseFloat(h.temp))} />
            <div
              className="gui-wx-hours"
              style={{ gridTemplateColumns: `repeat(${hours.length}, 1fr)` }}
            >
              {hours.map((hour, i) => (
                <div key={i}>
                  <div className={i === 0 ? "gui-t2" : "gui-t3"}>
                    {hour.time}
                  </div>
                  <Glyph
                    name={
                      (hour.icon as IconName) in ICONS
                        ? (hour.icon as IconName)
                        : "cloud"
                    }
                    size={17}
                    className="gui-wx-hicon"
                  />
                  <div>{hour.temp}</div>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    );
  },
});

const Place = defineComponent({
  name: "Place",
  description:
    'One row of Places: name, detail ("42 runs · 30 cm fresh"), meta (right, "38 km"), status (under meta, "Open"), statusTone (good | warn | bad | muted), image (real https photo URL from a tool result), action (when tapped).',
  props: z.object({
    name: z.string(),
    detail: z.string().optional(),
    meta: z.string().optional(),
    status: z.string().optional(),
    statusTone: status,
    image: z.string().optional(),
    action: action().optional(),
  }),
  component: ({ props }) => {
    const run = useRun(props.name, props.action);
    const body = (
      <>
        <Photo src={props.image} className="gui-place-photo" />
        <span className="gui-place-main">
          <span className="gui-place-name">{props.name}</span>
          {props.detail && (
            <span className="gui-place-detail gui-t2">{props.detail}</span>
          )}
        </span>
        {(props.meta || props.status) && (
          <span className="gui-place-side">
            {props.meta && <span>{props.meta}</span>}
            {props.status && (
              <span className={`gui-status is-${props.statusTone ?? "muted"}`}>
                {props.status}
              </span>
            )}
          </span>
        )}
      </>
    );
    return props.action ? (
      <button type="button" className="gui-place is-action" {...run}>
        {body}
      </button>
    ) : (
      <div className="gui-place">{body}</div>
    );
  },
});

const Places = defineComponent({
  name: "Places",
  description:
    'A titled list of places, venues, shops or results with optional photos. title ("Ski areas near you"), note (right of the title, "3 open today"), places: Place rows.',
  props: z.object({
    title: z.string(),
    note: z.string().optional(),
    places: z.array(Place.ref),
  }),
  component: ({ props, renderNode }) => (
    <div className="gui-view gui-places">
      <div className="gui-view-head">
        <span className="gui-view-title">{props.title}</span>
        {props.note && <span className="gui-t3">{props.note}</span>}
      </div>
      <div className="gui-rows">{renderNode(props.places)}</div>
    </div>
  ),
});

const Pair = defineComponent({
  name: "Pair",
  description:
    'A label/value fact ("Gate", "42B"), used by Journey and Details.',
  props: z.object({ label: z.string(), value: z.string() }),
  component: ({ props }) => (
    <div className="gui-pair">
      <span className="gui-t3">{props.label}</span>
      <span>{props.value}</span>
    </div>
  ),
});

const Journey = defineComponent({
  name: "Journey",
  description:
    'A flight, train, bus or drive from A to B. code ("AI 864 · Air India"), from ("BOM"), fromTime ("14:20"), to ("DEL"), toTime ("16:35"), status ("On time"), statusTone (good | warn | bad | muted), progress (0-100, how far along; omit before departure), fromNote / toNote ("T2"), facts: up to 3 Pair facts ("Lands in", "48 min"), mode: plane (default) | train | bus | car.',
  props: z.object({
    code: z.string(),
    from: z.string(),
    fromTime: z.string(),
    to: z.string(),
    toTime: z.string(),
    status: z.string().optional(),
    statusTone: status,
    progress: z.number().optional(),
    fromNote: z.string().optional(),
    toNote: z.string().optional(),
    facts: z.array(Pair.ref).optional(),
    mode: z.enum(["plane", "train", "bus", "car"]).optional(),
  }),
  component: ({ props }) => {
    const progress =
      typeof props.progress === "number"
        ? Math.max(0, Math.min(100, props.progress))
        : null;
    const facts = ((props.facts ?? []) as unknown[])
      .map((node) => (node as { props?: Record<string, string> })?.props)
      .filter((fact): fact is Record<string, string> => !!fact?.label)
      .slice(0, 3);
    return (
      <div className="gui-view gui-journey">
        <div className="gui-wx-top">
          <span className="gui-t3">{props.code}</span>
          {props.status && (
            <span className={`gui-status is-${props.statusTone ?? "muted"}`}>
              {props.status}
            </span>
          )}
        </div>
        <div className="gui-journey-route">
          <div>
            <div className="gui-journey-code">{props.from}</div>
            <div className="gui-t2 gui-journey-time">
              {props.fromTime}
              {props.fromNote && ` · ${props.fromNote}`}
            </div>
          </div>
          <div className="gui-journey-track">
            {progress !== null && (
              <div
                className="gui-journey-done"
                style={{ width: `${progress}%` }}
              />
            )}
            <span
              className={`gui-journey-mode is-${props.mode ?? "plane"}`}
              style={{ left: `${progress ?? 50}%` }}
            >
              <Glyph name={props.mode ?? "plane"} size={16} />
            </span>
          </div>
          <div className="gui-journey-end">
            <div className="gui-journey-code">{props.to}</div>
            <div className="gui-t2 gui-journey-time">
              {props.toTime}
              {props.toNote && ` · ${props.toNote}`}
            </div>
          </div>
        </div>
        {facts.length > 0 && (
          <>
            <div className="gui-hairline" />
            <div
              className="gui-facts"
              style={{ gridTemplateColumns: `repeat(${facts.length}, 1fr)` }}
            >
              {facts.map((fact, i) => (
                <div key={i}>
                  <div className="gui-t3">{fact.label}</div>
                  <div className="gui-fact-value">{fact.value}</div>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    );
  },
});

const Hero = defineComponent({
  name: "Hero",
  description:
    'One big answer: a price, rate, score, time, count or conversion. label ("USD to INR"), value ("₹83.42"), line (one short sentence, "Up 0.3% today"), detail (smaller, "Updated 2 min ago"), trend (up | down colours the line green/red), icon (optional, right).',
  props: z.object({
    label: z.string(),
    value: z.string(),
    line: z.string().optional(),
    detail: z.string().optional(),
    trend: z.enum(["up", "down"]).optional(),
    icon: icon.optional(),
  }),
  component: ({ props }) => (
    <div className="gui-view gui-hero">
      <div className="gui-t3 gui-hero-label">{props.label}</div>
      <div className="gui-wx-hero">
        <div>
          <div className="gui-hero-value">{props.value}</div>
          {props.line && (
            <div
              className={`gui-hero-line${props.trend ? ` is-${props.trend}` : ""}`}
            >
              {props.trend === "up" && <ArrowUpRight size={14} aria-hidden />}
              {props.trend === "down" && (
                <ArrowDownRight size={14} aria-hidden />
              )}
              {props.line}
            </div>
          )}
          {props.detail && (
            <div className="gui-t2 gui-hero-detail">{props.detail}</div>
          )}
        </div>
        {props.icon && (
          <Glyph name={props.icon} size={36} className="gui-wx-glyph" />
        )}
      </div>
    </div>
  ),
});

// ── Building blocks (for anything the views do not cover) ────────────────

const Icon = defineComponent({
  name: "Icon",
  description: `An icon. Weather icons colour themselves. name: ${iconNames.join("|")}. size: s|m|l. tone (rare, for meaning only): ${ICON_TONES.join("|")}.`,
  props: z.object({
    name: icon,
    size: z.enum(["s", "m", "l"]).optional(),
    tone: z.enum(ICON_TONES).optional(),
  }),
  component: ({ props }) => (
    <Glyph
      name={props.name}
      size={props.size === "l" ? 28 : props.size === "s" ? 13 : 18}
      tone={props.tone}
    />
  ),
});

const Heading = defineComponent({
  name: "Heading",
  description:
    "A title. size: l (one big number) | m (default, a section title) | s (small grey label).",
  props: z.object({
    text: z.string(),
    size: z.enum(["l", "m", "s"]).optional(),
  }),
  component: ({ props }) => (
    <div className={`gui-heading is-${props.size ?? "m"}`}>{props.text}</div>
  ),
});

const Text = defineComponent({
  name: "Text",
  description: "Plain text. tone: default | muted | faint.",
  props: z.object({
    text: z.string(),
    tone: z.enum(["default", "muted", "faint"]).optional(),
  }),
  component: ({ props }) => (
    <p className={`gui-text is-${props.tone ?? "default"}`}>{props.text}</p>
  ),
});

const Markdown = defineComponent({
  name: "Markdown",
  description:
    "A short block of prose (markdown: bold, lists, links). Only for explanation the views cannot hold.",
  props: z.object({ text: z.string() }),
  component: ({ props }) => (
    <div className="gui-markdown">
      <AgentMarkdown markdown={props.text} />
    </div>
  ),
});

const Badge = defineComponent({
  name: "Badge",
  description:
    "A small status word. tone: good | warn | bad | muted (default).",
  props: z.object({ text: z.string(), tone: status }),
  component: ({ props }) => (
    <span className={`gui-status is-${props.tone ?? "muted"}`}>
      {props.text}
    </span>
  ),
});

const Stat = defineComponent({
  name: "Stat",
  description:
    "A labelled number in a row of stats. trend: up (green) | down (red) | flat.",
  props: z.object({
    label: z.string(),
    value: z.string(),
    detail: z.string().optional(),
    trend: z.enum(["up", "down", "flat"]).optional(),
  }),
  component: ({ props }) => {
    const Arrow =
      props.trend === "up"
        ? ArrowUpRight
        : props.trend === "down"
          ? ArrowDownRight
          : props.trend === "flat"
            ? ArrowRight
            : null;
    return (
      <div className="gui-stat">
        <span className="gui-t3">{props.label}</span>
        <span className="gui-stat-value">{props.value}</span>
        {(props.detail || Arrow) && (
          <span className={`gui-stat-detail is-${props.trend ?? "none"}`}>
            {Arrow && <Arrow size={12} strokeWidth={2} aria-hidden />}
            {props.detail}
          </span>
        )}
      </div>
    );
  },
});

const Progress = defineComponent({
  name: "Progress",
  description: "A labelled bar. value: 0-100.",
  props: z.object({
    label: z.string(),
    value: z.number(),
    detail: z.string().optional(),
  }),
  component: ({ props }) => {
    const value = Math.max(0, Math.min(100, Number(props.value) || 0));
    return (
      <div className="gui-progress">
        <div className="gui-progress-head">
          <span>{props.label}</span>
          <span className="gui-t3">
            {props.detail ?? `${Math.round(value)}%`}
          </span>
        </div>
        <div className="gui-progress-track">
          <div className="gui-progress-fill" style={{ width: `${value}%` }} />
        </div>
      </div>
    );
  },
});

const Divider = defineComponent({
  name: "Divider",
  description: "A hairline between sections.",
  props: z.object({}),
  component: () => <div className="gui-hairline" />,
});

const Details = defineComponent({
  name: "Details",
  description: "A compact list of label/value facts.",
  props: z.object({ pairs: z.array(Pair.ref) }),
  component: ({ props, renderNode }) => (
    <div className="gui-details">{renderNode(props.pairs)}</div>
  ),
});

const Table = defineComponent({
  name: "Table",
  description:
    "A small table for comparisons. rows are arrays of cells in column order. Few columns, short cells.",
  props: z.object({
    columns: z.array(z.string()),
    rows: z.array(z.array(z.union([z.string(), z.number()]))),
  }),
  component: ({ props }) => (
    <div className="gui-table-wrap">
      <table className="gui-table">
        <thead>
          <tr>
            {props.columns.map((column, i) => (
              <th key={i}>{column}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {props.rows.map((row, r) => (
            <tr key={r}>
              {row.map((cell, c) => (
                <td key={c}>{String(cell)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  ),
});

const Steps = defineComponent({
  name: "Steps",
  description: "A numbered sequence of short steps.",
  props: z.object({ items: z.array(z.string()) }),
  component: ({ props }) => (
    <ol className="gui-steps">
      {props.items.map((item, i) => (
        <li key={i}>
          <span className="gui-step-n">{i + 1}</span>
          <span>{item}</span>
        </li>
      ))}
    </ol>
  ),
});

const ListItem = defineComponent({
  name: "ListItem",
  description:
    "One row of a List: title, subtitle, meta (right), icon, action (when tapped), image (real https thumbnail URL from a tool result). For places prefer Places.",
  props: z.object({
    title: z.string(),
    subtitle: z.string().optional(),
    meta: z.string().optional(),
    icon: icon.optional(),
    action: action().optional(),
    image: z.string().optional(),
  }),
  component: ({ props }) => {
    const run = useRun(props.title, props.action);
    const body = (
      <>
        <Photo src={props.image} className="gui-place-photo is-small" />
        {props.icon && !safeImage(props.image) && (
          <Glyph name={props.icon} size={17} />
        )}
        <span className="gui-place-main">
          <span className="gui-place-name">{props.title}</span>
          {props.subtitle && (
            <span className="gui-place-detail gui-t2">{props.subtitle}</span>
          )}
        </span>
        {props.meta && <span className="gui-place-side">{props.meta}</span>}
      </>
    );
    return props.action ? (
      <button type="button" className="gui-place is-action" {...run}>
        {body}
      </button>
    ) : (
      <div className="gui-place">{body}</div>
    );
  },
});

const List = defineComponent({
  name: "List",
  description: "Rows of ListItem, divided by hairlines.",
  props: z.object({ items: z.array(ListItem.ref) }),
  component: ({ props, renderNode }) => (
    <div className="gui-rows">{renderNode(props.items)}</div>
  ),
});

const Image = defineComponent({
  name: "Image",
  description:
    "A photo. src must be a real https image URL from a tool result (never invented). shape: wide (default) | square. caption optional. A broken image hides itself.",
  props: z.object({
    src: z.string(),
    alt: z.string().optional(),
    caption: z.string().optional(),
    shape: z.enum(["wide", "square"]).optional(),
  }),
  component: ({ props }) => {
    const [state, setState] = useState<"loading" | "ready" | "broken">(
      "loading",
    );
    const url = safeImage(props.src);
    if (!url || state === "broken") return null;
    return (
      <figure className={`gui-image is-${props.shape ?? "wide"} is-${state}`}>
        <img
          src={url}
          alt={props.alt ?? ""}
          referrerPolicy="no-referrer"
          decoding="async"
          onLoad={() => setState("ready")}
          onError={() => setState("broken")}
        />
        {props.caption && <figcaption>{props.caption}</figcaption>}
      </figure>
    );
  },
});

const Button = defineComponent({
  name: "Button",
  description:
    'A quiet pill button. action: @ToAssistant("next request") asks for the next view; @OpenUrl("https://…") opens a link.',
  props: z.object({
    label: z.string(),
    action: action().optional(),
  }),
  component: ({ props }) => {
    const run = useRun(props.label, props.action);
    return (
      <button type="button" className="gui-pill" {...run}>
        {props.label}
      </button>
    );
  },
});

const Buttons = defineComponent({
  name: "Buttons",
  description: "A row of 1-3 Buttons (follow-ups), always last.",
  props: z.object({ buttons: z.array(Button.ref) }),
  component: ({ props, renderNode }) => (
    <div className="gui-pills">{renderNode(props.buttons)}</div>
  ),
});

const Input = defineComponent({
  name: "Input",
  description:
    "A text field inside a Form. name is the field key sent with the form.",
  props: z.object({
    name: z.string(),
    label: z.string().optional(),
    placeholder: z.string().optional(),
  }),
  component: ({ props }) => {
    const field = useStateField<string>(props.name);
    const streaming = useIsStreaming();
    return (
      <label className="gui-field">
        {props.label && <span className="gui-t3">{props.label}</span>}
        <input
          className="gui-input"
          value={(field.value as string | undefined) ?? ""}
          placeholder={props.placeholder}
          disabled={streaming}
          onChange={(event) => field.setValue(event.target.value)}
        />
      </label>
    );
  },
});

const Select = defineComponent({
  name: "Select",
  description: "A choice inside a Form. options are the visible choices.",
  props: z.object({
    name: z.string(),
    options: z.array(z.string()),
    label: z.string().optional(),
  }),
  component: ({ props }) => {
    const field = useStateField<string>(props.name);
    const streaming = useIsStreaming();
    return (
      <label className="gui-field">
        {props.label && <span className="gui-t3">{props.label}</span>}
        <select
          className="gui-input"
          value={(field.value as string | undefined) ?? ""}
          disabled={streaming}
          onChange={(event) => field.setValue(event.target.value)}
        >
          <option value="" disabled>
            …
          </option>
          {props.options.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </label>
    );
  },
});

const Form = defineComponent({
  name: "Form",
  description:
    "Collects input: fields (Input/Select) then buttons. A @ToAssistant button sends the field values with its request.",
  props: z.object({
    name: z.string(),
    fields: z.array(z.union([Input.ref, Select.ref])),
    buttons: Buttons.ref,
  }),
  component: ({ props, renderNode }) => (
    <FormNameContext.Provider value={props.name}>
      <div className="gui-form" role="form">
        {renderNode(props.fields)}
        {renderNode(props.buttons)}
      </div>
    </FormNameContext.Provider>
  ),
});

const Card = defineComponent({
  name: "Card",
  description: "A quiet raised group, e.g. one option among several.",
  props: z.object({ children: z.array(z.any()) }),
  component: ({ props, renderNode }) => (
    <div className="gui-card">{renderNode(props.children)}</div>
  ),
});

const Stack = defineComponent({
  name: "Stack",
  description:
    "Lays children out. direction: column (default) | row (side by side, wraps). gap: s | m (default) | l. align: start | center | end.",
  props: z.object({
    children: z.array(z.any()),
    direction: z.enum(["column", "row"]).optional(),
    gap: z.enum(["s", "m", "l"]).optional(),
    align: z.enum(["start", "center", "end"]).optional(),
  }),
  component: ({ props, renderNode }) => (
    <div
      className={`gui-stack is-${props.direction ?? "column"} gap-${props.gap ?? "m"} align-${props.align ?? "stretch"}`}
    >
      {renderNode(props.children)}
    </div>
  ),
});

const Canvas = defineComponent({
  name: "Canvas",
  description: "The root of every reply. children are laid out top to bottom.",
  props: z.object({ children: z.array(z.any()) }),
  component: ({ props, renderNode }) => (
    <div className="gui-canvas">{renderNode(props.children)}</div>
  ),
});

export const canvasLibrary = createLibrary({
  root: "Canvas",
  components: [
    Canvas,
    Weather,
    Hour,
    Places,
    Place,
    Journey,
    Hero,
    Stack,
    Card,
    Heading,
    Text,
    Markdown,
    Badge,
    Icon,
    Image,
    Stat,
    Progress,
    Divider,
    Details,
    Pair,
    Table,
    Steps,
    List,
    ListItem,
    Buttons,
    Button,
    Form,
    Input,
    Select,
  ],
});

export const CANVAS_EXAMPLES = [
  `root = Canvas([wx, more])
wx = Weather("Pune", "24°", "Light rain until 3 pm", "rain", "29°", "18°", hours, "Updated now")
hours = [Hour("Now", "rain", "24°"), Hour("2 pm", "rain", "26°"), Hour("4 pm", "cloud", "28°"), Hour("6 pm", "sun", "27°"), Hour("8 pm", "moon", "23°"), Hour("10 pm", "moon", "21°")]
more = Buttons([Button("This weekend", @ToAssistant("What's the weather this weekend in Pune?")), Button("Next 10 days", @ToAssistant("Show the 10-day forecast for Pune"))])`,
  `root = Canvas([spots, more])
spots = Places("Ski areas near you", "2 open today", [s1, s2])
s1 = Place("Snowpeak", "42 runs · 30 cm fresh", "38 km", "Open", "good", "https://upload.wikimedia.org/snowpeak.jpg", @ToAssistant("Tell me more about Snowpeak"))
s2 = Place("Cold Hollow", "9 runs · night skiing", "71 km", "Opens 4 pm", "muted")
more = Buttons([Button("Compare lift prices", @ToAssistant("Compare lift ticket prices for Snowpeak and Cold Hollow"))])`,
  `root = Canvas([trip])
trip = Journey("AI 864 · Air India", "BOM", "14:20", "DEL", "16:35", "On time", "good", 62, "T2", "T3", [Pair("Lands in", "48 min"), Pair("Gate", "42B"), Pair("Belt", "7")])`,
];

let spec: string | null = null;

/** The component reference sent with each Canvas request (built once). */
export function canvasSpec(): string {
  spec ??= canvasLibrary.prompt({
    preamble:
      "You answer the user by filling a small, calm, premium view in OpenUI Lang for Grain's Agent notch. It should feel like one of Apple's own apps: quiet, precise, nothing extra.",
    additionalRules: [
      "The root statement is always `root = Canvas([...])`.",
      "Use a designed view whenever it fits: Weather for weather, Places for places, venues, shops or any list of results with photos, Journey for flights, trains, buses and drives, Hero for one big number or short answer. Add at most a Buttons row after it. Use the building blocks only when no view fits.",
      "Keep words few: titles of 2-5 words, details as short fragments joined with ' · ', no sentences where a fragment will do. Never repeat what the view already shows.",
      "Colour is not yours to decorate with: weather icons colour themselves; use status tones (good, warn, bad) only for real status such as Open, Closed, On time, Delayed.",
      "Use photos (Place/ListItem image, Image) only with image URLs that appeared in tool results in this conversation; search the web for one when it clearly helps. Never invent or guess an image URL — leave the image out instead. The example image URL is a placeholder: never copy it.",
      "Use @OpenUrl only for real, well-known URLs or search URLs; never invent links.",
      "Follow-up buttons send a complete, standalone request with @ToAssistant. Offer 1-3 at most, or none.",
    ],
    examples: CANVAS_EXAMPLES,
  });
  return spec;
}
