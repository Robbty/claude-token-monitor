# Claude Token Display — Bedienung

Diese Anleitung beschreibt das **Statusfenster**, das du gerade vor dir
hast: pro laufender Claude-Code-Session eine Karte mit Auslastungs-Balken,
Token-Werten, Idle-Timer und vier Action-Buttons.

Für die **CLI-Bedienung von `claude-tokens`** (Pipelines, tmux, Skripte,
Cronjobs) sowie für Build- und Installations-Anleitungen → siehe die
**[Projekt-README öffnen](/help?doc=main)** (auch über den 📚-Button
oben in dieser Toolbar erreichbar; sie öffnet in einem zweiten Fenster).

---

## Display-App starten

```bash
# System-weit: alle laufenden Claude-Sessions auf dem Rechner
./display/launcher.sh

# Eingegrenzt: nur Sessions in einem Projektverzeichnis
./display/launcher.sh /pfad/zu/deinem-projekt

# Anderer Port (Default: 8766)
./display/launcher.sh --port 8888
```

Beenden: Fenster schließen oder im Launcher-Terminal `Strg+C`.

Der Launcher findet `claude-tokens` automatisch im `$PATH` oder unter
`../target/release/`. Über `CLAUDE_TOKENS_BIN=/pfad/zu/claude-tokens` lässt sich
ein konkretes Binary erzwingen.

Der Default-Port **8766** ist bewusst ein anderer als der von
`codex-token-monitor` (8765) — beide Statusfenster können also gleichzeitig
offen sein (rechts Codex, links Claude).

### Desktop-Icon (zum Anklicken)

Ein Skript legt einen Menü-Eintrag und ein anklickbares Desktop-Symbol an:

```bash
./display/install-desktop.sh                 # system-weit
./display/install-desktop.sh --cwd /pfad     # auf ein Projekt eingegrenzt
./display/install-desktop.sh --no-desktop    # nur Anwendungsmenü, kein Icon
```

Es erzeugt `claude-token-monitor.desktop` in `~/.local/share/applications/`
(Anwendungsmenü) und auf dem Desktop, mit dem mitgelieferten `icon.png`. Pfade
werden dynamisch ermittelt und `claude-tokens` (PATH oder
`../target/release/`) fest in die `Exec`-Zeile geschrieben (`CLAUDE_TOKENS_BIN`),
damit der Start auch aus einer minimalen Desktop-Session-`PATH` klappt — bei
verschobenem Repo das Skript einfach erneut ausführen.

Beim ersten Doppelklick verlangen XFCE/GNOME ggf. eine Bestätigung
(Rechtsklick → „Diese Datei ausführbar machen" / „Allow Launching"). Das Skript
markiert das Icon bereits als vertrauenswürdig (`gio … metadata::trusted true`),
was die Nachfrage in den meisten Fällen erübrigt.

## Die Karte im Detail

```text
╭──────────────────────────────────────────────────────────╮
│ ▮▮▮▮░░░░░░░░░░░░░░░░░░░░  726 k / 73% frei              │  ← Balken oben
│ Allgemeines · ~/projekte/claude-token-monitor  (Opus 4.8·high) ● ⌚ 0s │  ← Arbeitsfläche · cwd · Modell·Effort · Status · Idle
│ 4d7ef72b-af9e · 274k / 1.0M · Σ 33.7M  [📁][⚡][📋][↻]  │  ← ID · Verbrauch · Buttons
╰──────────────────────────────────────────────────────────╯
```

### Arbeitsfläche · Pfad

Der Projektpfad wird mit `~` abgekürzt (Tooltip zeigt den vollen Pfad). Davor
steht — gedimmt — der **Name der Arbeitsfläche**, auf der das Terminal der
Session liegt. Ermittelt wird das Fenster wie beim ⚡-Knopf (exakt über
`WINDOWID` bei laufender Session, sonst Titel-Heuristik; siehe unten) und die
Arbeitsfläche über `wmctrl -d`. Kein Präfix erscheint, wenn `wmctrl` fehlt
oder kein Fenster zuzuordnen ist (z. B. IDE-integriertes Terminal ohne
`WINDOWID`).

### Balken (oben)

- **Füllgrad** entspricht dem prozentualen Kontextverbrauch.
- **Farbe** ist *positional* mit vier Stützpunkten: grün → gelb → rot → violett.
  Der linkeste Pixel bleibt immer grün; neue Pixel beim Wachsen nehmen die
  Farbe ihrer X-Position.
