# Handoff: claude-token-monitor

**Auftrag:** Baue ein Tool für **Claude Code** (Anthropics CLI), das dieselbe
Funktion liefert wie das fertige `codex-token-monitor` für die OpenAI-Codex-
CLI. Diese Anleitung enthält alles, was du brauchst — Mission, Vorlage,
bekannte Unterschiede zwischen den beiden Welten und einen Phasenplan.

---

## 1. Mission

Liefere zwei Komponenten:

1. **CLI `claude-tokens`** — liest live aus den Session-Dateien einer
   laufenden Claude-Code-Session den Token- und Kontext-Stand aus und
   gibt ihn auf stdout aus (KV oder JSON). Geeignet für Bash-/jq-
   Pipelines, tmux-Statusleisten, Skripte.

2. **Display-App** (Python-stdlib-Server + Chromium-`--app`-Fenster) —
   ein schlankes Statusfenster, das die Sessions als Karten visualisiert.

Das Vorbild **codex-token-monitor** liegt unter:

- Repo: <https://github.com/Robbty/codex-token-monitor>
- Lokal: `/home/peter/projekte/codex-token-monitor/`
- Aktuellste Version: **v0.5.3** (Stand 2026-06-02)

Lies dir dessen `README.md`, `display/README.md` und `scripts/build.sh`
durch — das ist deine architektonische Vorlage. Idealerweise wird
`claude-token-monitor` ein "Geschwister-Repo" mit identischer Struktur,
sodass beide nebeneinander betrieben werden können.

Zwischen v0.5.0 und v0.5.3 ist das **Plan-Widget** dazugekommen — eine
zweite Datenebene über dem bisherigen Per-Session-Tail. Es ist in den
Abschnitten 2.7 und 3.7 dieses Handoffs separat behandelt, weil es
deutliche Claude-spezifische Recherche braucht.

---

## 2. Funktionsumfang (Übernahme aus codex-token-monitor)

Übernimm möglichst 1:1, mit gleicher CLI-Schnittstelle (nur `codex`→`claude`).
**Stand der Vorlage zum Zeitpunkt dieses Handoffs:**

- letzter Tag: `v0.5.0` (CLI mit `compact_count`)
- letzter Commit auf `main`: `b497c22` — UI-Fix der Sortierreihenfolge
  (in der Vorlage noch nicht getaggt)

### CLI-Flags

| Flag | Bedeutung |
|---|---|
| `--thread <UUID>` | konkrete Session per UUID |
| `--cwd [PATH]` | Session in PATH (Default `$PWD`) |
| `--codex-home DIR` | hier: `--claude-home DIR` (Default `$CLAUDE_HOME` oder `~/.claude`) |
| `-f`, `--follow` | Live-Stream |
| `--json` | NDJSON statt KV |
| `--locate` | nur Session-Pfad ausgeben |
| `--wait`, `--wait-timeout SECS` | warten bis Session da ist |
| `--all` | Multi-Session (system-weit ohne `--cwd`, projektgebunden mit) |
| `--max-age MINUTES` | Aktivitäts-Filter via mtime (Default 5) |
| `--watch-new` | bei `--all --follow` neue Sessions live aufnehmen |
| `--require-open` | nur Sessions mit lebendigem Schreib-Handle (`/proc`) |

### Ausgabefelder (KV/JSON)

- `session_id`, `session_cwd`, `session_active`
- `context_window`, `percent_left`, `percent_used`
- `tokens_in_context`, `session_total_tokens`
- `total_input_tokens`, `total_output_tokens`, `total_cached_input_tokens`
- `last_*_tokens` Variante
- `compact_count` — wie oft der Kontext bereits komprimiert wurde
- Rate-Limit-Felder falls verfügbar (siehe Abschnitt 5)

### Display-App

- Mehrere Sessions als Karten, horizontaler Auslastungs-Balken mit
  4-Farben-Gradient (grün→gelb→rot→violett), positional (Linke bleibt
  grün; gefüllt bis `percent_used%`)
- Balken-Label: `198 k / 78% frei`
- Status-Punkt (grün=aktiv, grau=geschlossen, gelb=unbekannt)
- Idle-Timer
- Session-ID-Klick kopiert UUID
- Vier Action-Buttons: 📁 Verzeichnis öffnen, ⚡ Terminal fokussieren,
  📋 Pfad zur Session-Datei kopieren, ↻ Handover-Prompt
- `↻ N×`-Badge wenn die Session schon komprimiert wurde
- Worker-Schalter (falls Claude analoge Background-Helper hat — siehe
  Abschnitt 5)
- Hilfe-Button (📖), öffnet ein separates Chromium-Popup mit
  gerenderter README

**Sortierreihenfolge der Karten** (zuletzt geändert in
`codex-token-monitor@b497c22`):

1. echte Sessions vor Workern
2. innerhalb echter Sessions: **`compact_count` absteigend** zuerst
   (jeder Compact kostet einen kompletten Turn → Cost-Proxy)
3. Tiebreaker: `percent_used` absteigend (vollster Balken zuerst)
4. Worker am Ende, sortiert nach `session_total_tokens` absteigend

