<p align="center">
  <img src="../assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png" width="128" height="128" alt="Strix-Symbol: eine Eule, halb Federn, halb Schaltkreis, mit einer Schreibfeder, auf einer dunkelgrünen Kachel">
</p>

<h1 align="center">Strix</h1>

<p align="center">
  <b>WYSIWYG-Mathematik für Ingenieure: Gleichung auf die Seite tippen, und sie rechnet.</b>
</p>

<p align="center">
  Strix ist ein quelloffener Dokumenteditor, komplett in Rust geschrieben, um Mathematik- und
  Ingenieuraufgaben zu lösen. Du schreibst die Aufgabe wie auf Papier (Text, Tabellen,
  Gleichungen, Zeichnungen), und die Gleichungen leben: Sie tragen Einheiten, lösen nach der
  Unbekannten auf, prüfen Grenzwerte und rechnen neu, sobald sich ein Wert ändert. Übungen behalten
  eine gesperrte Struktur mit Ergebnisübersicht, und CAD-Zeichnungen folgen den Berechnungen. Nativ
  unter Windows, macOS, Linux und BSD, und im Browser per WebAssembly.
</p>

<p align="center">
  <a href="https://nabre.github.io/strix/de/"><b>Website</b></a> ·
  <a href="https://github.com/nabre/strix/releases/latest"><b>Download</b></a> ·
  <a href="https://nabre.github.io/strix/app/?sample"><b>Im Browser ausprobieren</b></a> ·
  <a href="https://github.com/nabre/strix/issues"><b>Problem melden</b></a>
</p>

<p align="center"><a href="../README.md">English</a> · <a href="README.it.md">Italiano</a> · <a href="README.fr.md">Français</a> · Deutsch</p>

