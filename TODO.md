# Strix — Piano di Sviluppo Didattico e Matematico (`TODO`)

> **Regola di manutenzione:** Questo documento contiene le specifiche tecniche e architetturali dettagliate degli sviluppi e dei miglioramenti ancora da implementare. **Ogni voce completata, coperta da test (`cargo xtask ci`) e annotata nel progress log di [`ROADMAP.md`](ROADMAP.md) va rimossa da questo file**, così che resti sempre e solo la coda di lavoro attiva.

---

## Vincoli architetturali trasversali (validi per ogni punto)

* **Separazione dei layer (`L0–L6`):** logica matematica e grafica pura in `L1` (`strix-calc`, `strix-cad`, nuovo `strix-plot`), orchestrazione e comandi in `L4` (`wordcraft-engine`), interfaccia sottile in `L6` (`wordcraft-ui-egui`), verificata da `cargo xtask layers`.
* **Standard *Never Crash*:** nessun `unwrap()`, `expect()`, `panic!`, `todo!`, `unreachable!`, `unsafe`; aritmetica saturante/controllata, limiti superiori espliciti su ogni collezione e iterazione, copertura in `crates/engine/src/tests.rs::hostile_params_never_panic`.
* **Tutto è un comando (`CommandSpec`):** ogni funzione è invocabile in modo identico da Ribbon, ricerca comandi, CLI (`strix-cli`), canale di controllo e server MCP, senza mai aprire finestre modali nelle chiamate programmatiche.
* **Compatibilità `.docx` e `wasm32`:** round-trip trasparente con Microsoft Word tramite content control `strix:*`, `customXml/strixObjects.xml` e `docProps/custom.xml`; zero dipendenze incompatibili con `strix-web` (`cargo xtask wasm`).
* **Mergeabilità con upstream WordCraft:** codice nuovo confinato in crate `strix-*` o file/moduli dedicati di Strix, riducendo al minimo le righe di aggancio nei file condivisi.
* **Terminologia:** nei testi pubblici/UI italiani usare *momenti di valutazione* (mai *verifica* per indicare il compito nel lessico di prodotto; vedi [`docs/brand/MARKETING.md`](docs/brand/MARKETING.md)).

---

## 1. Miglioramenti alle funzionalità didattiche esistenti

### [ ] `D1` — Dati iniziali (`strix:data`) e Varianti visibili nella UI dei Quiz (`calc` e `drawing`)
* **Problema attuale:**
  1. In `crates/engine/src/cmd/exercise.rs` (`ExerciseData`, `with_data`, `seeds`), dati e varianti sono già salvati su `<strix:data>` del controllo `TAG_EXERCISE` per qualsiasi tipo di esercizio. Tuttavia in `crates/ui-egui/src/exercise_pane.rs` (righe 114–126), quando `st["kind"] != "problem"`, il pannello disegna `Questions` ed esegue `return`, nascondendo `01 Variants` e `02 Data`.
  2. In `crates/engine/src/cmd/exercise/items.rs` (`exercise_blocks`), i quiz non creano mai il blocco visivo `TAG_DATA`: se si impostano dati via CLI/MCP per un quiz `calc`/`drawing`, i calcoli funzionano ma lo studente non legge i dati nel documento.
* **Specifica:**
  * Quando `with_data` popola dati non vuoti su un quiz privo di `Part::Data`, `update(s)` inserisce automaticamente `TAG_DATA` subito dopo `TAG_TEXT` (o dopo `TAG_TITLE` se `TAG_TEXT` è assente) e prima di `TAG_QUESTIONS`; se i dati vengono svuotati su un quiz, rimuove `TAG_DATA`.
  * Aggiungere `showData: bool` (default `true`) su `ExerciseData` per consentire anche l'uso di parametri "nascosti" interpolati direttamente nel testo.
  * In `crates/ui-egui/src/exercise_pane.rs`, esporre `Variants` e `Data` anche per i quiz (espansi automaticamente se sono presenti quesiti `calc`/`drawing` o se `data.items` non è vuoto; collassabili altrimenti).

