# Alurtmee site (GitHub Pages)

The marketing + downloads site, served from this `docs/` folder. Plain static HTML/CSS/JS —
no build step, no dependencies.

```
docs/
├── index.html            landing page
├── downloads.html        live downloads (current + last 4 releases)
├── .nojekyll             serve files as-is (skip Jekyll)
└── assets/
    ├── css/styles.css    design system + the app's six themes as CSS vars
    ├── js/main.js        theme switcher, scroll reveals, starfield, nav
    ├── js/downloads.js   fetches releases from the GitHub REST API
    ├── img/              logo, favicon, video poster
    └── video/orb.mp4     hero/showcase animation
```

## Publishing

Two ways — pick one in **Settings → Pages**:

- **GitHub Actions (recommended).** Set *Source* to **GitHub Actions**. The
  [`pages.yml`](../.github/workflows/pages.yml) workflow deploys `docs/` on every push to `main`
  that touches it. The site is served at `https://snoodleboot-io.github.io/alurtmee/`.
- **Deploy from a branch.** Set *Source* to **Deploy from a branch**, branch `main`, folder
  `/docs`. No workflow needed.

## Notes

- **Downloads are live.** `downloads.js` calls the unauthenticated GitHub Releases API at runtime,
  so new releases appear automatically — no redeploy. Until the first release exists it shows a
  "build from source" fallback. (Unauthenticated GitHub API is rate-limited to 60 req/hr per IP;
  more than enough for a visitor.)
- **Themes are the real ones.** The six palettes in `styles.css` mirror
  [`crates/app/src/theme.rs`](../crates/app/src/theme.rs). If a skin changes there, update the
  matching `[data-theme="…"]` block here.
- **The orb** floats via `mix-blend-mode: lighten`, which drops its pure-black backdrop against the
  dark page background.
