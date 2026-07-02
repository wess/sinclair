// An Obsidian-style live-preview markdown editor, built on CodeMirror 6.
//
// The document stays plain markdown; a view plugin walks the syntax tree and,
// on every line except the one holding the cursor, hides the formatting marks
// (`#`, `**`, `` ` ``, `>`) and turns `[[wiki-links]]` into clickable widgets.
// The line being edited shows its raw markdown. Text styling (bold, italic,
// heading size, code) comes from a highlight style that is always applied, so
// the rendered and raw forms look consistent. Bundled to `../dist/editor.js`.

import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
  keymap,
  placeholder,
} from "@codemirror/view";
import { EditorState, Facet, type Range } from "@codemirror/state";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";

export interface EditorHandle {
  setDoc(text: string): void;
  getDoc(): string;
  focus(): void;
  destroy(): void;
}

export interface EditorOptions {
  doc: string;
  onChange: (doc: string) => void;
  onOpenLink: (target: string) => void;
}

// Callback the wiki-link widget reads to open a note, threaded through state.
const openLinkFacet = Facet.define<(target: string) => void, (target: string) => void>({
  combine: (values) => values[0] ?? (() => {}),
});

// The marks hidden on inactive lines (their text is styled by the highlighter).
const HIDDEN_MARKS = new Set([
  "HeaderMark",
  "EmphasisMark",
  "StrikethroughMark",
  "CodeMark",
  "QuoteMark",
  "LinkMark",
]);

const WIKI_RE = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;

class WikiWidget extends WidgetType {
  constructor(
    readonly target: string,
    readonly label: string,
  ) {
    super();
  }
  eq(other: WikiWidget) {
    return other.target === this.target && other.label === this.label;
  }
  toDOM(view: EditorView) {
    const a = document.createElement("span");
    a.className = "cm-wikilink";
    a.textContent = this.label;
    a.addEventListener("mousedown", (e) => {
      e.preventDefault();
      view.state.facet(openLinkFacet)(this.target);
    });
    return a;
  }
  ignoreEvent() {
    return false;
  }
}

class BulletWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const s = document.createElement("span");
    s.className = "cm-bullet";
    s.textContent = "•";
    return s;
  }
}

// Line numbers (1-based) touched by any selection range — kept "raw".
function activeLines(state: EditorState): Set<number> {
  const lines = new Set<number>();
  for (const r of state.selection.ranges) {
    const from = state.doc.lineAt(r.from).number;
    const to = state.doc.lineAt(r.to).number;
    for (let n = from; n <= to; n++) lines.add(n);
  }
  return lines;
}

function buildDecorations(view: EditorView): DecorationSet {
  const active = activeLines(view.state);
  const decos: Range<Decoration>[] = [];
  const hide = Decoration.replace({});

  for (const { from, to } of view.visibleRanges) {
    // Heading size: a line decoration per level, always on (Obsidian keeps the
    // heading big while you edit it; only the `#` marks toggle).
    syntaxTree(view.state).iterate({
      from,
      to,
      enter: (node) => {
        const name = node.name;
        const line = view.state.doc.lineAt(node.from);
        const onActive = active.has(line.number);

        const heading = /^ATXHeading([1-6])$/.exec(name);
        if (heading) {
          decos.push(Decoration.line({ class: `cm-h${heading[1]}` }).range(line.from));
          return;
        }
        if (name === "ListMark" && !onActive) {
          const mark = view.state.doc.sliceString(node.from, node.to);
          if (mark === "-" || mark === "*" || mark === "+") {
            decos.push(Decoration.replace({ widget: new BulletWidget() }).range(node.from, node.to));
          }
          return;
        }
        if (HIDDEN_MARKS.has(name) && !onActive && node.to > node.from) {
          decos.push(hide.range(node.from, node.to));
        }
      },
    });

    // Wiki-links: replace `[[target]]` with a widget off the active line.
    const text = view.state.doc.sliceString(from, to);
    WIKI_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = WIKI_RE.exec(text))) {
      const start = from + m.index;
      const end = start + m[0].length;
      if (active.has(view.state.doc.lineAt(start).number)) continue;
      const target = m[1].trim();
      const label = (m[2] ?? m[1]).trim();
      decos.push(Decoration.replace({ widget: new WikiWidget(target, label) }).range(start, end));
    }
  }

  return Decoration.set(decos, true);
}

const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.viewportChanged) {
        this.decorations = buildDecorations(u.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

const highlight = HighlightStyle.define([
  { tag: t.heading1, fontSize: "1.7em", fontWeight: "700", lineHeight: "1.3" },
  { tag: t.heading2, fontSize: "1.45em", fontWeight: "700", lineHeight: "1.3" },
  { tag: t.heading3, fontSize: "1.25em", fontWeight: "700" },
  { tag: [t.heading4, t.heading5, t.heading6], fontWeight: "700" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.monospace, fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", color: "#c0caf5" },
  { tag: [t.link, t.url], color: "#7aa2f7" },
  { tag: t.quote, color: "#9aa5ce", fontStyle: "italic" },
  { tag: t.list, color: "#7aa2f7" },
]);

const theme = EditorView.theme(
  {
    "&": { color: "#c8c8cc", backgroundColor: "transparent", height: "100%" },
    ".cm-scroller": {
      fontFamily: "-apple-system, system-ui, sans-serif",
      fontSize: "15px",
      lineHeight: "1.7",
      padding: "8px 4px 40vh",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-content": { maxWidth: "760px", margin: "0 auto", caretColor: "#7aa2f7" },
    ".cm-cursor": { borderLeftColor: "#7aa2f7" },
    ".cm-h1": { fontSize: "1.7em", fontWeight: "700" },
    ".cm-h2": { fontSize: "1.45em", fontWeight: "700" },
    ".cm-h3": { fontSize: "1.25em", fontWeight: "700" },
    ".cm-wikilink": {
      color: "#7aa2f7",
      cursor: "pointer",
      borderBottom: "1px solid rgba(122,162,247,0.35)",
    },
    ".cm-wikilink:hover": { borderBottomColor: "#7aa2f7" },
    ".cm-bullet": { color: "#7aa2f7", paddingRight: "6px" },
    ".cm-selectionBackground, ::selection": { backgroundColor: "rgba(122,162,247,0.25)" },
    ".cm-activeLine": { backgroundColor: "rgba(255,255,255,0.03)" },
  },
  { dark: true },
);

export function createEditor(parent: HTMLElement, opts: EditorOptions): EditorHandle {
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: opts.doc,
      extensions: [
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        markdown({ base: markdownLanguage }),
        syntaxHighlighting(highlight),
        livePreview,
        theme,
        EditorView.lineWrapping,
        placeholder("Start writing…"),
        openLinkFacet.of(opts.onOpenLink),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) opts.onChange(u.state.doc.toString());
        }),
      ],
    }),
  });

  return {
    setDoc(text: string) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text },
      });
    },
    getDoc() {
      return view.state.doc.toString();
    },
    focus() {
      view.focus();
    },
    destroy() {
      view.destroy();
    },
  };
}
