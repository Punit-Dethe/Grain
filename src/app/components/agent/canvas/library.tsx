// The Canvas component library (generative UI experiment, see
// docs/Generative UI/PLAN.md). The model writes OpenUI Lang against these
// components; the same definitions generate the prompt that describes them, so
// what the model is told and what renders can never drift apart.
//
// Look: calm and premium (the approved Apple-style mockups). Designed views
// (Weather, Places, Journey, Hero) carry the design; the model only fills in
// their data. Colour is never decoration: it comes from meaning (weather,
// status, trends), mostly chosen here rather than by the model.
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
  Image as ImageIcon,
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
import { createContext, useContext, useState, type ReactNode } from "react";
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
  image: ImageIcon,
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

/** The most rows or items a block shows, so a view never needs to scroll
 * (also stated in the prompt). */
const LIMITS = {
  places: 5,
  list: 5,
  rows: 6,
  columns: 5,
  steps: 6,
  pairs: 6,
  buttons: 3,
  stats: 4,
};

/** What a wide (landscape) Canvas fits instead (also in the prompt). */
const WIDE_LIMITS = { places: 6, hours: 8 };

/** Whether the view is the wide (landscape) layout: views re-arrange for it. */
const WideLayout = createContext(false);

/** Meaning-only colour the model may pick: real status. */
const STATUS = ["good", "warn", "bad", "muted"] as const;
const status = z.enum(STATUS).optional();
/** What a number means for the user (bad air is "bad" even as it rises):
 * colours a Hero line or Stat detail. Direction is `trend`, never colour. */
const tone = z.enum(["good", "warn", "bad"]).optional();
const trend = z.enum(["up", "down", "flat"]).optional();

/** The most top-level blocks a view shows (besides its Buttons row): more
 * reads as a crowded dump, never as a designed panel. */
const MAX_BLOCKS = 4;

/** The props of an element node the parser resolved (or `{}`). */
const nodeProps = (node: unknown): Record<string, unknown> =>
  (node as { props?: Record<string, unknown> } | null)?.props ?? {};

/** How a list's rows show their leading slot. A photo row keeps an icon tile
 * when its photo is missing; with no photo in the list at all the tiles go
 * (a column of identical placeholder tiles is noise), and icons stay only,
 * bare, when they actually tell the rows apart. */
type RowLead = "photo" | "icon" | "none";
const RowLeadContext = createContext<RowLead | null>(null);

function rowLead(nodes: unknown[]): RowLead {
  const rows = nodes.map(nodeProps);
  if (rows.some((row) => safeImage(row.image))) return "photo";
  const icons = new Set(rows.map((row) => row.icon).filter(Boolean));
  return icons.size > 1 ? "icon" : "none";
}

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

/** A row's thumbnail: the photo, or (missing or broken) an icon tile of the
 * same size, so rows never shift. */
function Thumb({
  src,
  icon: fallback,
  small = false,
  tall = false,
}: {
  src: unknown;
  icon: IconName;
  small?: boolean;
  /** 2:3 portrait instead of a square. */
  tall?: boolean;
}) {
  const [broken, setBroken] = useState(false);
  const url = broken ? null : safeImage(src);
  const className = `gui-thumb${small ? " is-small" : ""}${tall ? " is-tall" : ""}`;
  return url ? (
    <img
      className={className}
      src={url}
      alt=""
      referrerPolicy="no-referrer"
      decoding="async"
      onError={() => setBroken(true)}
    />
  ) : (
    <span className={`${className} is-empty`} aria-hidden>
      <Glyph name={fallback} size={small ? 15 : 19} />
    </span>
  );
}

/** A row's leading slot, per its list's RowLead (a row outside a list
 * shows whatever it has). */
