# STRIX — Manuale operativo di marketing, brand e posizionamento

> **Scopo:** fonte di verità unica per chiunque (persona o agente AI) scriva o controlli testi
> promozionali, il sito (`site/`), i README, il materiale di lancio, le immagini del brand e i
> micro-testi dell'app **Strix**. Modello di riferimento: il manuale di
> [Atollo](https://github.com/nabre/atollo/blob/main/docs/brand/MARKETING.md), di cui Strix
> condivide tavolozza e regole di tono.
>
> **Ultimo aggiornamento:** 2026-11-12 · **Modifica:** in italiano «momenti di valutazione», mai «verifica» (decisione del proprietario: sito, README, tagline); prima: riposizionamento sul docente (esercizi di
> calcolo, risposta multipla, verifiche, punteggi, griglia dei voti pianificata): direttive,
> categoria, definizione canonica, pilastri, «Come funziona», persone, tagline, descrizione del
> repository, pitch, video e canali.

---

## 1. Direttive per chi scrive

1. **Pubblico e categoria fissi.** Strix si rivolge al **docente**: è l'**editor dei momenti di valutazione e
   degli esercizi** (EN: *test and exercise editor for teachers*). Il racconto segue sempre la
   giornata del docente, in quest'ordine: **esercizi di calcolo**, **domande a risposta multipla**,
   **momenti di valutazione** (compiti in classe, prove), **punteggi** e **griglia per l'assegnazione dei
   voti**. La matematica viva, il CAD, gli agenti e Rust sono i mezzi che lo rendono possibile, non
   il titolo. Non ridurre Strix a "calcolatrice", "generatore di quiz", "plugin", "clone di" o
   "alternativa a" un altro prodotto.
2. **Nessun confronto con altri prodotti.** Niente tabelle "noi contro loro", niente "meglio di…",
   niente "come X ma…", niente "stile <prodotto>". Si dice cosa fa Strix, come lo fa e cosa
   garantisce. I nomi dei **formati** (DOCX, ODT, RTF, LaTeX, PDF…) si citano: sono formati, non
   concorrenti. L'unico nome di progetto terzo obbligatorio è **WordCraft**, nella sezione licenza
   (Strix ne è un fork e la nota è richiesta).
3. **Mai materiale di altri vendor.** Nessuna icona, schermata, modello, font o immagine di
   Microsoft, Adobe, Avid, Autodesk (regola del repository, `CLAUDE.md`). Le schermate sono **solo
   di Strix**, fatte con `ui_shot` su documenti scritti da noi.
4. **Tono: concreto, da sala docenti, verificabile.** Niente "rivoluzionario", "magico", "il
   migliore". Un docente si convince con un momento di valutazione pronto da stampare: l'esercizio con i dati
   in due varianti, la risposta multipla con le caselle, la tabella dei punteggi che fa da sola i
   totali. Un esempio concreto vale più di un aggettivo. Il lessico è quello della scuola
   (momento di valutazione, compito, griglia, punteggio, voto, variante A/B, DSA), non quello del software.
5. **Trasparenza sullo stato.** Strix è in **alpha**. Ciò che è pianificato si scrive "pianificato"
   (sezione 9.4), mai al presente: oggi la **griglia dei voti** (dai punti al voto) è
   **pianificata**. I numeri si prendono dalle fonti (sezione 11), mai a memoria.
6. **Parità multilingua: `EN`, `IT`, `FR`, `DE`.** Ogni testo per il pubblico (sito, README,
   tagline) esiste nelle quattro lingue, con le stesse chiavi. L'inglese è la lingua di
   riferimento; l'app stessa parla 27 lingue. Nei termini scolastici si usa la parola del paese
   (IT *momenti di valutazione*, mai *verifica*, FR *évaluation*, DE *Klassenarbeit*, EN *test*), non una traduzione letterale.
7. **Colori solo dalla tavolozza Atollo** (sezione 6), gli stessi di `theme::Tokens` nell'app.

---

## 2. Identità e posizionamento

### 2.1 Scheda sintetica

| Attributo | Valore ufficiale |
| :--- | :--- |
| **Nome** | **Strix Writer** (dal 2026-11-08). Nome completo nei titoli, negli installer, negli store, sul sito e alla prima menzione; nel testo corrente basta **Strix**, sempre con la S maiuscola. Mai "STRIX" nel testo corrente: solo nel wordmark, che resta STRIX |
| **Binari** | `strix` (app desktop), `strix-cli` (riga di comando e server MCP), `strix-web` (browser) |
| **Pubblico** | **il docente** (scuola secondaria, ITS, università: materie con calcolo, ma non solo) |
| **Categoria** | **Test and exercise editor for teachers** · *Editor di momenti di valutazione ed esercizi per il docente* |
| **Formato nativo** | `.xmath` (documento), `.xmathm` (modello); DOCX, ODT, RTF si risalvano nel loro formato solo se il documento non ha oggetti Strix |
| **Piattaforme** | Windows (x64, x86, ARM64), macOS (universal), Linux (x86_64, aarch64, riscv64), FreeBSD, web (WebAssembly) |
| **Linguaggio** | Rust puro, nessun `unsafe`, nessun panic nel codice di produzione |
| **Licenza** | MIT OR Apache-2.0 |
| **Sito** | <https://nabre.github.io/strix/> |
| **Repository** | <https://github.com/nabre/strix> |
| **Stato** | alpha |

### 2.2 Definizione canonica (base di ogni testo)

* **English:**
  > **Strix Writer — tests and exercises for teachers.**
  > An open-source editor, written in Rust, for preparing tests. Calculation exercises keep their
  > data, solution and results live, with variants A, B, C for each row of desks; multiple-choice,
  > true/false and open questions carry their points; one file per exercise makes an archive, and
  > a test is put together from it, with or without the solutions. A scores table adds the points
  > up by criterion by itself; a grading grid that turns points into marks is planned. Native on
  > Windows, macOS, Linux and BSD, and in the browser.
* **Italiano:**
  > **Strix Writer — Momenti di valutazione ed esercizi per il docente.**
  > Un editor open source, scritto in Rust, per preparare i momenti di valutazione. Gli esercizi di calcolo
  > tengono vivi dati, soluzione e risultati, con le varianti A, B, C per ogni fila di banchi; le
  > domande a risposta multipla, vero/falso e aperte portano il loro punteggio; un file per
  > esercizio fa l'archivio, e da lì si compone il momento di valutazione, con o senza soluzioni. La tabella dei
  > punteggi somma da sola i punti per criterio; la griglia che trasforma i punti in voti è
  > pianificata. Nativo su Windows, macOS, Linux e BSD, e nel browser.
* **Français :**
  > **Strix Writer — Évaluations et exercices pour l'enseignant.**
  > Un éditeur open source, écrit en Rust, pour préparer les évaluations. Les exercices de calcul
  > gardent leurs données, leur corrigé et leurs résultats vivants, avec des variantes A, B, C pour
  > chaque rangée ; les questions à choix multiples, vrai/faux et ouvertes portent leurs points ;
  > un fichier par exercice forme une banque, et l'évaluation se compose à partir d'elle, avec ou
  > sans corrigé. Un tableau des points fait seul les totaux par critère ; une grille qui
  > convertit les points en notes est prévue. Natif sur Windows, macOS, Linux et BSD, et dans le
  > navigateur.
* **Deutsch:**
  > **Strix Writer — Klassenarbeiten und Übungen für Lehrkräfte.**
  > Ein quelloffener Editor in Rust, um Klassenarbeiten vorzubereiten. Rechenaufgaben halten
  > Angaben, Lösung und Ergebnisse lebendig, mit Varianten A, B, C für jede Bankreihe;
  > Multiple-Choice-, Richtig/Falsch- und offene Fragen tragen ihre Punkte; eine Datei pro Aufgabe
  > ergibt eine Aufgabensammlung, aus der die Klassenarbeit entsteht, mit oder ohne Lösungen. Eine
  > Punktetabelle summiert selbst nach Kriterien; ein Notenschlüssel, der Punkte in Noten umrechnet,
  > ist geplant. Nativ unter Windows, macOS, Linux und BSD, und im Browser.

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

Ogni presentazione completa si regge su questi cinque punti, in quest'ordine: è la giornata del
docente, dalla preparazione al voto.

1. **Esercizi di calcolo** — il nucleo
   * Didattica › Nuovo esercizio: titolo, testo, **dati** (simbolo, descrizione, valore, unità),
     domande annidate su tre livelli (a), a.1), a.1.1)), **soluzione** con ogni domanda ripetuta e
     lo spazio per svolgerla, **riepilogo dei risultati** unico a fine documento. La struttura
     non si rompe modificando.
   * **Varianti** A, B, C… dei dati: sceglierne una ricalcola dati, soluzione e riepilogo. Una
     fila di banchi, una variante, nessuna soluzione da rifare a mano.
   * Sotto c'è la **matematica viva**: l'equazione scritta nella pagina *è* il calcolo, con un
     solo `=` (definisce `t = 10 s`, mostra `s = a·t²/2 =`, risolve `v² = 2·a·d`, verifica
     `s ≤ 150 m ✓`), **unità** controllate a ogni passo, **passaggi della soluzione**, tabelle
     calcolate. Ogni esercizio è una **sandbox**: due esercizi possono definire entrambi `t`.
   * Disegni **CAD** che seguono i dati (cambi `b` e il triangolo si ridisegna) e **carta a quadretti**
     dopo le domande, come sul quaderno.
