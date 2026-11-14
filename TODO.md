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

*(Tutte le voci pianificate — Fasi 1–6: `D1`–`D6`, `M1`–`M6`, `F1`–`F9`, `LO1` — sono state completate, coperte da test e registrate in [`ROADMAP.md`](ROADMAP.md).)*

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