function RowLeadSlot({
  image,
  icon: rowIcon,
  fallback,
  small = false,
  tall = false,
}: {
  image: unknown;
  icon?: IconName;
  fallback: IconName;
  small?: boolean;
  tall?: boolean;
}) {
  const listLead = useContext(RowLeadContext);
  const lead =
    listLead ?? (safeImage(image) ? "photo" : rowIcon ? "icon" : "none");
  if (lead === "none") return null;
  if (lead === "icon")
    return (
      <span className="gui-row-glyph" aria-hidden>
        {rowIcon && <Glyph name={rowIcon} size={17} />}
      </span>
    );
  return (
    <Thumb src={image} icon={rowIcon ?? fallback} small={small} tall={tall} />
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
  description: `Current weather. place ("Pune"), temp ("24°"), condition (short, "Light rain until 3 pm"), icon, high/low ("29°"), hours: up to 6 Hour columns starting with "Now" (${WIDE_LIMITS.hours} in a wide Canvas, beside the current conditions), updated (optional, "Updated now").`,
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
    const wide = useContext(WideLayout);
    const hours = ((props.hours ?? []) as unknown[])
      .map((node) => (node as { props?: Record<string, string> })?.props)
      .filter((hour): hour is Record<string, string> => !!hour?.time)
      .slice(0, wide ? WIDE_LIMITS.hours : 6);
    return (
      <div className="gui-view gui-wx">
        <div className="gui-wx-now">
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
        </div>
        {hours.length > 0 && (
          <div className="gui-wx-later">
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
          </div>
        )}
      </div>
    );
  },
});

