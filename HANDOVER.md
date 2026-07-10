# HANDOVER — Stand 2026-07-10 (Session 5d8a0fb0)

Übergabe an die nächste Session. Projekt-Grundlagen stehen in `CLAUDE.md`
(dort ist der heutige Stand bereits eingepflegt — diese Datei ist nur der
Schnellüberblick der Session).

## 1. In dieser Session erledigt

1. **🗑 Chat-Löschen** in „Letzte Chats": Lösch-Spalte mit Inline-Bestätigung
   („Löschen? Ja/✕"), neuer Endpoint `POST /chat-delete` (strikte
   sid-Validierung, 409 bei laufender Session, endgültiges `unlink`).
2. **Worker-Erkennung im Rust-CLI**: Subagent-Transkripte
   (`<session-uuid>/subagents/agent-*.jsonl`) werden erkannt →
   `is_worker=true` + `parent_session_id`; Liveness = Parent-Session lebt
   UND mtime < 120 s (`WORKER_FRESH_SECS` proc.rs = `WORKER_FRESH_SEC`
   server.py-Sweeper).
3. **„Worker"-Schalter im Hauptfenster** (wie codex-token-monitor): Worker
   default ausgeblendet, Zähler `X aktiv (+Y Worker)`, Karten mit ⚙-Badge,
   ohne ↻-Rollover. Alter „Leere"-Toggle im Code umbenannt →
   `show-empty`/`isEmpty`/`session--empty`.
4. **Worker in „Letzte Chats"**: Schalter „Worker" (Setting
   `chats.show_workers`, Default aus), ⚙-Badge in der Thema-Spalte, Verlauf
   mit Rollen „Auftrag"/„Worker", kein resume-Knopf. Achtung: In
   Worker-Dateien tragen ALLE Records `isSidechain:true` — Meta/Detail-
   Extraktion hat dafür `allow_sidechain`.
5. **Chats-Default-Sortierung** auf „Verzeichnis" (Ort) geändert.
6. **⚡/📁-Arbeitsflächen-Bug behoben**: Matcher akzeptiert nur noch
   Terminal-/IDE-Fenster (Browser-Treffer zogen auf falsche Arbeitsfläche),
   matcht Pfade auch `~`-abgekürzt, ai-title-Suche mit 256k-Tail +
   Head-Fallback, Fallback auf eindeutiges „✳ Claude Code"-Terminal bei
   laufender Session ohne ai-title. Gegen echte Fensterliste verifiziert
   (TradingBot → „TB & Depth-Stream" usw.).
7. **Karten-Sortierung wählbar**: Dropdown Auslastung/Ort/Startzeit in der
   Topbar, persistiert als `display.sort`; CLI liefert dafür neu
   `started_at` (Timestamp des ersten Events). Gruppen echte → Worker →
   leere bleiben immer.
8. **Fensterbreite**: Launcher 520 → **680×640**; Topbar bricht in schmalen
   Fenstern sauber um (`flex-wrap`).
9. Binary nach `~/.local/bin` installiert, **laufender Monitor neu
   gestartet** (läuft mit allem oben, Fenster 680×640, sticky).

## 2. Tests / Builds

- `cargo build --release` — **grün** (17 s, keine Warnings im Output-Tail).
- `node --check display/static/app.js` / `python3 -m py_compile
  display/server.py` / `bash -n display/launcher.sh` — **grün**.
- Live-Verifikation (alles grün): curl-Tests gegen Fake-`CLAUDE_HOME`
  (chats-data/chat-detail/chat-delete inkl. Worker + Path-Traversal-Abwehr),
  CLI gegen echtes Subagent-Transkript und Fake-Worker unter lebendem
  Projekt-Slug, Playwright-Durchklick beider Fenster (Toggles, Badge,
  Delete-Sperre bei aktiver Session, Sortier-Modi + Persistenz + Reload).
- Es gibt weiterhin **keine** `cargo #[test]`s — nur manuelle Verifikation.

## 3. Offene TODOs

- **Phase 5 (aus HANDOFF.md, weiter offen)**: öffentliches GitHub-Repo als
  Remote, Release v0.1.0 taggen, Tarball (`./scripts/build.sh --tarball`)
  als Asset hochladen.
- **Alles Heutige ist NICHT committet** (15 geänderte Dateien, siehe unten) —
  sinnvoll als 3–4 thematische Commits: (a) Chat-Löschen, (b) Worker
  CLI+UI, (c) Fenster-Matcher-Fix, (d) Karten-Sortierung + Breite.
  Konvention beachten: nie `git add -A`, nur spezifische Pfade.
- Optional angeboten, nicht beauftragt: 🗑 in Papierkorb statt `unlink`;
  „Startzeit"-Sortierung neueste-zuerst statt älteste-zuerst.

## 4. Kontext-Snippets

- Branch: `main`, letzter Commit: `90b8ade` („CLAUDE.md: Stand nach
  Feature-Runde vom 2026-07-09"), Arbeitsbaum: nur Modifikationen, nichts
  Neues untracked.
- Geänderte Dateien:
  `src/{locate,main,proc,protocol,render,state}.rs`,
  `display/{server.py,index.html,chats.html,launcher.sh}`,
  `display/static/{app.js,app.css}`,
  `CLAUDE.md`, `README.md`, `display/README.md`.
- Neue Endpoints/Settings: `POST /chat-delete`;
  `chats.show_workers` (bool), `display.sort` (`usage|dir|start`).
- Neue CLI-Felder: `is_worker`, `parent_session_id`, `started_at`.
- Test-Artefakte (Fake-Homes, Server-Logs) liegen nur im Session-Scratchpad
  unter `/tmp/claude-1000/…/scratchpad/` — nichts davon im Repo.
