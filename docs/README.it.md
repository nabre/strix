<p align="center">
  <img src="../assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png" width="128" height="128" alt="Icona di Strix: un gufo, metà piume e metà circuito, con una penna, su una tessera verde scuro">
</p>

<h1 align="center">Strix</h1>

<p align="center">
  <b>Matematica WYSIWYG per l'ingegneria: scrivi l'equazione nella pagina, e calcola.</b>
</p>

<p align="center">
  Strix è un editor di documenti open source, scritto interamente in Rust, per risolvere problemi
  di matematica e di ingegneria. Scrivi il problema come su carta (testo, tabelle, equazioni,
  disegni) e le equazioni sono vive: portano le unità di misura, ricavano l'incognita, verificano
  i limiti e si aggiornano appena cambia un dato. Gli esercizi hanno una struttura bloccata con il
  riepilogo dei risultati, e i disegni CAD seguono i calcoli. Gira in modo nativo su Windows,
  macOS, Linux e BSD, e nel browser con WebAssembly.
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

Tutto è nella scheda **Matematica**. Dettagli (in inglese): [calc.md](calc.md).

## Esercizi

**Matematica › Nuovo esercizio** inserisce un problema svolto con una struttura che le modifiche non
rompono: titolo, testo, dati, domande con lettere annidate su tre livelli; la soluzione ripete ogni
domanda in sola lettura con lo spazio per lo svolgimento; un **riepilogo dei risultati** a fine
documento. Ogni esercizio è una **sandbox di calcolo** (due esercizi possono definire entrambi `L`)
e i dati possono avere **varianti** A, B, C… che ricalcolano tutto. Dettagli: [exercise.md](exercise.md).

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
L'interfaccia segue la lingua del sistema (27 lingue, italiano compreso).

## File

| | Formati |
|---|---|
| **Salva** | `.xmath` (documento) e `.xmathm` (modello): un pacchetto OOXML che mantiene vivi calcoli, esercizi e disegni |
| **Esporta** (copie statiche) | PDF, PNG, LaTeX, HTML, Markdown |
| **Apre** (importa) | `.docx`, `.docm`, `.dotx`, `.dotm`, `.doc`, `.odt`, `.rtf`, `.md`, `.html`, `.tex`, `.txt` |

Un file `.xmath` resta un pacchetto standard: altri programmi mostrano gli ultimi valori calcolati.

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
e sul [sito](https://nabre.github.io/strix/it/#download): Windows (MSI e ZIP portabile; x64, x86,
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
