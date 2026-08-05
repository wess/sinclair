const RELEASES = "https://github.com/wess/sinclair/releases";

const commands = {
  mac: "brew install --cask wess/packages/sinclair",
  linux: [
    "sudo apt install ./sinclair_*_amd64.deb",
    "# or run the AppImage:",
    "chmod +x Sinclair-*.AppImage && ./Sinclair-*.AppImage",
  ].join("\n"),
  windows: "scoop install https://raw.githubusercontent.com/wess/sinclair/main/packaging/scoop/sinclair.json",
  source: "cargo run -p app --release",
};

const osLabels = { mac: "macOS", linux: "Linux", windows: "Windows", source: "your platform" };
const altHints = {
  mac: "or  brew install --cask wess/packages/sinclair",
  linux: "or .deb, tarball, and other architectures below",
  windows: "beta build · unsigned installer (expect a SmartScreen prompt) · or use Scoop",
  source: "or build it from source below",
};

const copyText = async (text, button) => {
  try {
    await navigator.clipboard.writeText(text);
    const previous = button.textContent;
    button.textContent = "Copied";
    window.setTimeout(() => {
      button.textContent = previous;
    }, 1400);
  } catch {
    button.textContent = "Select text";
  }
};

for (const button of document.querySelectorAll("[data-copy]")) {
  button.addEventListener("click", () => copyText(button.dataset.copy, button));
}

const installCode = document.querySelector("[data-install-code]");
const copyWide = document.querySelector(".copywide");
const tabs = [...document.querySelectorAll("[data-install-tab]")];
const downloadsBox = document.querySelector("[data-install-downloads]");
const hero = document.querySelector("[data-download-hero]");

// --- platform + architecture detection ---
const detect = () => {
  const ua = navigator.userAgent || "";
  const uad = navigator.userAgentData;
  const plat = (uad && uad.platform) || navigator.platform || ua;
  let os = "mac";
  if (/win/i.test(plat) || /Windows/i.test(ua)) os = "windows";
  else if (/mac|iphone|ipad|ipod/i.test(plat) || /Mac OS X/i.test(ua)) os = "mac";
  else if (/linux|android|cros|x11/i.test(plat) || /Linux/i.test(ua)) os = "linux";
  const arch = /aarch64|arm64|armv8/i.test(ua) ? "aarch64" : "x86_64";
  return { os, arch };
};

const state = { ...detect(), release: null };

const assetUrl = (test) =>
  state.release && (state.release.assets.find(test) || {}).browser_download_url;

// direct-download links for a given OS, pulled from the live release assets
const downloadsFor = (os) => {
  if (!state.release) return [];
  const out = [];
  const add = (test, label) => { const u = assetUrl(test); if (u) out.push({ label, url: u }); };
  if (os === "mac") add((a) => /\.dmg$/i.test(a.name), ".dmg — Universal · Apple Silicon & Intel");
  if (os === "windows") {
    add((a) => /windows.*\.msi$/i.test(a.name), "Installer .msi · x64");
    add((a) => /windows.*\.zip$/i.test(a.name), "Portable .zip · x64");
  }
  if (os === "linux") {
    add((a) => /x86_64\.AppImage$/i.test(a.name), "AppImage · x86_64");
    add((a) => /aarch64\.AppImage$/i.test(a.name), "AppImage · ARM64");
    add((a) => /amd64\.deb$/i.test(a.name), ".deb · x86_64");
    add((a) => /arm64\.deb$/i.test(a.name), ".deb · ARM64");
    add((a) => /linux-x86_64\.tar\.gz$/i.test(a.name), "tarball · x86_64");
    add((a) => /linux-aarch64\.tar\.gz$/i.test(a.name), "tarball · ARM64");
  }
  return out;
};

const primaryFor = (os) => {
  if (os === "mac") return assetUrl((a) => /\.dmg$/i.test(a.name));
  if (os === "windows") return assetUrl((a) => /windows.*\.msi$/i.test(a.name));
  if (os === "linux") {
    const arm = state.arch === "aarch64";
    return assetUrl((a) => /\.AppImage$/i.test(a.name) && (arm ? /aarch64/i : /x86_64/i).test(a.name));
  }
  return null;
};

const renderDownloads = (os) => {
  if (!downloadsBox) return;
  const items = downloadsFor(os);
  if (!items.length) {
    downloadsBox.innerHTML =
      os === "source"
        ? `<a class="dl ghost" href="https://github.com/wess/sinclair">View the source on GitHub<span>&rarr;</span></a>`
        : `<a class="dl ghost" href="${RELEASES}/latest">All downloads on GitHub Releases<span>&rarr;</span></a>`;
    return;
  }
  downloadsBox.innerHTML =
    items.map((i) => `<a class="dl" href="${i.url}">${i.label}<span>&darr;</span></a>`).join("") +
    `<a class="dl ghost" href="${RELEASES}">Other versions &amp; checksums<span>&rarr;</span></a>`;
};

const renderHero = () => {
  if (!hero) return;
  const os = state.os;
  hero.querySelector("[data-dl-os]").textContent = `Detected: ${osLabels[os] || "your platform"}`;
  hero.querySelector("[data-dl-ver]").textContent = state.release ? state.release.tag_name : "";
  const btn = hero.querySelector("[data-dl-primary]");
  const url = primaryFor(os);
  btn.href = url || `${RELEASES}/latest`;
  btn.textContent = os === "source" ? "View releases" : `Download for ${osLabels[os]}`;
  hero.querySelector("[data-dl-alt]").textContent = altHints[os] || "";
  hero.hidden = false;
};

const selectTab = (key) => {
  installCode.textContent = commands[key];
  copyWide.dataset.copy = commands[key];
  for (const t of tabs) t.classList.toggle("active", t.dataset.installTab === key);
  renderDownloads(key);
};

for (const tab of tabs) {
  tab.addEventListener("click", () => selectTab(tab.dataset.installTab));
}

// initial paint from detection, refined once the release lands
selectTab(state.os === "source" ? "mac" : state.os);
renderHero();

if (navigator.userAgentData && navigator.userAgentData.getHighEntropyValues) {
  navigator.userAgentData
    .getHighEntropyValues(["architecture"])
    .then((v) => {
      if (/arm/i.test(v.architecture || "")) {
        state.arch = "aarch64";
        renderHero();
      }
    })
    .catch(() => {});
}

fetch("https://api.github.com/repos/wess/sinclair/releases/latest")
  .then((r) => (r.ok ? r.json() : null))
  .then((rel) => {
    if (!rel) return;
    state.release = rel;
    renderHero();
    const active = (tabs.find((t) => t.classList.contains("active")) || {}).dataset;
    renderDownloads(active ? active.installTab : state.os);
  })
  .catch(() => {});

const formatStars = (stars) => {
  if (!Number.isFinite(stars)) return "GitHub";
  if (stars >= 1000) return `${(stars / 1000).toFixed(1)}k stars`;
  return `${stars} stars`;
};

fetch("https://api.github.com/repos/wess/sinclair")
  .then((response) => (response.ok ? response.json() : null))
  .then((repo) => {
    if (!repo) return;
    for (const target of document.querySelectorAll("[data-star-count]")) {
      target.textContent = formatStars(repo.stargazers_count);
    }
  })
  .catch(() => {});