2. **Domande a risposta multipla, vero/falso e aperte**
   * Tipi di esercizio: **problema**, **risposta aperta** (uno spazio di altezza data in cm, a
     quadretti o bianco), **risposta multipla** (opzioni A), B), C)… con le caselle; più risposte
     giuste ammesse) e **vero/falso**.
   * Le risposte giuste si spuntano nell'esercizio e si **nascondono insieme alla soluzione**: lo
     spazio resta, la pagina dello studente non cambia impaginazione.
   * Ogni domanda ha i suoi **punti**, mostrati accanto e sommati nella tabella dei punteggi.
3. **Momenti di valutazione: dall'archivio al compito in classe**
   * **Un file per esercizio** è l'archivio, con le sue proprietà (classe, indirizzo, materia,
     docente, durata, punteggio) leggibili anche da Word.
   * Un **documento master** compone la serie da consegnare: numerazione, frontespizio, salti di
     pagina, parti mostrate (con o senza soluzione e risultati) restano del master, i file
     dell'archivio non cambiano mai.
   * **Formattazione DSA** (Lettura, Scrittura, Calcolo e memoria, combinabili) applicata alla
     stesso documento, pagina sempre bianca; esportazione in **PDF** per la stampa.
4. **Punteggi e griglia dei voti**
   * **Tabella dei punteggi** (Didattica › Punteggi): una riga per esercizio, una coppia di colonne
     per criterio (IMP, MOD, CAL/RIS…: punti ottenuti e punti massimi), **totali** per riga e per
     colonna che seguono mentre si scrive. Il totale è la proprietà **Punteggio** del file.
   * **Griglia per l'assegnazione dei voti** (dai punti al voto, con soglie e arrotondamenti):
     **pianificata**. Fino ad allora si scrive solo "pianificata".
