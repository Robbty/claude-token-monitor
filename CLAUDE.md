# CLAUDE.md — Arbeitsanleitung für dieses Projekt

Diese Datei wird beim Start von Claude Code in diesem Verzeichnis automatisch
geladen. Sie fasst zusammen, was du brauchst, um hier produktiv weiterzuarbeiten.

## Was ist das?

**claude-token-monitor** — ein Tool, das die Token-/Kontext-Auslastung
laufender **Claude-Code**-Sessions sichtbar macht. Geschwister-Tool zu
[`codex-token-monitor`](https://github.com/Robbty/codex-token-monitor) (gleiche
Struktur/CLI, nur für Claude Code statt Codex). Zwei Komponenten:

1. **`claude-tokens`** (Rust-CLI, `src/`) — liest die Session-JSONL-Dateien
   unter `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`, die Claude Code
   sowieso schreibt, und gibt Token-/Kontext-Stand auf stdout aus (KEY=VALUE
   oder JSON). Subcommand `claude-tokens plan` für kontoseitige Rate-Limits.
2. **Display-App** (`display/`) — Python-stdlib-Server + Chromium-`--app`-
   Fenster, das die Sessions als Karten mit Auslastungs-Balken visualisiert,
   plus optionalem Plan-Widget.

Das Tool ruft Claude Code **nie** selbst auf — es liest nur dessen Datei-Output
(und für `plan` den OAuth-Token aus `~/.claude/.credentials.json`).

## Repo-Struktur

```
src/
  main.rs        CLI (clap), Single- + Multi-Session-Orchestrierung (Threads + mpsc)
  locate.rs      Session-Auswahl: Selector { ThreadId, Cwd, MostRecent, All }; cwd↔Slug-Encoding
  models.rs      Modell→Kontextfenster-Mapping (Opus/Sonnet 4.x = 1M, Haiku = 200k)
  proc.rs        Prozessbasierte Aktiv-Erkennung (exe …/claude/versions/ + K-neueste Dateien)
  tail.rs        JSONL-Tailing (poll-basiert, kein notify)
  protocol.rs    schmale serde-Mirrors der Claude-JSONL (#[serde(other)]-Catch-All)
  state.rs       TokenState-Aggregation (per-Turn-Summen, compact_count, model, …)
  render.rs      KV- und JSON-Ausgabe
  plan.rs        `plan`-Subcommand: liest .credentials.json, ruft oauth/usage, Exit-Codes
display/
  launcher.sh          startet server.py + Chromium-App-Fenster (Default-Port 8766)
  install-desktop.sh   erzeugt .desktop-Launcher + Desktop-Icon
  server.py            HTTP/SSE-Bridge, spawnt claude-tokens, normalisiert /plan, /open-dir /focus-terminal /copy
  index.html
  help.html            eigenständiges Hilfe-Fenster (rendert README via marked.js)
  chats.html           eigenständiges „Letzte Chats"-Fenster (Tabelle + Verlaufs-Detail)
  static/{app.css,app.js,marked.min.js}
  icon.svg / icon.png  App-Icon
  README.md            UI-Bedienung (wird im 📖-Fenster gerendert)
scripts/build.sh       baut gnu+musl + stagt target/release/bundle/ (+ optional Tarball)
README.md              Projekt-README (CLI-Doku)
HANDOFF.md             Ursprungs-Auftrag des Vorgänger-Chats
```

Datenquelle (read-only): `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`.
Encoding-Regel: jeder `/` und `.` im cwd wird zu `-`
(`/home/peter/.codex` → `-home-peter--codex`).

## Build / Test / Release