- **Hover-Tooltip** zeigt alle Werte: Token verbraucht, frei, Kontextfenster
  gesamt, Prozentangaben.

### Modell · Effort

Rechts neben dem Pfad steht — sofern bekannt — ein kleines Badge mit dem
**Modell** der Session (z. B. „Opus 4.8", aus der `model`-Angabe der letzten
Antwort abgeleitet) und, falls vorhanden, dem **Reasoning-Effort**
(`low`/`medium`/`high`). Der Tooltip nennt die vollständige Modell-ID. Sessions,
die noch kein Modell gemeldet haben, zeigen das Badge nicht.

### Statuspunkt (●)

- **Grün** — ein `claude`-Prozess läuft in diesem Projekt, die Session ist live.
- **Grau** — Claude-Session beendet, letzter Stand eingefroren (Karte
  verschwindet innerhalb von ~3 Sekunden).
- **Gelb** — Status unbekannt (kein Linux/WSL2, oder `/proc` nicht lesbar).

Die Aktiv-Erkennung ist prozessbasiert: Claude Code hält die Session-Datei nicht
dauerhaft offen, deshalb wird geprüft, ob ein laufender `claude`-Prozess im cwd
der Session arbeitet **und** ob es die neueste Session-Datei dieses Projekts ist.

### Idle-Timer (⌚) und „wartet"-Markierung

Zeigt, wie lange die Session-Datei **nicht mehr beschrieben** wurde (echte
Datei-mtime, alle 3 s aktualisiert — nicht der Verbindungszeitpunkt). Liegt das
länger als **10 Minuten** zurück, wird der ⌚-Timer **gelb** und um ein
explizites „· wartet" ergänzt (die Session lebt, wartet aber auf Eingabe); der
Tooltip erklärt „Werte sind noch nicht live". Der Balken selbst bleibt voll hell
(alle Balken haben denselben Kontrast), damit die Markierung nicht wie ein
Rendering-Fehler wirkt. Das Fenster ist großzügig, weil ein arbeitender Agent
(langer Tool-Lauf, lange Generierung, Sub-Agents) durchaus Minuten zwischen
Schreibvorgängen liegen kann — der ⌚-Timer zeigt das echte Alter ohnehin.

Das ist genau der Fall einer **frisch gestarteten Session, in der noch kein
Prompt lief** — dann existiert nur die Datei der Vorgänger-Session, deren Zahlen
gezeigt werden, bis der erste Turn läuft. Sobald du in der Session etwas
ausführst, ist die Karte sofort wieder live.

### Session-ID (klickbar)

Die gekürzte UUID. **Klick kopiert die vollständige Session-ID** in die
Zwischenablage — praktisch für `claude-tokens --thread <UUID>`.

### Action-Buttons

| Button | Aktion |
|---|---|
| **📁** | Öffnet `session_cwd` im Dateimanager — auf der Arbeitsfläche des Projekts: Ein bereits offenes Dateimanager-Fenster mit dem Ordner wird nach vorne geholt (inkl. Arbeitsflächen-Wechsel); sonst wird erst auf die Arbeitsfläche des Session-Terminals gewechselt und dort geöffnet (`wmctrl` + `xdg-open`; ohne `wmctrl` einfach `xdg-open`) |
| **⚡** | Holt das Fenster der Session nach vorne und wechselt dabei auf dessen Arbeitsfläche (`wmctrl`). Bei **laufender** Session ist das exakt das Terminal, in dem der Claude-Prozess läuft — das Terminal vererbt seine Fenster-ID (`WINDOWID`) an den Prozess; Titel, Ordnernamen oder Arbeitsflächen spielen dann keine Rolle. Sonst greift die Heuristik: nur Terminal-/IDE-Fenster kommen infrage (andere Fenster zögen auf ihre Arbeitsfläche), gematcht über den aktuellen Chat-Titel der Session (Claude Code ersetzt den Terminal-Titel während der Arbeit durch den Chat-Titel), über den Pfad im Fenstertitel (auch `~`-abgekürzt) und notfalls ein eindeutiges „✳ Claude Code"-Terminal; Gleichstände löst die Arbeitsfläche mit weiteren Projekt-Fenstern im Titel auf |
| **📋** | Kopiert den absoluten Pfad zur Session-Datei (`~/.claude/projects/…/<uuid>.jsonl`) in die Zwischenablage |
| **↻** | Erzeugt das Handover **direkt in der Session**: Der Prompt wird ins Terminal der Session eingefügt und abgeschickt (Zwischenablage + Paste-Tastendruck + Return per `xdotool`; nur bei exaktem `WINDOWID`-Match, siehe ⚡). Ein bestehendes `HANDOVER.md` wird dabei nur aktualisiert. Die Karte zeigt danach „⏳ Handover läuft" und fragt, sobald `HANDOVER.md` geschrieben und die Session wieder ruhig ist: „Chat schließen? Ja/✕" — „Ja" schickt `/exit` an dasselbe Terminal. Ohne `xdotool`, ohne exaktes Fenster (z. B. IDE-Terminal) oder bei beendeter Session wird der Prompt wie früher **nur kopiert** (Toast nennt den Grund) |