const Place = defineComponent({
  name: "Place",
  description:
    'One row of Places: name (just the name, "Snowpeak"), detail (one short line, "42 runs · 30 cm fresh"), meta (right, very short: "38 km", "₹430"), status (under meta, a real status only: "Open", "Sold out", "Delayed"; a feature such as "Recliners" belongs in detail), statusTone (good | warn | bad | muted), image (real https photo URL from a tool result), action (when tapped), icon (only when it tells rows apart; a list with no photos shows no tiles).',
  props: z.object({
    name: z.string(),
    detail: z.string().optional(),
    meta: z.string().optional(),
    status: z.string().optional(),
    statusTone: status,
    image: z.string().optional(),
    action: action().optional(),
    icon: icon.optional(),
  }),
  component: ({ props }) => {
    const run = useRun(props.name, props.action);
    const body = (
      <>
        <RowLeadSlot image={props.image} icon={props.icon} fallback="map-pin" />
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
  description: `A titled list of places, venues, shops, cinemas or results. title ("Ski areas near you"), note (right of the title, "3 open today"), places: up to ${LIMITS.places} Place rows (${WIDE_LIMITS.places}, in two columns, in a wide Canvas).`,
  props: z.object({
    title: z.string(),
    note: z.string().optional(),
    places: z.array(Place.ref),
  }),
  component: function PlacesView({ props, renderNode }) {
    const wide = useContext(WideLayout);
    const places = props.places.slice(
      0,
      wide ? WIDE_LIMITS.places : LIMITS.places,
    );
    return (
      <div className="gui-view gui-places">
        <div className="gui-view-head">
          <span className="gui-view-title">{props.title}</span>
          {props.note && <span className="gui-t3">{props.note}</span>}
        </div>
        <RowLeadContext.Provider value={rowLead(places)}>
          <div className="gui-rows">{renderNode(places)}</div>
        </RowLeadContext.Provider>
      </div>
    );
  },
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

/** Value sizes, largest first, as [class, px]. */
const STAT_SIZES = [
  ["l", 26],
  ["m", 20],
  ["s", 16],
  ["xs", 14],
] as const;
type StatSize = (typeof STAT_SIZES)[number][0];
const StatSizeContext = createContext<StatSize>("m");
/** Rough width of a figure glyph per px of font size (tabular, light). */
const GLYPH_W = 0.58;
/** Gap + hairline between stat columns (`.gui-stats-cell` in canvas.css). */
const STAT_GUTTER = 16;

function TrendArrow({ trend: dir }: { trend?: string }) {
  const Arrow =
    dir === "up"
      ? ArrowUpRight
      : dir === "down"
        ? ArrowDownRight
        : dir === "flat"
          ? ArrowRight
          : null;
  return Arrow ? <Arrow size={13} strokeWidth={2} aria-hidden /> : null;
}

const Stat = defineComponent({
  name: "Stat",
  description:
    'One figure in a Stats group or a Hero. label ("PM10"), value (short, ≤ 8 characters: "65", "₹430", "3h 15m"; a unit goes in the label, "PM10 · µg/m³"), detail (optional, a few words, "Day 14 advance"), tone (good | warn | bad: what the figure means for the user, colours the detail), trend (up | down | flat: an arrow for the direction only).',
  props: z.object({
    label: z.string(),
    value: z.string(),
    detail: z.string().optional(),
    tone,
    trend,
  }),
  component: ({ props }) => {
    const size = useContext(StatSizeContext);
    return (
      <div className="gui-stat">
        <span className="gui-stat-label gui-t3">{props.label}</span>
        <span className={`gui-stat-value is-${size}`}>{props.value}</span>
        {(props.detail || props.trend) && (
          <span className={`gui-stat-detail is-${props.tone ?? "none"}`}>
            <TrendArrow trend={props.trend} />
            <span>{props.detail}</span>
          </span>
        )}
      </div>
    );
  },
});

/** Columns and value size for a group of stats in `room` px: every value on
 * one line, all the same size (the longest decides), as large as fits, with
 * two rows of two before anything gets small. */
export function statLayout(
  values: string[],
  room: number,
): { cols: number; size: StatSize } {
  const longest = Math.max(1, ...values.map((value) => [...value].length));
  const fits = (cols: number) =>
    STAT_SIZES.find(
      ([, px]) =>
        longest * px * GLYPH_W <= room / cols - (cols > 1 ? STAT_GUTTER : 0),
    )?.[0];
  const all = Math.max(1, values.length);
  const inOne = fits(all);
  if (inOne && (inOne !== "xs" || all <= 2)) return { cols: all, size: inOne };
  if (all === 4) {
    const inTwo = fits(2);
    if (inTwo) return { cols: 2, size: inTwo };
  }
  return { cols: all, size: inOne ?? "xs" };
}

/** A stats grid laid out for `room` px (see statLayout). */
function StatsGrid({
  nodes,
  room,
  renderNode,
}: {
  nodes: unknown[];
  room: number;
  renderNode: (node: unknown) => ReactNode;
}) {
  const stats = nodes
    .filter((node) => typeof nodeProps(node).value === "string")
    .slice(0, LIMITS.stats);
  if (!stats.length) return null;
  const { cols, size } = statLayout(
    stats.map((node) => String(nodeProps(node).value)),
    room,
  );
  return (
    <StatSizeContext.Provider value={size}>
      <div
        className="gui-stats"
        style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
      >
        {stats.map((node, i) => (
          <div
            key={i}
            className={`gui-stats-cell${i % cols === 0 ? " is-first" : ""}`}
          >
            {renderNode(node)}
          </div>
        ))}
      </div>
    </StatSizeContext.Provider>
  );
}

/** Width the stats get: a tall view, a wide one, and beside a wide Hero. */
const STATS_ROOM = { tall: 440, wide: 660, besideHero: 400 };

const Stats = defineComponent({
  name: "Stats",
  description: `A row of 2-${LIMITS.stats} Stat figures of equal weight (key numbers, a mini dashboard). It picks its own columns and number size so values never wrap. Never lay Stats out yourself with a Stack.`,
  props: z.object({ stats: z.array(Stat.ref) }),
  component: function StatsView({ props, renderNode }) {
    const wide = useContext(WideLayout);
    return (
      <div className="gui-view">
        <StatsGrid
          nodes={props.stats}
          room={wide ? STATS_ROOM.wide : STATS_ROOM.tall}
          renderNode={renderNode}
        />
      </div>
    );
  },
});

const Hero = defineComponent({
  name: "Hero",
  description: `One big answer: a price, rate, score, AQI, time, count or plan. label ("Thane · US AQI"), value (short: "165", "₹83.42", "100K"), line (a few words under it, "Unhealthy", "Up 0.3% today"), tone (good | warn | bad: what the answer means for the user, colours the line; bad air is bad even when the number goes up), detail (smaller, "PM2.5 main pollutant · 1 am"), stats (optional, 2-${LIMITS.stats} Stat figures shown with it: below, or beside it in a wide Canvas), trend (up | down: an arrow for the direction only), icon (optional, right; not with stats).`,
  props: z.object({
    label: z.string(),
    value: z.string(),
    line: z.string().optional(),
    tone,
    detail: z.string().optional(),
    stats: z.array(Stat.ref).optional(),
    trend: z.enum(["up", "down"]).optional(),
    icon: icon.optional(),
  }),
  component: function HeroView({ props, renderNode }) {
    const wide = useContext(WideLayout);
    const stats = (props.stats ?? []) as unknown[];
    const withStats = stats.length > 0;
    return (
      <div className={`gui-view gui-hero${withStats ? " has-stats" : ""}`}>
        <div className="gui-hero-main">
          <div className="gui-t3 gui-hero-label">{props.label}</div>
          <div className="gui-wx-hero">
            <div>
              <div className="gui-hero-value">{props.value}</div>
              {props.line && (
                <div
                  className={`gui-hero-line${props.tone ? ` is-${props.tone}` : ""}`}
                >
                  <TrendArrow trend={props.trend} />
                  {props.line}
                </div>
              )}
              {props.detail && (
                <div className="gui-t2 gui-hero-detail">{props.detail}</div>
              )}
            </div>
            {props.icon && !withStats && (
              <Glyph name={props.icon} size={36} className="gui-wx-glyph" />
            )}
          </div>
        </div>
        {withStats && (
          <div className="gui-hero-stats">
            {!wide && <div className="gui-hairline" />}
            <StatsGrid
              nodes={stats}
              room={wide ? STATS_ROOM.besideHero : STATS_ROOM.tall}
              renderNode={renderNode}
            />
          </div>
        )}
      </div>
    );
  },
});

// ── Building blocks (for anything the views do not cover) ────────────────

const Header = defineComponent({
  name: "Header",
  description:
    'The top of a composed view (not needed above a designed view, which has its own title): title (2-5 words, "Ramayana · IMAX 3D"), subtitle (one short grey line, "Cinepolis Lake Shore · Thane West"), note (right, very short, "Tonight").',
  props: z.object({
    title: z.string(),
    subtitle: z.string().optional(),
    note: z.string().optional(),
  }),
  component: ({ props }) => (
    <div className="gui-header">
      <div className="gui-view-head">
        <span className="gui-view-title">{props.title}</span>
        {props.note && <span className="gui-t3">{props.note}</span>}
      </div>
      {props.subtitle && (
        <div className="gui-t2 gui-header-sub">{props.subtitle}</div>
      )}
    </div>
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
  description: `A compact list of up to ${LIMITS.pairs} label/value facts.`,
  props: z.object({ pairs: z.array(Pair.ref) }),
  component: ({ props, renderNode }) => (
    <div className="gui-details">
      {renderNode(props.pairs.slice(0, LIMITS.pairs))}
    </div>
  ),
});

const Table = defineComponent({
  name: "Table",
  description: `A small table for comparisons: up to ${LIMITS.columns} columns and ${LIMITS.rows} rows. rows are arrays of cells in column order; short cells.`,
  props: z.object({
    columns: z.array(z.string()),
    rows: z.array(z.array(z.union([z.string(), z.number()]))),
  }),
  component: ({ props }) => (
    <div className="gui-table-wrap">
      <table className="gui-table">
        <thead>
          <tr>
            {props.columns.slice(0, LIMITS.columns).map((column, i) => (
              <th key={i}>{column}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {props.rows.slice(0, LIMITS.rows).map((row, r) => (
            <tr key={r}>
              {row.slice(0, LIMITS.columns).map((cell, c) => (
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
  description: `A numbered sequence of up to ${LIMITS.steps} short steps.`,
  props: z.object({ items: z.array(z.string()) }),
  component: ({ props }) => (
    <ol className="gui-steps">
      {props.items.slice(0, LIMITS.steps).map((item, i) => (
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
    "One row of a List: title, subtitle, meta (right), icon, action (when tapped), image (real https image URL from a tool result), shape (square, default | tall: for portrait images). For places prefer Places.",
  props: z.object({
    title: z.string(),
    subtitle: z.string().optional(),
    meta: z.string().optional(),
    icon: icon.optional(),
    action: action().optional(),
    image: z.string().optional(),
    shape: z.enum(["square", "tall"]).optional(),
  }),
  component: ({ props }) => {
    const run = useRun(props.title, props.action);
    const body = (
      <>
        <RowLeadSlot
          image={props.image}
          icon={props.icon}
          fallback="image"
          small
          tall={props.shape === "tall"}
        />
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
  description: `Up to ${LIMITS.list} rows of ListItem, divided by hairlines. Rows show icons only when the icons differ (never the same icon on every row) and tiles only when some row has a photo.`,
  props: z.object({ items: z.array(ListItem.ref) }),
  component: ({ props, renderNode }) => {
    const items = props.items.slice(0, LIMITS.list);
    return (
      <RowLeadContext.Provider value={rowLead(items)}>
        <div className="gui-rows">{renderNode(items)}</div>
      </RowLeadContext.Provider>
    );
  },
});

const Image = defineComponent({
  name: "Image",
  description:
    "An image. src must be a real https image URL from a tool result (never invented). shape: wide (default) | square | tall (portrait, 2:3), matching the image. Several side by side in a Stack row share the width. caption optional. A broken image shows a quiet placeholder.",
  props: z.object({
    src: z.string(),
    alt: z.string().optional(),
    caption: z.string().optional(),
    shape: z.enum(["wide", "square", "tall"]).optional(),
  }),
  component: ({ props }) => {
    const [state, setState] = useState<"loading" | "ready" | "broken">(
      "loading",
    );
    const url = safeImage(props.src);
    if (!url || state === "broken")
      return (
        <figure className={`gui-image is-${props.shape ?? "wide"} is-empty`}>
          <span className="gui-image-empty" aria-hidden>
            <ImageIcon size={22} strokeWidth={1.5} />
          </span>
          {props.caption && <figcaption>{props.caption}</figcaption>}
        </figure>
      );
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
  description: `A row of 1-${LIMITS.buttons} Buttons (follow-ups), always last.`,
  props: z.object({ buttons: z.array(Button.ref) }),
  component: ({ props, renderNode }) => (
    <div className="gui-pills">
      {renderNode(props.buttons.slice(0, LIMITS.buttons))}
    </div>
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
    "Groups blocks. direction: column (default) | row: only for 2-3 Cards or Images side by side (never Stats, Heroes or loose text; use Stats or Hero stats for numbers). gap: s | m (default) | l. align: start | center | end.",
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
  description:
    'The root of every reply. children are laid out top to bottom. layout: "tall" (default) or "wide", a short landscape panel (see the rules).',
  props: z.object({
    children: z.array(z.any()),
    layout: z.enum(["tall", "wide"]).optional(),
  }),
  component: ({ props, renderNode }) => {
    const wide = props.layout === "wide";
    const children = props.children as unknown[];
    const isButtons = (node: unknown) =>
      (node as { typeName?: string } | null)?.typeName === "Buttons";
    const blocks = children
      .filter((node) => !isButtons(node))
      .slice(0, MAX_BLOCKS);
    const buttons = children.filter(isButtons).slice(-1);
    return (
      <WideLayout.Provider value={wide}>
        <div className={`gui-canvas${wide ? " is-wide" : ""}`}>
          {renderNode(blocks)}
          {renderNode(buttons)}
        </div>
      </WideLayout.Provider>
    );
  },
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
    Stats,
    Stat,
    Header,
    Stack,
    Card,
    Heading,
    Text,
    Markdown,
    Badge,
    Image,
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
  `root = Canvas([price, more], "wide")
price = Hero("ACME · Nasdaq", "$182.40", "2.1% today", "good", "At close, 4:00 pm", [Stat("Open", "$178.60"), Stat("Day high", "$183.20"), Stat("Volume", "21.2M")], "up")
more = Buttons([Button("Five-year chart", @ToAssistant("Show ACME's five-year price chart"))])`,
  `root = Canvas([air, more])
air = Hero("Thane · US AQI", "165", "Unhealthy", "bad", "PM2.5 main pollutant · keep outdoor time short", [Stat("PM2.5 · µg/m³", "61", "24× WHO limit", "bad"), Stat("PM10 · µg/m³", "65"), Stat("Ozone · µg/m³", "128")])
more = Buttons([Button("Health tips", @ToAssistant("How do I protect myself from AQI 165 in Thane?")), Button("This week", @ToAssistant("Thane air quality forecast for this week"))])`,
  `root = Canvas([head, facts, shows, more])
head = Header("Ramayana · IMAX 3D", "Cinepolis Lake Shore · Thane West", "Tonight")
facts = Details([Pair("Ticket", "From ₹430"), Pair("Runtime", "3h 15m · U · Hindi")])
shows = List([ListItem("7:30 pm", "IMAX 3D · ends 10:45 pm", "Filling fast"), ListItem("11:45 pm", "IMAX 3D · ends 3 am", "Available")])
more = Buttons([Button("Seat map", @ToAssistant("Show the seat map for Ramayana IMAX 3D at 7:30 pm at Cinepolis Lake Shore"))])`,
];

let spec: string | null = null;

/** The component reference sent with each chat request (built once). */
export function canvasSpec(): string {
  spec ??= canvasLibrary.prompt({
    preamble:
      "A view is a small, calm, premium panel written in OpenUI Lang for Grain's Agent notch. It should feel like one of Apple's own apps: quiet, precise, nothing extra.",
    additionalRules: [
      "The root statement is always `root = Canvas([...])`.",
      "Use a designed view whenever it fits: Weather for weather, Places for places, venues, cinemas, shops or any list of results, Journey for flights, trains, buses and drives, Hero for one big number or short answer (give it stats for its 2-4 key figures). Add at most a Buttons row after it. Use the building blocks only when no view fits.",
      `ONE IDEA PER VIEW: a designed view alone, or a Header plus at most two blocks (e.g. Details and a List), plus at most one Buttons row. Never stack four sections; anything past ${MAX_BLOCKS} blocks is dropped. If two things compete, show the one asked about and offer the other as a follow-up button.`,
      `The view never scrolls. Pick its shape with Canvas's layout. "tall" (default): about 440 px wide (a Table may use up to 680) and 480 px tall, for lists and anything read top to bottom. "wide": a short landscape panel about 660 px wide and 300 px tall, for content that reads across: hourly or daily strips, a Hero with its stats beside it, a Stats dashboard, or two columns of places. Places shows ${WIDE_LIMITS.places} in two columns there, Weather ${WIDE_LIMITS.hours} hours beside the current conditions. Limits (tall): Places ${LIMITS.places} rows, Weather 6 hours, List ${LIMITS.list} rows, Table ${LIMITS.rows} rows x ${LIMITS.columns} columns, Steps ${LIMITS.steps}, Details ${LIMITS.pairs} pairs, Stats ${LIMITS.stats}, Journey 3 facts. Need more? Show the top ones and add a follow-up button such as "Show more".`,
      "Layout is the components' job, not yours: numbers go in Stats or a Hero's stats (they size themselves), facts in Details, choices in a List or Places. Use Stack rows only for 2-3 Cards or Images side by side.",
      'Keep words few: titles of 2-5 words, details as short fragments joined with " · ", no sentences where a fragment will do. Figures are short (≤ 8 characters: "65", "₹430", "20K", "3h 15m"); a unit or explanation goes in the label or detail, never in the value. Never repeat what the view already shows (a Header title does not repeat a Places title).',
      "Colour is not yours to decorate with: weather icons colour themselves. tone (good, warn, bad) says what a figure means for the user (AQI 165 is bad, a price drop for a buyer is good), and trend is only the arrow's direction. A Place status is only a real status (Open, Sold out, On time, Delayed), never a feature or a recommendation.",
      "Use photos (Place/ListItem image, Image) only with image URLs that appeared in tool results in this conversation; search the web for one when it clearly helps. Never invent or guess an image URL — leave the image out instead (a place without a photo shows its icon in a tile). The example image URL is a placeholder: never copy it.",
      "Use @OpenUrl only for real, well-known URLs or search URLs; never invent links.",
      "Follow-up buttons send a complete, standalone request with @ToAssistant. Offer 1-3 at most, or none.",
    ],
    examples: CANVAS_EXAMPLES,
  });
  return spec;
}
