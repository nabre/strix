<p align="center">
  <img src="../assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png" width="128" height="128" alt="Icona di Strix: un gufo, metà piume e metà circuito, con una penna, su una tessera verde scuro">
</p>

<h1 align="center">Strix</h1>

<p align="center">
  <b>Verifiche ed esercizi per il docente: scrivi la verifica, soluzioni e punteggi li tiene Strix.</b>
</p>

<p align="center">
  Strix è un editor open source, scritto interamente in Rust, per preparare le verifiche. Gli
  esercizi di calcolo tengono vivi dati, soluzione e risultati, con le varianti A, B, C per ogni
  fila di banchi; le domande a risposta multipla, vero/falso e aperte portano il loro punteggio;
  un file per esercizio fa l'archivio, e da lì si compone la verifica, con o senza soluzioni. La
  tabella dei punteggi somma da sola i punti per criterio; la griglia che trasforma i punti in
  voti è pianificata. Sotto, le equazioni nella pagina calcolano, con le unità di misura. Gira in
  modo nativo su Windows, macOS, Linux e BSD, e nel browser con WebAssembly.
</p>

<p align="center">
  <a href="https://nabre.github.io/strix/it/"><b>Sito</b></a> ·
  <a href="https://github.com/nabre/strix/releases/latest"><b>Scarica</b></a> ·
  <a href="https://nabre.github.io/strix/app/?sample"><b>Provalo nel browser</b></a> ·
  <a href="https://github.com/nabre/strix/issues"><b>Segnalazioni</b></a>
</p>

<p align="center"><a href="../README.md">English</a> · Italiano · <a href="README.fr.md">Français</a> · <a href="README.de.md">Deutsch</a></p>

> **Stato: alpha.** L'editor di documenti su cui Strix è costruito è maturo; il motore di calcolo,
> gli esercizi e il CAD sono nuovi e cambiano in fretta. Tieni una copia dei documenti importanti e
> [segnalaci cosa non va](https://github.com/nabre/strix/issues). Il README in inglese è il
> riferimento completo.

## Esercizi di calcolo

**Didattica › Esercizio › Nuovo esercizio** inserisce un problema svolto con una struttura che le
modifiche non rompono: titolo, testo, **dati** (simbolo, descrizione, valore, unità), domande con
lettere annidate su tre livelli (a), a.1), a.1.1)); la **soluzione** ripete ogni domanda in sola
lettura con lo spazio per lo svolgimento; un **riepilogo dei risultati** a fine documento.

La soluzione è un calcolo vivo (vedi [Matematica WYSIWYG](#matematica-wysiwyg)): i dati possono
avere **varianti** A, B, C… e sceglierne una ricalcola dati, soluzione e riepilogo, così ogni fila
di banchi ha i suoi numeri e nessuna soluzione si rifà a mano. Ogni esercizio è una **sandbox**
(due esercizi possono definire entrambi `L`); dopo le domande può seguire la carta a quadretti, e
un disegno CAD nel testo segue i dati.

## Risposta multipla e risposta aperta

Un esercizio può essere anche un quiz: **risposta aperta** (uno spazio di altezza data in cm, a
quadretti o bianco), **risposta multipla** (opzioni A), B), C)… con le caselle, anche più risposte
giuste) e **vero/falso**. Le risposte giuste si spuntano nell'esercizio e **si nascondono con la
soluzione**: le caselle restano vuote e gli spazi mantengono l'altezza, così la copia dello
studente è impaginata come quella del docente. Ogni domanda ha i suoi **punti**, mostrati accanto
e sommati nella tabella dei punteggi.

## Dall'archivio alla verifica

- **Un file per esercizio** fa l'archivio; ogni file porta le sue **proprietà d'archivio**
  (classe, sezione, materia, docente, durata, punteggio), visibili anche in Word
  ([archive.md](archive.md)).
- Un **documento master** compone la verifica da quei file, con la sua numerazione, il
  frontespizio, i salti di pagina e le parti mostrate (con o senza soluzioni e risultati);
  **i file dell'archivio non cambiano mai** ([master-documents.md](master-documents.md)).
- La **formattazione DSA** (Design › Formattazione documento › DSA) impagina la stessa
  verifica per gli studenti con DSA: Lettura, Scrittura, Calcolo e memoria, combinabili, pagina
  sempre bianca ([dsa.md](dsa.md)).
- Esportazione in **PDF** per la stampa.