## Bedeutung der Zahlen

| Wert | Bedeutung |
|---|---|
| **726 k / 73 % frei** | Aktuell freie Token absolut + gleichbedeutende Prozentangabe |
| **274 k / 1.0 M** | Aktuell belegt / Kontextfenster-Größe insgesamt (alle 4.x-Modelle = 1 M) |
| **Σ 33.7 M** | Kumulierter Token-Verbrauch über die gesamte Session, inklusive aller Compact-Vorgänge und Cache-Reads |
| **↻ 2×** | (Badge, nur sichtbar wenn > 0) Wie oft der Kontext zusammengefasst wurde — automatisch (Limit erreicht) oder über `/compact`. Die Σ-Summe enthält auch die Token dieser Compaction-Turns. |

Faustregel bei viel Verbrauch: ↻-Button drücken — das Handover entsteht direkt
in der Session; nach der Schließen-Nachfrage eine neue Session starten. Wenn
Claude stattdessen selbst auto-compact, wandert der ↻-Zähler eins hoch.

> Hinweis zur Σ-Summe: Sie wirkt oft sehr hoch, weil Claude pro Turn fast den
> kompletten Kontext erneut als `cache_read_input_tokens` zählt. Das ist die
> echte Gesamtsumme aller verarbeiteten Token des Chats — kein Fehler.

## Lade- und Frische-Zustände

Beim Start (oder nach einem Reconnect) braucht es einen Moment, bis frische
Daten vom Server da sind. Damit kein irreführendes „Ergebnis" gezeigt wird,
unterscheidet das Display drei Zustände sauber:

- **Verbinde / Lädt** — solange die SSE-Verbindung noch nicht steht bzw. kurz
  danach, erscheint ein pulsierender Platzhalter („Verbinde mit dem Server …" /
  „Suche aktive Claude-Sessions …") statt der „Keine aktiven Sessions"-Meldung.
- **Live** — frische Daten sind da, Balken hell.
- **Wartet** — Daten älter als 10 min: gelber ⌚-Timer mit „· wartet"
  (Balken bleibt hell; siehe [Idle-Timer](#idle-timer--und-wartet-markierung) oben).

Auch das **Plan-Widget** zeigt beim ersten Laden „Plan-Daten werden geladen …"
und überbrückt vorübergehende Fehler (kalter Cache, Token setzt sich gerade)
mit ein paar stillen Wiederholungen, bevor es überhaupt einen Fehler anzeigt —
so erscheint nie fälschlich „kein Plan", nur weil die erste Abfrage noch lief.

Die Schwellen lassen sich oben in `display/static/app.js` justieren:
`STALE_AFTER_MS` (Default 600 000 = 10 min) und `SETTLE_MS` (1 800 ms).

## Sortierung der Karten

Das Dropdown in der Kopfzeile bestimmt die Reihenfolge der Session-Karten
(persistiert als `display.sort` in der Config):

- **Auslastung** (Default) — am häufigsten komprimierte Session zuerst (jede
  Compaction kostet einen ganzen Turn, also ein „Verbrauch bisher"-Proxy),
  bei Gleichstand der vollere Balken.
- **Ort** — alphabetisch nach Projektpfad.
- **Startzeit** — nach Beginn der Session, die älteste zuerst (stabile
  Reihenfolge: Karten springen beim Arbeiten nicht umher).

Echte Sessions stehen dabei immer vor Workern, Worker vor leeren Sessions —
die gewählte Sortierung gilt innerhalb dieser Gruppen.

## „Worker"-Schalter

Claude Code startet für manche Aufgaben **Hintergrund-Worker** (Subagenten —
z. B. über das Agent-Tool oder Workflows). Jeder Worker hat ein eigenes
Kontextfenster und verbraucht eigene Token; sein Transkript liegt unter
`~/.claude/projects/<projekt>/<session-uuid>/subagents/agent-*.jsonl`.

Worker sind standardmäßig **ausgeblendet** — das Zählerlabel zeigt sie als
`X aktiv (+Y Worker)`. Der **„Worker"-Schalter** in der Kopfzeile blendet sie
als eigene Karten ein: mit ⚙-Badge (Tooltip nennt die startende Session),
normalem Kontext-Balken und den üblichen Aktionen — nur der ↻-Rollover-Knopf
fehlt, weil ein Worker kein fortsetzbarer Chat ist. Ein Worker gilt als
laufend (●), solange seine Session lebt und sein Transkript in den letzten
2 Minuten geschrieben wurde; danach friert die Karte ein und verschwindet
wie eine beendete Session.

## „Leere"-Schalter

Sessions, die noch **kein nutzbares Kontextfenster** melden (z. B. brandneu und
bisher nur mit synthetischen Events, also ohne echtes Modell), haben keinen
sinnvollen Balken und sind standardmäßig ausgeblendet. Das Zählerlabel zeigt sie
als `X aktiv (+Y leer)`.

Schiebe den **„Leere"-Schalter** in der Kopfzeile um, um sie einzublenden. Ihr
Balken füllt sich dann auf Basis eines **angenommenen** Kontextfensters (Wert
einer parallel laufenden echten Session, sonst der Default `ASSUMED_CONTEXT_WINDOW
= 1.000.000` in `display/static/app.js`). Der `cwd` solcher Zeilen ist gestrichelt
unterstrichen — ein Hinweis, dass die Werte einer Annahme unterliegen.


## 🕘 Letzte Chats

Der **🕘-Button** in der Kopfzeile öffnet ein eigenes Fenster mit einer Tabelle
der zuletzt aktiven Claude-Code-Chats — über **alle** Projektverzeichnisse
hinweg, nicht nur die gerade laufenden. So findest du schnell wieder, was wo
zuletzt Sache war.

**Spalten:**

- **★** — Favorit. Ein Klick auf ☆ heftet den Chat an: Favoriten stehen
  **immer zuoberst** (unabhängig von Alter und Sortierung) und fallen **nie
  aus den Limits** heraus — auch ein Wochen alter Chat bleibt gelistet,
  solange der Stern gesetzt ist. Erneuter Klick auf ★ löst ihn wieder.
  Favoriten werden persistiert (`chats.favorites`); beim Löschen eines
  Chats wird sein Stern automatisch mit entfernt.
- **Verzeichnis** — der Projektordner (Tooltip zeigt den vollen Pfad). Ein
  grüner Punkt ● markiert Chats, deren Session gerade läuft.
- **Wann** — letzte Aktivität als relative Zeit („vor 5 min", „gestern", …);
  Tooltip zeigt den exakten Zeitpunkt.
- **Thema** — der von Claude Code selbst vergebene Chat-Titel. Fehlt er, wird
  ersatzweise die erste eigene Nachricht des Chats angezeigt (kursiv).
  Über den **✎-Knopf** (erscheint beim Überfahren der Zeile) kannst du dem
  Chat einen **eigenen Namen** geben: Er ersetzt den automatischen Titel in
  der Spalte (gepunktet unterstrichen), der automatische Titel bleibt als
  Tooltip beim Drüberfahren sichtbar. Enter speichert, Esc bricht ab; ein
  leeres Feld stellt den automatischen Titel wieder her. Eigene Namen werden
  persistiert (`chats.names`) und beim Löschen des Chats mit entfernt.
- **🗑** — löscht den Chat (die Session-Datei unter `~/.claude/projects/…`)
  nach einer Bestätigungsrückfrage direkt in der Zeile („Löschen? Ja/✕").
  Das ist endgültig — der Chat lässt sich danach nicht mehr per
  `claude --resume` fortsetzen. Laufende Sessions (●) sind geschützt: ihr
  Lösch-Knopf ist deaktiviert, und auch der Server lehnt das Löschen ab.

Ein Klick auf die Spaltenköpfe **Verzeichnis** oder **Wann** sortiert die
Tabelle nach dieser Spalte (▲/▼ zeigt die Richtung; erneuter Klick dreht sie
um). Default ist „Verzeichnis" (Ort); innerhalb eines Verzeichnisses stehen
die Chats chronologisch mit den neuesten oben. „Wann" sortiert stattdessen
alles nach letzter Aktivität.

Der Schalter **„Aktive zuerst"** gibt laufenden Chats (●) Vorrang vor der
normalen Sortierung: Bei Sortierung nach „Wann" stehen alle aktiven Chats als
Block oben; bei Sortierung nach „Verzeichnis" rücken **ganze Verzeichnisse**
mit mindestens einem aktiven Chat nach oben (die Gruppierung bleibt erhalten,
innerhalb des Verzeichnisses steht der aktive Chat zuerst). Innerhalb der
Gruppen gilt die gewählte Spalten-Sortierung weiter. Die Einstellung wird wie
die Limits persistiert.

Der Schalter **„Worker"** (persistiert als `chats.show_workers`, Default aus)
nimmt auch die Hintergrund-Worker (Subagenten, siehe
[„Worker"-Schalter](#worker-schalter)) in die Liste auf. Sie sind am
**⚙ Worker**-Badge in der Thema-Spalte erkennbar, konkurrieren um dieselben
Listen-Plätze wie normale Chats und lassen sich genauso aufklappen und löschen.
Im aufgeklappten Verlauf heißen die Seiten „Auftrag" (der Prompt der startenden
Session) und „Worker"; der `claude --resume`-Knopf fehlt, weil sich ein
Worker-Transkript nicht fortsetzen lässt.

Der Schalter **„Nur ★"** (persistiert als `chats.only_favorites`, Default aus)
blendet alles außer den Favoriten aus — praktisch als feste Merkliste der
Chats, zu denen du immer wieder zurückkehrst.

**Klick auf eine Zeile** klappt einen kondensierten Gesprächsverlauf auf: die
letzten Nachrichten von dir und Claude (gekürzt), dazu Buttons zum Öffnen des
Verzeichnisses (📁) und zum Kopieren von `claude --resume <session-id>` (📋),
mit dem du den Chat direkt fortsetzen kannst.

**Limits** — beide kombinierbar, direkt im Fenster einstellbar und persistiert
(`~/.config/claude-token-monitor/config.json`):

- **Pro Verzeichnis** (Default 3): wie viele der neuesten Chats je Projekt
  berücksichtigt werden — so dominiert kein einzelnes Projekt die Liste.
- **Gesamt** (Default 10): wie viele Chats insgesamt angezeigt werden
  (chronologisch, neueste oben).

Favoriten (★) sind von beiden Limits ausgenommen: Sie zählen nicht gegen die
Plätze und konkurrieren nicht um sie — die Limits regeln nur den „normalen"
Rest der Liste.

Das Fenster liest nur die Session-Dateien unter `~/.claude/projects/…`, die
Claude Code ohnehin schreibt — verändert wird nichts, mit einer Ausnahme:
der explizit bestätigte 🗑-Löschvorgang entfernt die jeweilige Datei.

## Plan-Anzeige (kontoseitige Rate-Limits)

Unter der Topbar erscheint optional ein **Plan-Widget**, das deine
Anthropic-Konto-Ressourcen einblendet — die rollierenden Zeitfenster, die sich
unabhängig von der einzelnen Session füllen und leeren.

**Standardmäßig deaktiviert** — die Funktion liest das in
`~/.claude/.credentials.json` hinterlegte OAuth-Token und schickt es als
`Bearer`-Header an einen Anthropic-Backend-Endpoint. Wer das nicht möchte, lässt
das Widget einfach aus; der Rest des Displays funktioniert unverändert.

### Datenfluss

```text
┌──────────────┐         ┌────────────────┐         ┌──────────────────────┐
│   Browser    │ /plan   │ display/       │ spawn   │  claude-tokens plan  │
│   (app.js)   │ ──────► │ server.py      │ ──────► │  (Rust, ureq +       │
│   poll 60 s  │ ◄────── │  60 s Cache    │ ◄────── │   rustls)            │
└──────────────┘   JSON  │  normalisiert  │  stdout └──────────┬───────────┘
                         └────────────────┘                   │ HTTPS GET
                                                              ▼
              ┌────────────────────────────────────────────────────────────┐
              │ api.anthropic.com/api/oauth/usage                           │
              │ Authorization: Bearer <access_token>                        │
              │ anthropic-beta: oauth-2025-04-20 · anthropic-version: …     │
              └─────────────────┬──────────────────────────────────────────┘
                                │ liest aus
                                ▼
                       ~/.claude/.credentials.json
                       (von Claude Code selbst angelegt und aktualisiert)
```

Der HTTPS-Aufruf passiert **nur** in `claude-tokens plan`, gestartet von der
Display-App. Der `server.py`-Prozess kennt das Token nicht direkt — er liest nur
den stdout-JSON des Rust-Subcommands, **normalisiert** ihn in eine stabile Form
und cached ihn 60 s.

### Erste Aktivierung: Consent-Modal

Beim **ersten** Anklicken des Schalters „Plan-Tracking aktivieren" poppt ein
Modal mit dem genauen Datenfluss und der Endpoint-URL. Erst nach „Verstanden,
aktivieren" werden gleichzeitig zwei Werte in die Config geschrieben:

- `enabled: true` — Widget eingeschaltet
- `consent_acknowledged: true` — Modal beim nächsten Aktivieren überspringen

Beim Klick auf „Abbrechen" (oder Esc / Backdrop-Klick) bleibt der Schalter aus,
es wird nichts gespeichert, keine HTTPS-Anfrage gestellt. Der Fokus liegt
absichtlich auf „Abbrechen" — versehentliches Enter ist harmlos.

Consent-Status zurücksetzen (z. B. um das Modal noch einmal zu sehen):

```bash
rm ~/.config/claude-token-monitor/config.json
```

### Konfigurations-Datei

```json
{
  "plan_widget": {
    "enabled": false,
    "consent_acknowledged": false,
    "rows": {
      "main":    true,
      "credits": false
    }
  }
}
```

Liegt in `~/.config/claude-token-monitor/config.json` (oder
`$XDG_CONFIG_HOME/claude-token-monitor/config.json`, falls gesetzt). Vom Server
atomic geschrieben (über `.json.tmp` + `rename`), damit ein gleichzeitiger
Lese-Aufruf nie eine halbe Datei sieht. Pro-Feature-Zeilen (Opus, Sonnet, …)
werden zur Laufzeit erkannt und mit Default `false` ergänzt.

### Die Zeilen

Über das **⚙-Icon** in der Topbar öffnet sich ein Popover mit den Row-Toggles.
Jede aktivierte Zeile rendert eigenständig:

| Zeile | Quelle in der API-Response | Default | Anzeige |
|---|---|---|---|
| **Plan** | `five_hour` + `seven_day` | **an** | 5h + 7d Balken + `Plan · <plan_type>` Label |
| **Pro-Feature** (Opus, Sonnet, Cowork, …) | die jeweiligen `seven_day_*`/Codename-Töpfe, sofern ≠ `null` | aus | 7d Balken (eigene Zeile pro Topf) |
| **Extra-Nutzung** | `extra_usage` | aus | Text-Zeile (kein Balken — kein sinnvoller Nenner) |

Das `plan_type`-Label (`Pro`/`Max`) stammt aus dem `subscriptionType` deiner
`.credentials.json` und wird serverseitig ergänzt.

### Codename-Labels und dynamische Zeilen

Anthropic vergibt für einige Töpfe interne Codenamen. Der Server mappt die
bekannten auf lesbare Labels (`seven_day_opus` → „Opus · 7 Tage", `seven_day_sonnet`
→ „Sonnet · 7 Tage" usw.); unbekannte Codenamen bekommen ein generisches Label
(z. B. „Tangelo").

Wenn Anthropic später neue Töpfe ausrollt, erkennt das Widget diese automatisch:

1. der Topf taucht (nicht-`null`) in der Antwort auf,
2. ein neuer Toggle erscheint im ⚙-Popover, **default aus**,
3. er wird beim nächsten POST `/settings` mit persistiert.

So überrumpelt dich kein neues Feature mit einer plötzlich sichtbaren Zeile — du
musst es explizit einschalten.

### Reset-Countdown und Tooltip

Pro Balken steht rechts ein Block wie `27% · ⌛ 1 h 11 min`. Lesart:

- **27 %** — verbraucht im aktuellen Zeitfenster
- **⌛ 1 h 11 min** — Restzeit bis das Fenster auf 0 % zurückspringt

Das `⌛`-Symbol ist bewusst nicht das `↻` der Session-Karten (das bedeutet dort
*„Kontext wurde verdichtet"*) — zwei verschiedene Konzepte, die optisch nicht
verwechselbar sein sollen.

Der **Tooltip** auf der Zahlen-Spalte zeigt zusätzlich Fenster-Länge und exaktes
Reset-Datum:

```text
27% verbraucht
Fenster: 5 h
Reset in 1 h 11 min (11.06.2026, 13:34:22)
```

### Extra-Nutzungs-Zeile

```text
Extra-Nutzung   0 / 2000 EUR verbraucht
```

Zeigt den Verbrauch gegen dein monatliches Extra-Nutzungs-Limit (`monthly_limit`)
in der Konto-Währung. Ist Extra-Nutzung nicht aktiviert, steht das dort; bei
erreichtem Limit (`disabled_reason` gesetzt) wird die Zeile rot.

### Antwort-Schema (Auszug)

Der Server normalisiert die rohe Backend-Antwort in diese Form, die das Widget
rendert:

```jsonc
{
  "plan_type": "Pro",                          // aus subscriptionType ergänzt
  "main": {
    "five_hour": { "used_percent": 30, "reset_at": 1781200000, "window_seconds": 18000 },
    "seven_day": { "used_percent":  4, "reset_at": 1781233200, "window_seconds": 604800 }
  },
  "buckets": [                                 // nur nicht-null Töpfe, je eigene Zeile
    { "key": "seven_day_opus", "label": "Opus · 7 Tage",
      "used_percent": 12, "reset_at": 1781233200, "window_seconds": 604800 }
  ],
  "extra_usage": {
    "is_enabled": true, "monthly_limit": 2000, "used_credits": 0,
    "currency": "EUR", "utilization": null, "overage_limit_reached": false
  }
}
```

`reset_at` ist hier bereits in **Unix-Sekunden** umgerechnet (das Backend liefert
ISO-8601). Unbekannte Zusatzfelder werden ignoriert. Die rohe, unnormalisierte
Antwort siehst du jederzeit mit `claude-tokens plan` auf der Kommandozeile.

### Token-Refresh

Das `access_token` in `~/.claude/.credentials.json` hat eine begrenzte
Lebensdauer. Claude Code selbst refresht es bei jeder aktiven Nutzung — der
Display-Server liest die Datei einfach **bei jedem Plan-Call frisch ein** (über
`claude-tokens plan`, das zusätzlich `expiresAt` lokal prüft).

Solange du Claude Code regelmäßig benutzt, ist der Token frisch. Nach längerer
Untätigkeit kann er ablaufen. Symptom im Widget: *„Token abgelaufen — Claude Code
nutzen, damit der Token refresht wird"*. Lösung: eine einzige Claude-Interaktion
aktualisiert `.credentials.json`.

Ein automatischer Refresh-Pfad wäre möglich, ist aber bewusst nicht
implementiert: er würde Auth-State-Konflikte mit gleichzeitig laufendem Claude
Code riskieren.

### Fehlerzustände

| Code (Backend → UI) | Anzeige | Bedeutung |
|---|---|---|
| (keiner, Erstaufruf) | Plan-Daten werden geladen … | Erste Abfrage läuft (mit stiller Retry-Karenz) |
| `disabled` | (Widget bleibt versteckt) | Opt-in ist aus |
| `auth_missing` | Nicht eingeloggt — Claude Code starten und einloggen. | `.credentials.json` fehlt / kein `claudeAiOauth` (API-Key-Modus?) |
| `token_expired` | Token abgelaufen — Claude Code nutzen, damit der Token refresht wird. | `expiresAt` abgelaufen oder HTTP 401/403 |
| `network` | Netzwerk-Problem beim Abrufen der Plan-Daten. | api.anthropic.com unerreichbar / TLS-Fehler / 5xx |
| `timeout` | Timeout beim Abrufen der Plan-Daten. | HTTPS-Call dauerte > 10 s |
| `binary_missing` | claude-tokens-Binary nicht gefunden. | Server konnte das Binary nicht spawnen (PATH-Problem) |
| `binary_outdated` | claude-tokens ist zu alt für 'plan' — neue Version installieren. | Vorhandenes Binary kennt das `plan`-Subcommand noch nicht |
| `no_output` | claude-tokens lieferte keine Ausgabe. | Subprocess hat leeren stdout produziert |
| `bad_response` | Unverständliche Antwort vom Plan-Endpoint. | Antwort war kein gültiges JSON |
| `fetch_failed` | Verbindung zum Hilfs-Server unterbrochen. | Browser konnte `/plan` nicht erreichen (Server gestoppt?) |

### Aktualisierungsfrequenz

- **Browser pollt** alle 60 s
- **Server cached** 60 s

Die Zahlen ändern sich ohnehin nur, wenn du Claude Code aktiv benutzt —
häufigeres Pollen würde nur das Rate-Limit der Plan-Abfrage selbst belasten
(`api/oauth/usage` zählt natürlich auch). Im UI tickt der Reset-Countdown alle
30 s selbständig herunter (reines Client-Rendering, kein zusätzlicher
Backend-Call).

### Privacy-FAQ

**Wer sieht mein Auth-Token?**
- Der lokale Display-Server (`server.py`) liest das Token nicht selbst — er ruft
  nur `claude-tokens plan` auf. Der Rust-Subcommand liest `.credentials.json`,
  baut den Authorization-Header und schickt ihn an api.anthropic.com. Sonst
  niemand. (Für das `plan_type`-Label liest der Server zusätzlich nur das
  unkritische `subscriptionType`-Feld.)

**Wird das Token in Logs geschrieben?**
- Nein. Der Server loggt nur den HTTP-Pfad (`GET /plan`), nicht den
  Subprocess-Output. Der Rust-Subcommand gibt das Token nicht aus — bei einem
  `token_expired`-Fehler nur den abstrakten Status-Code.

**Was passiert mit der API-Antwort?**
- 60 s im Server-RAM gecacht, nichts auf Disk geschrieben. Beim Server-Stop ist
  der Cache weg.

**Welche Daten gehen wohin?**
- HTTPS-Header → api.anthropic.com (Token, User-Agent `claude-token-monitor/X.Y.Z`)
- Antwort → in deinem Browser, im Display-RAM. Nirgendwo sonst.

**Was, wenn ich das Vertrauen wieder entziehen will?**
- ⚙ → „Plan-Tracking aktivieren" abschalten → Polling stoppt sofort. Optional:
  Config-Datei löschen.

## Verbindungs-Indikator (oben rechts)

Der kleine farbige Punkt rechts in der Kopfzeile:

- **Grün** — verbunden mit dem lokalen Helfer-Server
- **Rot** — keine Verbindung (Server gestoppt? Hard-Reload mit `Strg+Shift+R`)

## System-weit vs. eingegrenzt

In der Kopfzeile steht entweder

- **„Alle Projekte"** — system-weiter Modus, alle Claude-Sessions auf dem Rechner
  werden gelistet (jede Karte zeigt ihren `cwd`)
- **Der absolute Pfad** — eingegrenzter Modus, nur Sessions in genau diesem
  Verzeichnis

### Sortierung der Karten

Von oben nach unten:

1. Zuerst nach **`compact_count` absteigend** (jede Kontext-Komprimierung kostet
   einen kompletten Turn → höchster bisheriger „Aufwand" steht oben; so springt
   eine gerade beobachtete Session nach einem Auto-Compact nicht unerwartet weit
   nach unten)
2. Bei gleichem `compact_count`: nach **`percent_used` absteigend** (der gerade
   vollste Balken oben)
3. Leere/annahmebasierte Sessions am Ende, nach kumuliertem Verbrauch absteigend

## Anpassung

- **Custom Handover-Prompt**: in `display/static/app.js` die Funktion
  `rolloverPrompt(snap)` editieren.
- **Farbverlauf des Balkens**: in `display/static/app.css` der Selektor
  `.bar__fill` mit dem `linear-gradient(...)`.
- **Frische-Schwellen**: `STALE_AFTER_MS` und `SETTLE_MS` oben in
  `display/static/app.js`.
- **Angenommenes Kontextfenster** für leere Sessions: Konstante
  `ASSUMED_CONTEXT_WINDOW` in `display/static/app.js`.
- **Fenster auf allen Arbeitsflächen**: Das Monitor-Fenster und seine
  Unterfenster (📖 Doku, 🕘 Letzte Chats) werden beim Öffnen automatisch per
  `wmctrl` auf allen Arbeitsflächen sichtbar gemacht („sticky"). Abschalten:
  `"windows": {"sticky": false}` in
  `~/.config/claude-token-monitor/config.json` (ohne `wmctrl` passiert
  einfach nichts).
- **Hilfe-Text**: einfach `display/README.md` editieren — wird beim nächsten
  📖-Klick neu gerendert (oder im Fenster `Strg+Shift+R`).
- **Mehrere Projekte gleichzeitig**: mehrere `launcher.sh`-Instanzen mit
  verschiedenen `--port`-Werten starten.

## Voraussetzungen

- `python3` (Standard auf jedem Linux/WSL2)
- ein Chromium-basierter Browser für das `--app`-Fenster
- `claude-tokens` (im PATH oder via `CLAUDE_TOKENS_BIN`-Variable)
- für die Buttons:
  - `xdg-open` (in jedem Linux-Desktop dabei)
  - `wmctrl` (`sudo apt install wmctrl`)
  - `xclip` oder `wl-clipboard` (`sudo apt install xclip`)
  - `xdotool` (`sudo apt install xdotool`) — nur für das direkte
    Handover-Erzeugen per ↻ (ohne fällt der Button aufs Kopieren zurück)

## Hilfe und Bugs

Vollständige Doku (Install, Build, CLI-Bedienung, Plan-Subcommand,
Portabilität): **[Projekt-README öffnen](/help?doc=main)**