### [ ] `D2` — Conversione modalità quesito in loco (`exercise.questionMode`) e controlli contestuali Quiz
* **Problema attuale:**
  * In `crates/engine/src/cmd/exercise/items.rs` non è possibile convertire una domanda esistente tra `open`, `choice`, `trueFalse`, `calc`, `drawing` senza eliminarla e ricrearla.
  * In `crates/ui-egui/src/exercise_quiz.rs` (righe 51–103), i quesiti `calc` e `drawing` mostrano solo i controlli generici `Space (cm)` e `Squared`.
* **Specifica:**
  * Nuovo comando `exercise.questionMode`: `{"exercise"?: 1-based, "question": 1-based, "type": "open"|"choice"|"trueFalse"|"calc"|"drawing"}`.
  * **Transizione conservativa:** il paragrafo domanda (`TAG_ITEM` + `TAG_POINTS`, formattazione e revisioni) resta intatto; muta solo `TAG_ANSWER`:
    * `Open ↔ Calc`: preserva `height`, `grid`, `spacing` e tutte le equazioni/paragrafi già scritti nello spazio risposta; passando a `Calc` appende `result_line_para` (`TAG_LINE`) e ricalcola; tornando a `Open` rimuove solo `TAG_LINE`.
    * `Choice ↔ TrueFalse`: da `trueFalse` a `choice` mantiene le 2 opzioni e ne aggiunge 2 vuote; da `choice` a `trueFalse` imposta `Vero / Falso` localizzati se le opzioni sono vuote.
    * `* → Drawing`: preserva `height`/`grid`/`spacing` e inserisce lo starter CAD solo se in `TAG_ANSWER` non c'è già un `GraphicKind::Cad`.
  * **UI in `exercise_quiz.rs`:**
    * Menu a tendina per cambiare `Mode` sulla domanda selezionata;
    * Per `calc`: anteprima live della riga risultato (`A = 15 cm²`) o dell'errore (`⚠`), più pulsante `Vai allo svolgimento`;
    * Per `drawing`: scala corrente del disegno e pulsante `Apri nell'editor CAD`.

### [ ] `D3` — Punteggi per sottodomanda nei Problemi (`a)`, `b)`, `c)`) e fix tabella `strix:scores` `1×1`
* **Problema attuale:**
  * Nei quiz i punti alimentano automaticamente il primo criterio di `strix:scores` (`crates/engine/src/cmd/exercise/scores.rs`, righe 243–259), mentre nei `problem` le domande `a)`, `b)` non possono dichiarare punti.
  * In `scores.rs` (righe 112–113), quando `exercises == 1 && criteria == 1`, `across` e `down` sono entrambi `false`, omettendo la cella di riepilogo totale.
* **Specifica:**
  * Estendere `exercise.questionPoints` anche agli esercizi `problem`: quando `points: Some(p)` è impostato su una domanda `STYLE_QUESTION`, inserire a fine riga il controllo bloccato `TAG_POINTS` (` (2 punti)`); quando `points: None`, rimuoverlo lasciando il formato classico.
  * Aggiornare `scores.rs` affinché, se almeno una domanda di un `problem` ha `TAG_POINTS`, la somma popoli automaticamente il primo criterio dell'esercizio.
  * Nel caso `1 esercizio × 1 criterio`, mantenere visibile il riepilogo del totale massimo (`__ / Max`).

### [ ] `D4` — Validazione continua su *tutte* le varianti (`A..Z`)
* **Problema attuale:** `data_info` (`crates/engine/src/cmd/exercise.rs`, righe 930–954) e il ricalcolo verificano solo la variante attiva (`e.data.active`), lasciando invisibili divisioni per zero, radici negative o disegni CAD degeneri nelle varianti inattive.
* **Specifica:**
  * Al cambio di firma o equazioni di un esercizio con più varianti, eseguire in memoria (senza toccare DOM o layout) la sandbox `strix-calc` e `Drawing::resolve` di `strix-cad` per tutte le varianti `0..N`.
  * Esporre gli errori per variante in `exercise.data` (`"variantErrors": {"B": [...], "D": [...]}`) e mostrare un badge `⚠` sui pulsanti variante `[ B ⚠ ]` in `exercise_pane.rs` con tooltip esplicativo.

