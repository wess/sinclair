const commands = {
  mac: "brew install --cask wess/packages/prompt",
  linux: [
    "# AppImage, deb, and tarball builds are on GitHub releases",
    "curl -L https://github.com/wess/prompt/releases/latest",
  ].join("\n"),
  source: "cargo run -p app --release",
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

for (const tab of document.querySelectorAll("[data-install-tab]")) {
  tab.addEventListener("click", () => {
    const key = tab.dataset.installTab;
    installCode.textContent = commands[key];
    copyWide.dataset.copy = commands[key];
    for (const other of document.querySelectorAll("[data-install-tab]")) {
      other.classList.toggle("active", other === tab);
    }
  });
}

const formatStars = (stars) => {
  if (!Number.isFinite(stars)) return "GitHub";
  if (stars >= 1000) return `${(stars / 1000).toFixed(1)}k stars`;
  return `${stars} stars`;
};

fetch("https://api.github.com/repos/wess/prompt")
  .then((response) => (response.ok ? response.json() : null))
  .then((repo) => {
    if (!repo) return;
    for (const target of document.querySelectorAll("[data-star-count]")) {
      target.textContent = formatStars(repo.stargazers_count);
    }
  })
  .catch(() => {});
