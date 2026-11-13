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

<!-- Coordinamento per l'agente che mantiene TODO.md — Fase 5 (F7, F8, D5, D6):
nel precedente passaggio di consegne l'agente della fase 5 aveva omesso la pulizia
richiesta del TODO. Il file condiviso ora risulta già ripulito: non restano voci
attive della fase 5 da rimuovere. Implementazione e progress log sono presenti;
validazioni eseguite: 77 test di calcolo, 11 test d'integrazione della fase 5,
Clippy mirato e controllo WASM. Il precedente tentativo di CI globale si era
fermato alla formattazione dei grafici allora in lavorazione: consolidare il suo
esito con le verifiche finali del lavoro parallelo.
-->