### [ ] `D5` — Potenziamento Accessibilità DSA (`design.dsa`)
* **Problema attuale:** In `crates/engine/src/cmd/exercise/dsa.rs` (`docs/dsa.md`), i run con corsivo diretto non vengono toccati, le cifre non attivano l'allineamento tabulare OpenType (`tnum`) per la discalculia e i *Problemi* non offrono una traccia del risultato atteso nella modalità *Maths and memory*.
* **Specifica:**
  * Propagare la feature OpenType `tnum` (cifre a larghezza fissa) nello shaper `crates/fonts` quando `design.dsa` è attivo.
  * Quando `dsa.italic == false`, rimuovere in modo reversibile anche il corsivo diretto nei `Run` di testo discorsivo (registrando gli intervalli in `customXml/strixObjects.xml` per il ripristino esatto), preservando il corsivo matematico ISO 80000-2 sulle variabili a singola lettera nelle equazioni.
  * Con *Maths and memory* attivo e `Show = false`, generare sotto lo spazio risposta delle domande dei *Problemi* la traccia guidata `simbolo = ____________ unità` ricavata dall'equazione marcata `Result`.

### [ ] `D6` — UI Archivio Didattico (`Teaching › Archive`) e isolamento Documenti Master
* **Problema attuale:** `archive.properties` (`crates/engine/src/cmd/exercise/archive.rs`) non ha interfaccia nella scheda `Teaching` (`crates/ui-egui/src/ribbon/strix.rs`), manca la ricerca su cartella e nei Master Document (`docs/master-documents.md`) cambiare impostazioni di vista/serie sporca i sottodocumenti riscrivendoli su disco.
* **Specifica:**
  * Aggiungere in `Teaching` l'editor delle proprietà d'archivio (*Materia*, *Classe*, *Indirizzo*, *Sezione*, *Durata*, nuovo campo *Argomenti/Tag*, più *Punteggio* calcolato) e il pannello `Banca Esercizi`.
  * Comando `archive.scan`: legge dai `.docx` di una cartella solo `docProps/core.xml` e `docProps/custom.xml` nello ZIP (senza parsare `word/document.xml`), filtrando istantaneamente nelle 4 lingue; azioni `Inserisci copia` e `Collega sottodocumento`. Su `strix-web` (WASM), supporto a `archive.importBundle` da archivio `.zip`.
  * Nei Master Document, separare `content_dirty` dagli override di vista/serie del master (`Show`, `design.dsa`, intestazione serie) così da non riscrivere mai su disco i sottodocumenti non modificati nel contenuto.

---

## 2. Miglioramenti al motore matematico (`strix-calc` e integrazione documento)

### [ ] `M1` — Formato numerico e opzioni di calcolo salvate nel documento
* **Problema attuale:** In `crates/engine/src/cmd/calc.rs` (righe 96–124), `calc_digits`, `calc_count`, `calc_zeros`, `calc_steps` e `calc_layout` vivono solo in `Prefs` (globali per utente) anziché nel documento.
* **Specifica:**
  * Introdurre una gerarchia di risoluzione `Equazione > Esercizio > Documento > Prefs`.
  * Persistenza delle opzioni di documento in `customXml/strixObjects.xml` (senza sporcare `wordcraft_doc::Settings` upstream): i nuovi documenti ereditano da `Prefs`, i documenti aperti usano le proprie impostazioni salvate.

### [ ] `M2` — Semantica unit-aware per `round()`, `floor()`, `ceil()`, `trunc()`
* **Problema attuale:** In `crates/calc/src/eval.rs` (righe 575–585), `round(x; n)` arrotonda il valore SI coerente: `round(30°; 0)` arrotonda `0.5236 rad` a `1 rad` (`57.3°`) e rifiuta le temperature (`°C`, `°F`).
* **Specifica:**
  * Con 1 o 2 argomenti (`round(x; n)`), arrotondare rispetto all'unità di visualizzazione `hint_unit` se presente (incluse `°`, `%`, multipli `cm`/`km` e temperature affini `°C`/`°F`), riconvertendo poi in SI e mantenendo `hint_unit`.
  * Supportare un 3° argomento unità opzionale `round(x; n; unità)` (es. `round(alpha; 1; °)`), applicando la stessa logica a `floor`, `ceil`, `trunc`.