```bash
# CLI bauen
cargo build --release                                    # gnu, target/release/claude-tokens
cargo build --release --target x86_64-unknown-linux-musl # statisch (braucht musl-tools)

# Komplett-Bundle + Tarball (bevorzugtes Distributions-Artefakt)
./scripts/build.sh --tarball

# Lokal installieren
install -m 755 target/release/claude-tokens ~/.local/bin/claude-tokens

# Syntax-Checks (es gibt KEINE cargo #[test]s — nur manuelle Verifikation)
node --check display/static/app.js
python3 -m py_compile display/server.py

# Display lokal testen (ohne Browser, nur Server-Endpunkte)
python3 display/server.py --port 18999 --claude-tokens-bin target/release/claude-tokens &
curl -s http://127.0.0.1:18999/readme | head -1
```

Verifikation des CLI gegen echte Daten: `claude-tokens --all --json | jq` oder
gegen eine bestimmte Session `claude-tokens --thread <uuid>`.

## Claude-Spezifika (verifiziert — wichtig, weicht teils vom HANDOFF.md ab)

- **Kontextfenster = 1.000.000** für Opus/Sonnet 4.x (nicht 200k wie im Handoff
  angenommen). Empirisch: eine Session hielt 643k gecachte Token ohne
  Auto-Compaction; `cache_read` kann das Fenster nicht überschreiten. Haiku =
  200k. Mapping in `src/models.rs`, Default 1M.
- **Compaction** = `system`-Event mit `subtype == "compact_boundary"` (Auto am
  Limit **oder** manuelles `/compact`). Kein Heuristik-Detektor nötig.
- **Aktiv-Erkennung ist prozessbasiert**, nicht handle-basiert: Claude Code hält
  die Datei **nicht** dauerhaft offen. Eine Session ist live, wenn `claude`-
  Prozesse in ihrem cwd laufen UND sie unter den K neuesten `.jsonl` des Projekts
  ist (K = Prozess-Anzahl in dem cwd → mehrere Instanzen bleiben sichtbar).
  Prozess-Erkennung über den **exe-Pfad** `…/claude/versions/…`, NICHT über
  `comm` — `comm` ist je nach Start „claude" oder die Version (z. B. „2.1.186",
  etwa bei `claude --resume`). Siehe `src/proc.rs`.
- **`<synthetic>`-Assistant-Events** (Interrupts/Fehler) tragen Null-Usage und
  werden ignoriert, sonst würde `tokens_in_context` auf 0 zurückspringen.
- **Worker = Subagent-Transkripte** unter
  `projects/<slug>/<session-uuid>/subagents/agent-<hex>.jsonl` (Agent-Tool/
  Workflows). Gleiches Record-Format, aber **alle** Records tragen
  `isSidechain: true` (Meta-/Detail-Extraktion in server.py muss das für
  Worker-Dateien erlauben, sonst bleiben sie leer). CLI markiert sie mit
  `is_worker=true` + `parent_session_id` (aus dem Pfad abgeleitet).
  Liveness: Parent-Session lebt UND Datei-mtime < 120 s
  (`WORKER_FRESH_SECS` in proc.rs = `WORKER_FRESH_SEC` in server.py) —
  ein fertiger Worker schreibt nie wieder.
- **Plan-Endpoint:** `GET https://api.anthropic.com/api/oauth/usage` mit
  `Authorization: Bearer`, `anthropic-beta: oauth-2025-04-20`,
  `anthropic-version: 2023-06-01`. Token aus `~/.claude/.credentials.json`
  → `claudeAiOauth.accessToken`. Kein Cloudflare (anders als claude.ai). Antwort:
  `five_hour`/`seven_day` (+ nullable `seven_day_*`/Codename-Töpfe) + `extra_usage`;
  `resets_at` ist ISO-8601. server.py normalisiert das fürs Widget.
- **Frische-Zustände im Display:** server.py schickt die Datei-mtime
  (`last_modified`); der Client zeigt damit das echte Daten-Alter und markiert
  Daten > 10 min (Sweeper aktualisiert mtime alle 3 s) am Timer als „· wartet"
  (gelb; Balken bleibt hell). Schwellen
  `STALE_AFTER_MS`/`SETTLE_MS`
  in `app.js`. Plan-Widget hat eine Lade-Karenz, damit am Start nicht „kein Plan"
  flackert.

