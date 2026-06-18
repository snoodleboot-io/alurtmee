/* Alurtmee site — interaction layer
   Theme switching mirrors the app's six skins; choice persists in localStorage. */

const THEMES = [
  { id: "nebula",    name: "Nebula",    accent: "#a838ff", accent2: "#21e6ff", bg: "#030208", note: "Brand" },
  { id: "aurora",    name: "Aurora",    accent: "#2fe0c4", accent2: "#5ad0ff", bg: "#080b0b", note: "" },
  { id: "velvet",    name: "Velvet",    accent: "#cf95ff", accent2: "#ff79d9", bg: "#0a080e", note: "" },
  { id: "synthwave", name: "Synthwave", accent: "#ff45e0", accent2: "#2ad4ff", bg: "#0a0710", note: "" },
  { id: "voltage",   name: "Voltage",   accent: "#b6ff3a", accent2: "#2ee6c0", bg: "#020301", note: "" },
  { id: "ionix",     name: "Ionix",     accent: "#28ecff", accent2: "#b14dff", bg: "#03050b", note: "Logo" },
];
const STORE_KEY = "alurtmee-theme";

function applyTheme(id, persist = true) {
  const t = THEMES.find((x) => x.id === id) || THEMES[0];
  if (t.id === "nebula") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", t.id);
  if (persist) {
    try { localStorage.setItem(STORE_KEY, t.id); } catch (_) {}
  }
  // reflect selection in any controls present on the page
  document.querySelectorAll("[data-theme-opt]").forEach((el) => {
    el.setAttribute("aria-checked", String(el.dataset.themeOpt === t.id));
  });
  document.querySelectorAll("[data-theme-card]").forEach((el) => {
    el.setAttribute("aria-pressed", String(el.dataset.themeCard === t.id));
  });
  const label = document.querySelector("[data-theme-label]");
  if (label) label.textContent = t.name;
  // recolor browser chrome
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", t.bg);
}

function initTheme() {
  let saved = null;
  try { saved = localStorage.getItem(STORE_KEY); } catch (_) {}
  applyTheme(saved || "nebula", false);

  // dropdown switcher in the navbar
  document.querySelectorAll("[data-theme-opt]").forEach((el) => {
    el.addEventListener("click", () => {
      applyTheme(el.dataset.themeOpt);
      const details = el.closest("details");
      if (details) details.open = false;
    });
  });
  // big preview cards in the themes section
  document.querySelectorAll("[data-theme-card]").forEach((el) => {
    el.addEventListener("click", () => applyTheme(el.dataset.themeCard));
  });
  // close the dropdown on outside click
  document.addEventListener("click", (e) => {
    document.querySelectorAll("details.theme-switch[open]").forEach((d) => {
      if (!d.contains(e.target)) d.open = false;
    });
  });
}

/* Sticky-nav border + mobile menu */
function initNav() {
  const nav = document.querySelector(".nav");
  if (!nav) return;
  const onScroll = () => nav.classList.toggle("scrolled", window.scrollY > 8);
  onScroll();
  window.addEventListener("scroll", onScroll, { passive: true });

  const toggle = nav.querySelector(".nav-toggle");
  if (toggle) toggle.addEventListener("click", () => nav.classList.toggle("open"));
  nav.querySelectorAll(".nav-links a").forEach((a) =>
    a.addEventListener("click", () => nav.classList.remove("open"))
  );
}

/* Scroll-reveal via IntersectionObserver */
function initReveal() {
  const els = document.querySelectorAll(".reveal");
  if (!els.length || !("IntersectionObserver" in window)) {
    els.forEach((el) => el.classList.add("in"));
    return;
  }
  const io = new IntersectionObserver(
    (entries) => {
      entries.forEach((e) => {
        if (e.isIntersecting) {
          e.target.classList.add("in");
          io.unobserve(e.target);
        }
      });
    },
    { threshold: 0.12, rootMargin: "0px 0px -8% 0px" }
  );
  els.forEach((el) => io.observe(el));
}

/* Pointer spotlight on feature cards */
function initSpotlight() {
  document.querySelectorAll(".card").forEach((card) => {
    card.addEventListener("pointermove", (e) => {
      const r = card.getBoundingClientRect();
      card.style.setProperty("--mx", `${e.clientX - r.left}px`);
      card.style.setProperty("--my", `${e.clientY - r.top}px`);
    });
  });
}

/* Tiny starfield on a canvas (cheap, respects reduced-motion) */
function initStars() {
  const canvas = document.querySelector(".stars");
  if (!canvas) return;
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const ctx = canvas.getContext("2d");
  let stars = [];
  let raf = null;

  function size() {
    canvas.width = window.innerWidth * devicePixelRatio;
    canvas.height = window.innerHeight * devicePixelRatio;
    const count = Math.min(140, Math.floor((window.innerWidth * window.innerHeight) / 14000));
    stars = Array.from({ length: count }, (_, i) => ({
      x: Math.random() * canvas.width,
      y: Math.random() * canvas.height,
      r: Math.random() * 1.3 * devicePixelRatio + 0.2,
      a: Math.random() * 0.5 + 0.2,
      tw: Math.random() * 0.02 + 0.004,
      p: Math.random() * Math.PI * 2,
    }));
  }
  function draw() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent-2").trim() || "#fff";
    for (const s of stars) {
      s.p += s.tw;
      const alpha = s.a + Math.sin(s.p) * 0.18;
      ctx.globalAlpha = Math.max(0, alpha);
      ctx.fillStyle = alpha > 0.55 ? accent : "#ffffff";
      ctx.beginPath();
      ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    raf = requestAnimationFrame(draw);
  }
  size();
  window.addEventListener("resize", size);
  if (reduce) {
    // single static paint
    for (const s of stars) { ctx.globalAlpha = s.a; ctx.fillStyle = "#fff"; ctx.beginPath(); ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2); ctx.fill(); }
  } else {
    draw();
  }
  // pause when tab hidden
  document.addEventListener("visibilitychange", () => {
    if (document.hidden && raf) { cancelAnimationFrame(raf); raf = null; }
    else if (!document.hidden && !reduce && !raf) { draw(); }
  });
}

/* Footer year */
function initYear() {
  const y = document.querySelector("[data-year]");
  if (y) y.textContent = new Date().getFullYear();
}

document.addEventListener("DOMContentLoaded", () => {
  initTheme();
  initNav();
  initReveal();
  initSpotlight();
  initStars();
  initYear();
});

// expose for the downloads page (classic script, no modules)
window.AlurtmeeThemes = THEMES;
