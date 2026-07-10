// claude-token-display — DOM logic and SSE client.

(() => {
  const sessions = new Map(); // session_id → snapshot
  const lastUpdate = new Map(); // session_id → epoch_ms of last received snapshot
  const sessionsEl = document.getElementById("sessions");
  const emptyEl = document.getElementById("empty");
  const loadingEl = document.getElementById("loading");
  const loadingMsgEl = document.getElementById("loading-msg");
  const countEl = document.getElementById("count");
  const cwdEl = document.getElementById("cwd");
  const connEl = document.getElementById("conn");
  const rowTpl = document.getElementById("row-template");
  const workerToggle = document.getElementById("show-workers");
  const emptyToggle = document.getElementById("show-empty");
  const sortSelect = document.getElementById("sort-mode");

  // Card order, persisted as display.sort in the server settings:
  // "usage" (most-compacted/fullest first), "dir" (by project path),
  // "start" (by session start time, oldest first).
  let sortMode = "usage";

  // Connection / freshness state. Until the SSE stream is open and has had a
  // brief moment to deliver the initial session dump, we show a neutral
  // "connecting / loading" placeholder instead of the misleading "no sessions"
  // empty state. Data older than STALE_AFTER_MS is flagged as not-yet-refreshed.
  let sseConnected = false;
  let connectedAt = 0;
  const SETTLE_MS = 1800;        // grace after connect before declaring "empty"
  // No file write for this long → flag the card as "wartet". Generous on
  // purpose: an agent mid-step (long tool run, long generation, sub-agents) can
  // legitimately go minutes between writes, and the ⌚ timer already shows the
  // exact age — so this only flags clearly parked sessions, not busy ones.
  const STALE_AFTER_MS = 600_000; // 10 min

  // Workers are subagent transcripts (<session>/subagents/agent-*.jsonl):
  // background agents a session spawned via the Agent tool or workflows. They
  // burn tokens in their own context window but are no interactive chats, so
  // the dashboard hides them by default; the "Worker" toggle shows them.
  let showWorkers = false;
  const isWorker = (snap) => !!snap.is_worker;

  // "Empty" sessions are those without a usable context window — e.g. a brand
  // new session that has only emitted synthetic events, so no model/window is
  // known yet. The dashboard hides them by default (no usable bar) but the
  // "Leere" toggle in the topbar can show them.
  let showEmpty = false;
  const isEmpty = (snap) =>
    !isWorker(snap) && (!snap.context_window || snap.context_window <= 0);

  // Empty sessions don't report a context window. Borrow it from any real
  // session if one is around (same model → same limit), else fall back.
  const ASSUMED_CONTEXT_WINDOW = 1_000_000; // Claude Code default (Opus/Sonnet 4.x)
  const effectiveContextWindow = (snap) => {
    if (snap.context_window && snap.context_window > 0) return snap.context_window;
    for (const s of sessions.values()) {
      if (s.context_window && s.context_window > 0) return s.context_window;
    }
    return ASSUMED_CONTEXT_WINDOW;
  };

  // -- formatting helpers --

  const fmtTokens = (n) => {
    if (n == null) return "—";
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
    if (n >= 1_000) return (n / 1_000).toFixed(0) + "k";
    return String(n);
  };

  const fmtIdle = (sec) => {
    if (sec == null) return "";
    if (sec < 60) return `${sec}s`;
    if (sec < 3600) {
      const m = Math.floor(sec / 60);
      const s = sec % 60;
      return s > 0 ? `${m}:${String(s).padStart(2, "0")}` : `${m}m`;
    }
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    return `${h}:${String(m).padStart(2, "0")}h`;
  };

  // True data age: prefer the session file's mtime (when it was actually
  // written) over the snapshot arrival time, so a reconnect doesn't reset the
  // age of long-idle sessions to zero — that is exactly the case where the
  // shown numbers are not live yet (e.g. no prompt run in this session).
  function dataAgeSec(snap) {
    const baseMs = snap && snap.last_modified
      ? snap.last_modified * 1000
      : (lastUpdate.get(snap?.session_id) ?? Date.now());
    return Math.max(0, Math.floor((Date.now() - baseMs) / 1000));
  }

  function applyAge(rowEl, snap) {
    const ageSec = dataAgeSec(snap);
    const stale = ageSec * 1000 > STALE_AFTER_MS;
    const idleEl = rowEl.querySelector(".idle");
    // All bars stay equally bright; staleness is shown only here on the timer
    // (yellow + an explicit "wartet" tag) so the bar can't be mistaken for a
    // contrast glitch. "wartet" reads as "alive but waiting for input" — it does
    // not contradict the green "running" status dot the way "inaktiv" would.
    idleEl.textContent = stale
      ? `⌚ ${fmtIdle(ageSec)} · wartet`
      : `⌚ ${fmtIdle(ageSec)}`;
    rowEl.classList.toggle("session--stale", stale);
    idleEl.title = stale
      ? `Seit ${fmtIdle(ageSec)} kein neuer Turn — die Session wartet auf Eingabe; ` +
        `die Werte sind noch nicht live (z. B. seit Start noch kein Prompt).`
      : "Seit letzter Aktivität";
  }

  // -- rendering --

  function ensureRow(sid) {
    let el = sessionsEl.querySelector(`[data-sid="${sid}"]`);
    if (el) return el;
    const frag = rowTpl.content.cloneNode(true);
    el = frag.querySelector(".session");
    el.dataset.sid = sid;
    wireButtons(el);
    sessionsEl.appendChild(el);
    return el;
  }

  function wireButtons(rowEl) {
    rowEl.querySelector(".sid").addEventListener("click", () => {
      const sid = rowEl.dataset.sid;
      postJson("/copy", { text: sid })
        .then(() => toast(`Session-ID kopiert: ${shortId(sid)}…`))
        .catch(() => toast("Kopieren fehlgeschlagen", true));
    });
    rowEl.querySelector(".btn--open").addEventListener("click", () => {
      const snap = sessions.get(rowEl.dataset.sid);
      if (!snap?.session_cwd) return;
      postJson("/open-dir", { path: snap.session_cwd, sid: rowEl.dataset.sid })
        .then((r) => toast(r.focused ? "Dateimanager-Fenster nach vorne geholt" : "Verzeichnis geöffnet"))
        .catch(() => toast("Konnte Verzeichnis nicht öffnen", true));
    });
    rowEl.querySelector(".btn--focus").addEventListener("click", () => {
      const snap = sessions.get(rowEl.dataset.sid);
      if (!snap?.session_cwd) return;
      postJson("/focus-terminal", { cwd: snap.session_cwd, sid: rowEl.dataset.sid })
        .then((r) => toast(r.warning ? r.warning : "Terminal fokussiert"))
        .catch(() => toast("Konnte Fenster nicht fokussieren", true));
    });
    rowEl.querySelector(".btn--path").addEventListener("click", () => {
      const snap = sessions.get(rowEl.dataset.sid);
      const p = snap?.session_path;
      if (!p) {
        toast("Pfad noch nicht aufgelöst", true);
        return;
      }
      postJson("/copy", { text: p })
        .then(() => toast(`Pfad kopiert: …/${p.split("/").slice(-2).join("/")}`))
        .catch(() => toast("Kopieren fehlgeschlagen", true));
    });
    rowEl.querySelector(".btn--rollover").addEventListener("click", () => {
      const snap = sessions.get(rowEl.dataset.sid);
      postJson("/copy", { text: rolloverPrompt(snap) })
        .then(() => toast("Rollover-Prompt kopiert (in Claude einfügen)"))
        .catch(() => toast("Kopieren fehlgeschlagen", true));
    });
  }

  function shortId(sid) {
    return sid.split("-").slice(0, 2).join("-");
  }

  function rolloverPrompt(snap) {
    return `Bitte fasse den aktuellen Stand kompakt in HANDOVER.md zusammen:

1. Was wurde in der bisherigen Session erledigt? (Stichpunkte, max. 10)
2. Welche Tests/Builds laufen aktuell grün/rot?
3. Was sind die offenen TODOs aus dem aktiven Plan?
4. Kontextspezifische Snippets: aktuelle Branch, letzter Commit-Hash,
   geänderte Dateien.

Halte HANDOVER.md unter 200 Zeilen. Beende dann den aktuellen Turn,
damit eine neue Session mit HANDOVER.md als Kontext starten kann.

(Session ${snap?.session_id ?? "—"} · ${snap?.percent_used ?? "?"}% Kontext verbraucht)`;
  }

  function renderRow(rowEl, snap) {
    const worker = isWorker(snap);
    const empty = isEmpty(snap);
    const ctx = effectiveContextWindow(snap);
    const used = snap.tokens_in_context ?? snap.session_total_tokens ?? 0;
    const free = Math.max(0, ctx - used);
    const pctUsed = ctx > 0 ? Math.min(100, Math.round((used / ctx) * 100)) : 0;
    const pctLeft = 100 - pctUsed;

    rowEl.classList.toggle("session--worker", worker);
    rowEl.classList.toggle("session--empty", empty);

    const badge = rowEl.querySelector(".worker-badge");
    badge.classList.toggle("hidden", !worker);
    if (worker) {
      badge.title = snap.parent_session_id
        ? `Hintergrund-Worker (Subagent) der Session ${snap.parent_session_id}`
        : "Hintergrund-Worker (Subagent)";
    }
    // A worker is no resumable chat — the rollover prompt makes no sense there.
    rowEl.querySelector(".btn--rollover").classList.toggle("hidden", worker);

    const fill = rowEl.querySelector(".bar__fill");
    const bar = rowEl.querySelector(".bar");
    const labelEl = rowEl.querySelector(".bar__label");

    fill.style.clipPath = `inset(0 ${(100 - pctUsed).toFixed(2)}% 0 0)`;
    labelEl.innerHTML =
      `<span class="free"></span>&nbsp;/&nbsp;<span class="pctfree"></span>%&nbsp;frei`;
    labelEl.querySelector(".free").textContent = fmtTokens(free);
    labelEl.querySelector(".pctfree").textContent = pctLeft;

    bar.title = empty
      ? `Leere Session — Kontextfenster noch nicht gemeldet, angenommen ` +
        `${ctx.toLocaleString("de-DE")}.\n` +
        `${used.toLocaleString("de-DE")} Token belegt (${pctUsed}%)\n` +
        `${free.toLocaleString("de-DE")} Token frei (${pctLeft}%)`
      : `${free.toLocaleString("de-DE")} Token frei (${pctLeft}%)\n` +
        `${used.toLocaleString("de-DE")} Token verbraucht (${pctUsed}%)\n` +
        `${ctx.toLocaleString("de-DE")} Token Kontextfenster gesamt`;

    const cwdElRow = rowEl.querySelector(".cwd");
    cwdElRow.textContent = snap.session_cwd ?? "(unbekannt)";
    cwdElRow.title = snap.session_cwd ?? "";

    const statusEl = rowEl.querySelector(".status");
    statusEl.classList.remove("status--active", "status--closed", "status--unknown");
    if (snap.session_active === true) {
      statusEl.classList.add("status--active");
      statusEl.title = worker
        ? "Worker läuft gerade (Transkript wird geschrieben)"
        : "Claude-Prozess läuft in diesem Projekt";
    } else if (snap.session_active === false) {
      statusEl.classList.add("status--closed");
      statusEl.title = worker
        ? "Worker fertig — letzter Stand eingefroren"
        : "Claude-Session beendet — letzter Stand eingefroren";
    } else {
      statusEl.classList.add("status--unknown");
      statusEl.title = "Status unbekannt";
    }

    applyAge(rowEl, snap);

    const sidEl = rowEl.querySelector(".sid");
    sidEl.textContent = shortId(snap.session_id ?? "");
    sidEl.title = `${snap.session_id}\n(klicken zum Kopieren)`;

    rowEl.querySelector(".usage").textContent =
      `${fmtTokens(used)} / ${fmtTokens(ctx)}`;
    rowEl.querySelector(".cumulative").textContent =
      `Σ ${fmtTokens(snap.session_total_tokens)}`;

    const cc = snap.compact_count ?? 0;
    const compactEl = rowEl.querySelector(".compact");
    if (cc > 0) {
      compactEl.querySelector(".compact-n").textContent = cc;
      const total = snap.session_total_tokens ?? 0;
      compactEl.title =
        `Kontext wurde ${cc}× zusammengefasst ` +
        `(Auto-Compact am Limit oder /compact).\n` +
        `Σ ${total.toLocaleString("de-DE")} Token sind die echte ` +
        `Gesamtsumme des Chats inklusive aller Compaction-Vorgänge.`;
      compactEl.classList.remove("hidden");
    } else {
      compactEl.classList.add("hidden");
    }
  }

  function applySnapshot(snap) {
    if (!snap.session_id) return;
    sessions.set(snap.session_id, snap);
    lastUpdate.set(snap.session_id, Date.now());
    renderUI();
  }

  function renderUI() {
    // 1. Drop closed sessions completely.
    for (const [sid, snap] of sessions) {
      if (snap.session_active === false) {
        const row = sessionsEl.querySelector(`[data-sid="${sid}"]`);
        if (row) row.remove();
        sessions.delete(sid);
        lastUpdate.delete(sid);
      }
    }

    // 2. Visible set based on the worker and empty-session toggles.
    const visible = [...sessions.values()].filter(
      (s) => (showWorkers || !isWorker(s)) && (showEmpty || !isEmpty(s))
    );

    // 3. Remove DOM rows that are no longer visible.
    const visibleIds = new Set(visible.map((s) => s.session_id));
    sessionsEl.querySelectorAll(".session").forEach((row) => {
      if (!visibleIds.has(row.dataset.sid)) row.remove();
    });

    // 4. Sort order: always real sessions first, then workers, then empty
    //    sessions. Within the groups the selected mode applies:
    //    - usage (default): most-compacted first (each compact costs a whole
    //      turn of tokens, so it's a 'cost so far' proxy), tiebreaker bar fill;
    //      workers/empty by cumulative consumption.
    //    - dir: by project path; start: by session start (oldest first).
    //    Usage order stays the final tiebreaker for the other modes.
    const rank = (s) => (isEmpty(s) ? 2 : isWorker(s) ? 1 : 0);
    const cmpUsage = (a, b) => {
      if (rank(a) === 0) {
        const cc = (b.compact_count ?? 0) - (a.compact_count ?? 0);
        if (cc !== 0) return cc;
        return (b.percent_used ?? 0) - (a.percent_used ?? 0);
      }
      return (b.session_total_tokens ?? 0) - (a.session_total_tokens ?? 0);
    };
    visible.sort((a, b) => {
      const r = rank(a) - rank(b);
      if (r !== 0) return r;
      if (sortMode === "dir") {
        const d = (a.session_cwd ?? "").localeCompare(b.session_cwd ?? "", "de");
        if (d !== 0) return d;
      } else if (sortMode === "start") {
        // ISO-8601 timestamps compare correctly as strings; unknown start
        // times sink to the bottom of their group.
        const sa = a.started_at ?? "￿";
        const sb = b.started_at ?? "￿";
        if (sa !== sb) return sa < sb ? -1 : 1;
      }
      return cmpUsage(a, b);
    });

    for (const snap of visible) {
      const row = ensureRow(snap.session_id);
      renderRow(row, snap);
      sessionsEl.appendChild(row);
    }

    // 5. Count label: visible groups join with "·", hidden ones show as "(+N …)".
    const all = [...sessions.values()];
    const workerCount = all.filter(isWorker).length;
    const emptyCount = all.filter(isEmpty).length;
    const realCount = all.length - workerCount - emptyCount;
    let label = `${realCount} aktiv`;
    if (workerCount > 0) {
      label += showWorkers ? ` · ${workerCount} Worker` : ` (+${workerCount} Worker)`;
    }
    if (emptyCount > 0) {
      label += showEmpty ? ` · ${emptyCount} leer` : ` (+${emptyCount} leer)`;
    }
    countEl.textContent = label;
    // Three placeholder states, so the user can tell "still loading" apart
    // from a genuine "no sessions": connecting → loading → empty.
    const connecting = !sseConnected;
    const settling =
      sseConnected && visible.length === 0 && Date.now() - connectedAt < SETTLE_MS;
    if (connecting || settling) {
      loadingMsgEl.textContent = connecting
        ? "Verbinde mit dem Server …"
        : "Suche aktive Claude-Sessions …";
      loadingEl.classList.remove("hidden");
      loadingEl.classList.add("loading-pulse");
      emptyEl.classList.add("hidden");
    } else {
      loadingEl.classList.add("hidden");
      loadingEl.classList.remove("loading-pulse");
      emptyEl.classList.toggle("hidden", visible.length > 0);
    }

    if (scope !== null && visible.length > 0) {
      cwdEl.textContent = scope;
      cwdEl.title = scope;
    }
  }

  if (workerToggle) {
    workerToggle.addEventListener("change", () => {
      showWorkers = workerToggle.checked;
      renderUI();
    });
  }
  if (emptyToggle) {
    emptyToggle.addEventListener("change", () => {
      showEmpty = emptyToggle.checked;
      renderUI();
    });
  }
  if (sortSelect) {
    sortSelect.addEventListener("change", () => {
      sortMode = sortSelect.value;
      renderUI();
      fetch("/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ display: { sort: sortMode } }),
      }).catch(() => {}); // Sortierung funktioniert auch ohne Persistenz
    });
  }

  // Server tells us whether we're scoped to a single project or system-wide.
  let scope = null;
  async function initScope() {
    try {
      const r = await fetch("/scope");
      const data = await r.json();
      scope = data.scope ?? null;
      if (scope === null) {
        cwdEl.textContent = "Alle Projekte";
        cwdEl.title = "PC-weite Übersicht aller laufenden Claude-Sessions";
      } else {
        cwdEl.textContent = scope;
        cwdEl.title = scope;
      }
    } catch {
      cwdEl.textContent = "?";
    }
  }

  // Refresh age timers (and the stale flag) every second.
  setInterval(() => {
    for (const [sid, snap] of sessions) {
      const row = sessionsEl.querySelector(`[data-sid="${sid}"]`);
      if (!row) continue;
      applyAge(row, snap);
    }
  }, 1000);

  // -- SSE connection --

  function connect() {
    const es = new EventSource("/events");
    es.onopen = () => {
      connEl.classList.remove("conn--disconnected");
      connEl.classList.add("conn--connected");
      connEl.title = "Mit dem Hilfs-Server verbunden";
      sseConnected = true;
      connectedAt = Date.now();
      renderUI();
      // Re-render once the settle window elapses, to flip a still-empty
      // dashboard from "Suche …" to the real "Keine aktiven Sessions".
      setTimeout(renderUI, SETTLE_MS + 100);
    };
    es.onmessage = (ev) => {
      try {
        const event = JSON.parse(ev.data);
        if (event.type === "snapshot") applySnapshot(event.data);
      } catch {
        /* ignore */
      }
    };
    es.onerror = () => {
      connEl.classList.remove("conn--connected");
      connEl.classList.add("conn--disconnected");
      connEl.title = "Keine Verbindung — Server gestoppt?";
      sseConnected = false;
      renderUI();
    };
  }

  // -- POST helper --

  async function postJson(path, payload) {
    const r = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!r.ok) throw new Error(`${r.status}`);
    const txt = await r.text();
    return txt ? JSON.parse(txt) : {};
  }

  // -- toast --

  let toastTimer;
  function toast(msg, isErr) {
    let el = document.querySelector(".toast");
    if (!el) {
      el = document.createElement("div");
      el.className = "toast";
      document.body.appendChild(el);
    }
    el.textContent = msg;
    el.style.borderColor = isErr ? "var(--red)" : "var(--border)";
    el.classList.add("toast--visible");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => el.classList.remove("toast--visible"), 2500);
  }

  // -- help window --

  let helpWin = null;
  function openHelp() {
    if (helpWin && !helpWin.closed) {
      helpWin.focus();
      return;
    }
    helpWin = window.open(
      "/help",
      "claude-token-help",
      "popup=yes,width=760,height=820,resizable=yes,scrollbars=yes"
    );
    if (helpWin) helpWin.focus();
  }

  // -- recent-chats window --

  let chatsWin = null;
  function openChats() {
    if (chatsWin && !chatsWin.closed) {
      chatsWin.focus();
      return;
    }
    chatsWin = window.open(
      "/chats",
      "claude-token-chats",
      "popup=yes,width=860,height=720,resizable=yes,scrollbars=yes"
    );
    if (chatsWin) chatsWin.focus();
  }

  document.addEventListener("click", (e) => {
    if (e.target.closest("#help-btn")) {
      e.preventDefault();
      openHelp();
    } else if (e.target.closest("#chats-btn")) {
      e.preventDefault();
      openChats();
    }
  });

  initScope().then(connect);

  // Fenster auf allen Arbeitsflächen anzeigen (per wmctrl sticky; abschaltbar
  // über windows.sticky in ~/.config/claude-token-monitor/config.json)
  postJson("/sticky", { title: document.title }).catch(() => {});

  // ---------------------------------------------------------------------
  // Plan widget (account-level rate limits — opt-in)
  // ---------------------------------------------------------------------
  //
  // Data path: server.py /plan → spawns `claude-tokens plan` → calls
  // api.anthropic.com/api/oauth/usage with the OAuth token stored under
  // ~/.claude/.credentials.json. The server normalizes the response into:
  //   { plan_type, main:{five_hour,seven_day}, buckets:[…], extra_usage }
  // The buckets are per-feature 7-day windows (Opus, Sonnet, …) that only
  // appear when they actually apply to the account.
  //
  // Poll once a minute when enabled — the numbers only move on actual Claude
  // usage, so higher frequency would just burn the user's own rate budget.

  const PLAN_POLL_MS = 60_000;
  const PLAN_WIDGET_EL = document.getElementById("plan-widget");
  const PLAN_SETTINGS_EL = document.getElementById("plan-settings");
  const PLAN_SETTINGS_BTN = document.getElementById("plan-settings-btn");
  const PS_ROWS_DYNAMIC = document.getElementById("ps-rows-dynamic");

  // Rows that have a fixed checkbox in index.html. Everything else is a
  // dynamically discovered per-feature bucket.
  const STATIC_ROWS = ["main", "credits"];

  let planSettings = null;
  let planData = null;
  let planError = null;
  let planFetchTimer = null;
  let planRetryTimer = null;
  let planLoaded = false;   // have we ever received good plan data?
  let planAttempts = 0;     // consecutive failed attempts during initial load
  let planErrStreak = 0;    // consecutive errors after data was already shown
  const PLAN_MAX_INITIAL_ATTEMPTS = 3;
  const PLAN_INITIAL_RETRY_MS = 2500;
  const bucketLabels = {}; // key → human label (filled from planData.buckets)

  function bucketLabel(key) {
    return bucketLabels[key] || key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }

  async function loadPlanSettings() {
    try {
      const r = await fetch("/settings");
      planSettings = await r.json();
    } catch {
      planSettings = { plan_widget: { enabled: false, rows: { main: true, credits: false } } };
    }
    // The card-sort preference travels in the same settings file.
    sortMode = planSettings?.display?.sort ?? "usage";
    if (sortSelect) {
      sortSelect.value = sortMode;
      renderUI();
    }
    syncSettingsUI();
    if (planSettings.plan_widget.enabled) startPlanPolling();
  }

  async function patchPlanSettings(patch) {
    const r = await fetch("/settings", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ plan_widget: patch }),
    });
    const j = await r.json();
    if (j.settings) {
      planSettings = j.settings;
      syncSettingsUI();
    }
  }

  function syncSettingsUI() {
    const pw = planSettings?.plan_widget;
    if (!pw) return;
    document.getElementById("ps-enabled").checked = !!pw.enabled;
    for (const k of STATIC_ROWS) {
      const el = document.getElementById(`ps-row-${k}`);
      if (el) el.checked = !!pw.rows[k];
    }
    // Render dynamically discovered per-feature bucket toggles.
    PS_ROWS_DYNAMIC.innerHTML = "";
    for (const [k, v] of Object.entries(pw.rows)) {
      if (STATIC_ROWS.includes(k)) continue;
      const lbl = document.createElement("label");
      lbl.className = "settings-popover__row";
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = !!v;
      cb.addEventListener("change", () =>
        patchPlanSettings({ rows: { [k]: cb.checked } }).then(renderPlanWidget)
      );
      const span = document.createElement("span");
      span.textContent = bucketLabel(k);
      lbl.appendChild(cb);
      lbl.appendChild(span);
      PS_ROWS_DYNAMIC.appendChild(lbl);
    }
  }

  function startPlanPolling() {
    if (planFetchTimer) clearInterval(planFetchTimer);
    planLoaded = false;
    planAttempts = 0;
    planErrStreak = 0;
    planError = null;
    renderPlanWidget();   // show "wird geladen …" immediately, not a blank/error
    fetchPlan();
    planFetchTimer = setInterval(fetchPlan, PLAN_POLL_MS);
  }
  function stopPlanPolling() {
    if (planFetchTimer) clearInterval(planFetchTimer);
    if (planRetryTimer) clearTimeout(planRetryTimer);
    planFetchTimer = null;
    planRetryTimer = null;
    planData = null;
    planError = null;
    planLoaded = false;
    PLAN_WIDGET_EL.classList.add("hidden");
    PLAN_WIDGET_EL.innerHTML = "";
  }

  async function fetchPlan() {
    if (planRetryTimer) { clearTimeout(planRetryTimer); planRetryTimer = null; }

    let result, isError = false;
    try {
      const r = await fetch("/plan");
      const j = await r.json();
      if (r.status === 403 || j.code === "disabled" || j.error || j.code) {
        isError = true;
        result = j;
      } else {
        result = j;
      }
    } catch (e) {
      isError = true;
      result = { error: String(e), code: "fetch_failed" };
    }

    if (!isError) {
      planData = result;
      planError = null;
      planLoaded = true;
      planAttempts = 0;
      planErrStreak = 0;
      ensureKnownBuckets(result.buckets || []);
    } else if (!planLoaded) {
      // Initial load: a transient error (cold cache, token still settling) must
      // not flash "kein Plan". Keep the loading state and retry a few times.
      planAttempts += 1;
      if (planAttempts < PLAN_MAX_INITIAL_ATTEMPTS) {
        planError = null;
        planRetryTimer = setTimeout(fetchPlan, PLAN_INITIAL_RETRY_MS);
      } else {
        planError = result;
        planData = null;
      }
    } else {
      // Already showed data: ride out a single hiccup on the last good values;
      // only surface an error if it persists across consecutive polls.
      planErrStreak += 1;
      if (planErrStreak >= 2) {
        planError = result;
        planData = null;
      }
    }
    renderPlanWidget();
  }

  function ensureKnownBuckets(list) {
    if (!planSettings) return;
    const rows = planSettings.plan_widget.rows;
    let added = false;
    for (const b of list) {
      bucketLabels[b.key] = b.label;
      if (!(b.key in rows)) {
        rows[b.key] = false; // default off — never surprise the user
        added = true;
      }
    }
    if (added) patchPlanSettings({ rows: { ...rows } });
  }

  function renderPlanWidget() {
    PLAN_WIDGET_EL.innerHTML = "";
    if (!planSettings?.plan_widget.enabled) {
      PLAN_WIDGET_EL.classList.add("hidden");
      return;
    }
    PLAN_WIDGET_EL.classList.remove("hidden");

    // Prefer showing data (even mid-refresh); only show an error when there is
    // genuinely no data to show; otherwise we are still loading.
    if (!planData) {
      if (planError) {
        PLAN_WIDGET_EL.appendChild(planErrorEl(planError));
      } else {
        const el = document.createElement("div");
        el.className = "plan__loading loading-pulse";
        el.textContent = "Plan-Daten werden geladen …";
        PLAN_WIDGET_EL.appendChild(el);
      }
      return;
    }

    const rows = planSettings.plan_widget.rows;

    if (rows.main && planData.main) {
      PLAN_WIDGET_EL.appendChild(mainRow(planData));
    }
    for (const bucket of planData.buckets || []) {
      if (rows[bucket.key]) {
        PLAN_WIDGET_EL.appendChild(bucketRow(bucket));
      }
    }
    if (rows.credits && planData.extra_usage) {
      PLAN_WIDGET_EL.appendChild(creditsRow(planData.extra_usage));
    }
  }

  function mainRow(data) {
    const row = document.createElement("div");
    row.className = "plan__row";

    const lbl = document.createElement("div");
    lbl.className = "plan__label";
    if (data.plan_type) {
      lbl.textContent = `Plan · ${data.plan_type}`;
      lbl.title = `Plan-Typ laut Konto: ${data.plan_type}`;
    } else {
      lbl.textContent = "Plan";
    }
    row.appendChild(lbl);

    const bars = document.createElement("div");
    bars.className = "plan__bars";
    if (data.main.five_hour) bars.appendChild(windowBar("5 h", data.main.five_hour));
    if (data.main.seven_day) bars.appendChild(windowBar("7 d", data.main.seven_day));
    row.appendChild(bars);
    return row;
  }

  function bucketRow(bucket) {
    const row = document.createElement("div");
    row.className = "plan__row";

    const lbl = document.createElement("div");
    lbl.className = "plan__label";
    lbl.textContent = bucket.label;
    row.appendChild(lbl);

    const bars = document.createElement("div");
    bars.className = "plan__bars";
    const windowLabel = bucket.window_seconds === 604800 ? "7 d" : "";
    bars.appendChild(windowBar(windowLabel, bucket));
    row.appendChild(bars);
    return row;
  }

  function windowBar(windowLabel, win) {
    const pct = Math.max(0, Math.min(100, win.used_percent ?? 0));
    const wrap = document.createElement("div");
    wrap.className = "plan__bar";

    if (windowLabel) {
      const w = document.createElement("span");
      w.className = "plan__bar-window";
      w.textContent = windowLabel;
      wrap.appendChild(w);
    }

    const track = document.createElement("div");
    track.className = "plan__bar-track";
    const fill = document.createElement("div");
    fill.className = "plan__bar-fill";
    fill.style.clipPath = `inset(0 ${(100 - pct).toFixed(2)}% 0 0)`;
    track.appendChild(fill);
    wrap.appendChild(track);

    const num = document.createElement("span");
    num.className = "plan__bar-num";
    num.textContent = `${pct}% · ⌛ ${fmtResetIn(win.reset_at)}`;
    if (win.reset_at) {
      const d = new Date(win.reset_at * 1000);
      const windowH = win.window_seconds ? (win.window_seconds / 3600).toFixed(0) + " h" : "—";
      num.title =
        `${pct}% verbraucht\n` +
        `Fenster: ${windowH}\n` +
        `Reset in ${fmtResetIn(win.reset_at)} (${d.toLocaleString("de-DE")})`;
    }
    wrap.appendChild(num);
    return wrap;
  }

  function fmtResetIn(unixSec) {
    if (!unixSec) return "—";
    const sec = unixSec - Math.floor(Date.now() / 1000);
    if (sec <= 0) return "jetzt";
    if (sec < 3600) return `${Math.floor(sec / 60)} min`;
    if (sec < 86400) {
      const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60);
      return m > 0 ? `${h} h ${m} min` : `${h} h`;
    }
    const d = Math.floor(sec / 86400), h = Math.floor((sec % 86400) / 3600);
    return h > 0 ? `${d} d ${h} h` : `${d} d`;
  }

  function creditsRow(eu) {
    const row = document.createElement("div");
    row.className = "plan__row";

    const lbl = document.createElement("div");
    lbl.className = "plan__label";
    lbl.textContent = "Extra-Nutzung";
    row.appendChild(lbl);

    const text = document.createElement("div");
    text.className = "plan__credits";
    const cur = eu.currency || "";
    const fmt = (v) =>
      v == null ? "—" : v.toLocaleString("de-DE", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    if (eu.is_enabled) {
      // Overage active: show consumed against the monthly cap.
      text.textContent = `${fmt(eu.used)} / ${fmt(eu.limit)} ${cur} verbraucht`;
    } else if (eu.limit != null) {
      // Off but a cap is configured — show it so the user sees their setting.
      text.textContent = `aus · Limit ${fmt(eu.limit)} ${cur}`;
    } else {
      text.textContent = "nicht eingerichtet";
    }
    if (!eu.is_enabled && eu.disabled_reason) {
      text.title = `Extra-Nutzung inaktiv (Grund laut Konto: ${eu.disabled_reason})`;
    }
    row.appendChild(text);
    return row;
  }

  function planErrorEl(err) {
    const el = document.createElement("div");
    el.className = "plan__error";
    const code = err.code || "unknown";
    const messages = {
      disabled:        "Plan-Tracking ist deaktiviert.",
      auth_missing:    "Nicht eingeloggt — Claude Code starten und einloggen.",
      token_expired:   "Token abgelaufen — Claude Code nutzen, damit der Token refresht wird.",
      network:         "Netzwerk-Problem beim Abrufen der Plan-Daten.",
      timeout:         "Timeout beim Abrufen der Plan-Daten.",
      binary_missing:  "claude-tokens-Binary nicht gefunden.",
      binary_outdated: "claude-tokens ist zu alt für 'plan' — neue Version installieren.",
      no_output:       "claude-tokens lieferte keine Ausgabe.",
      bad_response:    "Unverständliche Antwort vom Plan-Endpoint.",
      fetch_failed:    "Verbindung zum Hilfs-Server unterbrochen.",
    };
    el.textContent = `Plan: ${messages[code] || err.error || code}`;
    el.title = err.error || "";
    return el;
  }

  // --- settings popover wiring ---

  PLAN_SETTINGS_BTN.addEventListener("click", (e) => {
    e.stopPropagation();
    PLAN_SETTINGS_EL.classList.toggle("hidden");
  });
  document.addEventListener("click", (e) => {
    if (PLAN_SETTINGS_EL.classList.contains("hidden")) return;
    if (e.target.closest("#plan-settings") || e.target.closest("#plan-settings-btn")) return;
    PLAN_SETTINGS_EL.classList.add("hidden");
  });

  document.getElementById("ps-enabled").addEventListener("change", (e) => {
    const checked = e.target.checked;
    const consented = planSettings?.plan_widget?.consent_acknowledged === true;
    if (checked && !consented) {
      e.target.checked = false;
      showConsentModal();
      return;
    }
    patchPlanSettings({ enabled: checked }).then(() => {
      if (checked) startPlanPolling();
      else stopPlanPolling();
      renderPlanWidget();
    });
  });

  // --- consent modal ---

  const CONSENT_EL = document.getElementById("plan-consent");
  const CONSENT_OK = document.getElementById("plan-consent-ok");
  const CONSENT_CANCEL = document.getElementById("plan-consent-cancel");

  function showConsentModal() {
    CONSENT_EL.classList.remove("hidden");
    setTimeout(() => CONSENT_CANCEL.focus(), 50);
  }
  function hideConsentModal() {
    CONSENT_EL.classList.add("hidden");
  }

  CONSENT_OK.addEventListener("click", () => {
    patchPlanSettings({ enabled: true, consent_acknowledged: true }).then(() => {
      document.getElementById("ps-enabled").checked = true;
      hideConsentModal();
      startPlanPolling();
      renderPlanWidget();
    });
  });
  CONSENT_CANCEL.addEventListener("click", hideConsentModal);
  CONSENT_EL.querySelector(".modal__backdrop").addEventListener("click", hideConsentModal);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !CONSENT_EL.classList.contains("hidden")) {
      hideConsentModal();
    }
  });

  for (const k of STATIC_ROWS) {
    document.getElementById(`ps-row-${k}`).addEventListener("change", (e) => {
      patchPlanSettings({ rows: { [k]: e.target.checked } }).then(renderPlanWidget);
    });
  }

  // Tick reset countdowns without re-fetching from the server.
  setInterval(() => {
    if (!planData || !planSettings?.plan_widget.enabled) return;
    renderPlanWidget();
  }, 30_000);

  loadPlanSettings();
})();