5. **Il fondamento: un documento vero, ovunque, senza crash**
   * Sotto c'è un editor completo: barra multifunzione, stili, elenchi, tabelle, note, commenti,
     revisioni, ortografia, stampa, 27 lingue dell'interfaccia; apre e risalva DOCX finché il
     documento non ha oggetti Strix.
   * Una sola base di codice Rust per Windows, macOS, Linux, FreeBSD e il browser (WebAssembly):
     la versione web non richiede installazione sui PC del laboratorio.
   * **Mai un crash**: nessun `unwrap`, `expect` o `unsafe` nel codice di produzione; ogni comando
     è provato con parametri ostili; un panic sfuggito diventa un errore e il documento viene
     ripristinato.
   * **Privacy**: nessuna telemetria, nessun account, nessun controllo aggiornamenti; la rete si
     usa solo quando lo chiedi tu ([`docs/code-signing.md`](../code-signing.md), «Privacy»).
   * **Tutto è un comando** con id e parametri (barra multifunzione, `strix-cli`, **server MCP**):
     un agente AI può preparare un esercizio o un intero momento di valutazione.

---

## 5. Aspetti distintivi ("Come funziona")

Alla domanda «in cosa è diverso?» si risponde con questi quattro aspetti, mai con un confronto.
Sono la sezione «Come funziona» del sito (`site/strings/*.json`, chiavi `a1…a4`).