Begründung dieser Reihenfolge: würde man rein nach `percent_used`
sortieren, sackt eine Session nach Auto-Compact von ~90 % auf ~10 % und
springt unerwartet weit nach unten — der User verliert genau die Karte
aus den Augen, die ihn gerade interessiert. Mit `compact_count`-primary
bleibt die "teurere" Session prominent.

### 2.7 Plan-Widget (kontoseitige Rate-Limits)

**Neu seit v0.5.2 / v0.5.3** im Codex-Tool — und der Hauptpunkt, an
dem du für Claude eigene Recherche leisten musst, weil das Backend
ein anderes ist.

Im Codex-Tool sieht das Feature so aus: unterhalb der Topbar erscheint
optional ein **Plan-Widget**, das die rollierenden Rate-Limits des
ChatGPT-Plans anzeigt — unabhängig vom Per-Session-Kontextfenster. Die
Daten sind dieselben, die ChatGPT auf
<https://chatgpt.com/codex/cloud/settings/analytics> zeigt.

#### Bis zu vier separat togglebare Zeilen

| Zeile | Quelle | Default |
|---|---|---|
| **Plan** (Hauptkontingent) | `rate_limit.{primary,secondary}_window` | an |
| **Codex-Spark** (Premium-Feature) | `additional_rate_limits[…]` mit `limit_name == "GPT-5.3-Codex-Spark"` | aus |
| **Code-Review** | `code_review_rate_limit` | aus |
| **Credits** | `credits` (Balance + Spann-Schätzung) | aus |

Jede aktivierte Zeile rendert **zwei Balken nebeneinander**:

- **5 h-Fenster** (gleitend, 18 000 s)
- **7 d-Fenster** (gleitend, 604 800 s)

Plus rechts pro Balken: `<used_percent>% · ⌛ <reset_in>` (Reset-
Countdown). Bei schmalem Fenster wrappen die zwei Balken automatisch
untereinander (Flexbox).

Das Label der Hauptzeile zeigt den Plan-Typ aus der API:
`Plan · prolite` (oder was immer der `plan_type` im Response sagt).

Die **Credits-Zeile** zeigt keinen Balken — es gibt keinen sinnvollen
Nenner —, sondern Text:
`Balance: 0  ·  ≈ 0 lokale msg  ·  ≈ 0 cloud msg`. Bei
`overage_limit_reached: true` wird die Zeile rot.

#### Opt-in mit Consent-Modal

Beim **ersten** Anklicken des Master-Schalters poppt ein Dialog auf, der
genau erklärt:

- Welche lokale Datei gelesen wird (`~/.codex/auth.json`)
- An welchen Endpoint der Token geschickt wird
  (`https://chatgpt.com/backend-api/wham/usage`)
- Dass die Antwort nur 60 s im RAM gecacht wird, nichts auf Disk landet
- Wie man es jederzeit wieder ausschalten kann

Erst nach **„Verstanden, aktivieren"** wird `enabled: true` UND
`consent_acknowledged: true` zugleich gespeichert. Fokus liegt auf
**„Abbrechen"** — versehentliches Enter ist harmlos. Esc und
Backdrop-Klick schließen das Modal ohne Persistierung.

Konfig:

```json
{
  "plan_widget": {
    "enabled": false,
    "consent_acknowledged": false,
    "rows": {
      "main": true, "codex_spark": false,
      "code_review": false, "credits": false
    }
  }
}
```

Liegt in `~/.config/codex-token-monitor/config.json` (XDG-konform).
Server schreibt atomic (über `.tmp` + `rename`), Browser spiegelt in
`localStorage` für schnelles initiales Rendering.

#### Dynamische Zeilen-Erkennung

Wenn neue Einträge unter `additional_rate_limits` aus dem Backend
kommen (z. B. ein hypothetisches `GPT-6.0-Codex-Foo`), erkennt das
Widget diese automatisch:

1. `limit_name` wird normalisiert (`GPT-X.Y-Foo-Bar` → `foo_bar`)
2. Neuer Toggle erscheint im ⚙-Popover, **default aus**
3. Beim nächsten POST `/settings` mit-persistiert

So überrumpelt kein neues Backend-Feature den User mit einer plötzlich
sichtbaren Zeile.

#### Architektur

```text
┌──────────────┐         ┌────────────────┐         ┌─────────────────────┐
│   Browser    │ /plan   │ display/       │ spawn   │  codex-tokens plan  │
│   (app.js)   │ ──────► │ server.py      │ ──────► │  (Rust, ureq +      │
│   poll 60 s  │ ◄────── │  60 s Cache    │ ◄────── │   rustls)           │
└──────────────┘   JSON  └────────────────┘  stdout └──────────┬──────────┘
                                                              │ HTTPS GET
                                                              ▼
                              ┌────────────────────────────────────────────┐
                              │ chatgpt.com/backend-api/wham/usage         │
                              │ Authorization: Bearer <access_token>       │
                              └─────────────────┬──────────────────────────┘
                                                │ liest aus
                                                ▼
                                       ~/.codex/auth.json
```

