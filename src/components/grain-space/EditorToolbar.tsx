import { useTranslation } from "react-i18next";
import {
  Bold,
  Code,
  Heading1,
  Heading2,
  Heading3,
  Highlighter,
  Italic,
  Link as LinkIcon,
  List,
  ListChecks,
  ListOrdered,
  Minus,
  Quote,
  SquareCode,
  Strikethrough,
  Table as TableIcon,
} from "lucide-react";
import type { EditorHandle } from "./MarkdownEditor";

/**
 * [GRAIN] Formatting toolbar for the note editor. A thin, always-visible strip
 * of grouped actions that drive the CodeMirror editor through its imperative
 * `EditorHandle` — no extra editor state, so it costs nothing at idle. Hidden
 * entirely for read-only (foreign vault) notes.
 */

type Props = {
  editor: React.RefObject<EditorHandle | null>;
};

export function EditorToolbar({ editor }: Props) {
  const { t } = useTranslation();
  const run = (fn: (h: EditorHandle) => void) => () => {
    if (editor.current) fn(editor.current);
  };

  const btn = (
    key: string,
    label: string,
    icon: React.ReactNode,
    onClick: () => void,
  ) => (
    <button
      key={key}
      type="button"
      className="gs-fmt-btn"
      title={label}
      aria-label={label}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      {icon}
    </button>
  );

  const sep = (key: string) => <span key={key} className="gs-fmt-sep" />;

  const sz = { width: 15, height: 15 };

  return (
    <div
      className="gs-fmt"
      role="toolbar"
      aria-label={t("grainSpaceOverlay.fmtBar")}
    >
      {btn(
        "h1",
        t("grainSpaceOverlay.fmtH1"),
        <Heading1 {...sz} />,
        run((h) => h.heading(1)),
      )}
      {btn(
        "h2",
        t("grainSpaceOverlay.fmtH2"),
        <Heading2 {...sz} />,
        run((h) => h.heading(2)),
      )}
      {btn(
        "h3",
        t("grainSpaceOverlay.fmtH3"),
        <Heading3 {...sz} />,
        run((h) => h.heading(3)),
      )}
      {sep("s1")}
      {btn(
        "b",
        t("grainSpaceOverlay.fmtBold"),
        <Bold {...sz} />,
        run((h) => h.bold()),
      )}
      {btn(
        "i",
        t("grainSpaceOverlay.fmtItalic"),
        <Italic {...sz} />,
        run((h) => h.italic()),
      )}
      {btn(
        "s",
        t("grainSpaceOverlay.fmtStrike"),
        <Strikethrough {...sz} />,
        run((h) => h.strikethrough()),
      )}
      {btn(
        "hl",
        t("grainSpaceOverlay.fmtHighlight"),
        <Highlighter {...sz} />,
        run((h) => h.highlight()),
      )}
      {btn(
        "c",
        t("grainSpaceOverlay.fmtCode"),
        <Code {...sz} />,
        run((h) => h.code()),
      )}
      {sep("s2")}
      {btn(
        "ul",
        t("grainSpaceOverlay.fmtBullet"),
        <List {...sz} />,
        run((h) => h.bullet()),
      )}
      {btn(
        "ol",
        t("grainSpaceOverlay.fmtOrdered"),
        <ListOrdered {...sz} />,
        run((h) => h.ordered()),
      )}
      {btn(
        "task",
        t("grainSpaceOverlay.fmtTask"),
        <ListChecks {...sz} />,
        run((h) => h.task()),
      )}
      {btn(
        "quote",
        t("grainSpaceOverlay.fmtQuote"),
        <Quote {...sz} />,
        run((h) => h.quote()),
      )}
      {sep("s3")}
      {btn(
        "link",
        t("grainSpaceOverlay.fmtLink"),
        <LinkIcon {...sz} />,
        run((h) => h.link()),
      )}
      {btn(
        "table",
        t("grainSpaceOverlay.fmtTable"),
        <TableIcon {...sz} />,
        run((h) => h.table()),
      )}
      {btn(
        "codeblock",
        t("grainSpaceOverlay.fmtCodeBlock"),
        <SquareCode {...sz} />,
        run((h) => h.codeBlock()),
      )}
      {btn(
        "hr",
        t("grainSpaceOverlay.fmtRule"),
        <Minus {...sz} />,
        run((h) => h.rule()),
      )}
    </div>
  );
}