| Aspetto | Cosa dire | Fatti da citare |
| :--- | :--- | :--- |
| **Ogni esercizio è un calcolo** | Dati, soluzione e risultati si calcolano nella pagina; cambi un dato e tutto segue. | Varianti A/B/C, un solo `=`, unità controllate, riepilogo dei risultati, sandbox per esercizio. |
| **Ogni tipo di domanda** | Problemi, risposte aperte, risposta multipla e vero/falso nello stesso momento di valutazione, ciascuno col suo punteggio. | Caselle spuntate sulle risposte giuste, nascoste con la soluzione; spazio di risposta in cm, a quadretti o bianco. |
| **Dall'archivio al momento di valutazione** | Un file per esercizio; il master compone il momento di valutazione e lo impagina a modo suo senza toccare l'archivio. | Frontespizio, numerazione, con o senza soluzioni, formattazione DSA, PDF; proprietà d'archivio leggibili da Word. |
| **I punti si sommano da soli** | La tabella dei punteggi fa i totali per criterio ed esercizio mentre scrivi. | Criteri a due colonne (ottenuti/massimi), totale = proprietà Punteggio; griglia dei voti **pianificata**. |

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
   * **Simbolo** = l'icona dell'app (`assets/app-icon/strix.svg`): il gufo con la **piuma** del
     wordmark (stessa forma: vessillo con due tagli, rachide chiara, pennino): favicon, header,
     card social.
   * **Wordmark** = `docs/brand/wordmark-light.svg` (Deep Forest, per fondi chiari) e
     `wordmark-dark.svg` (Crest Mint, per fondi scuri): le lettere S T R X disegnate a tratto
     (spessore 12 su 64 di altezza delle maiuscole), senza font; la **I è una piuma** in Coral
     Orange (gradiente `#C2410C` → `#F97316` su fondo chiaro, `#EA580C` → `#FDBA74` su fondo
     scuro), inclinata di 16°, col pennino sulla linea di base e la punta che sale sopra le
     maiuscole. Proporzioni 290 × 88.
   * **Nell'app** (`crates/ui-egui/src/brand.rs`, immagini in `assets/brand/` scritte da
     `packaging/icons.sh`): simbolo + wordmark nella finestra Informazioni su Strix, simbolo sopra
     il wordmark nella finestra senza documento, wordmark Crest Mint in fondo al menu di File.
     Nella versione web, simbolo e wordmark nella schermata di caricamento.
   * In header si affiancano simbolo e wordmark (il simbolo a sinistra, altezza 1:1 con le
     maiuscole + 20%). Mai deformare, ricolorare con colori fuori tavolozza o mettere il simbolo su
     fondo verde senza il suo bordo.
   * Card social: `docs/brand/social.png` (1280×640), da `docs/brand/social.html`.

---

## 8. Messaggi per pubblico

### Persona 1 — Il docente (pubblico principale)

* **Chi è:** insegna matematica, fisica, materie tecniche (ma anche altre materie con domande a
  risposta multipla) e prepara compiti e momenti di valutazione ogni settimana, spesso in più varianti e per
  studenti con DSA.
* **Dolore:** riscrivere lo stesso esercizio con dati diversi per ogni fila e ricalcolare a mano
  le soluzioni; tenere allineati testo, soluzione e risultati; impaginare la versione con e senza
  soluzioni; sommare i punti e convertirli in voti a fine correzione; rifare il compito in
  formato DSA.