Neue CLI-Komponente: `codex-tokens plan` Subcommand. Liest
`tokens.access_token` aus `auth.json`, ruft den Endpoint via HTTPS
(`ureq` + `rustls`, damit musl-static erhalten bleibt), gibt die
JSON-Antwort auf stdout aus.

**Exit-Codes (für Pipelines):**

| Code | Bedeutung |
|---|---|
| `0` | Erfolg, JSON auf stdout |
| `2` | `auth_missing` — `auth.json` fehlt oder leerer Token |
| `3` | `token_expired` — Backend 401/403 |
| `4` | `network` / `timeout` |

Bei Fehlern wird stattdessen ein kleines Envelope ausgegeben:
`{"error": "kurze Beschreibung", "code": "kategorie"}`.

Server cached **60 s** (Plan-Daten ändern sich ohnehin nur bei
aktiver Nutzung — schneller pollen würde nur das eigene Rate-Limit
belasten). Browser pollt ebenfalls **60 s**, Reset-Countdown tickt
clientseitig alle 30 s herunter.

#### Antwort-Schema (Auszug — für Vergleich mit Claude)

```jsonc
{
  "plan_type": "prolite",
  "rate_limit": {
    "allowed": true,
    "limit_reached": false,
    "primary_window":   { "used_percent": 27, "limit_window_seconds": 18000,  "reset_at": 1780406150 },
    "secondary_window": { "used_percent": 18, "limit_window_seconds": 604800, "reset_at": 1780903122 }
  },
  "additional_rate_limits": [
    {
      "limit_name": "GPT-5.3-Codex-Spark",
      "metered_feature": "codex_bengalfox",
      "rate_limit": { /* gleiche Struktur wie rate_limit */ }
    }
  ],
  "code_review_rate_limit": null,
  "credits": {
    "balance": "0", "has_credits": false, "unlimited": false,
    "overage_limit_reached": false,
    "approx_local_messages": [0, 0], "approx_cloud_messages": [0, 0]
  }
}
```

`reset_at` ist Unix-Sekunden UTC. `limit_window_seconds` gibt die
Fensterlänge (18000 = 5 h, 604800 = 7 d). Unbekannte Zusatzfelder
werden ignoriert.

#### Token-Refresh

Das `access_token` ist ein JWT mit typisch **~60 min Gültigkeit**.
Codex selbst refresht es bei jeder Nutzung — das Tool tut das
absichtlich **nicht** (würde Auth-State-Konflikte mit gleichzeitig
laufendem Codex riskieren). Stattdessen liest das Tool `auth.json`
bei jedem Plan-Call frisch ein. Wenn der User lange untätig war:
Exit-Code 3, UI-Hinweis „Token abgelaufen — Codex starten, damit der
Token refresht wird". Eine beliebige Codex-Interaktion (auch `!ls`)
erneuert die Datei.

#### UI-Fehlermeldungen

Vollständige Codes, die im Widget erscheinen können:

| Code | Anzeige |
|---|---|
| (keiner, Erstaufruf) | Plan-Daten werden geladen … |
| `disabled` | (Widget bleibt versteckt) |
| `auth_missing` | Nicht eingeloggt — Codex starten und einloggen. |
| `token_expired` | Token abgelaufen — Codex starten, damit der Token refresht wird. |
| `network` | Netzwerk-Problem beim Abrufen der Plan-Daten. |
| `timeout` | Timeout beim Abrufen der Plan-Daten. |
| `binary_missing` | codex-tokens-Binary nicht gefunden. |
| `binary_outdated` | codex-tokens ist zu alt für 'plan' — neue Version installieren. |
| `no_output` | codex-tokens lieferte keine Ausgabe. |
| `bad_response` | Unverständliche Antwort vom Plan-Endpoint. |
| `fetch_failed` | Verbindung zum Hilfs-Server unterbrochen. |

---

## 3. Was bei Claude technisch anders ist

Hier kommen die wichtigen Unterschiede zum Codex-Format. Das ist der
Hauptaufwand, den du leisten musst.

### 3.1 Wo liegen die Daten?

| Was | Codex | Claude Code |
|---|---|---|
| Wurzelverzeichnis | `~/.codex/` | `~/.claude/` |
| Sessions-Pfad | `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl` | `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl` |
| Verzeichnis-Schlüssel | Datum-Hierarchie | **encoded cwd** |
| ENV-Override | `CODEX_HOME` | `CLAUDE_HOME` (üblicherweise) |

**Encoded-cwd-Konvention** bei Claude: jeder `/` im cwd-Pfad wird durch
`-` ersetzt, mit führendem `-`. Beispiele:

- `/home/peter` → `-home-peter`
- `/home/peter/projekte/foo` → `-home-peter-projekte-foo`
- `/home/peter/Nextcloud/peter/projekte/trading-bots/Cursor/TradingBot`
  → `-home-peter-Nextcloud-peter-projekte-trading-bots-Cursor-TradingBot`

→ Reverse-Engineering nötig: bei `--cwd /pfad/foo` musst du das Encoding
selbst durchführen und den passenden Projects-Unterordner aufsuchen.

→ Bei `--all` ohne `--cwd` einfach alle `*.jsonl` unter
`~/.claude/projects/*/` einsammeln.