## Konventionen

- **`target/` ist gitignored** und bleibt es. Niemals Binaries committen —
  Distribution ausschließlich über `scripts/build.sh` / Release-Assets.
- **Nie `git add -A` / `git add .`** — sonst landen `__pycache__/`, Screenshots
  (`*.png`) oder `.playwright-mcp/` im Repo. Immer spezifische Pfade adden.
  (`.gitignore` deckt diese ab, aber Vorsicht bleibt geboten.)
- **Release-Builds mit Pfad-Remap** (sonst leakt `/home/peter` in Panic-Pfade);
  `scripts/build.sh` macht das automatisch via `RUSTFLAGS=--remap-path-prefix …`.
- **Display-Assets** liegen unter `display/static/`. `index.html`/`help.html`
  laden `/static/...`.
- **Geschwister-Parität:** Default-Port **8766** (Codex: 8765), damit beide
  Tools parallel laufen. Feldnamen/CLI-Flags möglichst deckungsgleich zu Codex.

## Gelernte Klippen (nicht nochmal hineintappen)

1. CSS-Spezifität: `.modal.hidden { display:none }` / `.plan.hidden { display:none }`
   statt nur `.hidden`, wenn `.hidden` vor `.modal{display:flex}` in der Datei steht.
2. `pgrep/pkill -f 'server.py'` matcht die eigene Kommandozeile und killt die
   eigene Shell (Exit 143/144) — Test-Server lieber per Port stoppen
   (`fuser -k 8766/tcp`).
3. Kontextfenster NICHT auf 200k hardcoden (siehe oben, real 1M) — sonst zeigen
   Karten „>100%".
4. Karten-Sortierung primär nach `compact_count`, sekundär `percent_used` — sonst
   springt eine Session nach Auto-Compact unerwartet weg.
5. Plan-Subprocess-Output strikt prüfen: leerer stdout = wahrscheinlich veraltetes
   Binary ohne `plan`-Subcommand (`binary_outdated`), nicht „erfolgreich leer".

## Aktueller Stand & offene Punkte

v0.1.0: CLI (inkl. `plan`) + Display-App fertig und verifiziert. Am 2026-07-09
dazugekommen (alles im Initial-Commit, live getestet):