### [ ] `M3` — Localizzazione completa (`IT`, `EN`, `DE`, `FR`) delle note di calcolo
* **Problema attuale:** `numeric_note` in `crates/engine/src/cmd/calc.rs` (righe 233–236) gestisce solo `it` ed `en`, ignorando `de` e `fr`.
* **Specifica:** Spostare le stringhe in `crates/engine/src/cmd/exercise/words.rs` (`(numerical solution)`, `(soluzione numerica)`, `(numerische Lösung)`, `(solution numérique)`).

### [ ] `M4` — Estensione catalogo unità di misura e prefissi SI (`crates/calc/src/units.rs`)
* **Prefissi SI completi:** aggiungere `P` ($10^{15}$), `T` ($10^{12}$), `da` ($10^1$), `d` ($10^{-1}$), `p` ($10^{-12}$), `f` ($10^{-15}$).
  * *Regola anti-collisione:* match esatto su `UNITS` **prima** della decomposizione `prefisso + unità`, così `T` è Tesla mentre `THz`/`TW` sono tera-, `d` è giorno mentre `dm`/`dL`/`dB` sono deci-, e `pF` è picofarad senza collidere con `Pa`.
* **Nuove unità da aggiungere:**
  * *Elettromagnetismo:* `F` (farad), `H` (henry), `T` (tesla), `Wb` (weber), `S` (siemens);
  * *Fotometria:* `lm` (lumen), `lx` (lux) sulla dimensione base `cd` già presente;
  * *Chimica e Fisica:* `atm`, `torr`, `mmHg`, `eV`, `cal` (`kcal`), `Da` / `u` (massa atomica);
  * *Topografia, Agraria e Meccanica:* `ha` (ettaro), `are` ($100\text{ m}^2$), `gon`/`grad` ($\pi/200\text{ rad}$), `arcmin`, `arcsec`, `d`/`day`, `yr`, `rpm` ($2\pi/60\text{ s}^{-1}$), `kn` (nodo).