### 3.2 JSONL-Struktur

Claude-Code-Sessions sind JSONL, aber mit ganz anderem Schema als Codex:

```json
{"type":"permission-mode","permissionMode":"default","sessionId":"ed9eec09-…"}
{"type":"file-history-snapshot","messageId":"…","snapshot":{…}}
{"type":"user","cwd":"/home/peter","sessionId":"…","message":{"role":"user","content":"…"}}
{"type":"assistant","message":{"role":"assistant","content":[…],"usage":{…},"model":"claude-haiku-4-5-20251001"}}
{"type":"system","…"}
{"type":"attachment","…"}
{"type":"ai-title","…"}
{"type":"last-prompt","…"}
```

**Wichtige Events:**

- **`user` und `assistant`** sind die eigentlichen Konversationsturns.
- **`assistant`-Event enthält `message.usage`** — dort steckt die
  Token-Information. Beispiel:

  ```json
  "usage": {
    "input_tokens": 8,
    "cache_creation_input_tokens": 577,
    "cache_read_input_tokens": 36869,
    "output_tokens": 685,
    "service_tier": "standard",
    "cache_creation": { "ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 577 },
    "iterations": [ … ],
    "speed": "standard"
  }
  ```

- **`message.model`** im `assistant`-Event nennt das Modell:
  `claude-haiku-4-5-20251001`, `claude-sonnet-4-6`, `claude-opus-4-7`.

- **Erste Zeile = `permission-mode`** enthält `sessionId` als initialer
  Marker.

