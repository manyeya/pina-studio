// The download links, from the latest release on GitHub. Until it answers (or if it can't), each
// link goes to the releases page. Marks up:
//   <a data-dl="mac|win|zip|primary">   the file's link ("primary": the visitor's platform)
//   <span data-dl-size="mac|win|zip">   its size, "29.9 MB"
//   <span data-dl-label="primary">      "Download for macOS" or "…for Windows"
//   <span data-dl-other="label">        the other platform's name; <a data-dl="other"> its link
//   <span data-version>                 "0.3.0"
(() => {
  const REPO = "manyeya/pina-studio";
  const platform = (navigator.userAgentData?.platform || navigator.platform || "").toLowerCase();
  const os = platform.includes("win") ? "win" : "mac";
  const other = os === "win" ? "mac" : "win";
  const names = { mac: "macOS", win: "Windows" };
  const ends = { mac: ".dmg", win: "-windows-x64-setup.exe", zip: "-windows-x64.zip" };

  const each = (sel, f) => document.querySelectorAll(sel).forEach(f);
  const mb = (bytes) => `${(bytes / 1e6).toFixed(1)} MB`;

  each('[data-dl-label="primary"]', (el) => (el.textContent = `Download for ${names[os]}`));
  each('[data-dl-other="label"]', (el) => (el.textContent = names[other]));
  document.documentElement.dataset.os = os;

  fetch(`https://api.github.com/repos/${REPO}/releases/latest`)
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((release) => {
      const find = (key) => release.assets.find((a) => a.name.endsWith(ends[key]));
      const files = { mac: find("mac"), win: find("win"), zip: find("zip") };
      files.primary = files[os];
      files.other = files[other];
      for (const [key, file] of Object.entries(files)) {
        if (!file) continue;
        each(`[data-dl="${key}"]`, (el) => (el.href = file.browser_download_url));
        each(`[data-dl-size="${key}"]`, (el) => (el.textContent = mb(file.size)));
      }
      each("[data-version]", (el) => (el.textContent = release.tag_name.replace(/^v/, "")));
    })
    .catch(() => {});
})();
