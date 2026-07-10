# HANDOVER — Stand 2026-07-10 abends (Session 0eae6f02)

Übergabe an die nächste Session. Projekt-Grundlagen und alle Details stehen
in `CLAUDE.md` (aktuell gehalten) — diese Datei ist nur der Schnellüberblick.

## 1. In dieser Session erledigt

1. **Feature-Runde vom Vormittag committet** — die 15 uncommitteten Dateien
   des Vorgänger-Handovers als 5 thematische Commits (hunk-genau gestaged,
   jeder Zwischenstand gebaut/geprüft): 🗑 Chat-Löschen (`2ab907b`), Worker
   CLI+UI (`b4e2758`), Fenster-Matcher-Fix (`f651d6b`), Karten-Sortierung +
   Fensterbreite (`91faa1c`), Doku inkl. HANDOVER (`82a7de6`).
2. **Phase 5 abgeschlossen**: Das auf dem Account bereits existierende,
   LEERE öffentliche Repo `Robbty/claude-token-monitor` (angelegt
   2026-06-10) als `origin` genutzt — `main` gepusht, Beschreibung gesetzt,
   **v0.1.0** getaggt, Release mit musl-Tarball (1,3 MB, aus
   `./scripts/build.sh --tarball`) hochgeladen:
   <https://github.com/Robbty/claude-token-monitor/releases/tag/v0.1.0>
3. **⚡-Bug 1** (User-Report: Website-Karte landete bei Plakate): beide
   Sessions trugen denselben ai-title „Review handover documentation" →
   Titel-Match mehrdeutig, wmctrl-Reihenfolge entschied. Fix: Tiebreaker
   „Arbeitsflächen-Affinität" (`4849194`).
4. **⚡-Bug 2** (User-Report: zwei Claude-Terminals in verschiedenen Ordnern
   auf DERSELBEN Arbeitsfläche): Affinität läuft dann ins Leere. Fix
   grundsätzlich: **exakter WINDOWID-Match** — Terminals vererben `WINDOWID`
   an die Shell, claude erbt sie; `/proc/<pid>/environ` nennt das exakte
   Fenster (`_claude_terminal_window_ids`, dezimal vs. wmctrl-hex,
   `_win_id_int`). Läuft vor jeder Heuristik; Titel-Heuristik + Affinität
   nur noch Fallback für tote Sessions / Terminals ohne WINDOWID
   (z. B. IDE-integrierte). Commit `8072ffc`. Nebeneffekt: laufende Session
   trifft jetzt ihr Terminal, nicht mehr bevorzugt ein IDE-Fenster.
5. CLAUDE.md fortgeschrieben (Phase 5 erledigt, WINDOWID-Match,
   Affinitäts-Tiebreaker); display/README ⚡-Zeile entsprechend.
6. **Monitor-Server neu gestartet** (Port 8766, `~/.local/bin/claude-tokens`)
   — läuft mit dem WINDOWID-Fix; Chromium-Fenster verbinden per
   SSE-Reconnect von selbst.

## 2. Tests / Builds

- `cargo build --release` — **grün** (auch jeder Rust-Zwischencommit wurde
  als Index-Checkout gebaut).
- `node --check display/static/app.js`, `python3 -m py_compile
  display/server.py`, `bash -n display/launcher.sh` — **grün**.
- WINDOWID-Match **live verifiziert** gegen alle 5 laufenden Sessions
  (inkl. der beiden gleichnamigen Handover-Review-Terminals und
  TradingBot): jede trifft exakt ihr Fenster; Fallback ohne WINDOWID
  regressionsfrei (Simulation über direkten Import von server.py).
- Es gibt weiterhin **keine** `cargo #[test]`s — nur manuelle Verifikation.

## 3. Offene TODOs

- **Keine offenen Pflicht-Punkte** — Commits + Phase 5 (GitHub/Release)
  sind erledigt.
- User-Feedback zu ⚡ abwarten (beide Fixes sind live, aber der zweite
  Report kam, bevor der User den WINDOWID-Fix testen konnte).
- Optional angeboten, nie beauftragt: 🗑 in Papierkorb statt `unlink`;
  „Startzeit"-Sortierung neueste-zuerst statt älteste-zuerst.
- Denkbar (nicht besprochen): v0.1.1 taggen, damit das Release-Asset die
  ⚡-Fixes enthält — das v0.1.0-Tarball ist auf Stand `6b278c5`s Vorgänger
  (gebaut vor den ⚡-Fixes).

## 4. Kontext-Snippets

- Branch: `main`, HEAD: `8072ffc` („⚡: laufende Sessions exakt über
  WINDOWID matchen statt über Titel"), Arbeitsbaum **sauber**, alles
  gepusht (`origin` = <https://github.com/Robbty/claude-token-monitor>).
- Commits dieser Session (chronologisch): `2ab907b`, `b4e2758`, `f651d6b`,
  `91faa1c`, `82a7de6`, `6b278c5`, `4849194`, `8072ffc`.
- Zuletzt geänderte Dateien (⚡-Fixes): `display/server.py`
  (`_claude_terminal_window_ids`, `_win_id_int`, `_find_session_window`
  mit `claude_window_ids`-Fastpath + Affinitäts-Tiebreaker, beide
  Handler-Aufrufe), `CLAUDE.md`, `display/README.md`.
- Server-Neustart-Rezept: PID via `ss -ltnp | grep 8766`, `kill`, dann
  `nohup python3 display/server.py --port 8766 --claude-tokens-bin
  ~/.local/bin/claude-tokens &` (nie `pkill -f server.py` — gelernte
  Klippe 2).
- Matcher-Simulation ohne Browser: server.py per `importlib` laden und
  `_find_session_window(_wmctrl_windows(), cwd, title, session_live=…,
  claude_window_ids=_claude_terminal_window_ids(cwd))` direkt aufrufen.
