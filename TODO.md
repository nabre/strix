# Strix — Piano di Sviluppo Didattico e Matematico (`TODO`)

> **Regola di manutenzione:** Questo documento contiene le specifiche tecniche e architetturali dettagliate degli sviluppi e dei miglioramenti ancora da implementare. **Ogni voce completata, coperta da test (`cargo xtask ci`) e annotata nel progress log di [`ROADMAP.md`](ROADMAP.md) va rimossa da questo file**, così che resti sempre e solo la coda di lavoro attiva.

---

## Vincoli architetturali trasversali (validi per ogni punto)

* **Separazione dei layer (`L0–L6`):** logica matematica e grafica pura in `L1` (`strix-calc`, `strix-cad`, `strix-plot`), orchestrazione e comandi in `L4` (`wordcraft-engine`), interfaccia sottile in `L6` (`wordcraft-ui-egui`), verificata da `cargo xtask layers`.
* **Standard *Never Crash*:** nessun `unwrap()`, `expect()`, `panic!`, `todo!`, `unreachable!`, `unsafe`; aritmetica saturante/controllata, limiti superiori espliciti su ogni collezione e iterazione, copertura in `crates/engine/src/tests.rs::hostile_params_never_panic`.
* **Tutto è un comando (`CommandSpec`):** ogni funzione è invocabile in modo identico da Ribbon, ricerca comandi, CLI (`strix-cli`), canale di controllo e server MCP, senza mai aprire finestre modali nelle chiamate programmatiche.
* **Compatibilità `.docx` e `wasm32`:** round-trip trasparente con Microsoft Word tramite content control `strix:*`, `customXml/strixObjects.xml` e `docProps/custom.xml`; zero dipendenze incompatibili con `strix-web` (`cargo xtask wasm`).
* **Mergeabilità con upstream WordCraft:** codice nuovo confinato in crate `strix-*` o file/moduli dedicati di Strix, riducendo al minimo le righe di aggancio nei file condivisi.
* **Terminologia:** nei testi pubblici/UI italiani usare *momenti di valutazione* (mai *verifica* per indicare il compito nel lessico di prodotto; vedi [`docs/brand/MARKETING.md`](docs/brand/MARKETING.md)).

---

## Coda di lavoro attiva

*(Tutte le voci pianificate — Fasi 1, 2, 3, 4 e 5: `D1`–`D6`, `M1`–`M6`, `F1`–`F9` — sono state completate, coperte da test e registrate in [`ROADMAP.md`](ROADMAP.md).)*

### Fase 6 — Thesaurus scaricabili (`LO1`)

