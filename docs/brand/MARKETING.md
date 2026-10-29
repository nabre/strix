# STRIX — Manuale operativo di marketing, brand e posizionamento

> **Scopo:** fonte di verità unica per chiunque (persona o agente AI) scriva o controlli testi
> promozionali, il sito (`site/`), i README, il materiale di lancio, le immagini del brand e i
> micro-testi dell'app **Strix**. Modello di riferimento: il manuale di
> [Atollo](https://github.com/nabre/atollo/blob/main/docs/brand/MARKETING.md), di cui Strix
> condivide tavolozza e regole di tono.

---

## 1. Direttive per chi scrive

1. **Categoria fissa.** Strix è un **editor di documenti con matematica WYSIWYG per l'ingegneria**
   (EN: *WYSIWYG math document editor for engineering*). Non ridurlo a "calcolatrice",
   "plugin", "clone di" o "alternativa a" un altro prodotto.
2. **Nessun confronto con altri prodotti.** Niente tabelle "noi contro loro", niente "meglio di…",
   niente "come X ma…", niente "stile <prodotto>". Si dice cosa fa Strix, come lo fa e cosa
   garantisce. I nomi dei **formati** (DOCX, ODT, RTF, LaTeX, PDF…) si citano: sono formati, non
   concorrenti. L'unico nome di progetto terzo obbligatorio è **WordCraft**, nella sezione licenza
   (Strix ne è un fork e la nota è richiesta).
3. **Mai materiale di altri vendor.** Nessuna icona, schermata, modello, font o immagine di
   Microsoft, Adobe, Avid, Autodesk (regola del repository, `CLAUDE.md`). Le schermate sono **solo
   di Strix**, fatte con `ui_shot` su documenti scritti da noi.
4. **Tono: sobrio, ingegneristico, verificabile.** Niente "rivoluzionario", "magico", "il
   migliore". Gli ingegneri si convincono con un calcolo che torna: `M = q·L²/8 = 30.375 kN·m`,
   `σ ≤ f_yd ⇒ 30.375 MPa ≤ 355 MPa ✓`. Un esempio numerico vale più di un aggettivo.
5. **Trasparenza sullo stato.** Strix è in **alpha**. Ciò che è pianificato si scrive "pianificato"
   (sezione 8.4), mai al presente. I numeri si prendono dalle fonti (sezione 11), mai a memoria.
6. **Parità multilingua: `EN`, `IT`, `FR`, `DE`.** Ogni testo per il pubblico (sito, README,
   tagline) esiste nelle quattro lingue, con le stesse chiavi. L'inglese è la lingua di
   riferimento; l'app stessa parla 27 lingue.
7. **Colori solo dalla tavolozza Atollo** (sezione 5), gli stessi di `theme::Tokens` nell'app.

---

## 2. Identità e posizionamento

### 2.1 Scheda sintetica

| Attributo | Valore ufficiale |
| :--- | :--- |
| **Nome** | **Strix** (sempre con la S maiuscola; mai "STRIX" nel testo corrente, solo nel wordmark) |
| **Binari** | `strix` (app desktop), `strix-cli` (riga di comando e server MCP), `strix-web` (browser) |
| **Categoria** | **WYSIWYG math document editor for engineering** |
| **Formato nativo** | `.xmath` (documento), `.xmathm` (modello) |
| **Piattaforme** | Windows (x64, x86, ARM64), macOS (universal), Linux (x86_64, aarch64, riscv64), FreeBSD, web (WebAssembly) |
| **Linguaggio** | Rust puro, nessun `unsafe`, nessun panic nel codice di produzione |
| **Licenza** | MIT OR Apache-2.0 |
| **Sito** | <https://nabre.github.io/strix/> |
| **Repository** | <https://github.com/nabre/strix> |
| **Stato** | alpha |

### 2.2 Definizione canonica (base di ogni testo)

* **English:**
  > **Strix — WYSIWYG math for engineers.**
  > An open-source document editor, written in Rust, for working out math and engineering
  > problems. The equations you type in the page are live: they carry units, solve for the
  > unknown, check limits and update as soon as an input changes. Exercises keep a locked
  > structure with a results summary, and CAD drawings follow the calculations. Native on
  > Windows, macOS, Linux and BSD, and in the browser.
* **Italiano:**
  > **Strix — Matematica WYSIWYG per l'ingegneria.**
  > Un editor di documenti open source, scritto in Rust, per risolvere problemi di matematica e
  > di ingegneria. Le equazioni che scrivi nella pagina sono vive: portano le unità di misura,
  > ricavano l'incognita, verificano i limiti e si aggiornano appena cambia un dato. Gli esercizi
  > hanno una struttura bloccata con il riepilogo dei risultati, e i disegni CAD seguono i calcoli.
  > Nativo su Windows, macOS, Linux e BSD, e nel browser.
* **Français :**
  > **Strix — Les maths WYSIWYG pour l'ingénierie.**
  > Un éditeur de documents open source, écrit en Rust, pour résoudre des problèmes de
  > mathématiques et d'ingénierie. Les équations que vous tapez dans la page sont vivantes :
  > elles portent leurs unités, trouvent l'inconnue, vérifient les limites et se mettent à jour
  > dès qu'une donnée change. Les exercices gardent une structure verrouillée avec un récapitulatif
  > des résultats, et les dessins CAO suivent les calculs. Natif sur Windows, macOS, Linux et BSD,
  > et dans le navigateur.
* **Deutsch:**
  > **Strix — WYSIWYG-Mathematik für Ingenieure.**
  > Ein quelloffener Dokumenteditor in Rust, um Mathematik- und Ingenieuraufgaben zu lösen. Die
  > Gleichungen, die du in die Seite schreibst, leben: Sie tragen Einheiten, lösen nach der
  > Unbekannten auf, prüfen Grenzwerte und rechnen neu, sobald sich ein Wert ändert. Übungen
  > behalten eine gesperrte Struktur mit Ergebnisübersicht, und CAD-Zeichnungen folgen den
  > Berechnungen. Nativ unter Windows, macOS, Linux und BSD, und im Browser.

---

## 3. Narrativa: perché "Strix"

**Strix** è il genere dei gufi (l'allocco, *Strix aluco*, è il più comune in Europa). Il gufo vede
al buio: legge ciò che agli altri sfugge. Nel prodotto:

* **Il buio** è il calcolo nascosto: fogli di calcolo dietro un report, formule copiate a mano in
  un documento, numeri che invecchiano appena cambia un dato.
* **Lo sguardo del gufo** è Strix: ogni equazione nella pagina *è* il calcolo, visibile, con le sue
  unità e il suo risultato aggiornato. Niente resta nascosto in un'altra finestra.
* **La penna** (nell'icona) è il documento: Strix resta un editor in cui si scrive, non un
  ambiente di programmazione.

L'icona (`assets/app-icon/strix.svg`): un gufo metà piume e metà circuito, con una penna d'oca
arancione, su una tessera verde abissale. Le due metà sono le due anime del prodotto: la scrittura e
il calcolo.

La metafora si usa con misura: "Strix vede i numeri dietro le equazioni" sì; gergo notturno o
ornitologico nell'interfaccia no.

---

## 4. Pilastri del valore (4 + 1 fondamento)

Ogni presentazione completa si regge su questi cinque punti, in quest'ordine.

1. **Matematica WYSIWYG viva** — il nucleo
   * Nessun foglio separato, nessuna cella di codice, nessuna barra della formula: l'equazione
     scritta con l'editor di equazioni *è* il calcolo, impaginata come matematica.
   * **Un solo `=`** per tutto: definizione (`L = 4.5 m`), risultato (`M =`, o `M = kN·m` per
     scegliere l'unità), formula con valore (`M = q·L²/8 =`), equazione da **risolvere** per la sua
     unica incognita (`F = m·a`), **verifica** (`≤ ≥ < > ≠` con ✓/✗).
   * **Unità ovunque**: SI con prefissi, unità tecniche (`kN`, `MPa`, `bar`, `kWh`…),
     adimensionali conservate (`%`, `‰`, `°`, `rad`); errori dimensionali intercettati
     (`2 m + 3 s`); unità di lavoro che seguono ciò che scrivi; suggerimento sulle maiuscole
     ("forse kN?").
   * **Passaggi della soluzione**, **tabelle calcolate** con colonne in unità e totali, **Σ/∏,
     intervalli, vettori**, **formato numerico**. Un errore mostra `⚠` e il resto del foglio
     continua a calcolare.
2. **Esercizi con struttura bloccata**
   * Titolo, testo, dati, domande annidate su tre livelli (a), a.1), a.1.1)), soluzione con ogni
     domanda ripetuta in sola lettura, **riepilogo dei risultati** unico a fine documento.
   * Ogni esercizio è una **sandbox di calcolo**: due esercizi possono definire entrambi `L`.
   * **Varianti** A, B, C… dei dati: sceglierne una ricalcola tutto.
3. **Disegni CAD guidati dai calcoli**
   * Coordinate, raggi e angoli come **espressioni dei parametri** (`["L", 0]`, `L/2 + 0.3`):
     cambi `L = 4.5 m` e la trave si ridisegna; le quote leggono `{{L}}`.
   * Editor sul foglio del disegno con riga di comando (`@dx,dy`, `@d<a`), **snap esatti** sulle
     curve vere (kernel geometrico verificato a 1e-9), grip che conservano le espressioni
     (`L` → `L + 0.5`), pannello Proprietà e layer.
   * Salvato come JSON nel `.xmath` con un'immagine SVG automatica: altri programmi lo mostrano.
4. **Un documento vero, e pilotabile da agenti**
   * Sotto c'è un editor completo: barra multifunzione, stili, elenchi, tabelle, note,
     citazioni, sommari, commenti, revisioni, ortografia e sillabazione, stampa, carta a
     quadretti, 27 lingue dell'interfaccia.
   * **Tutto è un comando** con id e parametri: barra multifunzione, scorciatoie, ricerca dei
     comandi, `strix-cli`, canale di controllo e **server MCP** usano lo stesso registro. Le
     chiamate programmatiche non aprono mai finestre di dialogo.
5. **Il fondamento: Rust, ovunque, senza crash**
   * Una sola base di codice Rust per Windows, macOS, Linux, FreeBSD e il browser (WebAssembly).
   * **Mai un crash**: nessun `unwrap`, `expect` o `unsafe` nel codice di produzione; ogni comando
     è provato con parametri ostili; un panic sfuggito diventa un errore e il documento viene
     ripristinato.
   * **Privacy**: nessuna telemetria, nessun account, nessun controllo aggiornamenti; la rete si
     usa solo quando lo chiedi tu ([`docs/code-signing.md`](../code-signing.md), «Privacy»).

---

## 5. Aspetti distintivi ("Come funziona")

Alla domanda «in cosa è diverso?» si risponde con questi quattro aspetti, mai con un confronto.
Sono la sezione «Come funziona» del sito (`site/strings/*.json`, chiavi `a1…a4`).

| Aspetto | Cosa dire | Fatti da citare |
| :--- | :--- | :--- |
| **La pagina è il foglio di calcolo** | Scrivi il problema come su carta; le equazioni calcolano dove sono. | Lettura in ordine di lettura, ricalcolo dopo ogni modifica (quando il cursore lascia l'equazione), su F9 e all'apertura. |
| **Un `=` capisce cosa intendi** | Definisce, mostra, risolve o verifica a seconda di ciò che è già noto. | `L = 4.5 m`, `M = q·L²/8 =`, `F = m·a ⇒ a = 2.5 m/s²`, `σ ≤ f_yd ✓`. |
| **Le unità sono controllate a ogni passo** | Quantità, non numeri: un errore di dimensione non passa mai in silenzio. | SI e tecniche con prefissi, adimensionali conservate, unità di lavoro, suggerimenti. |
| **Tutto resta vivo, il file resta standard** | Risultati, tabelle, esercizi e disegni si aggiornano; il `.xmath` è un pacchetto OOXML che altri programmi aprono con gli ultimi valori calcolati. | Esporta copie statiche in PDF, PNG, LaTeX, HTML, Markdown; apre DOCX, DOC, ODT, RTF, HTML, Markdown, LaTeX, TXT. |

---

## 6. Colori (tavolozza Atollo)

Gli stessi valori di `crates/ui-egui/src/theme.rs` (`theme::Tokens`) e di
[`site/assets/site.css`](../../site/assets/site.css). Fonte: il manuale di Atollo, sezione 5.

### 6.1 Accenti del brand

| Nome | HEX | Ruolo |
| :--- | :--- | :--- |
| **Deep Forest** | `#0F3D2E` | Tessera dell'icona, wordmark su fondo chiaro, titoli |
| **Abyssal gradient** | `#0F3D2E` → `#05140F` | Sfondo dell'icona, card social |
| **Emerald (scuro)** | `#10B981` | Accento primario nel tema scuro (pulsanti, focus) |
| **Emerald hover (scuro)** | `#0EA572` | Hover del pulsante primario |
| **Reef Emerald (chiaro)** | `#047857` | Accento primario nel tema chiaro (contrasto ≥ 4.5:1 col bianco) |
| **Reef hover (chiaro)** | `#065F46` | Hover nel tema chiaro |
| **Surf Glow** | `#34D399` | Link e risalti nel tema scuro |
| **Crest Mint** | `#6EE7B7` | Iride del gufo, wordmark su fondo scuro |
| **Coral Orange** | `#F97316` (chiaro: `#C2410C`) | Penna dell'icona, attenzione, verifiche ✗ |
| **Sun Amber** | `#FBBF24` (chiaro: `#B45309`) | Snap e marcatori CAD, risalti caldi |
| **Unit Teal** | `#1B6F8A` (scuro: `#5BB6D1`) | Le **unità** di misura: lo stile carattere *Unit* dell'app (`crates/engine/src/cmd/calc.rs`) |

### 6.2 Neutri

| Token sito | Scuro | Chiaro | Uso |
| :--- | :--- | :--- | :--- |
| `--bg` | `#1F1F1F` | `#FFFFFF` | Sfondo pagina |
| `--panel` | `#181818` | `#F8F8F8` | Header, footer, bande |
| `--border` | `#2B2B2B` | `#E5E5E5` | Bordi |
| `--fg` | `#CCCCCC` | `#3B3B3B` | Testo |
| `--desc` | `#9D9D9D` | `#717171` | Testo secondario |
| `--ok` | `#73C991` | `#007100` | ✓ delle verifiche |

---

## 7. Tipografia e logo

1. **Interfaccia del sito:** `"Geist", system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`.
   Nessun font scaricato da CDN e nessun file di font nel sito (i font nuovi vanno in
   `storytold/craft-fonts`, mai in questo repository).
2. **Codice e comandi:** `"Geist Mono", "JetBrains Mono", ui-monospace, monospace`. Id dei comandi
   (`calc.insert`), percorsi, estensioni (`.xmath`) sempre in monospace.
3. **Matematica:** `"STIX Two Math", "STIX Two Text", "Latin Modern Roman", "Source Serif 4", serif`.
   Variabili in corsivo, **unità diritte e colorate** (Unit Teal), come nell'app.
4. **Logo:**
   * **Simbolo** = l'icona dell'app (`assets/app-icon/strix.svg`): favicon, header, card social.
   * **Wordmark** = `docs/brand/wordmark-light.svg` (Deep Forest, per fondi chiari) e
     `wordmark-dark.svg` (Crest Mint, per fondi scuri): le cinque lettere STRIX disegnate a tratto,
     senza font; la I, inclinata come una penna, è in Coral Orange.
   * In header si affiancano simbolo e wordmark (il simbolo a sinistra, altezza 1:1 con le
     maiuscole + 20%). Mai deformare, ricolorare con colori fuori tavolozza o mettere il simbolo su
     fondo verde senza il suo bordo.
   * Card social: `docs/brand/social.png` (1280×640), da `docs/brand/social.html`.

---

## 8. Messaggi per pubblico

### Persona 1 — L'ingegnere che scrive relazioni di calcolo

* **Dolore:** il calcolo vive in un foglio elettronico o in uno script, la relazione in un altro
  file; ogni modifica di un dato significa ricopiare formule e numeri, e un'unità sbagliata passa
  inosservata fino alla verifica.
* **Hook:** *"Scrivi l'equazione. Calcola."* · *"La relazione è il calcolo."*
* **Da mostrare:** la trave appoggiata (`L`, `q`, `M = q·L²/8 =`, `σ ≤ f_yd ✓`), i passaggi della
  soluzione, la tabella calcolata con più travi, il disegno CAD che si ridisegna con `L`,
  l'esportazione PDF.

### Persona 2 — Il docente

* **Dolore:** preparare esercizi con dati diversi per ogni compito, tenere allineati testo,
  soluzione e risultati, e impedire che la struttura si rompa modificando.
* **Hook:** *"Un esercizio, tante varianti, nessuna soluzione da rifare."*
* **Da mostrare:** Math › Esercizio, domande annidate, varianti A/B/C che ricalcolano tutto,
  riepilogo dei risultati, carta a quadretti, esportazione LaTeX e PDF.

### Persona 3 — Lo studente

* **Dolore:** risolvere problemi a mano con le unità, sbagliare una conversione, non capire dove.
* **Hook:** *"Le unità non ti tradiscono più."*
* **Da mostrare:** errore dimensionale intercettato, suggerimento "forse kN?", passaggi della
  soluzione, la versione web (niente installazione sul PC del laboratorio), gratis e open source.

### Persona 4 — Lo sviluppatore che lavora con agenti AI

* **Dolore:** gli agenti producono numeri in chat, non documenti verificabili.
* **Hook:** *"Ogni azione è un comando. Anche per il tuo agente."*
* **Da mostrare:** `claude mcp add strix -- strix-cli mcp`, `calc.insert`, `batch`,
  `render_page`, l'agente che scrive un esercizio e lo salva come `.xmath`; cartelle confinate
  (`--automation-read-root`, `--automation-write-root`).

---

## 9. Copy ufficiale pronto all'uso

### 9.1 Tagline

| Formato | EN | IT | FR | DE |
| :--- | :--- | :--- | :--- | :--- |
| **Categoria** | WYSIWYG math for engineers | Matematica WYSIWYG per l'ingegneria | Les maths WYSIWYG pour l'ingénierie | WYSIWYG-Mathematik für Ingenieure |
| **Hero** | Type the equation. It calculates. | Scrivi l'equazione. Calcola. | Tapez l'équation. Elle calcule. | Gleichung tippen. Sie rechnet. |
| **Tecnica** | Units, solving, checks and drawings, live in the page. | Unità, soluzioni, verifiche e disegni, vivi nella pagina. | Unités, résolution, vérifications et dessins, vivants dans la page. | Einheiten, Lösen, Nachweise und Zeichnungen, live auf der Seite. |
| **Fiducia** | Open source. No account. No telemetry. Never crashes by design. | Open source. Nessun account. Nessuna telemetria. Progettato per non andare in crash. | Open source. Sans compte. Sans télémétrie. Conçu pour ne jamais planter. | Open Source. Kein Konto. Keine Telemetrie. Absturzfrei konstruiert. |

### 9.2 Descrizione del repository GitHub (≤ 350 caratteri)

> WYSIWYG math for engineers: an open-source Rust document editor where equations calculate live,
> with units, solving, checks, exercises and CAD drawings driven by the results. Windows, macOS,
> Linux, BSD and the web. Scriptable over CLI and MCP.

* **Homepage:** `https://nabre.github.io/strix/`
* **Topics:** `rust`, `wysiwyg`, `math`, `engineering`, `calculation`, `units`,
  `equation-editor`, `document-editor`, `engineering-calculations`, `cad`, `education`,
  `exercises`, `mcp`, `egui`, `webassembly`, `cross-platform`, `open-source`
* **Social preview:** caricare `docs/brand/social.png` (Settings › General › Social preview).

Comando (da eseguire a mano, lo decide il proprietario):

```sh
gh repo edit nabre/strix \
  --description "WYSIWYG math for engineers: an open-source Rust document editor where equations calculate live, with units, solving, checks, exercises and CAD drawings driven by the results." \
  --homepage https://nabre.github.io/strix/ \
  --add-topic rust,wysiwyg,math,engineering,calculation,units,equation-editor,document-editor,engineering-calculations,cad,education,exercises,mcp,egui,webassembly,cross-platform,open-source
```

### 9.3 Elevator pitch (README, pagina "Informazioni", store)

**EN:**
> **Strix** is an open-source document editor for math and engineering problems, written in pure
> Rust. You write the problem as you would on paper (text, tables, equations, drawings), and the
> equations are live: they carry units, solve for the unknown, check limits and update as soon as
> an input changes. Exercises keep a locked structure with variants and a results summary; CAD
> drawings take their coordinates from the calculations. It saves `.xmath` files that other
> programs open with the last computed values, exports PDF, PNG, LaTeX, HTML and Markdown, and
> every action is a command that a script or an AI agent can run over the CLI or MCP. Native on
> Windows, macOS, Linux and FreeBSD, and in the browser. MIT or Apache-2.0.

**IT:**
> **Strix** è un editor di documenti open source per problemi di matematica e ingegneria, scritto
> interamente in Rust. Scrivi il problema come su carta (testo, tabelle, equazioni, disegni) e le
> equazioni sono vive: portano le unità, ricavano l'incognita, verificano i limiti e si aggiornano
> appena cambia un dato. Gli esercizi hanno una struttura bloccata con varianti e riepilogo dei
> risultati; i disegni CAD prendono le coordinate dai calcoli. Salva file `.xmath` che altri
> programmi aprono con gli ultimi valori calcolati, esporta PDF, PNG, LaTeX, HTML e Markdown, e
> ogni azione è un comando che uno script o un agente AI può eseguire da CLI o MCP. Nativo su
> Windows, macOS, Linux e FreeBSD, e nel browser. MIT o Apache-2.0.

### 9.4 Pianificato (si scrive sempre "pianificato")

Pannello Variabili, selettore di unità, `°C`/`°F`, funzioni utente `f(x) = …`, matrici, sistemi di
equazioni, grafici; per il CAD offset, taglia, estendi, raccorda, specchia, quote angolari,
tratteggi, vincoli e quote guidanti. Fonte: README, «Status and roadmap».

---

## 10. Lancio e promozione ("mostra, non dire")

### 10.1 Video di 40 secondi: *"Change one number"*

1. **0:00–0:10** Documento vuoto. Si scrive `L = 4.5 m`, `q = 12 kN/m`, `M = q·L²/8 =`: compare
   `30.375 kN·m`.
2. **0:10–0:20** `σ ≤ f_yd`: compare `✓`. Show Steps: la soluzione si dispone allineata a `=`.
3. **0:20–0:32** Sotto, il disegno CAD della trave con la quota `L = 4.5 m`. Si cambia `L` in
   `6 m`: momento, verifica (ora ✗ in arancio), quota e disegno cambiano insieme.
4. **0:32–0:40** Wordmark su `#1F1F1F`, accento `#10B981`: *"Strix — Type the equation. It
   calculates."* e l'indirizzo del sito.

Registrazione: solo finestre di Strix (`ui_shot` o la finestra dell'app per id), mai il desktop.

### 10.2 Canali e testi pronti

**Hacker News (`Show HN`)**

* *Titolo:* `Show HN: Strix – A Rust document editor where the equations calculate, with units`
* *Primo commento:*

  > Hi HN, Strix is a document editor for working out engineering problems. You type an equation
  > with the equation editor, and it is the calculation: `L = 4.5 m`, `M = q·L²/8 =` shows
  > `30.375 kN·m`, `F = m·a` solves for the one unknown, `σ ≤ f_yd` shows ✓ or ✗. Units are
  > checked on every step. Exercises have a locked structure with data variants, and CAD drawings
  > take their coordinates from the calculation's parameters, so changing `L` redraws the beam.
  >
  > It is pure Rust (egui front end, vello_cpu rendering, krilla for PDF), one code base for
  > Windows, macOS, Linux, FreeBSD and WebAssembly. Every action is a command in one registry,
  > so the same ids run from the ribbon, the CLI, a local control channel and an MCP server.
  > Production code has no unwrap/expect/unsafe, and every command is fuzzed with hostile
  > parameters. It is a fork of the WordCraft word processor, which gives it a real document
  > model underneath (styles, references, track changes, DOCX import).
  >
  > It's alpha. I'd love feedback on the `=` semantics and on which unit systems you need.

**Reddit**

* `r/rust` — angolo tecnico: registro comandi, niente panic, wasm, egui. Titolo: *"Strix: a pure
  Rust WYSIWYG math editor (egui + wasm) where equations calculate with units"*.
* `r/engineering`, `r/StructuralEngineering`, `r/civilengineering`, `r/MechanicalEngineering` —
  angolo pratico: la trave con la verifica e il disegno. Titolo: *"I built a free document editor
  where the equations in your calc report calculate themselves (with units)"*. Una sola immagine:
  la schermata hero.
* `r/matheducation`, `r/Teachers` — gli esercizi con varianti.
* `r/ClaudeAI`, `r/LocalLLaMA` — MCP: un agente scrive ed esegue un esercizio.

**Post breve (X, Mastodon, Bluesky, LinkedIn)**

* EN: *Type `M = q·L²/8 =` and get `30.375 kN·m`. Change `L` and the result, the check and the
  drawing follow. Strix: an open-source WYSIWYG math editor in Rust. Windows, macOS, Linux, BSD,
  web. https://nabre.github.io/strix/*
* IT: *Scrivi `M = q·L²/8 =` e ottieni `30.375 kN·m`. Cambi `L` e risultato, verifica e disegno
  seguono. Strix: editor open source di matematica WYSIWYG in Rust. Windows, macOS, Linux, BSD,
  web. https://nabre.github.io/strix/it/*

**Scuole, università, ITS**

Proposta: la versione web non richiede installazione sui PC del laboratorio; il docente prepara un
esercizio con varianti, lo esporta in PDF per il compito e distribuisce il `.xmath` per la
correzione. Gratis, open source, senza account.

### 10.3 Note di rilascio (modello)

```markdown
## Strix vX.Y.Z

**Highlights**
- <una riga per novità, con l'esempio numerico quando c'è>

**Downloads**: Windows (MSI, portable ZIP; x64, x86, ARM64) · macOS (universal DMG) ·
Linux (AppImage, deb, rpm, Flatpak, Arch; x86_64, aarch64; riscv64 deb/rpm/tar) · FreeBSD ·
Web (static ZIP). Checksums in `SHA256SUMS.txt`.

Strix is alpha: keep a copy of important documents. Report problems at
https://github.com/nabre/strix/issues.
```

---

## 11. Mappa del repository per chi aggiorna il marketing

| Cosa | Dove |
| :--- | :--- |
| Questo manuale | `docs/brand/MARKETING.md` |
| Wordmark | `docs/brand/wordmark-light.svg`, `docs/brand/wordmark-dark.svg` |
| Card social | `docs/brand/social.html` → `docs/brand/social.png` (Chromium headless, 1280×640: vedi l'intestazione del file HTML) |
| Icona | `assets/app-icon/strix.svg` (rendering: `packaging/icons.sh`) |
| Sito: modello e testi | `site/landing.html`, `site/partials/*.html`, `site/strings/{en,it,fr,de}.json` (stesse chiavi: la build si ferma se ne manca una) |
| Sito: stile | `site/assets/site.css` (colori della sezione 6) |
| Sito: schermate | `site/shots/<lingua>/shot-*-{light,dark}.png`, da `site/shots/*.jsonl` con `ui_shot` (vedi `site/shots/README.md`) |
| Sito: build | `cargo xtask site` → `_site/`; `cargo xtask site --check` controlla pagine, sitemap e link |
| Sito: pubblicazione | `.github/workflows/site.yml` (GitHub Pages, con la build web dell'ultima release in `/app/`) |
| README | `README.md` (EN), `docs/README.it.md`, `docs/README.fr.md`, `docs/README.de.md` |
| Privacy e firma | `docs/code-signing.md` |
| Numeri (comandi, test, righe) | `cargo xtask stats`, `ROADMAP.md` (Headline numbers) |
| Release e nomi dei file | `.github/workflows/release.yml`, `packaging/` |

Quando cambia un fatto (piattaforme, formati, comandi, stato), si aggiornano insieme: questo
manuale, `site/strings/*.json` nelle quattro lingue e i quattro README.
