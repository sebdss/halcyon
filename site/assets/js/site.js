/* Halcyon — interactions du site. Aucune dépendance. */
(() => {
  "use strict";
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
  const cfg = window.HALCYON_SITE || {};

  /* ---------- Langue de la page : <html lang="fr"> ou <html lang="en"> ---------- */
  const lang = (document.documentElement.lang || "fr").slice(0, 2) === "en" ? "en" : "fr";
  const STR = {
    fr: {
      openMenu: "Ouvrir le menu", closeMenu: "Fermer le menu",
      shareText: "Halcyon, launcher de jeux pour Windows et Linux", copied: "Lien copié", sendLink: "Envoyer le lien",
      size: (mo) => (mo >= 100 ? Math.round(mo) : mo.toFixed(1)).toString().replace(".", ",") + " Mo",
      locale: "fr-FR", prerelease: "Préversion", published: (d) => "Publiée le " + d, notes: "Notes de version",
      yourSystem: "Votre système", downloadFor: (os) => "Télécharger pour " + os,
      unavailableTitle: "Versions publiées sur GitHub", unavailableText: "La dernière version n'a pas pu être récupérée depuis cette page.",
      filmReplay: "Revoir le film"
    },
    en: {
      openMenu: "Open menu", closeMenu: "Close menu",
      shareText: "Halcyon, a game launcher for Windows and Linux", copied: "Link copied", sendLink: "Send the link",
      size: (mo) => (mo >= 100 ? Math.round(mo) : mo.toFixed(1)) + " MB",
      locale: "en-US", prerelease: "Pre-release", published: (d) => "Released " + d, notes: "Release notes",
      yourSystem: "Your system", downloadFor: (os) => "Download for " + os,
      unavailableTitle: "Releases on GitHub", unavailableText: "The latest release couldn't be loaded from this page.",
      filmReplay: "Watch again"
    }
  }[lang];
  // Textes de config.js : chaîne simple, ou { fr: "…", en: "…" }.
  const L = (v) => (v && typeof v === "object" ? v[lang] || v.fr : v);

  /* ---------- Navigation : état au défilement, section active, perche ---------- */
  const nav = $("#nav");
  const perch = $(".nav__perch");
  const onScroll = () => nav.classList.toggle("is-scrolled", window.scrollY > 8);
  // La marque de la barre prend le relais de celle de l'accueil quand celle-ci passe sous la barre.
  const heroBrand = $("[data-hero-brand]");
  if (heroBrand) {
    new IntersectionObserver(([e]) => nav.classList.toggle("at-top", e.isIntersecting), { rootMargin: "-64px 0px 0px 0px" }).observe(heroBrand);
  } else nav.classList.remove("at-top");
  onScroll();
  window.addEventListener("scroll", onScroll, { passive: true });

  // Sections sans entrée dans la barre : rattachées à l'entrée indiquée par data-nav-parent.
  const parentOf = {};
  $$("main > section[data-nav-parent]").forEach((s) => (parentOf[s.id] = s.dataset.navParent));
  const navLists = [$(".nav__links"), $(".sheet__links")].filter(Boolean);

  function placePerch() {
    const active = $(".nav__links a[aria-current='true']");
    if (!active || !perch) { perch && perch.classList.remove("is-on"); return; }
    perch.style.setProperty("--x", active.offsetLeft + 12 + "px");
    perch.style.setProperty("--w", active.offsetWidth - 24 + "px");
    perch.classList.add("is-on");
  }

  function setCurrent(id) {
    navLists.forEach((list) => {
      const links = $$("a[data-spy]", list);
      const target = links.find((a) => a.dataset.spy === id) || links.find((a) => a.dataset.spy === parentOf[id]);
      links.forEach((a) => a.setAttribute("aria-current", a === target ? "true" : "false"));
    });
    placePerch();
  }

  const sections = $$("main > section[id]");
  const visible = new Map();
  const spy = new IntersectionObserver((entries) => {
    entries.forEach((e) => visible.set(e.target.id, e.isIntersecting ? e.intersectionRect.height : 0));
    let best = null, max = 0;
    visible.forEach((h, id) => { if (h > max) { max = h; best = id; } });
    if (best) setCurrent(best);
  }, { rootMargin: "-64px 0px -35% 0px", threshold: [0, .1, .25, .5, .75, 1] });
  sections.forEach((s) => spy.observe(s));
  window.addEventListener("resize", placePerch);
  document.fonts && document.fonts.ready.then(placePerch);

  /* ---------- Menu mobile ---------- */
  const sheet = $("#menu");
  const toggle = $(".nav__toggle");
  let lastFocus = null;

  function openMenu() {
    lastFocus = document.activeElement;
    sheet.hidden = false;
    requestAnimationFrame(() => {
      sheet.classList.add("is-open");
      nav.classList.add("is-open");
      requestAnimationFrame(() => {
        const first = $("a[aria-current='true']", sheet) || $("a", sheet);
        first && first.focus({ preventScroll: true });
      });
    });
    toggle.setAttribute("aria-expanded", "true");
    toggle.setAttribute("aria-label", STR.closeMenu);
    document.body.style.overflow = "hidden";
  }
  function closeMenu(restore = true) {
    if (sheet.hidden) return;
    sheet.classList.remove("is-open");
    nav.classList.remove("is-open");
    toggle.setAttribute("aria-expanded", "false");
    toggle.setAttribute("aria-label", STR.openMenu);
    document.body.style.overflow = "";
    const done = () => { if (!sheet.classList.contains("is-open")) sheet.hidden = true; };
    reduced.matches ? done() : setTimeout(done, 200);
    if (restore && lastFocus) lastFocus.focus({ preventScroll: true });
  }
  toggle.addEventListener("click", () => (sheet.hidden ? openMenu() : closeMenu()));
  $$("a", sheet).forEach((a) => a.addEventListener("click", () => closeMenu(false)));
  document.addEventListener("keydown", (e) => {
    if (sheet.hidden) return;
    if (e.key === "Escape") { closeMenu(); return; }
    if (e.key === "Tab") {
      // Focus maintenu entre le bouton du menu et la feuille.
      const items = [toggle, ...$$("a, button", sheet)];
      const i = items.indexOf(document.activeElement);
      if (e.shiftKey && i <= 0) { e.preventDefault(); items[items.length - 1].focus(); }
      else if (!e.shiftKey && i === items.length - 1) { e.preventDefault(); items[0].focus(); }
    }
  });
  window.matchMedia("(min-width: 961px)").addEventListener("change", (m) => m.matches && closeMenu(false));

  /* ---------- Apparition progressive ---------- */
  const reveal = new IntersectionObserver((entries) => {
    entries.forEach((e) => {
      if (!e.isIntersecting) return;
      e.target.classList.add("is-in");
      reveal.unobserve(e.target);
    });
  }, { rootMargin: "0px 0px -8% 0px", threshold: .08 });
  $$(".reveal, .instrument").forEach((el) => reveal.observe(el));

  /* ---------- Onglets accessibles (outils) ---------- */
  function tabs(list, { onSelect } = {}) {
    const items = $$("[role='tab']", list);
    function select(tab, focus) {
      items.forEach((t) => {
        const on = t === tab;
        t.setAttribute("aria-selected", on);
        t.tabIndex = on ? 0 : -1;
        const panel = document.getElementById(t.getAttribute("aria-controls"));
        if (!panel) return;
        if (on && panel.hidden) {
          panel.hidden = false;
          panel.classList.remove("is-entering");
          void panel.offsetWidth;
          panel.classList.add("is-entering");
        } else if (!on) panel.hidden = true;
      });
      if (focus) tab.focus();
      if (list.scrollWidth > list.clientWidth) {
        tab.scrollIntoView({ block: "nearest", inline: "nearest", behavior: reduced.matches ? "auto" : "smooth" });
      }
      onSelect && onSelect(items.indexOf(tab));
    }
    items.forEach((t) => t.addEventListener("click", () => select(t, false)));
    list.addEventListener("keydown", (e) => {
      const i = items.indexOf(document.activeElement);
      if (i < 0) return;
      const next = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
      if (next) { e.preventDefault(); select(items[(i + next + items.length) % items.length], true); }
      if (e.key === "Home") { e.preventDefault(); select(items[0], true); }
      if (e.key === "End") { e.preventDefault(); select(items[items.length - 1], true); }
    });
    return { select: (i, focus) => select(items[i], focus), items };
  }
  const toolsList = $(".tools__index");
  if (toolsList) {
    tabs(toolsList);
    const setOrientation = (m) => toolsList.setAttribute("aria-orientation", m.matches ? "horizontal" : "vertical");
    const mq = window.matchMedia("(max-width: 960px)");
    setOrientation(mq);
    mq.addEventListener("change", setOrientation);
  }

  /* ---------- Galerie : piste à défilement aimanté, onglets synchronisés ---------- */
  const track = $("[data-g-track]");
  if (track) {
    const items = $$(".gallery__item", track);
    const tabList = $(".gallery__tabs");
    const tabEls = $$("[role='tab']", tabList);
    const idx = $("[data-g-index]");
    const prev = $("[data-g-prev]");
    const next = $("[data-g-next]");
    let current = 0;
    $("[data-g-total]").textContent = items.length;

    function mark(i) {
      current = i;
      tabEls.forEach((t, k) => { t.setAttribute("aria-selected", k === i); t.tabIndex = k === i ? 0 : -1; });
      items.forEach((it, k) => it.setAttribute("aria-hidden", k === i ? "false" : "true"));
      idx.textContent = i + 1;
      prev.disabled = i === 0;
      next.disabled = i === items.length - 1;
      const t = tabEls[i];
      if (t && tabList.scrollWidth > tabList.clientWidth) {
        const left = t.offsetLeft - tabList.clientWidth / 2 + t.offsetWidth / 2;
        tabList.scrollTo({ left, behavior: reduced.matches ? "auto" : "smooth" });
      }
    }
    function go(i, focusTab) {
      i = Math.max(0, Math.min(items.length - 1, i));
      track.scrollTo({ left: items[i].offsetLeft - track.offsetLeft, behavior: reduced.matches ? "auto" : "smooth" });
      mark(i);
      if (focusTab) tabEls[i].focus();
    }
    tabEls.forEach((t, i) => t.addEventListener("click", () => go(i)));
    tabList.addEventListener("keydown", (e) => {
      const d = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
      if (d) { e.preventDefault(); go(current + d, true); }
      if (e.key === "Home") { e.preventDefault(); go(0, true); }
      if (e.key === "End") { e.preventDefault(); go(items.length - 1, true); }
    });
    prev.addEventListener("click", () => go(current - 1));
    next.addEventListener("click", () => go(current + 1));
    // Balayage sur écran tactile : la piste défile seule ; l'onglet suit.
    let raf = 0;
    track.addEventListener("scroll", () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const i = Math.round(track.scrollLeft / (track.clientWidth + parseFloat(getComputedStyle(track).columnGap || 0)));
        if (i !== current && items[i]) mark(i);
      });
    }, { passive: true });
    mark(0);
  }

  /* ---------- Le film : rien n'est chargé avant le clic ; pause quand il quitte l'écran ---------- */
  // Chaque page référence le film de sa langue (fichiers -fr ou -en) ; le téléphone reçoit la
  // version 720p par <source media>. Sans JavaScript, la vidéo garde ses commandes natives.
  const film = $("[data-film]");
  if (film) {
    const video = $("video", film);
    const play = $("[data-film-play]", film);
    const label = $("[data-film-label]", film);
    video.controls = false;
    play.hidden = false;
    const start = () => {
      film.dataset.state = "playing";
      video.controls = true;
      const p = video.play();
      if (p && p.catch) p.catch(() => { film.dataset.state = ""; video.controls = false; });
    };
    play.addEventListener("click", start);
    video.addEventListener("ended", () => {
      film.dataset.state = "ended";
      video.controls = false;
      if (label) label.textContent = STR.filmReplay;
      play.setAttribute("aria-label", STR.filmReplay);
      play.focus({ preventScroll: true });
    });
    // Liens « Voir le film » : défilement jusqu'au film, lecture dans le même geste.
    $$("[data-film-start]").forEach((a) => a.addEventListener("click", (e) => {
      e.preventDefault();
      film.scrollIntoView({ behavior: reduced.matches ? "auto" : "smooth", block: "center" });
      start();
    }));
    // Pause quand le film sort de l'écran (seulement après y être entré : un lancement
    // depuis l'accueil commence pendant le défilement).
    let seen = false;
    new IntersectionObserver(([e]) => {
      if (e.intersectionRatio >= 0.25) { seen = true; return; }
      if (seen && !video.paused) video.pause();
      seen = false;
    }, { threshold: [0, 0.25, 0.5] }).observe(film);
  }

  /* ---------- Partager le lien (téléphone) ---------- */
  const share = $("[data-share]");
  if (share) {
    share.addEventListener("click", async () => {
      const url = location.href.split("#")[0];
      try {
        if (navigator.share) { await navigator.share({ title: "Halcyon", text: STR.shareText, url }); return; }
        await navigator.clipboard.writeText(url);
        share.textContent = STR.copied;
        setTimeout(() => (share.textContent = STR.sendLink), 2400);
      } catch (_) { /* partage annulé */ }
    });
  }

  /* ---------- Téléchargement : dernière version publiée sur GitHub ---------- */
  const releaseBox = $("[data-release]");
  if (!releaseBox || !cfg.repo) return;

  const detectOS = () => {
    const ua = navigator.userAgent || "";
    const p = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/Android|iPhone|iPad|iPod/i.test(ua)) return null;
    if (/Win/i.test(p) || /Windows/i.test(ua)) return "windows";
    if (/Linux/i.test(p) || /Linux|X11/i.test(ua)) return "linux";
    return null;
  };
  const userOS = detectOS();

  const fmtSize = (b) => (b ? STR.size(b / 1e6) : "");
  const fmtDate = (s) => new Date(s).toLocaleDateString(STR.locale, { day: "numeric", month: "long", year: "numeric" });
  const icon = (id, cls = "icon") => `<svg class="${cls}" aria-hidden="true"><use href="#i-${id}"/></svg>`;
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  // Adresses venues de l'API GitHub : HTTPS uniquement (jamais « javascript: » ni autre schéma).
  const safeUrl = (u) => (/^https:\/\//.test(String(u)) ? String(u) : "#");

  // Comparaison de versions « 1.0.0-alpha.5 » : numéros, puis préversion (sans préversion = plus récent).
  function versionAtLeast(v, min) {
    const parse = (x) => {
      const [core, pre] = String(x).replace(/^v/, "").split("-");
      return { n: core.split(".").map(Number), pre: pre ? pre.split(".").map((p) => (isNaN(p) ? p : Number(p))) : null };
    };
    const a = parse(v), b = parse(min);
    for (let i = 0; i < 3; i++) if ((a.n[i] || 0) !== (b.n[i] || 0)) return (a.n[i] || 0) > (b.n[i] || 0);
    if (!a.pre || !b.pre) return !a.pre || !!b.pre === !!a.pre;
    for (let i = 0; i < Math.max(a.pre.length, b.pre.length); i++) {
      const x = a.pre[i], y = b.pre[i];
      if (x === undefined) return false;
      if (y === undefined) return true;
      if (x !== y) return typeof x === typeof y ? x > y : typeof x === "number";
    }
    return true;
  }

  function pick(release, os) {
    const out = [];
    (cfg.assets[os] || []).forEach((spec) => {
      const a = release.assets.find((x) => spec.match.test(x.name));
      if (a) out.push({ ...spec, asset: a });
    });
    return out;
  }

  function render(release) {
    const pre = release.prerelease;
    const name = (release.tag_name || "").replace(/^v/, "");
    releaseBox.dataset.state = "ready";
    $("[data-release-version]", releaseBox).textContent = "Halcyon " + name;
    $("[data-release-meta]", releaseBox).innerHTML =
      (pre ? `<span class="tag tag--warm">${STR.prerelease}</span> ` : "") +
      esc(STR.published(fmtDate(release.published_at))) +
      ` · <a class="link" href="${esc(safeUrl(release.html_url))}" rel="noopener">${STR.notes}</a>`;
    $("[data-prerelease-note]").hidden = !pre;
    // Note « interface en anglais à partir de… » : masquée dès que la version publiée l'inclut.
    const langNote = $("[data-lang-note]");
    if (langNote && cfg.translatedFrom) langNote.hidden = versionAtLeast(name, cfg.translatedFrom);

    const foot = $("[data-footer-version]");
    if (foot) { foot.textContent = "Halcyon " + name + " · "; foot.hidden = false; }

    const sums = release.assets.find((a) => cfg.checksums && cfg.checksums.test(a.name));
    const sumsLink = $("[data-checksums]");
    if (sums && sumsLink) { sumsLink.href = safeUrl(sums.browser_download_url); sumsLink.hidden = false; }

    ["windows", "linux"].forEach((os) => {
      const col = $(`.os[data-os="${os}"]`);
      const files = pick(release, os);
      if (!col || !files.length) return;
      const [main, ...rest] = files;
      const isUser = userOS === os;
      col.classList.toggle("is-detected", isUser);
      $("[data-os-hint]", col).textContent = isUser ? STR.yourSystem : "";
      const label = os === "windows" ? "Windows" : "Linux";
      $("[data-os-primary]", col).innerHTML =
        `<a class="btn ${isUser ? "btn--warm" : "btn--line"} btn--big" href="${esc(safeUrl(main.asset.browser_download_url))}" download>` +
        `${icon("download")} ${esc(STR.downloadFor(label))}</a>` +
        `<span class="os__file">${esc(L(main.label))} · <code>${esc(main.asset.name)}</code> · ${esc(fmtSize(main.asset.size))}</span>`;
      $("[data-os-files]", col).innerHTML = rest.map((f) =>
        `<li><a href="${esc(safeUrl(f.asset.browser_download_url))}" download>` +
        `<b>${esc(L(f.label))}</b><small>${esc(L(f.note))}</small>` +
        `<span class="size">${esc(fmtSize(f.asset.size))}</span>${icon("download")}</a></li>`).join("");
      const footLink = $(`[data-dl-link="${os}"]`);
      if (footLink) footLink.href = safeUrl(main.asset.browser_download_url);
    });
  }

  function unavailable() {
    releaseBox.dataset.state = "error";
    $("[data-release-version]", releaseBox).textContent = STR.unavailableTitle;
    $("[data-release-meta]", releaseBox).textContent = STR.unavailableText;
  }

  const KEY = "halcyon-release:" + cfg.repo;
  const fromCache = () => {
    try {
      const c = JSON.parse(sessionStorage.getItem(KEY) || "null");
      return c && Date.now() - c.t < 10 * 60 * 1000 ? c.r : null;
    } catch (_) { return null; }
  };
  const cached = fromCache();
  if (cached) { render(cached); return; }

  fetch(`https://api.github.com/repos/${cfg.repo}/releases?per_page=20`, { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((list) => {
      // Dernière version publiée qui contient réellement des fichiers pour Windows ou Linux.
      const release = list.find((r) => !r.draft && (cfg.includePrereleases || !r.prerelease) &&
        (pick(r, "windows").length || pick(r, "linux").length));
      if (!release) return unavailable();
      const slim = {
        tag_name: release.tag_name, html_url: release.html_url, prerelease: release.prerelease, published_at: release.published_at,
        assets: release.assets.map((a) => ({ name: a.name, size: a.size, browser_download_url: a.browser_download_url }))
      };
      try { sessionStorage.setItem(KEY, JSON.stringify({ t: Date.now(), r: slim })); } catch (_) {}
      render(slim);
    })
    .catch(unavailable);
})();