Oggi `review.thesaurus` (`crates/engine/src/cmd/review/proofing.rs`, file upstream) è una lista fissa di ~30 parole inglesi. Il [repository dei dizionari di LibreOffice](https://github.com/LibreOffice/dictionaries), già usato da `crates/dictionaries` alla revisione fissa `REVISION`, contiene un thesaurus per quasi tutte le lingue di correzione, nel formato di testo MyThes (`th_*.dat`).

**Regole specifiche:**
* **Solo dati, scaricati su richiesta:** i file restano opere separate con la loro licenza (anche GPL/LGPL), scritta in un file accanto a quello scaricato; mai nel binario né nel repository (nessuna riga in `ATTRIBUTION.md`, che vale per i file del repo).
* **Clean room:** lettore scritto in Rust dalla descrizione del formato, mai dal codice di MyThes o LibreOffice. Formato: prima riga = codifica (`UTF-8`, `ISO8859-1`, `KOI8-R`…); poi righe `parola|n` seguite da `n` righe `(categoria)|sinonimo|sinonimo…`.
* **Input ostile:** limite di dimensione prima di leggere, lunghezza di riga e numero di voci limitati, `n` dichiarato non fidato, `get()` e slicing a confini di carattere, nessun panic.
* **Wasm:** download solo nelle build native; sul web il comando ripiega sulla lista interna.
* **Mergeabilità:** logica in file Strix (`crates/dictionaries/src/mythes.rs`, `crates/engine/src/cmd/thesaurus.rs`); in `proofing.rs` solo la riga che li chiama.

| Id | Compito | Fatto quando |
|---|---|---|
| `LO1.1` | `Source` acquisisce un `kind` (ortografia / thesaurus); righe thesaurus con la licenza letta dal README della cartella a `REVISION`: `it_IT/th_it_IT_v2.dat` (GPL-3.0), `es/th_es_v2.dat` (LGPL-2.1), `pt_PT/th_pt_PT.dat` (CC-BY-3.0), `pl_PL/th_pl_PL_v2.dat` (GPL/LGPL/MPL/Apache-2.0/CC-SA), `ru_RU/th_ru_RU_M_aot_and_v2.dat` (LGPL-2.1), `id/th_id_ID_v2.dat` (LGPL-2.1). `pt_BR`, `cs_CZ` ed `en` solo dopo aver confermato la licenza; se resta dubbia la lingua resta fuori | test: ogni riga ha licenza non vuota e pagina a `REVISION` |
| `LO1.2` | Lettore MyThes limitato (`mythes.rs`): codifiche UTF-8 e ISO-8859-x/KOI8-R, indice in memoria, ricerca per parola (minuscole, forma esatta poi forma base) | legge file reali; file vuoti, troncati, enormi, con `n` falso o codifica ignota danno errore, mai panic |
| `LO1.3` | `review.thesaurus` usa il thesaurus scaricato della lingua della parola (lingua del run, poi del documento), la lista interna come ripiego; risposta invariata (`word`, `synonyms`) più `meanings` per categoria | test engine con un piccolo thesaurus sintetico nostro; `hostile_params_never_panic` verde |
| `LO1.4` | Download offerto accanto a quello del dizionario (menu lingua della barra di stato, File › Opzioni › Lingua) e nel riquadro Thesaurus quando manca; file di licenza scritto accanto | screenshot `ui_shot`; testi in inglese, tedesco, francese, italiano |
| `LO1.5` | Documentazione (`docs/architecture.md` riga `dictionaries`, documento della funzione), progress log e revisione in `ROADMAP.md`; `cargo xtask ci` e `cargo xtask unused` | voce rimossa da questo file |

### Fase 7 — Proposte a WordCraft (`LO4`, issue su storytold/wordcraft)

Funzioni dell'editor: il piano è di WordCraft (vedi `ROADMAP.md`), Strix le riceve col merge upstream. L'agente di sviluppo non ha accesso a GitHub: **le issue le apre il proprietario** copiando i testi qui sotto (in inglese, la lingua di WordCraft). Strix implementa una proposta da sé solo se upstream la rifiuta, in file nuovi di Strix. Spuntare la voce quando la issue è aperta, rimuoverla quando è chiusa.

- [ ] **`LO4.1` — Font substitution: Gelasio for Georgia**
  > Documents asking for Georgia render with a generic serif when it isn't installed. Gelasio (OFL-1.1, Google Fonts) is designed metric-compatible with Georgia, like Carlito for Calibri and Caladea for Cambria already in `crates/fonts/src/word.rs`. Proposal: add Gelasio first in Georgia's substitution list; the font file, if bundled, goes to storytold/craft-fonts. Pagination of Georgia documents then matches Word's.
- [ ] **`LO4.2` — Hybrid PDF: the editable document attached to the exported PDF**
  > An option in Export › PDF to attach the source document (DOCX or ODT) to the PDF as an embedded file (ISO 32000-1 §7.11.4, `/AF` with PDF/A-3). Opening such a PDF in WordCraft offers to open the attached document for editing; any other viewer shows a normal PDF. Use case: a teacher sends one file that prints as-is and stays editable. Check krilla's support for embedded files first.
- [ ] **`LO4.3` — EPUB 3 export**
  > File › Export › EPUB built on the HTML writer in `crates/formats`, following the W3C EPUB 3.3 specification: XHTML content documents split at Heading 1 (or page breaks), navigation document from the headings, images and fonts as resources, metadata from document properties, reflowable layout. Output validated with EPUBCheck in tests where available.
- [ ] **`LO4.4` — Word completion (off by default)**
  > While typing, suggest the rest of a long word (≥ 8 letters by default) already used in the document, shown as a tip after the caret; Enter or Tab accepts, any other key ignores it. Off by default since Word doesn't do it; File › Options › Advanced: on/off, minimum word length, accept key. Words collected from the open document only, list capped.
- [ ] **`LO4.5` — Reading OpenOffice.org 1.x documents (`.sxw`, `.stw`)**
  > Old school and office archives still hold OpenOffice.org 1.x text documents. The format is a zip of XML close to ODF, documented in Sun's published "OpenOffice.org XML File Format 1.0" specification. Proposal: open them read-only through a mapping to the existing ODF reader (namespaces and element names that differ), saving as DOCX/ODT. Written from the specification only.

<!-- Coordinamento per l'agente che mantiene TODO.md — Fase 5 (F7, F8, D5, D6):
nel precedente passaggio di consegne l'agente della fase 5 aveva omesso la pulizia
richiesta del TODO. Il file condiviso ora risulta già ripulito: non restano voci
attive della fase 5 da rimuovere. Implementazione e progress log sono presenti;
validazioni eseguite: 77 test di calcolo, 11 test d'integrazione della fase 5,
Clippy mirato e controllo WASM. Il precedente tentativo di CI globale si era
fermato alla formattazione dei grafici allora in lavorazione: consolidare il suo
esito con le verifiche finali del lavoro parallelo.
-->