### [ ] `M5` — Libreria di funzioni STEM, statistica di laboratorio ed esponenti Unicode
* **Nuove funzioni in `crates/calc/src/parser.rs` ed `eval.rs`:**
  * *Algebriche/Trigonometriche:* `cbrt(x)`, `root(x; n)` (con divisione esatta degli esponenti `Dim`), `log2(x)`, `log(x; base)`, `atan2(y; x)` (richiede `Dim(y) == Dim(x)`), `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `floor`, `ceil`, `trunc`, `sign`, `mod(a; b)`.
  * *Combinatoria e Aritmetica intera:* `gcd`/`mcd`, `lcm`/`mcm`, `fact(n)` + operatore postfisso `n!`, `binom(n; k)` (senza overflow intermedio fino a $n = 1000$).
  * *Statistica e Regressione lineare per tabelle di laboratorio:* `median`, `var`, `varp`, `stdev`/`std`, `stdevp`, `slope(X; Y)` (dimensione $D_Y / D_X$), `intercept(X; Y)` (dimensione $D_Y$), `r2(X; Y)`.
* **Lexer Unicode (`crates/calc/src/lexer.rs`, righe 140–143):** leggere sequenze complete di apici Unicode `⁺⁻⁰¹²³⁴⁵⁶⁷⁸⁹` come `Tok::Sup(i32)` invece dei soli `¹²³`.

### [ ] `M6` — Risolutore simbolico di 1°/2° grado, `abs`, inferenza dimensionale e pannello `Variables`
* **Risolutore simbolico (`crates/calc/src/solve.rs`):**
  * Estrattore polinomiale simbolico `poly_coeffs(expr, var)` fino al 2° grado per equazioni in cui l'incognita compare più volte (es. $2x + 3 = x + 7$ o $s = v_0 t + \frac{1}{2} a t^2$).
  * Per il 2° grado: verifica omogeneità dimensionale di $B^2 - 4AC$, errore esplicito per $\Delta < 0$, generazione passaggi con formula risolutiva e calcolo di entrambe le radici $x_1, x_2$, più supporto a vincoli di dominio fisico espliciti (es. `t > 0`).
  * Correggere `Func::Abs` in `isolate_steps` (oggi scartato silenziosamente) ramificando $\pm$ per $|u(x)| = c$.
  * Nel fallback numerico, inferire algebricamente la dimensione dell'incognita per potenze generiche $x^k$ (es. `I = b * h^4 / 12`) e dimensioni inverse.
* **UI `Calculations › Variables`:** pannello laterale collegato a `calc.variables` per ispezionare e raggiungere tutte le grandezze del foglio e delle sandbox esercizio, con selettore rapido di conversione unità.

---

## 3. Nuove funzionalità STEM e didattiche

### [ ] `F1` — Griglia di valutazione (`exercise.grading` / `strix:grading`) e tolleranze
* Tabella automatica `strix:grading` agganciata al totale di `strix:scores` (`maxPoints: null` = automatico) con preset scolastici (`it-10`, `ch-6`, `de-6` con scala invertita, `fr-20`, `percent`, `custom`), soglia di sufficienza configurabile (`passRatio`, `passGrade`), curva `linear` o `piecewiseLinear` (spezzata a due pendenze), passo di arrotondamento (`0.1`, `0.25`, `0.5`, `1.0`) e visibilità `teacher` o `both`.
* Tolleranza opzionale sui quesiti `calc` (`tolerance`: `"2%"` o `"0.1 cm"`), mostrata con l'intervallo accettabile nella riga risultato della copia docente.

### [ ] `F2` — Opzioni e distrattori parametrici nei Quiz (`{{expr}}`) + rimescolamento per variante
* Controllo inline `strix:expr` (`{{espressione}}`) valutato nella sandbox dell'esercizio con unità formattata nello stile `Unit`, utilizzabile sia nel testo sia nelle opzioni `A), B), C), D)` dei quiz (così risposta esatta e distrattori basati su errori tipici si aggiornano da soli in ogni variante).
* Nota diagnostica opzionale `<strix:rationale>` sui distrattori, visibile nella sola copia docente.
* Flag `shuffleChoices: bool` sulle domande `choice`: permutazione deterministica delle scelte guidata da hash `(exercise_idx, question_idx, variant_name)`, senza duplicare i dati.

### [ ] `F3` — Generatore controllato di varianti (`exercise.generateVariants`)
* Comando che genera $N$ varianti (`A, B, C…`) a partire da domini discreti o intervalli sui dati (`min`, `max`, `step` o lista valori) e vincoli sui risultati (`maxDecimals`, `positiveResults`, `distinctFinalResults`, predicati `extra` come `L > b`), verificando in memoria sandbox `strix-calc` e geometria `strix-cad` entro un budget deterministico limitato (`MAX_CANDIDATE_EVALS = 5_000`).

### [ ] `F4` — Esportazione "Pacchetto Momento di Valutazione" (`file.exportPack`)
* Esportazione batch in un clic (cartella su desktop o singolo `.zip` su desktop e `strix-web` WASM) che produce:
  * PDF Studente per ciascuna variante (`Fila_A.pdf`, `Fila_B.pdf`… con `Show = false`);
  * PDF Studente versione DSA (`design.dsa` attivo);
  * PDF Correttore Docente (`Show = true`, svolgimenti, griglia voti e **Tabella Sinottica Multi-Variante** finale `Domanda | Fila A | Fila B | Fila C | Fila D`).

### [ ] `F5` — Grafici matematici e sperimentali vivi (`crates/plot` — `strix-plot`, layer `L1`)
* Nuovo crate `strix-plot` in `L1` integrato tramite `GraphicKind::Plot` (con SVG + `GraphicItem` vettoriali per `vello_cpu`, `krilla` PDF e Word round-trip).
* Tracciamento adattivo di funzioni `y = f(x)` e curve parametriche dal `Worksheet` attivo con rilevamento discontinuità/asintoti verticali, punti notevoli (zeri, estremi, tangenti, area integrale $\int_a^b f(x)\,dx$).
* Tracciamento serie sperimentali da `calc.table` con barre d'errore ($\pm \Delta x, \pm \Delta y$) e curva di regressione.
* **Modalità duale Studente/Docente (`answer: bool`):** nella copia studente stampa il solo piano cartesiano quotato e grigliato con identico ingombro; nella copia docente sovrappone curve e punti.

### [ ] `F6` — Disegno CAD "da completare" (`Layer.answer` ed `Entity.answer` in `strix-cad`)
* Attributo booleano `answer` su `Layer` ed `Entity` in `crates/cad/src/model.rs`.
* **Invarianza di impaginazione:** `bounds` e scala `Fit` sono sempre calcolati sull'intero disegno (incluse le entità `answer`), mentre il rendering con `Show = false` omette le primitive `answer`, lasciando allo studente la base geometrica in scala e lo spazio esatto per completare il disegno senza spostare i salti pagina.
* Toggle rapido `Anteprima Studente` nella barra dell'editor CAD.

### [ ] `F7` — Calcolo con incertezze e propagazione dell'errore (`±`) in `strix-calc`
* Campo `unc: Option<f64>` su `Quantity`, sintassi infissa `±` / `+-` (assoluta `12.4 ± 0.1 cm` o relativa `12.4 ± 2% cm`) e funzioni `val(x)`, `unc(x)`, `relunc(x)`.
* Due modalità didattiche selezionabili nelle opzioni di calcolo: **incertezze massime** (`max`) e **somma in quadratura / GUM** (`rss`).
* Formattazione metrologica in `crates/calc/src/format.rs`: arrotondamento automatico di $\Delta x$ a 1–2 cifre significative e allineamento automatico dei decimali del valore centrale.

### [ ] `F8` — Nuove modalità didattiche di quesito STEM (`worked`, `cloze`, `tableFill`, `matching`)
* `worked` (*Esempio svolto*): visibile per intero anche nella copia studente, senza punti né riga in `strix:scores`.
* `cloze` (*Completamento inline*): lacune `strix:blank` nel testo/formule sostituite da `________` (con eventuale unità) per lo studente e valorizzate (anche da `{{expr}}`) per il docente.
* `tableFill` (*Tabella calcolata da completare*): celle/colonne di `calc.table` marcate `studentBlank: true`, vuote per lo studente e calcolate per il docente.
* `matching` (*Abbinamento*): due colonne con rimescolamento deterministico per variante della colonna destra e caselle di associazione.

### [ ] `F9` — Controllo "Pre-flight" del compito (`exercise.check`)
* Comando puro diagnostico (per UI `Teaching › Check`, `exportPack` e agenti MCP) che ispeziona tutte le varianti `A..Z` segnalando con link `Pos` cliccabile:
  * *Errori:* `CalcError` o errori CAD in qualsiasi variante, domande `choice` senza risposta corretta o con opzioni duplicate/collidenti dopo il calcolo, sottodocumenti mancanti;
  * *Avvisi:* domande prive di `Result`/soluzione, dati iniziali inutilizzati (possibili refusi di simbolo), punti mancanti rispetto a `strix:scores`, spazio risposta inferiore all'altezza effettiva dello svolgimento;
  * *Info:* confronto somma durate esercizi vs durata documento, differenze nel numero di pagine tra Fila A, B, C….

---

## Sequenza di implementazione suggerita

1. **Fase 1 (Allineamento e fondamenta):** `D1`, `D2`, `D3`, `M1`, `M2`, `M3`
2. **Fase 2 (Potenza matematica):** `M4`, `M5`, `M6`
3. **Fase 3 (Valutazione, varianti e pre-flight):** `D4`, `F1`, `F2`, `F3`, `F9`, `F4`
4. **Fase 4 (Grafica scientifica e CAD didattico):** `F6`, `F5`
5. **Fase 5 (Laboratorio, nuovi quesiti, DSA e archivio):** `F7`, `F8`, `D5`, `D6`