* **Hook:** *"Prepara i momenti di valutazione. Soluzioni e punteggi li tiene Strix."* · *"Un esercizio, tante
  varianti, nessuna soluzione da rifare."*
* **Da mostrare, in quest'ordine:** un esercizio di calcolo con dati in due varianti (A e B) che
  ricalcolano la soluzione; una domanda a risposta multipla con la casella giusta spuntata che
  sparisce nascondendo la soluzione; il master che compone il momento di valutazione dall'archivio con il
  frontespizio; la tabella dei punteggi che fa i totali; la versione DSA; il PDF.

### Persona 2 — Lo studente

* **Dolore:** risolvere problemi a mano con le unità, sbagliare una conversione, non capire dove.
* **Hook:** *"Le unità non ti tradiscono più."*
* **Da mostrare:** errore dimensionale intercettato, suggerimento "forse kN?", passaggi della
  soluzione, la versione web (niente installazione sul PC del laboratorio), gratis e open source.

### Persona 3 — L'ingegnere e il tecnico

* **Dolore:** il calcolo vive in un foglio elettronico, la relazione in un altro file; ogni
  modifica di un dato significa ricopiare formule e numeri.
* **Hook:** *"Scrivi l'equazione. Calcola."*
* **Da mostrare:** la trave appoggiata (`L`, `q`, `M = q·L²/8 =`, `σ ≤ f_yd ✓`), la tabella
  calcolata, il disegno CAD che si ridisegna con `L`.

### Persona 4 — Lo sviluppatore che lavora con agenti AI

* **Dolore:** gli agenti producono numeri in chat, non documenti verificabili.
* **Hook:** *"Ogni azione è un comando. Anche per il tuo agente."*
* **Da mostrare:** `claude mcp add strix -- strix-cli mcp`, `exercise.insert`, `exercise.scores`,
  `batch`, `render_page`: l'agente che prepara un momento di valutazione con varianti e la salva come `.xmath`;
  cartelle confinate (`--automation-read-root`, `--automation-write-root`).

---

## 9. Copy ufficiale pronto all'uso

### 9.1 Tagline

| Formato | EN | IT | FR | DE |
| :--- | :--- | :--- | :--- | :--- |
| **Categoria** | Tests and exercises for teachers | Momenti di valutazione ed esercizi per il docente | Évaluations et exercices pour l'enseignant | Klassenarbeiten und Übungen für Lehrkräfte |
| **Hero** | Write the test. Strix keeps the solutions and the scores. | Prepara i momenti di valutazione. Soluzioni e punteggi li tiene Strix. | Écrivez l'évaluation. Strix tient le corrigé et les points. | Schreib die Klassenarbeit. Lösungen und Punkte führt Strix. |
| **Contenuto** | Calculation exercises, multiple choice, tests and scores, live in the page. | Esercizi di calcolo, risposta multipla, momenti di valutazione e punteggi, vivi nella pagina. | Exercices de calcul, QCM, évaluations et points, vivants dans la page. | Rechenaufgaben, Multiple Choice, Klassenarbeiten und Punkte, live auf der Seite. |
| **Fiducia** | Open source. No account. No telemetry. Never crashes by design. | Open source. Nessun account. Nessuna telemetria. Progettato per non andare in crash. | Open source. Sans compte. Sans télémétrie. Conçu pour ne jamais planter. | Open Source. Kein Konto. Keine Telemetrie. Absturzfrei konstruiert. |

### 9.2 Descrizione del repository GitHub (≤ 350 caratteri)

> Tests and exercises for teachers: an open-source Rust editor for calculation exercises with
> live solutions and variants, multiple-choice and open questions, tests assembled from an
> archive, and scores tables that add up by themselves. Windows, macOS, Linux, BSD and the web.

* **Homepage:** `https://nabre.github.io/strix/`
* **Topics:** `education`, `teachers`, `teaching`, `exercises`, `tests`, `quiz`,
  `multiple-choice`, `assessment`, `grading`, `math`, `wysiwyg`, `units`, `document-editor`,
  `rust`, `mcp`, `webassembly`, `cross-platform`, `open-source`