> **Status: Alpha.** Der Dokumenteditor, auf dem Strix aufbaut, ist ausgereift; Rechenkern, Übungen
> und CAD sind neu und entwickeln sich schnell. Bewahre eine Kopie wichtiger Dokumente auf und
> [sag uns, was nicht funktioniert](https://github.com/nabre/strix/issues). Das englische README
> ist die vollständige Referenz.

## WYSIWYG-Mathematik

Kein separates Rechenblatt, keine Codezellen, keine Formelleiste. Eine Gleichung im Dokument *ist*
die Berechnung: an Ort und Stelle mit dem Formeleditor getippt und als gesetzte Mathematik
angezeigt. Strix liest die berechneten Gleichungen in Lesereihenfolge und hält die Ergebnisse
aktuell: nach jeder Änderung (sobald der Cursor die Gleichung verlässt), mit F9 und beim Öffnen.

```text
L = 4.5 m                          definiert L
q = 12 kN/m                        Einheiten multiplizieren und dividieren wie Zahlen
M = q·L²/8 =                       →  M = q·L²/8 = 30.375 kN·m
W = 1000 cm³
f_yd = 355 MPa
σ = M/W =                          →  σ = M/W = 30.375 MPa
σ ≤ f_yd                           →  σ ≤ f_yd ⇒ 30.375 MPa ≤ 355 MPa ✓
F = m·a                            →  F = m·a ⇒ a = 2.5 m/s²        (a ist die einzige Unbekannte: gelöst)
```

- **Ein `=` für alles**: Definition, anzuzeigendes Ergebnis (auch in einer gewählten Einheit,
  `M = kN·m`), Formel mit Wert, Gleichung, die nach ihrer einen Unbekannten **gelöst** wird, oder
  **Nachweis** (`≤ ≥ < > ≠` mit ✓/✗).
- **Lösen**: Die Unbekannte wird symbolisch freigestellt, wenn sie einmal vorkommt, sonst
  numerisch gefunden.
- **Einheiten überall**: SI mit Präfixen, technische Einheiten (`mm`, `kN`, `MPa`, `bar`, `kWh`…),
  dimensionslose Einheiten bleiben (`%`, `‰`, `°`, `rad`). Dimensionsfehler (`2 m + 3 s`) werden
  gemeldet, nie still verrechnet. Die Arbeitseinheit folgt dem, was du schreibst; eine falsch
  geschriebene Einheit bekommt einen Hinweis („meintest du kN?“).
- **Lösungsschritte**, **berechnete Tabellen** (Spalten mit Einheiten und Formeln, `Σ =`,
  `mean =`, `min =`, `max =`), **Summen, Bereiche und Vektoren**, **Zahlenformat**.
- **Fehler halten das Blatt nicht an**: Eine fehlerhafte Anweisung zeigt `⚠` und ihre Meldung.

Alles liegt im Reiter **Math**. Details (auf Englisch): [calc.md](calc.md).

## Übungen

**Math › New Exercise** (Oberfläche noch nicht übersetzt) fügt eine gelöste Aufgabe ein, deren Struktur beim Bearbeiten nicht zerbricht:
Titel, Aufgabentext, Daten, Fragen mit Buchstaben auf drei Ebenen; die Lösung wiederholt jede Frage
schreibgeschützt mit Platz für den Lösungsweg; eine **Ergebnisübersicht** am Dokumentende. Jede
Übung ist eine **Rechen-Sandbox** (zwei Übungen können beide `L` definieren), und die Daten können
**Varianten** A, B, C… haben, die alles neu berechnen. Details: [exercise.md](exercise.md).

## CAD-Zeichnungen, die den Berechnungen folgen

**Einfügen › CAD Drawing** setzt eine technische Zeichnung in den Text. Koordinaten, Radien und
Winkel können **Ausdrücke der Parameter** sein (`["L", 0]`, `L/2 + 0.3`): Ändere `L = 4.5 m`, und
der Träger wird neu gezeichnet; Bemaßungen rufen Parameter ab (`{{L}}` zeigt `4.5 m`). Der Editor
auf dem Zeichenblatt hat eine Befehlszeile für Koordinaten, **exakte Objektfänge** auf den echten
Kurven, Griffe, die die Ausdrücke behalten, einen Eigenschaftenbereich und Layer. Die Zeichnung
wird als JSON in der `.xmath`-Datei gespeichert, zusammen mit einem SVG-Bild. Details: [cad.md](cad.md).

## Karopapier

**Einfügen › Graph Paper** kariert die Seite oder einen Block im Text wie ein Rechenheft, mit einer
kräftigeren Linie alle N Kästchen, scharf bei jedem Zoom und im PDF. Details: [graph-paper.md](graph-paper.md).

## Ein vollständiger Dokumenteditor

Darunter steckt eine echte Textverarbeitung (Strix ist ein Fork von WordCraft): Menüband,
Formatvorlagen, Listen, Tabellen, Bilder und Formen, Kopf- und Fußzeilen, Abschnitte und Spalten,
Fußnoten, Zitate und Inhaltsverzeichnisse, Kommentare und Änderungsverfolgung, Rechtschreibung und
Silbentrennung, Serienbriefe und Druck. Die Oberfläche folgt der Systemsprache (27 Sprachen,
Deutsch eingeschlossen).

## Dateien

| | Formate |
|---|---|
| **Speichert** | `.xmath` (Dokument) und `.xmathm` (Vorlage): ein OOXML-Paket, das Berechnungen, Übungen und Zeichnungen lebendig hält |
| **Exportiert** (statische Kopien) | PDF, PNG, LaTeX, HTML, Markdown |
| **Öffnet** (Import) | `.docx`, `.docm`, `.dotx`, `.dotm`, `.doc`, `.odt`, `.rtf`, `.md`, `.html`, `.tex`, `.txt` |

Eine `.xmath`-Datei bleibt ein Standardpaket: Andere Programme zeigen die zuletzt berechneten Werte.

## Für Agenten: Befehle, CLI, MCP

Alles, was Strix tut, ist ein **Befehl** in einem einzigen Register. Menüband, Tastenkürzel,
Befehlssuche, Kommandozeile, Steuerkanal und MCP-Server führen dieselben Befehle aus, und
programmatische Aufrufe öffnen nie Dialoge.

```sh
claude mcp add strix -- strix-cli mcp                      # Dokumente ohne Oberfläche
strix --control 7981 &                                     # oder die laufende App steuern
claude mcp add strix-app -- strix-cli mcp --connect 127.0.0.1:7981
```

Siehe [mcp.md](mcp.md) und das [Steuerprotokoll](control-protocol.md).

## Download

Installer und portable Versionen gibt es auf der [Release-Seite](https://github.com/nabre/strix/releases)
und auf der [Website](https://nabre.github.io/strix/de/#download): Windows (MSI und portables ZIP;
x64, x86, ARM64), macOS (Universal-DMG), Linux (AppImage, deb, rpm, Flatpak, Arch; x86_64 und
aarch64, dazu riscv64), FreeBSD und der Web-Build. Die Windows-Builds sind noch nicht signiert.
Datenschutz: Strix sendet nichts von sich aus (keine Telemetrie, kein Konto, keine Updateprüfung);
Details in [code-signing.md](code-signing.md).

## Bauen und starten

Du brauchst eine Rust-Toolchain (1.95 oder neuer); für den Web-Build zusätzlich
[Trunk](https://trunkrs.dev/) und das Ziel `wasm32-unknown-unknown`.

```sh
git clone https://github.com/nabre/strix
cd strix
cargo run --release -p strix -- --sample          # die Desktop-App mit einem Beispieldokument
cargo run --release -p strix -- aufgabe.xmath     # ein Strix-Dokument öffnen
strix-cli convert aufgabe.xmath aufgabe.pdf       # statischer Export: pdf, png, tex, html, md
```

Web: `cd apps/strix-web && trunk serve`, dann <http://127.0.0.1:8771/?sample> öffnen.

## Lizenz und Dank

Strix steht wahlweise unter der [MIT](../LICENSE-MIT)- oder der [Apache-2.0](../LICENSE-APACHE)-Lizenz.
Copyright (c) 2026 nabre und die Strix-Mitwirkenden. Strix ist ein Fork von
[WordCraft](https://github.com/storytold/wordcraft), weder mit dessen Autoren verbunden noch von
ihnen unterstützt; die erforderlichen Hinweise stehen in [NOTICE](../NOTICE). Mitgelieferte
Schriften, Symbole, Wörterbücher und Übersetzungen behalten ihre offenen Lizenzen, aufgeführt in
[ATTRIBUTION.md](../ATTRIBUTION.md). Die Farben folgen der [Atollo](https://github.com/nabre/atollo)-Palette.