- **🕘 „Letzte Chats"** (`display/chats.html` + Routen `/chats`, `/chats-data`,
  `/chat-detail` in server.py): Tabelle der zuletzt aktiven Chats aller
  Projekte. Titel aus dem `ai-title`-Record der Session-JSONL (steht in den
  ersten ~10 Zeilen → Head-Read; Fallback: erste echte User-Message).
  Zeilen-Klick = kondensierter Verlauf (Tail-Read) + `claude --resume`-Kopierknopf.
  Sortierbar per Spaltenkopf (Verzeichnis/Wann; Default: Verzeichnis),
  Schalter „Aktive zuerst" (bei Verzeichnis-Sortierung rücken ganze
  Verzeichnisse mit aktivem Chat hoch) und „Worker" (listet Subagenten mit
  ⚙-Badge; Verlauf zeigt „Auftrag"/„Worker", kein resume-Knopf). Settings
  `chats.{per_dir,total,active_first,show_workers}` (Default 3/10/aus/aus).
  🗑-Spalte pro Zeile löscht die Session-Datei nach Inline-Bestätigung
  („Löschen? Ja/✕") via `POST /chat-delete`; laufende Sessions sind geschützt
  (Knopf deaktiviert, Server antwortet 409), sid-Validierung wie `/chat-detail`
  (`_SID_RE` erlaubt auch `agent-<hex>`-Worker-IDs).
- **Worker-Toggle im Hauptfenster** (2026-07-10): CLI erkennt Subagent-
  Transkripte (`is_worker`/`parent_session_id`, Liveness siehe oben), app.js
  blendet sie per „Worker"-Schalter ein (⚙-Badge, kein ↻-Rollover, Zähler
  „X aktiv (+Y Worker)"). Der alte `show-workers`-Toggle „Leere" (Sessions ohne
  Kontextfenster) heißt im Code jetzt `show-empty`/`isEmpty`/`session--empty`.
- **Karten-Sortierung wählbar** (2026-07-10): Dropdown in der Topbar —
  Auslastung (Default, wie „gelernte Klippe" 4) / Ort (Pfad) / Startzeit
  (älteste zuerst; CLI liefert dafür `started_at` = Timestamp des ersten
  Events). Persistiert als `display.sort` via /settings. Gruppen-Reihenfolge
  echte Sessions → Worker → leere bleibt immer erhalten. Topbar bricht in
  schmalen Fenstern um (`flex-wrap`), Launcher-Fenster jetzt 680×640.
- **⚡/📁 landen auf der richtigen Arbeitsfläche**: Claude Code ersetzt den
  Terminal-Titel durch den aktuellen `aiTitle` — Fenster-Match daher über
  cwd **und** letzten ai-title (Helfer `_wmctrl_windows`,
  `_find_session_window`, `_find_file_manager_window`). 📁 fokussiert ein
  vorhandenes Dateimanager-Fenster statt neu zu öffnen; sonst erst
  `wmctrl -s <desktop>` des Session-Terminals, dann `xdg-open`.
  Match-Regeln (2026-07-10 verschärft, nachdem ⚡ auf falschen Arbeitsflächen
  landete): nur Terminal-/IDE-Klassen kommen infrage (ein Browser-Tab mit dem
  Verzeichnisnamen im Titel gewinnt sonst und zieht auf SEINE Arbeitsfläche);
  Stärke ai-title > voller cwd (absolut **und** `~`-abgekürzt — idle-Terminals
  zeigen `~/pfad`) > Basename. Sessions ohne ai-title titeln ihr Terminal nur
  „✳ Claude Code" — dieser generische Titel wird als Fallback genutzt, wenn
  die Session lebt und genau EIN solches Fenster existiert. ai-title-Suche:
  großes Tail-Fenster (256k, ein Tool-lastiger Turn sprengt 64k) mit
  Head-Fallback (`_chat_title_for_sid`). Gleichstand (zwei Sessions können
  denselben ai-title tragen, z. B. beide aus einem Handover-Review gestartet;
  Fenster-PIDs helfen nicht — xfce4-terminal ist ein Single-Daemon) wird über
  Arbeitsflächen-Affinität aufgelöst: es gewinnt der Kandidat, auf dessen
  Arbeitsfläche weitere Fenster (beliebige Klasse) den Session-cwd im Titel
  tragen — Projekt-Fenster sammeln sich pro Arbeitsfläche.
- **Sticky-Fenster**: Haupt- und Unterfenster pinnen sich beim Laden via
  `POST /sticky` (wmctrl `add,sticky`) auf alle Arbeitsflächen; abschaltbar
  über `windows.sticky` in der Config. Endpoint akzeptiert nur Titel mit
  Präfix „Claude Token Monitor".

**Git:** Repo initialisiert am 2026-07-09 (`main`); die Feature-Runde vom
2026-07-10 liegt als thematische Commits vor (🗑 Chat-Löschen, Worker,
Fenster-Matcher, Karten-Sortierung, Doku). **Phase 5 erledigt (2026-07-10):**
öffentliches Repo <https://github.com/Robbty/claude-token-monitor> ist Remote
`origin`, Release **v0.1.0** getaggt, musl-Tarball
(per `./scripts/build.sh --tarball` reproduzierbar) als Asset hochgeladen.
Siehe `HANDOFF.md` für den Ursprungsauftrag, `HANDOVER.md` für die
Session-Übergabe vom 2026-07-10.
