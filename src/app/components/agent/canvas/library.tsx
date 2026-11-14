// The Canvas component library (generative UI experiment, see
// docs/Generative UI/PLAN.md). The model writes OpenUI Lang against these
// components; the same definitions generate the prompt that describes them, so
// what the model is told and what renders can never drift apart.
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
  Calendar,
  Car,
  Check,
  Clock,
  Cloud,
  CloudRain,
  CloudSnow,
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
  Users,
  Wind,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { AgentMarkdown } from "../../markdown/Markdown";

/** Colour accents the model picks per reply (`.tone-*` in canvas.css). */
const TONES = [
  "neutral",
  "blue",
  "sky",
  "teal",
  "green",
  "amber",
  "orange",
  "red",
  "pink",
  "purple",
  "indigo",
  "slate",
] as const;
const tone = z.enum(TONES).optional();
type Tone = (typeof TONES)[number] | undefined;
const toneClass = (value: Tone) =>
  value && value !== "neutral" ? ` tone-${value}` : "";

/** Only https images; anything else (or a broken link) renders nothing. */
const safeImage = (src: unknown): string | null =>
  typeof src === "string" && /^https:\/\/\S+$/i.test(src.trim())
    ? src.trim()
    : null;

const ICONS = {
  sun: Sun,
  moon: Moon,
  cloud: Cloud,
  rain: CloudRain,
  snow: CloudSnow,
  wind: Wind,
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
  home: Home,
  phone: Phone,
  mail: Mail,
  link: Link,
  dollar: DollarSign,
  users: Users,
  "arrow-right": ArrowRight,
} satisfies Record<string, LucideIcon>;
const iconNames = Object.keys(ICONS) as [
  keyof typeof ICONS,
  ...(keyof typeof ICONS)[],
];

const IconSchema = z.object({
  name: z.enum(iconNames),
  size: z.enum(["s", "m", "l"]).optional(),
  tone,
});
const Icon = defineComponent({
  name: "Icon",
  description: `An icon, coloured by tone. name: ${iconNames.join("|")}. size: s|m|l (default m).`,
  props: IconSchema,
  component: ({ props }) => {
    const Glyph = ICONS[props.name] ?? Info;
    const px = props.size === "l" ? 28 : props.size === "s" ? 13 : 18;
    return (
      <Glyph
        className={`gui-icon${toneClass(props.tone)}`}
        size={px}
        strokeWidth={1.8}
        aria-hidden
      />
    );
  },
});

