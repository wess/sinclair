// Documentation chrome. Each doc page ships only its <main><article
// data-doc-page="slug">…</article></main>; this module builds the shared
// header, left sidebar, on-page table of contents, prev/next links, mobile
// menu, and per-code-block copy buttons from the manifest below. No build step.

const MANIFEST = [
  {
    title: "Getting started",
    pages: [
      { slug: "overview", title: "Overview", href: "docs.html" },
      { slug: "install", title: "Install", href: "install.html" },
    ],
  },
  {
    title: "Configuration",
    pages: [
      { slug: "configuration", title: "Configuration", href: "configuration.html" },
      { slug: "keybindings", title: "Keybindings & actions", href: "keybindings.html" },
      { slug: "themes", title: "Themes & appearance", href: "themes.html" },
    ],
  },
  {
    title: "Using Prompt",
    pages: [
      { slug: "workspace", title: "Tabs, splits & layouts", href: "workspace.html" },
      { slug: "palette", title: "Command palette & quick open", href: "palette.html" },
      { slug: "productivity", title: "Productivity", href: "productivity.html" },
    ],
  },
  {
    title: "Extending",
    pages: [
      { slug: "plugins", title: "Plugins", href: "plugins.html" },
      { slug: "plugintutorial", title: "Plugin tutorial", href: "plugintutorial.html" },
      { slug: "designer", title: "Prompt Designer", href: "designer.html" },
      { slug: "mcp", title: "MCP & automation", href: "mcp.html" },
    ],
  },
  {
    title: "Agents",
    pages: [{ slug: "agents", title: "Agent mesh", href: "agents.html" }],
  },
  {
    title: "Reference",
    pages: [
      { slug: "coverage", title: "Terminal coverage", href: "coverage.html" },
      { slug: "cli", title: "CLI reference", href: "cli.html" },
    ],
  },
];

const REPO = "https://github.com/wess/prompt";

const el = (tag, props = {}, ...kids) => {
  const node = Object.assign(document.createElement(tag), props);
  for (const k of kids) node.append(k);
  return node;
};

const slugify = (text) =>
  text
    .toLowerCase()
    .replace(/[^\w]+/g, "-")
    .replace(/^-+|-+$/g, "");

const flat = MANIFEST.flatMap((g) => g.pages);

function buildHeader() {
  const burger = el("button", { className: "dc-burger", type: "button", textContent: "☰" });
  burger.setAttribute("aria-label", "Toggle navigation");
  burger.addEventListener("click", () => document.body.classList.toggle("nav-open"));

  const brand = el("a", { className: "dc-brand", href: "index.html" });
  brand.append(
    Object.assign(el("img"), { src: "icon.png", alt: "", width: 28, height: 28 }),
    el("span", { textContent: "Prompt" }),
  );

  const gh = el("a", { className: "dc-ghbtn", href: REPO, textContent: "GitHub" });

  const header = el("header", { className: "dc-header" });
  header.append(
    burger,
    brand,
    el("a", { className: "toplink", href: "docs.html", textContent: "Docs" }),
    el("a", { className: "toplink", href: "index.html#install", textContent: "Install" }),
    el("span", { className: "spacer" }),
    gh,
  );
  return header;
}

function buildSidebar(currentSlug) {
  const nav = el("nav", { className: "dc-sidebar" });
  nav.setAttribute("aria-label", "Documentation");
  for (const group of MANIFEST) {
    const g = el("div", { className: "group" });
    g.append(el("p", { textContent: group.title }));
    for (const page of group.pages) {
      const a = el("a", { href: page.href, textContent: page.title });
      if (page.slug === currentSlug) a.classList.add("active");
      g.append(a);
    }
    nav.append(g);
  }
  return nav;
}

function buildToc(article) {
  const headings = [...article.querySelectorAll("h2, h3")];
  if (headings.length === 0) return null;
  const toc = el("nav", { className: "dc-toc" });
  toc.setAttribute("aria-label", "On this page");
  toc.append(el("p", { textContent: "On this page" }));
  const links = [];
  for (const h of headings) {
    if (!h.id) h.id = slugify(h.textContent);
    // hoverable heading anchor
    const anchor = el("a", { className: "anchor", href: `#${h.id}`, textContent: "#" });
    anchor.setAttribute("aria-hidden", "true");
    h.append(anchor);
    const link = el("a", { href: `#${h.id}`, textContent: h.textContent.replace(/#$/, "") });
    if (h.tagName === "H3") link.classList.add("sub");
    toc.append(link);
    links.push({ id: h.id, link });
  }
  // scrollspy
  const byId = new Map(links.map((l) => [l.id, l.link]));
  const seen = new Set();
  const obs = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) seen.add(e.target.id);
        else seen.delete(e.target.id);
      }
      const active = headings.find((h) => seen.has(h.id));
      for (const l of links) l.link.classList.remove("active");
      if (active) byId.get(active.id)?.classList.add("active");
    },
    { rootMargin: "-72px 0px -70% 0px" },
  );
  headings.forEach((h) => obs.observe(h));
  return toc;
}

function buildPrevNext(currentSlug) {
  const i = flat.findIndex((p) => p.slug === currentSlug);
  if (i < 0) return null;
  const wrap = el("div", { className: "dc-prevnext" });
  if (i > 0) {
    const p = flat[i - 1];
    const a = el("a", { className: "prev", href: p.href });
    a.append(el("span", { textContent: "Previous" }), document.createTextNode(p.title));
    wrap.append(a);
  } else {
    wrap.append(el("span", { style: "flex:1" }));
  }
  if (i < flat.length - 1) {
    const n = flat[i + 1];
    const a = el("a", { className: "next", href: n.href });
    a.append(el("span", { textContent: "Next" }), document.createTextNode(n.title));
    wrap.append(a);
  }
  return wrap;
}

function addCopyButtons(article) {
  for (const pre of article.querySelectorAll("pre")) {
    const code = pre.querySelector("code");
    if (!code) continue;
    const btn = el("button", { className: "copy", type: "button", textContent: "Copy" });
    btn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(code.innerText);
        btn.textContent = "Copied";
        setTimeout(() => (btn.textContent = "Copy"), 1400);
      } catch {
        btn.textContent = "Select";
      }
    });
    pre.append(btn);
  }
}

function init() {
  const main = document.getElementById("doc-main");
  const article = main?.querySelector(".doc-article");
  if (!main || !article) return;
  const slug = article.dataset.docPage || "";

  document.body.classList.add("docs");
  const header = buildHeader();
  const sidebar = buildSidebar(slug);
  const toc = buildToc(article);
  const prevnext = buildPrevNext(slug);
  if (prevnext) article.after(prevnext);
  addCopyButtons(article);

  const grid = el("div", { className: "dc-grid" });
  // place main where it currently is, then wrap
  const parent = main.parentNode;
  const placeholder = document.createComment("doc-grid");
  parent.insertBefore(placeholder, main);
  grid.append(sidebar, main);
  if (toc) grid.append(toc);
  parent.replaceChild(grid, placeholder);
  document.body.insertBefore(header, document.body.firstChild);

  // close mobile nav after following a link
  sidebar.addEventListener("click", (e) => {
    if (e.target.tagName === "A") document.body.classList.remove("nav-open");
  });
}

init();
