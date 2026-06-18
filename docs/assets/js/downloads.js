/* Downloads page — pulls the latest releases straight from GitHub so the page
   never goes stale. Shows the current release prominently plus up to 4 previous.
   No token, no build step: a single unauthenticated GitHub REST call. */

const REPO = "snoodleboot-io/alurtmee";
const API = `https://api.github.com/repos/${REPO}/releases?per_page=20`;
const RELEASES_URL = `https://github.com/${REPO}/releases`;
const MAX_PREVIOUS = 4;

const ICONS = {
  appimage: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12"/><path d="m8 11 4 4 4-4"/><path d="M5 21h14"/></svg>`,
  deb: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 8 12 3 3 8l9 5 9-5Z"/><path d="m3 8 9 5 9-5"/><path d="M3 8v8l9 5 9-5V8"/></svg>`,
  sum: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 12l2 2 4-4"/><path d="M21 12c0 5-3.5 7.5-8.5 9C7.5 19.5 4 17 4 12V6l8-3 8 3Z"/></svg>`,
  file: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>`,
};

function fmtBytes(n) {
  if (!n && n !== 0) return "";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(n < 10 && i > 0 ? 1 : 0)} ${u[i]}`;
}

function fmtDate(iso) {
  try {
    return new Date(iso).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  } catch (_) { return ""; }
}

function classifyAsset(a) {
  const n = a.name.toLowerCase();
  if (n.endsWith(".appimage")) return { kind: "appimage", label: "AppImage", sub: "Portable · Linux x86-64", icon: ICONS.appimage };
  if (n.endsWith(".deb")) return { kind: "deb", label: "Debian / Ubuntu", sub: ".deb package", icon: ICONS.deb };
  if (n.includes("sha256") || n.includes("sum")) return { kind: "sum", label: "SHA256SUMS", sub: "Verify integrity", icon: ICONS.sum };
  return { kind: "file", label: a.name, sub: "Asset", icon: ICONS.file };
}

// AppImage and .deb first, then checksums, then the rest.
const ASSET_ORDER = { appimage: 0, deb: 1, sum: 2, file: 3 };

function assetButton(a) {
  const c = classifyAsset(a);
  return `
    <a class="asset-btn" href="${a.browser_download_url}" data-kind="${c.kind}">
      <span class="ai">${c.icon}</span>
      <span class="al"><b>${c.label}</b><small>${c.sub}${a.size ? " · " + fmtBytes(a.size) : ""}</small></span>
    </a>`;
}

function assetsHTML(assets) {
  if (!assets || !assets.length) {
    return `<a class="asset-btn" href="${RELEASES_URL}" data-kind="file"><span class="ai">${ICONS.file}</span><span class="al"><b>View on GitHub</b><small>Assets attached to this release</small></span></a>`;
  }
  return [...assets]
    .map((a) => ({ a, c: classifyAsset(a) }))
    .sort((x, y) => (ASSET_ORDER[x.c.kind] - ASSET_ORDER[y.c.kind]))
    .map(({ a }) => assetButton(a))
    .join("");
}

function escapeHTML(s) {
  return (s || "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}

// First ~16 lines of notes, markdown stripped lightly for a readable summary.
function notesSummary(body) {
  if (!body) return "";
  const text = body
    .replace(/\r/g, "")
    .replace(/^#+\s*/gm, "")
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .trim();
  const lines = text.split("\n").slice(0, 16).join("\n");
  return escapeHTML(lines);
}

function renderLatest(r) {
  const el = document.querySelector("#dl-latest");
  if (!el) return;
  el.innerHTML = `
    <div class="dl-latest-top">
      <div class="meta">
        <span class="badge">◆ Latest release</span>
        <h2>${escapeHTML(r.name || r.tag_name)}</h2>
        <p class="when">Published ${fmtDate(r.published_at)}${r.tag_name ? " · " + escapeHTML(r.tag_name) : ""}</p>
      </div>
      <a class="btn btn-ghost" href="${r.html_url}" target="_blank" rel="noopener">Release notes →</a>
    </div>
    <div class="dl-latest-top" style="padding-top:0">
      <div class="dl-assets">${assetsHTML(r.assets)}</div>
    </div>`;
}

function releaseItem(r, isCurrent) {
  const notes = notesSummary(r.body);
  return `
    <details class="release"${isCurrent ? " open" : ""}>
      <summary>
        <span class="v">${escapeHTML(r.name || r.tag_name)}</span>
        ${isCurrent
          ? `<span class="tag-rel cur">Current</span>`
          : `<span class="tag-rel">Previous</span>`}
        <span class="when">${fmtDate(r.published_at)}</span>
        <svg class="chev" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
      </summary>
      <div class="release-body">
        <div class="dl-assets">${assetsHTML(r.assets)}</div>
        ${notes ? `<div class="notes">${notes}</div>` : ""}
        <a href="${r.html_url}" target="_blank" rel="noopener" style="color:var(--accent-2);font-size:var(--step--1)">Full notes on GitHub →</a>
      </div>
    </details>`;
}

function renderEmpty() {
  const latest = document.querySelector("#dl-latest");
  const list = document.querySelector("#dl-list");
  if (latest) {
    latest.innerHTML = `
      <div class="dl-empty">
        <h2 style="font-size:var(--step-1);margin-bottom:.6rem">No public releases yet</h2>
        <p style="color:var(--muted);max-width:46ch;margin:0 auto 1.4rem">
          Packaged builds land here automatically the moment the first release is cut.
          In the meantime you can build from source or watch the repo.</p>
        <div class="hero-cta" style="justify-content:center">
          <a class="btn btn-primary" href="${RELEASES_URL}" target="_blank" rel="noopener">Releases on GitHub</a>
          <a class="btn btn-ghost" href="https://github.com/${REPO}#option-c--build-from-source" target="_blank" rel="noopener">Build from source</a>
        </div>
      </div>`;
  }
  if (list) list.innerHTML = "";
}

function renderError() {
  const latest = document.querySelector("#dl-latest");
  const list = document.querySelector("#dl-list");
  if (latest) {
    latest.innerHTML = `
      <div class="dl-empty">
        <h2 style="font-size:var(--step-1);margin-bottom:.6rem">Couldn't reach GitHub</h2>
        <p style="color:var(--muted);max-width:46ch;margin:0 auto 1.4rem">
          The live release list is temporarily unavailable. Head to GitHub for every build.</p>
        <a class="btn btn-primary" href="${RELEASES_URL}" target="_blank" rel="noopener">Open Releases on GitHub</a>
      </div>`;
  }
  if (list) list.innerHTML = "";
}

async function loadReleases() {
  const list = document.querySelector("#dl-list");
  try {
    const res = await fetch(API, { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) throw new Error(`GitHub ${res.status}`);
    let releases = await res.json();
    releases = (releases || []).filter((r) => !r.draft && !r.prerelease);
    if (!releases.length) { renderEmpty(); return; }

    const [current, ...rest] = releases;
    const previous = rest.slice(0, MAX_PREVIOUS);

    renderLatest(current);

    if (list) {
      const header = `<h2 class="reveal in">All versions <span style="color:var(--muted);font-weight:400;font-size:var(--step--1)">— current + last ${previous.length}</span></h2>`;
      list.innerHTML = header + releaseItem(current, true) + previous.map((r) => releaseItem(r, false)).join("");
    }
  } catch (e) {
    console.error(e);
    renderError();
  }
}

document.addEventListener("DOMContentLoaded", loadReleases);