* **Social preview:** caricare `docs/brand/social.png` (Settings › General › Social preview).

Comando (da eseguire a mano, lo decide il proprietario):

```sh
gh repo edit nabre/strix \
  --description "Tests and exercises for teachers: an open-source Rust editor for calculation exercises with live solutions and variants, multiple-choice and open questions, tests assembled from an archive, and scores tables that add up by themselves." \
  --homepage https://nabre.github.io/strix/ \
  --add-topic education,teachers,teaching,exercises,tests,quiz,multiple-choice,assessment,grading,math,wysiwyg,units,document-editor,rust,mcp,webassembly,cross-platform,open-source
```

### 9.3 Elevator pitch (README, pagina "Informazioni", store)

**EN:**
> **Strix Writer** is an open-source editor for preparing tests, written in pure Rust. Calculation
> exercises keep their data, solution and results live: the equations in the page calculate,
> with units, and variants A, B, C give each row of desks its own numbers without redoing a
> single solution. Multiple-choice, true/false and open questions carry their points, and the
> correct answers hide with the solution. One file per exercise makes an archive; a master
> document puts a test together from it, with its own cover page, numbering and DSA formatting,
> with or without the solutions, and never changes the archive. A scores table adds the points up
> by criterion as you type; a grading grid from points to marks is planned. It saves `.xmath`
> files that other programs open with the last computed values, exports PDF, and every action is
> a command that a script or an AI agent can run. Native on Windows, macOS, Linux and FreeBSD, and
> in the browser. MIT or Apache-2.0.

**IT:**
> **Strix Writer** è un editor open source per preparare i momenti di valutazione, scritto interamente in
> Rust. Gli esercizi di calcolo tengono vivi dati, soluzione e risultati: le equazioni nella
> pagina calcolano, con le unità, e le varianti A, B, C danno a ogni fila di banchi i suoi numeri
> senza rifare una sola soluzione. Le domande a risposta multipla, vero/falso e aperte portano il
> loro punteggio, e le risposte giuste si nascondono con la soluzione. Un file per esercizio fa
> l'archivio; un documento master ne compone il momento di valutazione, con il suo frontespizio, la sua
> numerazione e la formattazione DSA, con o senza soluzioni, senza mai toccare l'archivio. La
> tabella dei punteggi somma i punti per criterio mentre scrivi; la griglia dai punti ai voti è
> pianificata. Salva file `.xmath` che altri programmi aprono con gli ultimi valori calcolati,
> esporta in PDF, e ogni azione è un comando che uno script o un agente AI può eseguire. Nativo su
> Windows, macOS, Linux e FreeBSD, e nel browser. MIT o Apache-2.0.

### 9.4 Pianificato (si scrive sempre "pianificato")

**Griglia per l'assegnazione dei voti** (dai punti al voto: soglie, scala, arrotondamento, voto
per studente); tabella dei risultati per esercizio e controllo dei valori attesi; riepilogo per
variante; pannello Archivio e ricerca nell'archivio; per il calcolo pannello Variabili, selettore
di unità, `°C`/`°F`, funzioni utente `f(x) = …`, matrici, sistemi, grafici; per il CAD gli
strumenti di modifica, tratteggi, quote angolari e vincoli. Fonti: README («Status and
roadmap»), `docs/exercise.md` e `docs/archive.md` («Next»).

---

## 10. Lancio e promozione ("mostra, non dire")

### 10.1 Video di 40 secondi: *"One exercise, every row"*

1. **0:00–0:10** Didattica › Nuovo esercizio: un'automobile che parte da ferma, dati `v = 90 km/h`,
   `t = 10 s`, domanda a) «l'accelerazione»; nella soluzione `a = v/t =` mostra `2.5 m/s²`.
2. **0:10–0:20** Variante B: `v = 72 km/h`. Soluzione (`a = 2 m/s²`) e riepilogo dei risultati
   cambiano da soli.
3. **0:20–0:30** Un esercizio a risposta multipla: la casella giusta è spuntata; Mostra › Soluzione
   spento: le spunte spariscono, lo spazio resta.
