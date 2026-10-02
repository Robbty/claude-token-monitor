# claude-token-monitor

Stdout-Anzeige für Token- und Kontext-Daten der aktuell laufenden **Claude
Code**-Session. Liest direkt die Session-JSONL-Datei unter
`~/.claude/projects/<encoded-cwd>/<uuid>.jsonl` — kein Patchen von Claude Code
nötig, keine Abhängigkeit auf interne Pakete. Nur die JSON-Felder, die das Tool
nutzt, sind als minimaler Serde-Typ nachgebaut; unbekannte Felder und unbekannte
Event-Typen werden ignoriert, damit Claude-Code-Updates nichts brechen.

Es ist das Geschwister-Tool zu
[`codex-token-monitor`](https://github.com/Robbty/codex-token-monitor) — gleiche
CLI-Schnittstelle, gleiche Display-App, nur für Anthropics Claude Code statt
OpenAIs Codex. Beide lassen sich parallel installieren und betreiben
(verschiedene Default-Ports: 8765 vs. 8766).

## Schnellinstallation (ohne Bauen)

> Vorgefertigte Binaries und ein `scripts/build.sh`-Bundle folgen mit dem ersten
> getaggten Release (Phase 5). Bis dahin → [Bauen](#bauen) aus dem Quellcode.

Sobald Releases vorliegen, statisch verlinkte Variante (läuft auf jedem
x86_64-Linux/WSL2 ohne Abhängigkeiten):

```bash
mkdir -p ~/.local/bin
curl -sSL https://github.com/Robbty/claude-token-monitor/releases/latest/download/claude-tokens-x86_64-linux-musl \
  -o ~/.local/bin/claude-tokens
chmod +x ~/.local/bin/claude-tokens
claude-tokens --version
```

Falls `~/.local/bin` nicht im `$PATH` liegt, in `~/.bashrc` ergänzen:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Integrität prüfen (optional, aber empfohlen):

```bash
curl -sSL https://github.com/Robbty/claude-token-monitor/releases/latest/download/SHA256SUMS \
  | grep claude-tokens-x86_64-linux-musl \
  | sha256sum -c -
```

## Bauen

```bash
cd /pfad/zu/claude-token-monitor
cargo build --release
# Binary: target/release/claude-tokens
```

Das Binary ist dynamisch gegen die glibc des Build-Systems gelinkt — siehe
[Portabilität](#portabilität-und-statischer-build) für die Variante, die auf
jedem Linux läuft.

### Komplett-Bundle (Binary + Display-App)

Wer beides nutzen will (CLI **und** das Statusfenster), kann das mit einem
Skript in einem Schritt erzeugen:

```bash
./scripts/build.sh                 # baut gnu+musl, stagt musl-Bundle
./scripts/build.sh --tarball       # zusätzlich ein versendbares .tar.gz
./scripts/build.sh --variant gnu   # dynamische Variante im Bundle
./scripts/build.sh --skip-build    # nur neu stagen ohne Cargo-Aufruf
```

Ergebnis liegt unter `target/release/bundle/`:

```text
target/release/bundle/
├── claude-tokens        ← Binary (default: musl, statisch)
├── display/             ← komplette Display-App
├── README.md            ← diese Doku
└── SHA256SUMS           ← Prüfsumme des Binarys
```

Mit `--tarball` zusätzlich
`target/release/claude-token-monitor-v<version>-x86_64-linux-musl.tar.gz`
(~1,3 MB), das man einfach an andere Rechner verschicken kann. Das Skript setzt
beim Bauen automatisch `--remap-path-prefix`, damit kein `/home/<user>`-Pfad in
die Binary (Panic-Meldungen) leakt.

## Installation

Damit `claude-tokens` aus jedem Terminal aufrufbar ist, das Binary in ein
Verzeichnis im `$PATH` legen. Übliche Optionen:

**Variante 1 — `~/.local/bin` (XDG-Standard, empfohlen):**

```bash
mkdir -p ~/.local/bin
install -m 755 target/release/claude-tokens ~/.local/bin/

# Prüfen, ob ~/.local/bin im PATH steht:
echo "$PATH" | tr ':' '\n' | grep -q "$HOME/.local/bin" \
  && echo "PATH ok" \
  || echo 'export PATH="$HOME/.local/bin:$PATH"  # in ~/.bashrc ergänzen'
```

**Variante 2 — `~/bin` (klassisch):**

```bash
mkdir -p ~/bin
install -m 755 target/release/claude-tokens ~/bin/
# Bei Bedarf in ~/.bashrc ergänzen:
#   export PATH="$HOME/bin:$PATH"
```

**Variante 3 — `/usr/local/bin` (systemweit, benötigt sudo):**

```bash
sudo install -m 755 target/release/claude-tokens /usr/local/bin/
```

**Variante 4 — direkt mit Cargo:**

```bash
cargo install --path /pfad/zu/claude-token-monitor
# Binary landet in ~/.cargo/bin/claude-tokens
```

Hinweis: `/bin` ist auf Linux meist ein Symlink auf `/usr/bin` und für System-
Tools reserviert — eigene Binaries gehören dort nicht hin. Nimm stattdessen
Variante 1, 2 oder 3.

Test nach der Installation:

```bash
which claude-tokens
claude-tokens --version
```

## Verwendung

```text
claude-tokens [OPTIONS]

  --thread <UUID>     An eine konkrete Session binden (Session-UUID = Dateiname).
  --cwd [PATH]        An die Session binden, deren cwd zu PATH passt (Default: $PWD).
  --claude-home DIR   Überschreibt CLAUDE_HOME (Default: $CLAUDE_HOME oder ~/.claude).
  -f, --follow        Folgt der Session-Datei und gibt bei jedem neuen Assistant-Turn eine neue Momentaufnahme aus.
      --json          Einzeiliges JSON statt KEY=VALUE.
      --locate        Gibt den ermittelten Session-Pfad aus und beendet sich.
      --wait          Wartet, bis eine passende Session-Datei erscheint, statt sofort abzubrechen.
                      Damit kann der Monitor vor Claude Code gestartet werden.
      --wait-timeout SECS  Sekunden-Limit für --wait (Default: unbegrenzt).
      --all           Multi-Session: ALLE aktiven Sessions gleichzeitig verfolgen.
                      Mit --cwd auf ein Projektverzeichnis eingegrenzt; ohne --cwd
                      system-weit (jede Claude-Session auf dem Rechner). Jeder Block
                      wird mit "=== session <uuid> ===" markiert.
      --max-age MINUTES    Nur mit --all: Sessions, die länger nicht beschrieben wurden,
                           gelten als nicht-aktiv (Default: 5).
      --watch-new     Nur mit --all --follow: scannt alle 5 s nach neu aufgetauchten Sessions
                      und hängt sie live in den Stream.
      --require-open  Nur Sessions, die aktuell live sind (ein laufender claude-Prozess
                      arbeitet im Projekt; siehe unten). Linux/WSL2; kein Effekt auf anderen Systemen.
  -h, --help / -V, --version
```

Standardauswahl: die zuletzt geschriebene Session-Datei (neueste mtime).

## Schnellstart: Live-Anzeige neben einer Claude-Session

Zwei Terminals, beide im selben Projektverzeichnis. Die Reihenfolge ist
egal — dank `--wait` darf der Monitor vor oder nach Claude Code starten.

**Terminal 1 — Monitor:**

```bash
cd /pfad/zu/deinem-projekt
claude-tokens --cwd --follow --wait
```

**Terminal 2 — Claude Code:**

```bash
cd /pfad/zu/deinem-projekt
claude
```

In Terminal 1 läuft ab jetzt ein Dauerstream: nach jedem abgeschlossenen
Claude-Turn erscheint ein neuer Snapshot, getrennt durch eine `---`-Zeile.

Beenden: `Strg+C` im Monitor-Terminal.

## Mehrere Claude-Sessions im selben Verzeichnis

Standardmäßig betrachtet `claude-tokens` nur **eine** Session pro Verzeichnis
(die jüngste). Wenn du mehrere Claude-Instanzen parallel im selben
Projektverzeichnis laufen lässt — etwa in verschiedenen tmux-Panes oder
Terminals — kannst du sie alle gemeinsam erfassen:

```bash
claude-tokens --cwd --all --follow
```

Jede Session wird in der Ausgabe durch einen eigenen Block-Header sichtbar
abgegrenzt:

```text
=== session 731e7670-5501-4c62-99b4-6d48c16a9e2d ===
session_id=731e7670-5501-4c62-99b4-6d48c16a9e2d
session_cwd=/pfad/zum/projekt
percent_left=93
…
---
=== session a98977ff-89ae-4c1d-9f0a-1e2d3c4b5a6f ===
session_id=a98977ff-89ae-4c1d-9f0a-1e2d3c4b5a6f
session_cwd=/pfad/zum/projekt
percent_left=42
…
---
```

Im JSON-Modus (`--json --all`) wird stattdessen NDJSON ausgegeben: eine
JSON-Zeile pro Session und Update, ideal für `jq`-Pipelines.

### Was zählt als "aktive" Session?

Per Default werden Sessions ignoriert, deren letzte Änderung länger als
**5 Minuten** zurückliegt. Damit fallen alte Sessions desselben
Verzeichnisses automatisch raus. Das Fenster ist konfigurierbar:

```bash
claude-tokens --cwd --all --max-age 30    # 30 min - großzügiger
claude-tokens --cwd --all --max-age 1     # 1 min - nur ganz frische
```

Soll nichts gefiltert werden (z. B. für eine Archiv-Auswertung), kann
ein sehr hoher Wert verwendet werden:

```bash
claude-tokens --cwd --all --max-age 99999
```

### Neue Sessions während des Laufs erkennen

Standardmäßig folgt der Monitor nur den Sessions, die beim Start vorhanden
waren. Mit `--watch-new` scannt er zusätzlich alle 5 Sekunden das
Projects-Verzeichnis auf neu aufgetauchte Sessions:

```bash
claude-tokens --cwd --all --follow --watch-new --wait
```

Damit kannst du den Monitor *vor* Claude Code starten und beliebig viele
Claude-Instanzen nachträglich hinzustarten — sie erscheinen automatisch
im Stream.

### Nur tatsächlich laufende Sessions: `--require-open`

Der `--max-age`-Filter ist eine Heuristik über die Datei-`mtime` — sie kann
lange Denkpausen einer aktiven Session fälschlich als „nicht aktiv" werten.
Eine präzisere Variante prüft per `/proc`, ob die Session wirklich **live**
ist:

```bash
claude-tokens --cwd --all --follow --require-open --watch-new --wait
```

Anders als Codex hält Claude Code die Session-Datei **nicht** dauerhaft mit
einem Schreib-Handle offen (es hängt an und schließt wieder). Deshalb prüft
`claude-tokens` die Liveness **prozessbasiert**.

**Exakt über Claude Codes Session-Register:** neuere Claude-Code-Versionen
führen pro laufendem Prozess `~/.claude/sessions/<pid>.json` (u. a. `sessionId`
— wird bei `/resume` und `/clear` nachgezogen — und `procStart`, die
Kernel-Startzeit als Schutz gegen PID-Wiederverwendung). Nennt das Register für
einen laufenden `claude`-Prozess diese Session, ist sie live — und der Prozess
„leiht" keiner anderen Session Leben (eine frisch gestartete, noch leere
Session in einem Unterverzeichnis lässt so keine längst beendete Session des
Projekts wieder aufleben).

**Heuristik für Prozesse ohne Register-Eintrag** (ältere Versionen):

- Laufen `claude`-Prozesse, deren Arbeitsverzeichnis zum Projekt-Slug der
  Session encodiert? Ein Prozess gilt als Claude Code, wenn sein Programmpfad
  unter `…/claude/versions/…` liegt (der `comm`-Name ist unzuverlässig — je nach
  Start, z. B. `claude --resume`, ist er „claude" *oder* die bloße Version wie
  „2.1.186").
- **und** ist die Datei unter den *K neuesten* `.jsonl` ihres Projektordners,
  wobei `K` = Anzahl der laufenden `claude`-Prozesse in diesem Verzeichnis ist
  (jede Instanz schreibt genau eine Datei — so bleiben mehrere parallele
  Instanzen sichtbar, alte beendete Sessions fallen raus)?

Eigenschaften:

- Linux/WSL2-only (liest `/proc/<pid>/exe`, `…/comm`, `…/cwd`)
- Das eigene `claude-tokens`-Tool liegt nicht unter `claude/versions/` und wird
  korrekt ignoriert
- Ausgabe enthält zusätzlich das Feld `session_active=true|false`
- Alte, abgeschlossene Sessions desselben Projekts werden korrekt als
  *nicht* aktiv erkannt, auch wenn dort gerade ein neuer Claude läuft

### Filtern in Bash

Nur eine bestimmte Session aus dem Multi-Stream herauspicken:

```bash
SESSION=731e7670-5501-4c62-99b4-6d48c16a9e2d
claude-tokens --cwd --all --follow \
  | awk -v s="=== session $SESSION ===" '
      $0 == s {p=1}
      p {print}
      p && /^---$/ {p=0}
    '
```

Mit JSON-Output und `jq`:

```bash
claude-tokens --cwd --all --json --follow \
  | jq -c --arg s "$SESSION" 'select(.session_id == $s)'
```

Aggregierte Übersicht aller aktiven Sessions:

```bash
claude-tokens --cwd --all --json \
  | jq -s 'map({id: .session_id, left: .percent_left, used: .tokens_in_context})'
```

## Aufruf innerhalb der Claude-Code-TUI

Manchmal will man den Token-Stand sehen, ohne ein zweites Terminal zu öffnen.
Claude Code kann das Kommando in der laufenden Session selbst ausführen.

**Variante A — Direkt ausführen mit `!`-Präfix (empfohlen):**

In der Eingabezeile tippen:

```
!claude-tokens --cwd
```

Das `!` führt das Kommando sofort als Shell-Befehl in der Session aus, ohne das
Modell zu befragen. Die Ausgabe landet direkt im Verlauf, kostet kein
Modell-Token und bricht den laufenden Gedankengang nicht ab.

**Variante B — Claude selbst aufrufen lassen:**

Wenn das Modell die Zahlen in seine Antwort einbeziehen soll, einfach
fragen:

```
Führe claude-tokens --cwd aus und sag mir, wie viel Kontext noch frei ist.
```

Claude führt das Kommando in einem normalen Tool-Call aus und kommentiert
das Ergebnis. Kostet einen Turn.

### Warnung: `--follow` / `-f` niemals in der TUI verwenden!

`claude-tokens --follow` läuft endlos. Innerhalb der Claude-Code-TUI würde
dieses Kommando **die laufende Eingabe blockieren**, weil Claude auf das Ende
des Shell-Aufrufs wartet, das niemals kommt. Verwende `--follow`
**ausschließlich** in einem separaten Terminal.

In der TUI nur Snapshot-Aufrufe (ohne `-f`):

```text
!claude-tokens --cwd            # ok
!claude-tokens --cwd --json     # ok
!claude-tokens --cwd --follow   # NICHT verwenden!
```

Falls doch einmal `-f` in der TUI hängt: `Esc` bricht das laufende Kommando ab.
Hilft das nicht, im *anderen* Terminal `pkill -INT claude-tokens`.

## Ausgabeformat (KEY=VALUE, Default)

```text
session_id=731e7670-5501-4c62-99b4-6d48c16a9e2d
session_cwd=/pfad/zu/deinem-projekt
session_active=true
started_at=2026-07-10T06:19:37.595Z  # Zeitstempel des ersten Events = Session-Beginn
model=claude-opus-4-8           # Modell des letzten echten Turns
compact_count=2                 # Wie oft der Kontext zusammengefasst wurde (auto/manuell /compact)
turns=70                        # Anzahl Assistant-Turns mit Usage-Daten
context_window=1000000          # Aus dem Modellnamen abgeleitet (alle 4.x = 1M, mit Selbstkorrektur)
percent_left=63
percent_used=37
tokens_in_context=125499        # Aktuelle Belegung: input + cache_creation + cache_read + output des letzten Turns
session_total_tokens=5133642    # Kumulierter Verbrauch des Chats, inkl. aller Compact-Vorgänge
total_input_tokens=4949674      # Summe input + cache_creation + cache_read
total_cached_input_tokens=4936524
total_output_tokens=183968
total_tokens=5133642
last_input_tokens=125383        # Prompt des letzten Turns (input + cache_creation + cache_read)
last_cached_input_tokens=125381
last_output_tokens=116
last_total_tokens=125499
---
```

Der Trenner `---` schließt einen Block ab — relevant im `--follow`-Modus,
wo mehrere Snapshots hintereinander geschrieben werden.

Bei **Worker-Sessions** (Subagent-Transkripte unter
`<session-uuid>/subagents/agent-*.jsonl` — Hintergrund-Agenten, die eine
Session z. B. über das Agent-Tool startet) kommen zwei Felder dazu:

```text
is_worker=true
parent_session_id=731e7670-5501-4c62-99b4-6d48c16a9e2d   # die startende Session
```

Im JSON-Format sind `is_worker` (bool) und `parent_session_id` (string|null)
immer enthalten. Ein Worker gilt als aktiv, solange seine Parent-Session lebt
**und** sein Transkript in den letzten 120 s geschrieben wurde — ein fertiger
Worker ändert seine Datei nie wieder.

> Hinweis: Die kontoseitigen Rate-Limits (5h-/7d-Plan-Fenster) sind **nicht**
> Teil dieser Per-Session-Ausgabe — sie kommen aus dem separaten
> [`plan`-Subcommand](#plan-subcommand-kontoseitige-rate-limits).

## Bash-Beispiele

Direkt-Auswertung per `eval` (nur Variablen ohne unsichere Zeichen):

```bash
eval "$(claude-tokens | grep -E '^(percent_left|tokens_in_context|context_window)=')"
echo "Frei:    ${percent_left}%"
echo "Belegt:  ${tokens_in_context} / ${context_window} Token"
```

Einzelner Wert via `awk`:

```bash
PCT=$(claude-tokens | awk -F= '/^percent_left=/ {print $2}')
echo "Noch $PCT % Kontext frei"
```

JSON-Pipeline mit `jq`:

```bash
claude-tokens --json | jq '{left: .percent_left, used: .tokens_in_context}'
```

Live-Logfile schreiben:

```bash
claude-tokens --follow --json >> ~/claude-tokens.log &
```

Spezifische Session per UUID:

```bash
claude-tokens --thread 731e7670-5501-4c62-99b4-6d48c16a9e2d
```

An die Session im aktuellen Projektverzeichnis koppeln:

```bash
claude-tokens --cwd
```

## Wie wird die richtige Session erkannt?

Claude Code legt seine Sessions unter `~/.claude/projects/<encoded-cwd>/` ab.
Der Verzeichnisname ist das Arbeitsverzeichnis, in dem jedes
nicht-alphanumerische Zeichen (also nicht nur `/` und `.`, sondern z. B. auch
`_`) durch `-` ersetzt ist:

```text
/home/peter                 → -home-peter
/home/peter/.codex          → -home-peter--codex
/home/peter/nc_peter/foo    → -home-peter-nc-peter-foo
```

Der Dateiname ist die Session-UUID (`<uuid>.jsonl`). Drei Auswahl-Modi, in
dieser Priorität:

1. `--thread <UUID>` — Dateiname ist `<UUID>.jsonl`. Deterministisch.
2. `--cwd [PATH]` — encodiert PATH zum Projekt-Slug und nimmt die neueste
   `.jsonl` in diesem Projektordner.
3. Ohne Option: die jüngste Datei nach Modifikationszeit über alle Projekte.
   Bei mehreren parallel laufenden Sessions ggf. ambig — dann lieber
   `--thread`, `--cwd` oder den Multi-Session-Modus
   ([siehe oben](#mehrere-claude-sessions-im-selben-verzeichnis)) verwenden.

Die **Aktiv-Erkennung** (`session_active`) ist prozessbasiert: eine Session gilt
als live, wenn Claude Codes Session-Register (`~/.claude/sessions/<pid>.json`)
sie einem laufenden `claude`-Prozess zuordnet — ohne Register-Eintrag, wenn ein
`claude`-Prozess in ihrem cwd läuft **und** sie die neueste `.jsonl` im
Projektordner ist (siehe
[`--require-open`](#nur-tatsächlich-laufende-sessions---require-open)).

## Plan-Subcommand: kontoseitige Rate-Limits

Neben dem Per-Session-Kontextfenster (das mit `--follow` & Co. abgefragt wird)
hat dein Anthropic-Konto **rollierende Rate-Limits** über die einzelne Session
hinaus: ein gleitendes 5-Stunden-Fenster und ein gleitendes 7-Tage-Fenster, plus
optional Sonderlimits für einzelne Modelle/Features (Opus, Sonnet, …) und einen
Extra-Nutzungs-Stand (Credits).

Das Subcommand `claude-tokens plan` ruft genau diese Werte vom
Anthropic-Backend ab und gibt die rohe JSON-Antwort aus.

```bash
claude-tokens plan
claude-tokens plan | jq '.five_hour'
claude-tokens plan | jq -r '"5h: \(.five_hour.utilization)% · 7d: \(.seven_day.utilization)%"'
```

### Wie es funktioniert

```text
claude-tokens plan
   │
   │ liest claudeAiOauth.accessToken (frisch bei jedem Aufruf)
   ▼
~/.claude/.credentials.json
   │
   │ HTTPS GET mit:
   │   Authorization: Bearer <access_token>
   │   anthropic-beta: oauth-2025-04-20
   │   anthropic-version: 2023-06-01
   ▼
https://api.anthropic.com/api/oauth/usage
   │
   │ JSON-Antwort
   ▼
stdout
```

Voraussetzungen:

- **Claude Code per OAuth eingeloggt** (Pro/Max): `~/.claude/.credentials.json`
  muss einen gültigen `claudeAiOauth.accessToken` enthalten. Bei reiner
  API-Key-Nutzung gibt es diesen Endpoint nicht (→ Exit 2).
- **Netzwerk**: erreichbares `api.anthropic.com` (HTTPS, Standard-CA-Vertrauen).
  Im Gegensatz zur `claude.ai`-Web-API steht hier **kein** Cloudflare davor — der
  OAuth-Token allein genügt, ohne Browser-Cookies.

Der Subcommand schickt **nur** den Bearer-Token plus die beiden Anthropic-Header,
keine persönliche Info im User-Agent (statisch `claude-token-monitor/<version>`).
Der Token wird nicht ge-loggt, nicht ausgegeben.

### Ausgabe

Bei Erfolg (Exit 0) — die rohe JSON-Antwort des Backends:

```jsonc
{
  "five_hour": { "utilization": 30.0, "resets_at": "2026-06-10T18:20:00.731485+00:00" },
  "seven_day": { "utilization":  4.0, "resets_at": "2026-06-12T03:00:00.731513+00:00" },
  "seven_day_opus":   null,            // pro-Modell-Wochenlimit (meist nur bei Max)
  "seven_day_sonnet": null,
  "seven_day_cowork": null,
  "tangelo": null, "iguana_necktie": null, "cinder_cove": null,   // interne Feature-Töpfe
  "extra_usage": {
    "is_enabled": true, "monthly_limit": 2000, "used_credits": 0.0,
    "currency": "EUR", "utilization": null, "disabled_reason": null
  }
}
```

- `utilization` ist der verbrauchte Prozentsatz im jeweiligen Fenster.
- `resets_at` ist ein **ISO-8601-Zeitstempel** (UTC) — anders als bei Codex, das
  Unix-Sekunden liefert.
- Die `null`-Töpfe gelten für dein Konto nicht / werden nicht separat gezählt.
  Auf einem Pro-Konto sind die meisten `null`; echte Zahlen hast du bei
  `five_hour`, `seven_day` und `extra_usage`.

Bei Fehlern (Exit ≠ 0) — ein kleines Error-Envelope:

```json
{ "error": "kurze Beschreibung", "code": "kategorie" }
```

### Exit-Codes

| Code | Bedeutung | Aktion |
|---|---|---|
| `0` | Erfolg, JSON auf stdout | — |
| `2` | `auth_missing` — `.credentials.json` fehlt, kein `claudeAiOauth` (API-Key-Modus?) oder leerer Token | In Claude Code per OAuth (Pro/Max) einloggen |
| `3` | `token_expired` — Token abgelaufen (lokal `expiresAt` geprüft) oder Backend-401/403 | Eine beliebige Claude-Code-Interaktion erneuert den Token |
| `4` | `network` / `timeout` — api.anthropic.com unerreichbar oder > 10 s | Internet prüfen, später erneut versuchen |

Mit diesen vier Codes lassen sich Pipelines sauber bauen, ohne stderr zu parsen.

### Token-Lebensdauer

Das `access_token` ist ein OAuth-Token mit begrenzter Gültigkeit. Claude Code
selbst refresht es bei jeder aktiven Nutzung — das Tool tut das **nicht**, es
liest `.credentials.json` bei jedem Plan-Call frisch ein und prüft zusätzlich
das `expiresAt`-Feld lokal. Solange du Claude Code regelmäßig benutzt, ist alles
frisch. Bei abgelaufenem Token liefert `claude-tokens plan` Exit 3 — eine
einzige Claude-Interaktion erneuert die Datei.

Ein automatischer OAuth-Refresh wäre möglich, ist aber bewusst nicht
implementiert: er würde Auth-State-Konflikte mit gleichzeitig laufendem Claude
Code riskieren (beide schreiben dann `.credentials.json`).

### Bash-Beispiele

Quick-Check der 5-h-Auslastung:

```bash
claude-tokens plan | jq -r '.five_hour.utilization'
```

Reset-Countdown in Minuten (ISO-Datum → Sekunden):

```bash
claude-tokens plan | jq -r '
  .five_hour | "noch \(((.resets_at | fromdateiso8601) - now) / 60 | floor) min bis Reset"'
```

5h + 7d auf einen Blick:

```bash
claude-tokens plan | jq -r '
  "5h:  \(.five_hour.utilization)%",
  "7d:  \(.seven_day.utilization)%"'
```

Live-Counter in einer separaten tmux-Pane:

```bash
watch -n 60 'claude-tokens plan | jq -r "\"5h: \" + (.five_hour.utilization|tostring) + \"%\""'
```

### Privacy in zwei Sätzen

Der Subcommand schickt deinen Claude-OAuth-Token an genau einen Endpoint:
`api.anthropic.com/api/oauth/usage`. Sonst nichts — keine Telemetrie, kein
Drittsystem, keine Persistierung der Antwort. Volle Architektur und FAQ stehen in
[`display/README.md`](display/README.md).

## Display-App (Statusfenster ohne Browser-Chrome)

Unter `display/` liegt ein kleines Statusfenster, das die laufenden
Claude-Sessions in Echtzeit anzeigt — pro Session eine Karte mit horizontalem
Auslastungs-Balken (Verlauf grün→gelb→rot→violett), freier Token-Zahl,
Arbeitsflächen-Name + `~`-abgekürztem Projektpfad, Modell/Effort-Badge,
Idle-Timer und vier Action-Buttons. Geschlossene Sessions verschwinden
automatisch; Sessions ohne neue Aktivität (z. B. frisch gestartet, noch kein
Prompt) markiert der ⌚-Timer als „· wartet".

Über der Sessions-Liste liegt optional ein **Plan-Widget**, das die oben
beschriebenen kontoseitigen Rate-Limits als kompakte Balken-Zeilen zeigt
(`claude-tokens plan` als Datenquelle, opt-in, Consent-Modal beim ersten
Aktivieren). Details in [`display/README.md`](display/README.md).

Architektur: ein Python-Helfer-Server (`server.py`, stdlib only) spawnt
`claude-tokens --all --follow --watch-new --require-open --json` als Subprozess
und streamt die NDJSON-Snapshots per Server-Sent Events an das Browserfenster
(`index.html` + `app.css` + `app.js`). Der Browser läuft im `--app=`-Modus ohne
URL-Leiste und sieht aus wie eine native App. Das Plan-Widget nutzt einen zweiten
Pfad: `GET /plan` auf demselben Server, der `claude-tokens plan` spawnt, die
Antwort in eine stabile Form normalisiert und 60 s cached.

### Starten

**System-weit** — alle Claude-Sessions auf dem Rechner:

```bash
cd /pfad/zu/claude-token-monitor
./display/launcher.sh
```

**Auf ein Projektverzeichnis eingegrenzt:**

```bash
./display/launcher.sh /pfad/zu/deinem-projekt
```

Optional ein anderer Port (Default **8766**, damit es neben
`codex-token-monitor` auf 8765 laufen kann):

```bash
./display/launcher.sh --port 8888
```

Beenden: Fenster schließen oder `Strg+C` im Launcher-Terminal.

### Voraussetzungen

- `python3` (Standard auf Linux/WSL2)
- ein Chromium-basierter Browser (`chromium`, `google-chrome`, `brave-browser`, `microsoft-edge`)
- `claude-tokens` installiert (siehe oben) oder im Repo gebaut (`../target/release/`)
- für die Action-Buttons:
  - `xdg-open` (in jedem Linux-Desktop dabei) — für „📁 Verzeichnis öffnen"
  - `wmctrl` (`sudo apt install wmctrl`) — für „⚡ Terminal fokussieren"
  - `xclip` oder `wl-clipboard` — für „📋/↻ Kopieren" und Session-ID-Klick
  - `xdotool` (`sudo apt install xdotool`) — für das direkte Handover-Erzeugen
    per „↻" (ohne xdotool wird der Prompt nur kopiert)

## Portabilität und statischer Build

Der Standard-Build mit `cargo build --release` erzeugt ein dynamisch gegen
glibc gelinktes Binary. Es läuft auf jedem **x86_64 Linux** mit gleicher
oder neuerer glibc als das Build-System (typisch: alle aktuellen Distros,
auch WSL2 mit Standard-Ubuntu). Auf älteren Systemen, auf Alpine/musl-Distros
oder unter ARM funktioniert es so nicht.

### Statisch verlinktes Binary für maximale Portabilität

Da das Tool reines Rust ist (TLS über `rustls`/`ring`, kein OpenSSL), lässt sich
mit dem musl-Target ein **vollständig statisches** Binary bauen, das auf
praktisch jedem x86_64-Linux läuft — unabhängig von glibc-Version und
Distribution, auch in minimalen Containern.

```bash
# Einmalig: musl-Target installieren
rustup target add x86_64-unknown-linux-musl

# Statisches Binary bauen
cargo build --release --target x86_64-unknown-linux-musl

# Ergebnis (komplett portabel, ~2 MB):
# target/x86_64-unknown-linux-musl/release/claude-tokens
```

Auf Debian/Ubuntu wird zusätzlich das System-Paket `musl-tools` benötigt, weil
`ring` Assembly über einen C-Cross-Compiler baut:

```bash
sudo apt install musl-tools     # Debian/Ubuntu
sudo dnf install musl-gcc       # Fedora
```

Test, dass das Resultat tatsächlich statisch ist:

```bash
file target/x86_64-unknown-linux-musl/release/claude-tokens   # → "statically linked"
ldd  target/x86_64-unknown-linux-musl/release/claude-tokens   # → "not a dynamic executable"
```

### Andere Architekturen

| Zielsystem            | Target-Triple                     |
|-----------------------|-----------------------------------|
| x86_64 Linux glibc    | `x86_64-unknown-linux-gnu`        |
| x86_64 Linux musl     | `x86_64-unknown-linux-musl`       |
| ARM64 Linux (RPi 4+)  | `aarch64-unknown-linux-gnu`       |
| ARM64 Linux musl      | `aarch64-unknown-linux-musl`      |

Cross-Kompilation funktioniert mit `rustup target add <triple>` plus dem
passenden Linker. Für die meisten Fälle ist die musl-Variante x86_64 die
richtige Wahl. Windows-Native (außer WSL2) und macOS bräuchten ein jeweils
eigenes Build-Target.

## Robustheit gegenüber Claude-Code-Updates

Die Datenstrukturen in `src/protocol.rs` sind ein minimaler Serde-Spiegel der
JSONL-Form auf Disk. Das Tool liest nur die Felder, die es braucht
(`type`, `message.usage.{input_tokens, cache_creation_input_tokens,
cache_read_input_tokens, output_tokens}`, `message.model`, `cwd`, sowie
`system`-Events mit `subtype == "compact_boundary"`); unbekannte Felder **und
unbekannte Event-Typen** werden über `#[serde(other)]` ignoriert. Solange diese
Kernfelder bestehen, läuft das Tool ohne Anpassung weiter.

Zwei Werte muss das Tool selbst ableiten, weil Claude Code sie nicht in die
Datei schreibt:

- **Kontextfenster** — aus `message.model` gemappt (`src/models.rs`). Opus,
  Sonnet und Haiku 4.x laufen in Claude Code mit **1.000.000** Token (empirisch
  verifiziert: eine Opus-Session hielt 643k gecachte Token ohne Auto-Compaction;
  eine Haiku-Session mit 188,8k Kontext zeigte in Claude Code 19 % = /1M).
  Unbekannte Modelle defaulten auf 1M. Übersteigt der beobachtete Kontext einer
  Session das angenommene Fenster, stuft das Tool auf die nächste plausible
  Fenstergröße hoch — der Prompt kann das echte Fenster nie überschreiten, die
  Beobachtung ist also eine harte Untergrenze. So entsteht nie ein unmöglicher
  „>100%"-Balken.
- **Compact-Zähler** — Anzahl der `system`-Events mit `subtype ==
  "compact_boundary"` (Auto-Compact am Limit *oder* manuelles `/compact`).

## Lizenz

MIT