const Heading = defineComponent({
  name: "Heading",
  description:
    "A title. size: l (the one big answer, e.g. a temperature or price) | m (default) | s (section label). tone colours it.",
  props: z.object({
    text: z.string(),
    size: z.enum(["l", "m", "s"]).optional(),
    tone,
  }),
  component: ({ props }) => (
    <div
      className={`gui-heading is-${props.size ?? "m"}${toneClass(props.tone)}`}
    >
      {props.text}
    </div>
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
    "A short block of rich prose (markdown: bold, lists, links). Prefer structured components; use this only for explanation.",
  props: z.object({ text: z.string() }),
  component: ({ props }) => (
    <div className="gui-markdown">
      <AgentMarkdown markdown={props.text} />
    </div>
  ),
});

const Badge = defineComponent({
  name: "Badge",
  description: "A small pill label, e.g. a status or category, tinted by tone.",
  props: z.object({ text: z.string(), tone }),
  component: ({ props }) => (
    <span className={`gui-badge${toneClass(props.tone)}`}>{props.text}</span>
  ),
});

const Stat = defineComponent({
  name: "Stat",
  description:
    "A labelled number. trend: up (green) | down (red) | flat shows an arrow beside detail. tone colours the value.",
  props: z.object({
    label: z.string(),
    value: z.string(),
    detail: z.string().optional(),
    trend: z.enum(["up", "down", "flat"]).optional(),
    tone,
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
      <div className={`gui-stat${toneClass(props.tone)}`}>
        <span className="gui-stat-label">{props.label}</span>
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
  description: "A labelled bar. value: 0-100. tone colours the bar.",
  props: z.object({
    label: z.string(),
    value: z.number(),
    detail: z.string().optional(),
    tone,
  }),
  component: ({ props }) => {
    const value = Math.max(0, Math.min(100, Number(props.value) || 0));
    return (
      <div className={`gui-progress${toneClass(props.tone)}`}>
        <div className="gui-progress-head">
          <span>{props.label}</span>
          <span className="gui-faint">
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
  description: "A thin horizontal rule between sections.",
  props: z.object({}),
  component: () => <hr className="gui-divider" />,
});

const Pair = defineComponent({
  name: "Pair",
  description: "One label/value line inside Details.",
  props: z.object({ label: z.string(), value: z.string() }),
  component: ({ props }) => (
    <div className="gui-pair">
      <span className="gui-faint">{props.label}</span>
      <span>{props.value}</span>
    </div>
  ),
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
    "A small table. rows are arrays of cells in column order. Keep it to a few columns.",
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

const ListItem = defineComponent({
  name: "ListItem",
  description:
    "One row in a List: title, optional subtitle, optional meta (right side, e.g. distance or price), optional icon, optional action when clicked, optional image (a real https thumbnail URL from a tool result, shown instead of the icon).",
  props: z.object({
    title: z.string(),
    subtitle: z.string().optional(),
    meta: z.string().optional(),
    icon: z.enum(iconNames).optional(),
    action: action().optional(),
    image: z.string().optional(),
  }),
  component: ({ props }) => {
    const run = useRun(props.title, props.action);
    const [broken, setBroken] = useState(false);
    const thumb = broken ? null : safeImage(props.image);
    const Glyph = props.icon && !thumb ? ICONS[props.icon] : null;
    const body = (
      <>
        {thumb && (
          <img
            className="gui-item-thumb"
            src={thumb}
            alt=""
            referrerPolicy="no-referrer"
            decoding="async"
            onError={() => setBroken(true)}
          />
        )}
        {Glyph && (
          <Glyph className="gui-icon" size={16} strokeWidth={1.8} aria-hidden />
        )}
        <span className="gui-item-main">
          <span className="gui-item-title">{props.title}</span>
          {props.subtitle && (
            <span className="gui-item-sub">{props.subtitle}</span>
          )}
        </span>
        {props.meta && <span className="gui-item-meta">{props.meta}</span>}
      </>
    );
    return props.action ? (
      <button type="button" className="gui-item is-action" {...run}>
        {body}
      </button>
    ) : (
      <div className="gui-item">{body}</div>
    );
  },
});

const List = defineComponent({
  name: "List",
  description: "A list of ListItem rows (places, options, results).",
  props: z.object({ items: z.array(ListItem.ref) }),
  component: ({ props, renderNode }) => (
    <div className="gui-list">{renderNode(props.items)}</div>
  ),
});

const Button = defineComponent({
  name: "Button",
  description:
    'A button. action: @ToAssistant("next request") asks for the next UI; @OpenUrl("https://…") opens a link. variant: primary | secondary (default). tone colours a primary button.',
  props: z.object({
    label: z.string(),
    action: action().optional(),
    variant: z.enum(["primary", "secondary"]).optional(),
    tone,
  }),
  component: ({ props }) => {
    const run = useRun(props.label, props.action);
    return (
      <button
        type="button"
        className={`gui-button is-${props.variant ?? "secondary"}${toneClass(props.tone)}`}
        {...run}
      >
        {props.label}
      </button>
    );
  },
});

const Buttons = defineComponent({
  name: "Buttons",
  description: "A row of Buttons (follow-ups go here, at the end).",
  props: z.object({ buttons: z.array(Button.ref) }),
  component: ({ props, renderNode }) => (
    <div className="gui-buttons">{renderNode(props.buttons)}</div>
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
        {props.label && <span className="gui-faint">{props.label}</span>}
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
        {props.label && <span className="gui-faint">{props.label}</span>}
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
  description:
    "A raised group of related content, e.g. one day of a forecast or one option. tone gives it a coloured gradient.",
  props: z.object({ children: z.array(z.any()), tone }),
  component: ({ props, renderNode }) => (
    <div className={`gui-card${toneClass(props.tone)}`}>
      {renderNode(props.children)}
    </div>
  ),
});

function CanvasImage({
  src,
  alt,
  caption,
  shape,
}: {
  src: string;
  alt?: string;
  caption?: string;
  shape?: "wide" | "square" | "tall";
}) {
  const [state, setState] = useState<"loading" | "ready" | "broken">("loading");
  const url = safeImage(src);
  if (!url || state === "broken") return null;
  return (
    <figure className={`gui-image is-${shape ?? "wide"} is-${state}`}>
      <img
        src={url}
        alt={alt ?? ""}
        referrerPolicy="no-referrer"
        decoding="async"
        onLoad={() => setState("ready")}
        onError={() => setState("broken")}
      />
      {caption && <figcaption>{caption}</figcaption>}
    </figure>
  );
}

const Image = defineComponent({
  name: "Image",
  description:
    "A photo. src must be a real https image URL from a tool result (never invented). shape: wide (default, 16:9) | square | tall. A broken image hides itself.",
  props: z.object({
    src: z.string(),
    alt: z.string().optional(),
    caption: z.string().optional(),
    shape: z.enum(["wide", "square", "tall"]).optional(),
  }),
  component: ({ props }) => <CanvasImage {...props} />,
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
  description:
    "The root of every reply. children are laid out top to bottom. tone washes the whole panel in a soft colour that sets the mood.",
  props: z.object({ children: z.array(z.any()), tone }),
  component: ({ props, renderNode }) => (
    <div className={`gui-canvas${toneClass(props.tone)}`}>
      {renderNode(props.children)}
    </div>
  ),
});

export const canvasLibrary = createLibrary({
  root: "Canvas",
  components: [
    Canvas,
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
  `root = Canvas([now, week, more], "sky")
now = Stack([icon, temp, Text("Light rain until 3 pm", "muted")], "row", "m", "center")
icon = Icon("rain", "l", "sky")
temp = Heading("14°C", "l")
week = Stack([d1, d2, d3], "row", "s")
d1 = Card([Text("Tue", "faint"), Icon("cloud", "m", "slate"), Text("16° / 9°")], "slate")
d2 = Card([Text("Wed", "faint"), Icon("sun", "m", "amber"), Text("19° / 10°")], "amber")
d3 = Card([Text("Thu", "faint"), Icon("rain", "m", "blue"), Text("13° / 8°")], "blue")
more = Buttons([Button("Hourly", @ToAssistant("Show the hourly forecast for today")), Button("Weekend?", @ToAssistant("What about this weekend?"))])`,
  `root = Canvas([Heading("Ski areas near you"), spots, more], "teal")
spots = List([s1, s2])
s1 = ListItem("Snowpeak", "Open · 42 runs · fresh snow", "38 km", "mountain", @OpenUrl("https://www.openstreetmap.org/search?query=Snowpeak"))
s2 = ListItem("Ridgeback", "Open · 18 runs", "55 km", "mountain", @ToAssistant("Tell me more about Ridgeback"))
more = Buttons([Button("Compare prices", @ToAssistant("Compare lift ticket prices for these"), "primary", "teal")])`,
];

let spec: string | null = null;

/** The component reference sent with each Canvas request (built once). */
export function canvasSpec(): string {
  spec ??= canvasLibrary.prompt({
    preamble:
      "You answer the user by writing a small interactive UI in OpenUI Lang for Grain's Agent notch.",
    additionalRules: [
      "The root statement is always `root = Canvas([...])`.",
      "Prefer a few strong components over many small ones; no more than about 12 statements for a simple answer.",
      "Use @OpenUrl only for real, well-known URLs or search URLs; never invent links.",
      "Follow-up buttons send a complete, standalone request with @ToAssistant.",
      "Make it look alive, never flat grey: give the Canvas a tone that sets the mood and colour the key Cards, Icons, Stats and Badges. Match the subject: weather: amber for sunny, sky for mild or clear, slate for cloud or rain, blue for cold or snow, indigo for night; money: green for gains, red for losses; nature, places and travel: teal or green; warnings: amber or red; food: orange; fun or social: pink or purple. Use one main tone plus at most two supporting ones.",
      "Use Image (or a ListItem image) whenever a real photo helps: a place, a dish, a product, a person, an event. Only use image URLs that appeared in tool results in this conversation; search the web for one when it would clearly help. Never invent or guess an image URL; without a real one, use Icons instead.",
    ],
    examples: CANVAS_EXAMPLES,
  });
  return spec;
}
