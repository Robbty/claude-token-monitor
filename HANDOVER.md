# HANDOVER — Stand 2026-07-12 (Session db732430, gestartet im codex-token-monitor-cwd)

Übergabe an die nächste Session. Projekt-Grundlagen und alle Details stehen
in `CLAUDE.md` (aktuell gehalten) — diese Datei ist nur der Schnellüberblick.

## 1. In dieser Session erledigt

1. **User-Report untersucht:** Chat `97ca3c5e` (TB-TradingBot, Haiku 4.5)
   zeigte im Monitor **94 %**, in Claude Code selbst aber **18 %**. Ursache:
   Die Session hat 188.940 Token im Kontext; `models.rs` mappte
   `claude-haiku-4` auf 200k (→ 94 %), tatsächlich läuft Haiku 4.5 in
   Claude Code aber mit **1M-Fenster** (188.940 / 1M ≈ 19 % — exakt die
   Chat-Anzeige). Der Monitor rechnete also mit falschem Nenner.
2. **Wichtiger Irrweg, nicht wiederholen:** Zunächst schien die Session-
   Datei das Fenster zu enthalten (`context_window: 1000000` per grep
   gefunden) — das war aber nur **Doku-Text im Attachment des
   claude-api-Skills**, kein Metadatum. Systematische Prüfung ALLER
   JSON-Schlüssel aller lokalen Session-Dateien bestätigt: Claude Code
   schreibt das Kontextfenster **nirgends strukturiert** in die JSONL.
   Der Kommentar in `models.rs` stimmte also; „Fenster aus der Datei
   lesen" ist als Feature nicht umsetzbar.
3. **Fix umgesetzt** (uncommittet, siehe TODOs):
   - `src/models.rs`: `claude-haiku-4` → 1_000_000; neue Funktion
     `context_window_at_least(model, observed)` — rundet einen
     beobachteten Kontext, der die Tabellen-Annahme übersteigt, auf die
     nächste plausible Fenstergröße hoch (Leiter 200k/500k/1M, darüber
     nächste volle 100k). Begründung im Modul-Kommentar: der Prompt kann
     das echte Fenster nie überschreiten → Beobachtung = harte Untergrenze.
     Achtung Toolchain: `i64::div_ceil` ist hier noch unstable (E0658),
     daher manuell `(x + 99_999) / 100_000`.
   - `src/state.rs`: neues Feld `max_context_tokens` (Maximum über alle
     Turns, gepflegt in `apply_assistant`); `context_window()` nutzt jetzt
     `context_window_at_least`. Damit **Selbstkorrektur**: künftige
     Fenster-Änderungen von Anthropic erzeugen nie mehr >100 %-Balken.
   - Doku: CLAUDE.md (Repo-Struktur-Zeile + Abschnitt „Claude-Spezifika"),
     README.md (2 Stellen), display/README.md (Zahlen-Tabelle) — alle
     „Haiku = 200k"-Aussagen korrigiert.
4. **Verifiziert:**
   - Session `97ca3c5e`: `--thread … --json` zeigt jetzt
     `context_window=1000000`, `percent_used=19` (vorher 94).
   - Selbstkorrektur: synthetische Session (Wegwerf-Datei unter
     `~/.claude/projects/-tmp-ctm-selftest/`, danach gelöscht) mit 1,15M
     beobachtetem Kontext → Fenster 1.200.000, 96 %. ✓
   - `--all --json`: alle Live-Sessions plausibel (claude-fable-5 →
     unbekannter Prefix → Default 1M).
   - Display braucht keinen eigenen Fix: `app.js` übernimmt
     `context_window` aus der CLI-Ausgabe (`effectiveContextWindow` ist
     nur Fallback für Worker ohne Wert).

## 2. Tests / Builds

- `cargo build --release` — **grün** (nach dem div_ceil-Fix).
- Keine JS/Python-Änderungen in dieser Session → keine Syntax-Checks nötig.
- Weiterhin **keine** `cargo #[test]`s — nur manuelle Verifikation.

## 3. Offene TODOs

1. **Committen** — 5 Dateien uncommittet: `src/models.rs`, `src/state.rs`,
   `CLAUDE.md`, `README.md`, `display/README.md`. Deutscher Commit-Stil,
   Author `Robbty <robbty01@gmail.com>` (per `git -c`, nicht global).
   User hat Commit-Frage noch nicht beantwortet — vor dem Committen
   bestätigen lassen.
2. **Binary installieren** — `~/.local/bin/claude-tokens` ist noch der
   Stand vom 10. Juli (OHNE den Fix); der laufende Monitor-Server (Port
   8766) zeigt für Haiku-Sessions weiter 94 %. Nach Freigabe:
   `install -m 755 target/release/claude-tokens ~/.local/bin/claude-tokens`
   (oder musl-Build wie im Release-Prozess) + Server-Neustart (Rezept in
   §4). System-modifizierende Aktion → **erst fragen** (Memory-Regel).
3. Aus dem Vorgänger-Handover weiter offen: v0.1.1-Release denkbar
   (v0.1.0-Tarball enthält weder ⚡-Fixes noch diesen Fenster-Fix);
   optional 🗑→Papierkorb und „Startzeit"-Sortierung (nie beauftragt).

## 4. Kontext-Snippets

- Branch: `main`, HEAD: `8cdc6f2` („HANDOVER.md: Übergabe nach Session
  0eae6f02"), Arbeitsbaum **schmutzig** (die 5 Dateien aus §3.1 + diese
  Datei), origin gepusht bis HEAD.
- Diese Session lief im cwd `~/projekte/codex-token-monitor` (User-Frage
  begann dort), die Arbeit betraf aber DIESES Repo — neuen Chat am besten
  direkt in `~/projekte/claude-token-monitor` starten.
- Beweis-Rechnung: 10 input + 168.559 cache_creation + 20.219 cache_read
  = 188.788 (letzter Turn); 188.788/200k = 94,4 %, 188.788/1M = 18,9 %.
- Session-Dateien nach Schlüsseln durchsuchen (Irrweg-Check §1.2):
  JSON-Zeilen parsen und Pfade rekursiv sammeln — `grep` nach
  `context_window` findet nur Skill-Doku-Text, keine Metadaten.
- Server-Neustart-Rezept: PID via `ss -ltnp | grep 8766`, `kill`, dann
  `nohup python3 display/server.py --port 8766 --claude-tokens-bin
  ~/.local/bin/claude-tokens &` (nie `pkill -f server.py` — gelernte
  Klippe 2).