## Punteggi e voti

La **tabella dei punteggi** (Didattica › Punteggi › Tabella punteggi) ha una riga per esercizio e
due colonne per criterio (IMP, MOD, CAL/RIS…: i punti assegnati sul foglio e quelli che
l'esercizio vale); i **totali** per riga e per colonna seguono mentre scrivi, e il totale
generale è la proprietà **Punteggio** del file.

*Pianificato:* una **griglia per l'assegnazione dei voti** che trasforma i punti in voti (soglie,
scala, arrotondamento). Dettagli (in inglese): [exercise.md](exercise.md).

## Matematica WYSIWYG

Niente foglio separato, niente celle di codice, niente barra della formula. Un'equazione nel
documento *è* il calcolo: scritta sul posto con l'editor di equazioni e mostrata come matematica
impaginata. Strix legge le equazioni calcolate in ordine di lettura e tiene aggiornati i risultati:
dopo ogni modifica (aspettando che il cursore lasci l'equazione), con F9 e all'apertura del file.

```text
L = 4.5 m                          definisce L
q = 12 kN/m                        le unità si moltiplicano e dividono come i numeri
M = q·L²/8 =                       →  M = q·L²/8 = 30.375 kN·m
W = 1000 cm³
f_yd = 355 MPa
σ = M/W =                          →  σ = M/W = 30.375 MPa
σ ≤ f_yd                           →  σ ≤ f_yd ⇒ 30.375 MPa ≤ 355 MPa ✓
F = m·a                            →  F = m·a ⇒ a = 2.5 m/s²        (a è l'unica incognita: ricavata)
```

- **Un solo `=` per tutto**: definizione, risultato da mostrare (anche nell'unità scelta,
  `M = kN·m`), formula con il suo valore, equazione da **risolvere** per l'unica incognita, o
  **verifica** (`≤ ≥ < > ≠` con ✓/✗).
- **Soluzione**: l'incognita è isolata simbolicamente quando compare una volta, altrimenti
  trovata numericamente.
- **Unità ovunque**: SI con prefissi, unità tecniche (`mm`, `kN`, `MPa`, `bar`, `kWh`…),
  adimensionali conservate (`%`, `‰`, `°`, `rad`). Gli errori dimensionali (`2 m + 3 s`) vengono
  segnalati, mai calcolati in silenzio. L'unità di lavoro segue ciò che scrivi; un'unità con le
  maiuscole sbagliate riceve un suggerimento («forse kN?»).
- **Passaggi della soluzione**, **tabelle calcolate** (colonne con unità e formule, `Σ =`,
  `mean =`, `min =`, `max =`), **sommatorie, intervalli e vettori**, **formato numerico**.
- **Gli errori non fermano il foglio**: un'istruzione che non torna mostra `⚠` e il messaggio.

Il calcolo è nella scheda **Matematica**; esercizi, punteggi e carta a quadretti nella scheda **Didattica**. Dettagli (in inglese): [calc.md](calc.md).

## Disegni CAD guidati dai calcoli

**Inserisci › Disegno CAD** mette un disegno tecnico nel testo. Coordinate, raggi e angoli possono
essere **espressioni dei parametri** (`["L", 0]`, `L/2 + 0.3`): cambi `L = 4.5 m` e la trave si
ridisegna; le quote richiamano i parametri (`{{L}}` mostra `4.5 m`). L'editor sul foglio del
disegno ha una riga di comando per le coordinate, **snap esatti** sulle curve vere, grip che
conservano le espressioni, un pannello Proprietà e i layer. Il disegno è salvato in JSON nel file
`.xmath` insieme a un'immagine SVG. Dettagli: [cad.md](cad.md).

## Foglio a quadretti

**Inserisci › Foglio a quadretti** quadretta la pagina o un blocco del testo, come un quaderno, con
una linea più marcata ogni N quadretti, nitida a ogni ingrandimento e in PDF.
Dettagli: [graph-paper.md](graph-paper.md).

## Un editor di documenti completo

Sotto c'è un word processor vero (Strix è un fork di WordCraft): barra multifunzione, stili,
elenchi, tabelle, immagini e forme, intestazioni e piè di pagina, sezioni e colonne, note,
citazioni e sommari, commenti e revisioni, ortografia e sillabazione, stampa unione e stampa.
L'interfaccia segue la lingua del sistema (italiano, inglese, tedesco o francese).

## File

| | Formati |
|---|---|
| **Salva** | `.xmath` (documento) e `.xmathm` (modello): un pacchetto OOXML che mantiene vivi calcoli, esercizi e disegni |
| **Risalva** (documenti senza oggetti Strix) | `.docx`, `.docm`, `.dotx`, `.dotm`, `.odt`, `.rtf`, `.txt`, Word XML |
| **Esporta** (copie statiche) | PDF, PNG, LaTeX, HTML, Markdown |
| **Apre** | `.docx`, `.docm`, `.dotx`, `.dotm`, `.doc`, `.odt`, `.rtf`, `.md`, `.html`, `.tex`, `.txt` |

Strix apre un `.docx` (o `.odt`, `.rtf`…) e lo risalva nello stesso formato, finché il documento
non contiene **oggetti Strix**: calcoli, tabelle calcolate, esercizi, disegni CAD o fogli a
quadretti. Quei formati ne perderebbero gli automatismi di calcolo e il sorgente dei disegni,
quindi un documento che ne contiene si salva solo in `.xmath`: Salva propone un `.xmath` con lo
stesso nome e dice cosa andrebbe perso («3 calcoli, 1 disegno CAD»), Salva con nome esclude i
formati che li perderebbero, e `file.save`, `strix-cli convert` e MCP `save_document` rifiutano con
la stessa spiegazione. Le esportazioni (PDF, PNG, LaTeX, HTML, Markdown) sono copie e funzionano
sempre. Un file `.xmath` resta un pacchetto standard: altri programmi mostrano gli ultimi valori
calcolati.

## Per gli agenti: comandi, CLI, MCP

Tutto ciò che Strix fa è un **comando** in un unico registro. Barra multifunzione, scorciatoie,
ricerca dei comandi, riga di comando, canale di controllo e server MCP eseguono gli stessi comandi,
e le chiamate programmatiche non aprono mai finestre di dialogo.

```sh
claude mcp add strix -- strix-cli mcp                      # documenti senza interfaccia
strix --control 7981 &                                     # oppure guida l'app aperta
claude mcp add strix-app -- strix-cli mcp --connect 127.0.0.1:7981
```

Vedi [mcp.md](mcp.md) e il [protocollo di controllo](control-protocol.md).

## Scarica

Installatori e versioni portabili sono nella [pagina delle release](https://github.com/nabre/strix/releases)
e sul [sito](https://nabre.github.io/strix/it/#download): Windows (MSI, ZIP portabile ed exe portabile, che tiene le impostazioni in una cartella
`StrixData` accanto a sé; x64, x86,
ARM64), macOS (DMG universale), Linux (AppImage, deb, rpm, Flatpak, Arch; x86_64 e aarch64, più
riscv64), FreeBSD e la build web. Le build Windows non sono ancora firmate. Privacy: Strix non
invia nulla da solo (niente telemetria, niente account, niente controllo degli aggiornamenti);
dettagli in [code-signing.md](code-signing.md).

## Compilare ed eseguire

Serve una toolchain Rust (1.95 o successiva); per la build web anche [Trunk](https://trunkrs.dev/)
e il target `wasm32-unknown-unknown`.

```sh
git clone https://github.com/nabre/strix
cd strix
cargo run --release -p strix -- --sample          # l'app desktop con un documento di esempio
cargo xtask dev                                   # build di debug, ricompilata e riavviata a ogni modifica (watchexec)
                                                  # senza schermo (sandbox, SSH) o con --web: build web su http://127.0.0.1:8771/?sample
cargo run --release -p strix -- problema.xmath    # apre un documento Strix
strix-cli convert problema.xmath problema.pdf     # esportazione statica: pdf, png, tex, html, md
```

Web: `cd apps/strix-web && trunk serve`, poi apri <http://127.0.0.1:8771/?sample>.

## Licenza e crediti

Strix è distribuito con doppia licenza [MIT](../LICENSE-MIT) o [Apache-2.0](../LICENSE-APACHE), a
tua scelta. Copyright (c) 2026 nabre e i contributori di Strix. Strix è un fork di
[WordCraft](https://github.com/storytold/wordcraft) e non è affiliato né approvato dai suoi autori;
le note richieste sono in [NOTICE](../NOTICE). Font, icone, dizionari e traduzioni inclusi
mantengono le loro licenze aperte, elencate in [ATTRIBUTION.md](../ATTRIBUTION.md).