- **`user`-Events enthalten `cwd`** — das ist der einzige Ort, wo das
  Working Directory direkt im JSONL steht (also nicht im Dateinamen
  rekonstruierbar wie bei Codex' `session_meta`).

### 3.3 Token-Kalkulation

Bei Codex liefert das Backend einen explizit aufsummierten
`total_token_usage` und ein `model_context_window`. Claude **nicht**:

- **Du musst pro Turn aggregieren.** Iteriere über alle
  `assistant`-Events, summiere die `usage`-Felder:
  - `total_input_tokens` = Summe `input_tokens` + Summe
    `cache_creation_input_tokens` + Summe `cache_read_input_tokens`
  - `total_output_tokens` = Summe `output_tokens`
  - `total_tokens` = total_input + total_output
  - `tokens_in_context` = **Tokens des letzten Turns** (alles, was zu
    Beginn des nächsten Turns im Kontext stehen müsste — typischerweise
    `cache_read_input_tokens` + `cache_creation_input_tokens` +
    `output_tokens` des letzten Events)

- **`model_context_window` musst du selbst aus dem Modellnamen mappen:**

  | Modell | Context Window |
  |---|---|
  | claude-haiku-4-5* | 200_000 |
  | claude-sonnet-4-6* | 200_000 (Standard) bzw. 1_000_000 (Long-Context-Variante, falls aktiv) |
  | claude-opus-4-7* | 200_000 |
  | (unbekannt/ältere) | Fallback 200_000 |

  Das Mapping sollte in einer Konstante/JSON-Datei leben, leicht erweiterbar.

### 3.4 Kontext-Komprimierung erkennen

Bei Codex gibt es ein klares `context_compacted`-Event. Bei Claude
weiß ich es nicht sicher — das ist eine **Erkundungsaufgabe für Phase 1**.

Hypothesen, die du prüfen solltest:
- Gibt es einen `system`-Event mit einem Hint auf compact?
- Wird ein `ai-title`-Event nach jedem Compact neu emittiert?
- Sinkt `cache_read_input_tokens` plötzlich drastisch (= neuer Kontext)?

Wenn kein expliziter Marker existiert, kann der Heuristik-Detektor
verwendet werden: `tokens_in_context` sinkt zwischen zwei Turns
signifikant (z. B. < 20% des Vortrags).

### 3.5 Background-Worker (Claude-Desktop)

Codex-Desktop spawnt beim Start eine Reihe "Worker"-Sessions ohne
`model_context_window`. Bei Claude (Desktop oder Code) ist mir derzeit
**nicht bekannt**, ob es ein analoges Phänomen gibt. Phase-1-Aufgabe:
prüfen, ob in `~/.claude/projects/*/` Sessions ohne Modell-/cwd-Daten
auftauchen, die System-Worker sein könnten. Wenn ja: gleichen Toggle-
Mechanismus einbauen wie in Codex-Display (Schiebe-Schalter „Worker"
in der Topbar).

### 3.6 Aktiv-Erkennung

**Hier ist nichts anders** als bei Codex. Der `/proc`-basierte Check
(„hält irgendein Prozess die Datei mit einem **Schreib**-Handle offen?")
funktioniert identisch. Übernimm das Modul `src/proc.rs` 1:1.

### 3.7 Plan-Daten bei Claude — Recherche-Aufgabe

Das in Abschnitt 2.7 beschriebene Plan-Widget gibt's bei Claude
ziemlich sicher in **anderer Form** — und es ist nicht trivial, wo die
Daten liegen. Hier die Fragen, die du beantworten musst, mit Hypothesen
und Vorgehen:

#### Welche Anthropic-Plan-Sorten gibt es überhaupt?

Anthropic-Konten kommen in mehreren Schubladen mit unterschiedlichen
Rate-Limit-Mechanismen:

| Plan | Wie limitiert? | Wo sichtbar? |
|---|---|---|
| **Claude.ai Free** | Tageslimit für Nachrichten | claude.ai-UI |
| **Claude.ai Pro** | 5h-Fenster + größerer Pool | claude.ai-UI / Settings |
| **Claude.ai Max** | Wie Pro, höhere Limits | claude.ai-UI / Settings |
| **Claude.ai Team/Enterprise** | Org-Quoten | Admin-Konsole |
| **Anthropic API** | Pro-Org-Tier (1/2/3/4), x-ratelimit-*-Header in jedem Response | API-Header + console.anthropic.com |
| **Claude Code** | hängt davon ab, *womit* der User authentifiziert ist (Pro/Max oder API-Key) | je nach Auth-Modus unterschiedlich |

Claude Code kann nämlich beides: gegen das Claude.ai-Abo gehen
(OAuth-Login mit Pro/Max) **oder** gegen einen API-Key (Pay-as-you-go,
API-Limits). Das Tool muss **beide Wege** abdecken — oder zumindest
sauber erkennen, welcher gerade aktiv ist.

#### Hypothesen — wo könnten die Daten herkommen?

**Hypothese A — JSONL selbst:** Die Session-`.jsonl` unter
`~/.claude/projects/*/` könnte in den `assistant`-Events bereits
Rate-Limit-Hinweise mitliefern (analog zu Codex' `rate_limits`-Feld in
den Rollouts). Schau dir die `usage`-Objekte und alle `system`-Events
durch — bei der Codex-CLI tauchen Rate-Limit-Blöcke gelegentlich in
den Logs auf, vielleicht auch hier.

**Hypothese B — claude-CLI-Subcommand:** Was sagt `claude --help`?
Probiere mindestens:
```bash
claude --help | grep -iE 'usage|limit|quota|account|plan'
claude usage   2>&1
claude limits  2>&1
claude account 2>&1
claude doctor  2>&1
claude login status 2>&1
```
Das war beim Codex-Tool **vergeblich** (keine Subcommands), könnte
aber bei Claude besser stehen — Anthropic-CLIs sind generell weniger
verschachtelt.

**Hypothese C — Auth-Token + Backend-Endpoint:** Wenn der User per
OAuth eingeloggt ist, liegt ein Token in `~/.claude/` (Datei finden!).
Suchen mit:
```bash
find ~/.claude/ -maxdepth 2 -type f -name '*.json' | xargs ls -la
```
Wenn ein Token-File da ist, in claude.ai mit F12 → Network den
Plan-/Usage-Endpoint identifizieren (genau die Mechanik, die wir auch
für Codex genutzt haben — siehe `display/server.py::_serve_plan` im
Codex-Repo als Implementierungs-Muster). Anthropic-Domains, die in
Frage kommen: `claude.ai`, `console.anthropic.com`, `api.anthropic.com`.

**Hypothese D — API-Response-Header:** Bei API-Key-Nutzung kommen die
Limits **in jedem Response-Header** zurück
(`anthropic-ratelimit-requests-remaining`,
`anthropic-ratelimit-tokens-remaining`,
`anthropic-ratelimit-*-reset` usw. — siehe Anthropic-API-Doku). Die
JSONL-Files speichern die Header **nicht direkt**, aber falls Claude
Code sie in einem `system`-Event mitloggt, ließe sich der jeweils
letzte Stand pro Session ablesen und live aggregieren.

#### Empfohlenes Vorgehen

1. Erst Hypothese B (CLI-Subcommands) — das ist 5 Minuten.
2. Dann Hypothese A (JSONL-Inhalt) — `grep -c 'rate\|limit' ~/.claude/projects/*/*.jsonl`.
3. Wenn beides leer ist: Hypothese D (API-Header) für API-Key-Nutzer
   und Hypothese C (Backend-Endpoint) für OAuth-Nutzer.

Für das **Konsentmodal** der Display-App: der Text muss klar machen,
welches der beiden Auth-Modelle gerade aktiv ist und woher die Daten
kommen werden. Beim Codex-Tool war das einfach — bei Claude wird es
plan-abhängig differenziert.

#### Was die UI-Schicht NICHT zu interessieren braucht

Wenn du das Datenmodell richtig schneidest, ist das **Plan-Widget
strukturell identisch** zum Codex-Tool — egal woher die Zahlen kommen.
Die CLI emittiert auf stdout dasselbe JSON-Schema wie `codex-tokens
plan`:

```jsonc
{
  "plan_type": "Pro",
  "rate_limit": {
    "primary_window":   { "used_percent": …, "limit_window_seconds": 18000,  "reset_at": … },
    "secondary_window": { "used_percent": …, "limit_window_seconds": 604800, "reset_at": … }
  },
  "additional_rate_limits": [ ... ],
  "credits": { ... }
}
```

— und alle UI-Komponenten lassen sich 1:1 wiederverwenden. Das ist
der **Architektur-Vorteil**, an dem du dich orientieren solltest:
Datenquelle hinter dem `plan`-Subcommand kapseln, alles oberhalb
gleich lassen.

Falls Claude weniger/mehr Felder hat als Codex (z. B. ein Tageslimit
statt Wochenlimit für Free-User): `secondary_window` ist optional, die
UI rendert nur, was da ist.

---

## 4. Empfohlene Phasen

### Phase 1: Erkundung (1–2 Stunden)

1. Klone das Vorbild zur Referenz:
   ```bash
   git clone https://github.com/Robbty/codex-token-monitor /tmp/cd
   ```
2. Inspiziere das Format. Nimm 3–5 echte Claude-Sessions aus
   `~/.claude/projects/`, schreibe ein kleines Python-Skript, das
   `usage`-Verläufe extrahiert und prüfe Hypothesen aus Abschnitt 3.4.
3. **Beantworte vor allem:**
   - Welche `assistant`-`message.model`-Werte tauchen real auf?
   - Gibt es einen expliziten Compact-/Summary-Indikator?
   - Schreibt Claude die JSONL-Datei wirklich open-handle-mäßig oder via
     atomic-rename? (Letzteres würde den `/proc`-Check brechen.)
   - Gibt es Worker-artige Sessions ohne user/cwd?
4. Lege ein Repo `claude-token-monitor` an (analog Struktur).

### Phase 2: Rust-CLI

1. `Cargo.toml`-Setup analog (`edition = "2024"`, gleiche Dependencies:
   `serde`, `serde_json`, `anyhow`, `clap`, `dirs`).
2. `src/protocol.rs` — Claude-spezifische Serde-Mirrors der JSONL.
3. `src/state.rs` — `TokenState` mit `compact_count`, `session_active`.
4. `src/locate.rs` — Selector-Logik adaptiert: Project-Slug-Encoding,
   Walking, mtime-Filter.
5. `src/proc.rs` — **1:1 übernehmen** vom Codex-Repo.
6. `src/tail.rs` — sehr wahrscheinlich 1:1 übernehmbar.
7. `src/render.rs` — gleiche KV-/JSON-Felder.
8. `src/main.rs` — gleiche CLI-Schnittstelle.

**Wichtig:** halte das Datenmodell schmal — nur Felder, die wir brauchen,
mit `#[serde(other)]`-Catch-All, damit zukünftige Claude-Updates nicht
brechen.

### Phase 3: Display-App (Basis ohne Plan-Widget)

`display/`-Ordner mit identischer Struktur:
- `server.py` (Python-stdlib, SSE-Bridge zu `claude-tokens`)
- `index.html`, `static/{app.css,app.js,marked.min.js}`
- `help.html`, `README.md`, `launcher.sh`

Nur kleine Anpassungen: Default-Port (z. B. 8766 statt 8765, damit
beide parallel laufen können), Branding-Text.

Dieses ist ein guter Zwischenstand zum Release einer **v0.1.0** —
voll funktionsfähiges Per-Session-Tail-Tool. Plan-Widget folgt in
Phase 4 als nächste Minor-Version (v0.2.0).

### Phase 4: Plan-Widget

Setzt voraus, dass die Recherche aus 3.7 abgeschlossen ist. Dann
analoger Aufbau zum Codex-Tool — siehe als Implementierungs-Vorlage
direkt:

- `codex-token-monitor/src/plan.rs` (CLI-Subcommand: lese
  Auth-Datei, HTTPS-Call, JSON auf stdout, differenzierte Exit-Codes)
- `codex-token-monitor/src/main.rs` (clap-Subcommand-Pattern mit
  optionalem `command` neben dem flat-flag Mode für Backwärtskompatibilität)
- `codex-token-monitor/display/server.py` (Helpers `_load_settings`,
  `_save_settings`, `_get_plan_cached`, Handler `_serve_settings`,
  `_serve_plan`, `_handle_update_settings`)
- `codex-token-monitor/display/static/app.js` (Plan-Widget-Rendering
  ab Kommentar „Plan widget (account-level rate limits — opt-in)")
- `codex-token-monitor/display/static/app.css` (`.plan*`-Klassen,
  `.modal*`-Klassen)
- `codex-token-monitor/display/index.html` (Plan-Widget-Container,
  Settings-Popover-Block, Consent-Modal-Block)

Reihenfolge der Arbeitsschritte:

1. **CLI-Subcommand** zuerst (`claude-tokens plan`) — Standalone
   testbar ohne Browser. JSON-Output verifizieren.
2. **Server-Endpoints** (`/settings`, `/plan`). Cache + Opt-in-Gate
   per 403. Atomar geschriebene Config-Datei.
3. **UI** zuletzt: Topbar erweitern, Plan-Widget rendern, Settings-
   Popover, Consent-Modal. Default-Fokus auf „Abbrechen" im Modal.

**Wichtig bei der Subcommand-Integration in clap:** das Codex-Tool
hat einen optionalen `command: Option<Command>` neben den flat-Flag-
Args, damit `claude-tokens --all --follow` weiterhin ohne Subcommand
funktioniert. Bei Bedarf in dessen `src/main.rs` nachschauen.

**Wichtig bei rustls + musl-static:** ureq 2.x mit
`default-features = false, features = ["tls", "json"]` zieht rustls
plus webpki-roots. musl-static funktioniert dann, **aber das
`musl-tools`-System-Paket muss installiert sein**
(`sudo apt install musl-tools` auf Ubuntu/Debian) — `ring` kompiliert
Assembly via C-Cross-Compiler. Beim Codex-Tool ist das in v0.5.2
genau über diese Hürde gestolpert.

**Atomare Setting-Writes:** Server.py schreibt `config.json.tmp` und
benennt um — wichtig, damit ein gleichzeitiger Lesezugriff nie eine
halbe Datei sieht. **Nicht** mit `os.replace()` ersetzen, das ist auf
Linux das gewünschte Atomic-rename-Verhalten.

**Token-Refresh** absichtlich NICHT selbst implementieren — siehe
Erläuterung in 2.7. Bei abgelaufenem Token Exit-Code 3 + UI-Hinweis,
fertig.

Endziel dieser Phase: v0.2.0 mit funktionierendem Plan-Widget.

### Phase 5: Bundle + Release

- `scripts/build.sh` (1:1 portierbar, nur Pfade umbiegen)
- v0.2.0-Tag mit Tarball-Asset
- GitHub-Repo öffentlich publizieren (analog `Robbty/claude-token-monitor`)

---

## 5. Bekannte Klippen aus dem Codex-Tool (lesen statt nochmal selbst tappen)

Diese Lektionen sind bereits gelernt — übernimm sie direkt:

1. **CSS-Spezifität bei `.hidden`**: Wenn `.hidden { display: none }` vor
   `.modal { display: flex }` definiert ist, gewinnt das spätere `.modal`
   (gleiche Spezifität). Lösung: `.modal.hidden { display: none }`.

2. **Modal als HTML-Overlay funktioniert, aber separates OS-Fenster ist
   besser**: `window.open(url, name, "popup=yes,...")` in Chromium-`--app`-
   Mode → frei verschiebbares, dragbares Fenster ohne URL-Leiste.

3. **`wmctrl -a <pattern>`** nimmt das ERSTE Match — Dateimanager
   blockieren oft das gewünschte IDE-Fenster. Lösung: `wmctrl -lx` plus
   WM_CLASS-Scoring, das IDE-Klassen bevorzugt und Dateimanager
   ausschließt. Siehe `codex-token-monitor/display/server.py::_handle_focus_terminal`.

4. **Path-Remap bei Rust-Release-Builds**: `RUSTFLAGS="--remap-path-prefix
   $HOME/.cargo=/cargo ..."` setzen, sonst leakt der User-Pfad
   `/home/<user>` in die Binary (in Panic-Meldungen). Siehe
   `scripts/build.sh`.

5. **`/proc`-Aktiv-Check nur Schreib-Handles zählen** (`fdinfo/<n>`
   parsen, `flags & 0o3 != 0`). Sonst werden lesende Tail-Reader (das
   Tool selbst) als „aktiv" gemeldet → false positive.

6. **Worker-Sessions ohne `model_context_window`** brauchen einen
   Fallback (`effectiveContextWindow`-Funktion): erst Wert einer echten
   parallel laufenden Session übernehmen, dann eine Modell-spezifische
   Konstante. Bei Claude relevant, weil das Backend NIE explizit ein
   Context-Window meldet — der Fallback ist das primäre Verfahren.

7. **`git add -A` ist gefährlich**. Lieber spezifische Pfade adden.
   Sonst landen `__pycache__/`, generierte PDFs etc. im Repo.

8. **Display-Files in `display/static/`** halten — `index.html` lädt
   `/static/app.css`, `/static/app.js` etc. Trennung von Server-/UI-
   Code von Anfang an.

9. **Atomare stdout-Writes im Multi-Thread**: ein vollständiger
   Snapshot-Block muss in einem einzelnen `write_all` rausgehen, damit
   parallele Tail-Threads nicht ihre Ausgaben verschränken.

10. **`--watch-new` rescannt regelmäßig** das Sessions-Verzeichnis und
    erkennt nachträglich gestartete Sessions. Default-Intervall 5 s
    funktioniert in der Praxis gut.

11. **Sortierung der Display-Karten NICHT allein nach `percent_used`**
    — sonst springt eine gerade beobachtete Session nach Auto-Compact
    ohne Vorwarnung weit nach unten in der Liste und verschwindet aus
    dem Blickfeld. Stattdessen: primär `compact_count`, sekundär
    `percent_used`. Das hält teure Sessions stabil oben.

12. **`session_total_tokens` enthält bereits die Compact-Tokens**.
    Codex (und vermutlich Claude) zählt im akkumulierten Token-Wert
    auch die Token, die für die Zusammenfassungs-Turns verbraucht
    wurden. Du musst daher NICHT zusätzlich Pre-/Post-Compact-Summen
    tracken — die Σ-Summe ist bereits die wahre Gesamtsumme des Chats.
    Tooltip muss das aber klar erklären, sonst hält der Nutzer den
    Wert für unplausibel hoch.

13. **Subprocess-Output strikt validieren**: Wenn `claude-tokens plan`
    aus dem Server gespawnt wird und leeren stdout zurückgibt, ist
    das KEIN „erfolgreiche Antwort, halt leer" — es ist mit hoher
    Wahrscheinlichkeit ein **veraltetes Binary**, das das `plan`-
    Subcommand noch nicht kennt (Symptom dann: `error: unexpected
    argument 'plan' found` auf stderr). Lösung im Codex-Tool: stderr
    capturen, leer-stdout als eigenen Fehlertyp `binary_outdated`
    behandeln, im UI eine klare Aufforderung „neue Version
    installieren" anzeigen. Niemals leeren stdout silent als `{}` an
    den Browser weiterreichen — das versteckt den Bug genau dort,
    wo's am ungünstigsten ist.

14. **Modal braucht drei Schließ-Wege**: Cancel-Button, Esc-Taste,
    Backdrop-Klick. Wer nur einen davon implementiert, baut für sich
    selbst eine Falle — Tester werden zwangsläufig den anderen Weg
    versuchen, und es fühlt sich kaputt an, wenn nichts passiert.
    Default-Fokus auf den sicheren Choice (Cancel/Abbrechen), nicht
    auf Confirm — versehentliches Enter darf nicht zu einer
    unbeabsichtigten Aktion führen.

15. **Plan-Widget Polling NICHT zu häufig**. Die Backend-Antwort
    ändert sich nur bei aktiver Nutzung des Chat-Backends — kein
    Bedarf für sub-minütiges Pollen. Außerdem zählt der Plan-Call
    selbst aufs Rate-Limit. 60 s Browser-Poll + 60 s Server-Cache
    ist der sweet spot. Reset-Countdown im UI tickt client-seitig
    alle 30 s (rein DOM, kein zusätzlicher Backend-Call).

16. **Auth-Token frisch einlesen pro Plan-Call** statt einmal beim
    Server-Start. Begründung: der Token wird vom CLI-Tool selbst
    (Codex / Claude) während aktiver Nutzung in der `auth.json`
    aktualisiert. Beim Server-Start gecached, würden wir nach 60 min
    auf einen abgelaufenen Token zeigen, obwohl daneben ein frischer
    in der Datei liegt. Aus `plan.rs` heraus jeden Call frisch
    `auth.json` lesen, dann Bearer setzen. Trivial in IO, robust.

17. **Token-Refresh selbst zu implementieren ist heikel**: Anthropic
    /OpenAI legen via OAuth einen `refresh_token` ab, mit dem das
    eigentliche CLI-Tool den `access_token` erneuert. Wenn das Tool
    das parallel auch tut, gibt es Race Conditions mit dem CLI: einer
    schreibt `auth.json`, der andere liest gerade — Dateninkonsistenz
    möglich. Lieber Token-Expiry sauber als eigenen Fehlerzustand
    melden, der User wird in der Praxis sowieso gleich `codex`/`claude`
    erneut nutzen.

---

## 6. Was am Ende stehen sollte

- Repo `claude-token-monitor` (lokal + GitHub) mit gleicher Struktur
- Beide Tools parallel installierbar in `~/.local/bin/`
  (`codex-tokens` und `claude-tokens`)
- Beide Display-Apps parallel startbar (verschiedene Default-Ports)
- Beide CLIs haben einen `plan`-Subcommand, der das jeweilige Backend
  abfragt und ein **identisches JSON-Schema** ausgibt — sodass eine
  Visualisierungskomponente theoretisch beide Tools auswerten könnte
  (interessant für eine zukünftige Meta-Übersicht „alle Plan-Stände
  auf einen Blick")
- Wer mag, kann beide Statusfenster nebeneinander offen halten und
  sieht: rechts seine Codex-Sessions samt Plan-Auslastung, links
  seine Claude-Code-Sessions samt Plan-Auslastung

---

## 7. Wenn du loslegst

Erste Schritte für den neuen Chat:

```bash
# 1. Vorlage holen
git clone https://github.com/Robbty/codex-token-monitor /tmp/codex-token-monitor

# 2. Erkunde echte Claude-Sessions
ls ~/.claude/projects/
head -5 ~/.claude/projects/*/[0-9a-f]*.jsonl 2>/dev/null | head -30

# 3. Im neuen Verzeichnis Projekt aufsetzen
cd /home/peter/Schreibtisch/projekte/claude-token-monitor
cargo init --name claude-token-monitor

# 4. README mit Mission + diesem Handoff als Quelle
cp HANDOFF.md README.md  # als Ausgangspunkt
```

Viel Erfolg. Frag den User Robbty (Peter) bei Designentscheidungen
nach — er hat das Codex-Tool engmaschig mit-designt und kennt die
gewünschte UX im Detail.

— Vorgänger-Chat, 2026-05-24
— Ergänzung Plan-Widget (Abschnitte 2.7, 3.7, Phase 4, Klippen 13–17),
  2026-06-04. Stand des Vorbilds: codex-token-monitor v0.5.3.