4. **0:30–0:36** La tabella dei punteggi in frontespizio: si scrivono i punti, i totali seguono.
5. **0:36–0:40** Wordmark su `#1F1F1F`, accento `#10B981`: *"Strix — Write the test. Strix keeps
   the solutions and the scores."* e l'indirizzo del sito.

Registrazione: solo finestre di Strix (`ui_shot` o la finestra dell'app per id), mai il desktop.

### 10.2 Canali e testi pronti

**Scuole, università, ITS (canale principale)**

Proposta: il docente prepara un esercizio per file, compone il momento di valutazione da un master, stampa la
variante A e la variante B in PDF, la versione DSA dallo stesso documento, e la tabella dei
punteggi è già pronta per la correzione. La versione web non richiede installazione sui PC del
laboratorio. Gratis, open source, senza account. Luoghi: gruppi e forum di docenti per materia,
animatori digitali, corsi di formazione, dipartimenti di matematica e di discipline tecniche.

**Reddit**

* `r/Teachers`, `r/matheducation`, `r/physicsteachers`, `r/ScienceTeachers` — angolo pratico: un
  compito con varianti e punteggi. Titolo: *"I built a free editor for tests: calculation
  exercises with variants, multiple choice and a scores table that adds itself up"*. Una sola
  immagine: il compito con la tabella dei punteggi.
* `r/rust` — angolo tecnico: registro comandi, niente panic, wasm, egui.
* `r/engineering`, `r/StructuralEngineering` — la trave con la verifica e il disegno.
* `r/ClaudeAI`, `r/LocalLLaMA` — MCP: un agente prepara un compito con varianti.

**Hacker News (`Show HN`)**

* *Titolo:* `Show HN: Strix – A Rust editor for teachers' tests, where the exercises calculate`
* *Primo commento:*

  > Hi HN, Strix is an editor for preparing school tests. A calculation exercise has data,
  > questions and a solution whose equations calculate, with units (`M = q·L²/8 =` shows
  > `30.375 kN·m`), and variants A/B/C of the data recalculate the solution, so each row of desks
  > gets its own numbers. Multiple-choice, true/false and open questions carry their points; a
  > scores table adds them up by criterion. One file per exercise is the archive, and a master
  > document assembles the test with or without solutions, without touching the files.
  >
  > It is pure Rust (egui front end, vello_cpu rendering, krilla for PDF), one code base for
  > Windows, macOS, Linux, FreeBSD and WebAssembly. Every action is a command in one registry,
  > so the same ids run from the ribbon, the CLI and an MCP server. Production code has no
  > unwrap/expect/unsafe. It is a fork of the WordCraft word processor, which gives it a real
  > document model underneath.
  >
  > It's alpha. I'd love feedback from teachers on what a grading grid should do.

**Post breve (X, Mastodon, Bluesky, LinkedIn)**

* EN: *One exercise, variants A and B, solutions recalculated, multiple choice, a scores table
  that adds itself up. Strix: an open-source editor for teachers' tests. Windows, macOS, Linux,
  BSD, web. https://nabre.github.io/strix/*
* IT: *Un esercizio, varianti A e B, soluzioni ricalcolate, risposta multipla, tabella dei
  punteggi che somma da sola. Strix: editor open source per i momenti di valutazione. Windows, macOS, Linux,
  BSD, web. https://nabre.github.io/strix/it/*

### 10.3 Note di rilascio (modello)

```markdown
## Strix vX.Y.Z

**Highlights**
- <una riga per novità, con l'esempio numerico quando c'è>

**Downloads**: Windows (MSI, portable ZIP, portable exe; x64, x86, ARM64) · macOS (universal DMG) ·
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
| Numeri (comandi, test, righe) | `cargo xtask stats` |
| Release e nomi dei file | `.github/workflows/release.yml`, `packaging/` |

Quando cambia un fatto (piattaforme, formati, comandi, stato), si aggiornano insieme: questo
manuale, `site/strings/*.json` nelle quattro lingue e i quattro README.
