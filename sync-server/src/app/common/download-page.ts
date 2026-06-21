/**
 * Renders the public IDC download landing page as a single self-contained HTML
 * string (no external JS/CSS/fonts -- everything inline so it works behind a
 * strict CSP and on a cold cache).
 *
 * The page is data-driven at view time: client-side JS fetches each platform's
 * Tauri updater manifest (`/idc/<target>/x86_64/latest.json`) from the releases
 * host and fills in the live version + download link, so it never goes stale as
 * releases ship. If a manifest is unreachable (e.g. a platform not yet
 * published) the card degrades gracefully to a disabled state.
 *
 * Visual language mirrors the desktop app's editorial design system (warm paper
 * surfaces, ink text, crimson accent, Inter, restrained scale).
 */
export function renderDownloadPage (releasesHost: string, nonce: string): string {
  // The page only ever talks to this host; injected into the inline script as a
  // JSON string literal so there is no interpolation-in-attribute surface.
  const hostJson = JSON.stringify(releasesHost)

  return `<!DOCTYPE html>
<html lang="en" dir="ltr">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>IDC System &mdash; Download</title>
<meta name="description" content="Download the IDC System desktop app for Windows, macOS, and Linux." />
<style>
  :root {
    --paper: #FDFDFA; --paper-2: #F7F5EE; --surface: #FFFFFF;
    --line: #ECE8DB; --line-2: #DDD8C7;
    --ink: #0A1230; --ink-2: #1C2851; --ink-3: #5E5A4E; --ink-4: #94907F;
    --crimson: #C0263A; --crimson-dark: #9A1F30;
    --success: #047857; --gold: #B45309;
    --radius: 6px; --radius-lg: 12px;
    --ease: cubic-bezier(.2,.7,.2,1);
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; }
  body {
    background: var(--paper); color: var(--ink-2);
    font-family: 'Inter', system-ui, -apple-system, Segoe UI, Roboto, sans-serif;
    letter-spacing: -0.011em; line-height: 1.5;
    -webkit-font-smoothing: antialiased;
  }
  .wrap { max-width: 880px; margin: 0 auto; padding: 64px 28px 80px; }
  .eyebrow {
    display: inline-flex; align-items: center; gap: 10px;
    font-size: 11px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.12em; color: var(--ink-3);
  }
  .eyebrow::before { content: ""; width: 22px; height: 2px; background: var(--crimson); }
  h1 {
    margin: 14px 0 0; font-size: 38px; font-weight: 700;
    letter-spacing: -0.026em; line-height: 1.08; color: var(--ink);
  }
  .lede { margin: 12px 0 0; font-size: 15px; color: var(--ink-3); max-width: 56ch; }
  .grid {
    margin-top: 40px; display: grid; gap: 16px;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  }
  .card {
    background: var(--surface); border: 1px solid var(--line);
    border-radius: var(--radius-lg); padding: 22px 22px 20px;
    display: flex; flex-direction: column; gap: 4px;
    transition: transform .2s var(--ease), box-shadow .2s var(--ease), border-color .2s var(--ease);
  }
  .card.detected { border-color: var(--line-2); box-shadow: 0 4px 16px rgba(10,18,48,0.05); }
  .card-os { font-size: 15px; font-weight: 600; color: var(--ink); letter-spacing: -0.01em; }
  .card-meta { font-size: 12px; color: var(--ink-3); min-height: 18px; }
  .card-ver {
    font-family: 'Geist Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px; color: var(--ink-4); font-variant-numeric: tabular-nums;
    min-height: 16px;
  }
  .badge {
    align-self: flex-start; margin-bottom: 8px;
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em;
    padding: 2px 8px; border-radius: 999px; background: var(--paper-2); color: var(--ink-3);
  }
  .badge.you { background: #EFF4FD; color: #1D4ED8; }
  .btn {
    margin-top: 14px; display: inline-flex; align-items: center; justify-content: center;
    height: 40px; padding: 0 16px; border-radius: var(--radius);
    font-size: 13px; font-weight: 600; text-decoration: none; cursor: pointer;
    border: 1px solid var(--line-2); background: var(--paper-2); color: var(--ink-2);
    transition: background .16s var(--ease), color .16s var(--ease), transform .16s var(--ease), box-shadow .16s var(--ease);
  }
  .btn:hover { background: var(--surface); }
  .btn.primary { background: var(--crimson); border-color: var(--crimson); color: #fff; }
  .btn.primary:hover { background: var(--crimson-dark); transform: translateY(-1px); box-shadow: 0 6px 16px rgba(192,38,58,0.15); }
  .btn[aria-disabled="true"] {
    opacity: .5; pointer-events: none; background: var(--paper-2); color: var(--ink-4); border-color: var(--line);
  }
  .note {
    margin-top: 36px; padding: 16px 18px; border: 1px solid var(--line);
    border-radius: var(--radius-lg); background: var(--paper-2);
    font-size: 13px; color: var(--ink-3);
  }
  .note b { color: var(--ink-2); font-weight: 600; }
  footer {
    margin-top: 48px; padding-top: 20px; border-top: 1px solid var(--line);
    font-size: 12px; color: var(--ink-4);
    display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center;
  }
  footer .mono { font-family: 'Geist Mono', ui-monospace, monospace; }
  a.link { color: var(--crimson); text-decoration: none; }
  a.link:hover { text-decoration: underline; }
</style>
</head>
<body>
<div class="wrap">
  <header>
    <span class="eyebrow">IDC System</span>
    <h1>Download the desktop app</h1>
    <p class="lede">The offline-first clinic workstation for reception, accounting, and daily close. Pick your platform &mdash; updates install themselves after the first download.</p>
  </header>

  <div class="grid" id="cards">
    <section class="card" data-platform="windows-x86_64">
      <span class="badge" data-badge>Windows</span>
      <span class="card-os">Windows 10 / 11</span>
      <span class="card-meta" data-meta>64-bit installer</span>
      <span class="card-ver" data-ver>&nbsp;</span>
      <a class="btn" data-dl href="#" aria-disabled="true">Checking&hellip;</a>
    </section>
    <section class="card" data-platform="darwin-aarch64">
      <span class="badge" data-badge>macOS</span>
      <span class="card-os">macOS (Apple Silicon)</span>
      <span class="card-meta" data-meta>M1 / M2 / M3</span>
      <span class="card-ver" data-ver>&nbsp;</span>
      <a class="btn" data-dl href="#" aria-disabled="true">Checking&hellip;</a>
    </section>
    <section class="card" data-platform="darwin-x86_64">
      <span class="badge" data-badge>macOS</span>
      <span class="card-os">macOS (Intel)</span>
      <span class="card-meta" data-meta>Intel Macs</span>
      <span class="card-ver" data-ver>&nbsp;</span>
      <a class="btn" data-dl href="#" aria-disabled="true">Checking&hellip;</a>
    </section>
    <section class="card" data-platform="linux-x86_64">
      <span class="badge" data-badge>Linux</span>
      <span class="card-os">Linux (AppImage)</span>
      <span class="card-meta" data-meta>64-bit, most distros</span>
      <span class="card-ver" data-ver>&nbsp;</span>
      <a class="btn" data-dl href="#" aria-disabled="true">Checking&hellip;</a>
    </section>
  </div>

  <div class="note">
    <b>Already installed?</b> You do not need this page &mdash; the app checks for updates on its own and installs them in the background. This page is only for the first install on a new machine.
  </div>

  <footer>
    <span>IDC System</span>
    <span class="mono" id="latest-line">&nbsp;</span>
  </footer>
</div>

<script nonce="${nonce}">
(function () {
  "use strict";
  var HOST = ${hostJson};
  // Map each card's data-platform to (a) the Tauri updater "target" segment used
  // in the manifest URL path, and (b) the Tauri manifest platform key.
  var PLATFORMS = {
    "windows-x86_64": { target: "windows", key: "windows-x86_64" },
    "darwin-aarch64": { target: "darwin",  key: "darwin-aarch64" },
    "darwin-x86_64":  { target: "darwin",  key: "darwin-x86_64"  },
    "linux-x86_64":   { target: "linux",   key: "linux-x86_64"   }
  };

  function detectPlatform () {
    var p = (navigator.platform || "") + " " + (navigator.userAgent || "");
    if (/Win/i.test(p)) return "windows-x86_64";
    if (/Mac/i.test(p)) {
      // Apple Silicon is hard to detect reliably; default to ARM for modern Macs.
      return /Intel/i.test(p) ? "darwin-x86_64" : "darwin-aarch64";
    }
    if (/Linux|X11/i.test(p)) return "linux-x86_64";
    return null;
  }

  var detected = detectPlatform();
  var newestVersion = null;

  function fileUrl (target, name) {
    return "https://" + HOST + "/idc/" + target + "/x86_64/" + name;
  }

  function setCard (card, opts) {
    var btn = card.querySelector("[data-dl]");
    var ver = card.querySelector("[data-ver]");
    var meta = card.querySelector("[data-meta]");
    if (opts.url) {
      btn.href = opts.url;
      btn.removeAttribute("aria-disabled");
      btn.setAttribute("download", "");
      btn.textContent = "Download";
      if (card.getAttribute("data-platform") === detected) {
        btn.classList.add("primary");
        card.classList.add("detected");
      }
    } else {
      btn.setAttribute("aria-disabled", "true");
      btn.textContent = opts.unavailable ? "Not available yet" : "Unavailable";
    }
    if (opts.version) ver.textContent = "v" + opts.version;
    if (opts.metaText) meta.textContent = opts.metaText;
  }

  function noteVersion (version) {
    if (version && (!newestVersion || version > newestVersion)) {
      newestVersion = version;
      var line = document.getElementById("latest-line");
      if (line) line.textContent = "Latest release v" + version;
    }
  }

  function getJson (target, file) {
    return fetch(fileUrl(target, file), { cache: "no-store" }).then(function (r) {
      if (!r.ok) throw new Error(file + " " + r.status);
      return r.json();
    });
  }

  function load (card) {
    var platform = card.getAttribute("data-platform");
    var spec = PLATFORMS[platform];
    if (!spec) { setCard(card, {}); return; }

    // Prefer install.json -- the human-runnable first-time installer published
    // by the deploy script (Windows -setup.exe, Linux AppImage). Fall back to
    // the updater's latest.json so the page still works on a host that hasn't
    // published install.json yet (older deploy). Degrade if neither exists.
    getJson(spec.target, "install.json")
      .then(function (m) {
        var entry = m && m.platforms && m.platforms[spec.key];
        var url = entry && entry.url;
        if (!url) throw new Error("no installer entry");
        noteVersion(m && m.version);
        setCard(card, { url: url, version: (m && m.version) || null });
      })
      .catch(function () {
        return getJson(spec.target, "latest.json").then(function (m) {
          var entry = m && m.platforms && m.platforms[spec.key];
          var url = entry && entry.url;
          noteVersion(m && m.version);
          setCard(card, { url: url || null, version: (m && m.version) || null, unavailable: !url });
        });
      })
      .catch(function () {
        // Neither manifest published for this platform (or transient): degrade.
        setCard(card, { unavailable: true });
      });
  }

  // Highlight the detected card's badge.
  if (detected) {
    var dc = document.querySelector('[data-platform="' + detected + '"]');
    if (dc) {
      var badge = dc.querySelector("[data-badge]");
      if (badge) { badge.textContent = "Your system"; badge.classList.add("you"); }
    }
  }

  var cards = document.querySelectorAll("#cards .card");
  for (var i = 0; i < cards.length; i++) load(cards[i]);
})();
</script>
</body>
</html>`
}
