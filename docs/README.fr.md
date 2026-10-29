<p align="center">
  <img src="../assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png" width="128" height="128" alt="Icône de Strix : une chouette, mi-plumes mi-circuit, avec une plume d'écriture, sur une tuile vert foncé">
</p>

<h1 align="center">Strix</h1>

<p align="center">
  <b>Les maths WYSIWYG pour l'ingénierie : tapez l'équation dans la page, elle calcule.</b>
</p>

<p align="center">
  Strix est un éditeur de documents open source, écrit entièrement en Rust, pour résoudre des
  problèmes de mathématiques et d'ingénierie. Vous écrivez le problème comme sur papier (texte,
  tableaux, équations, dessins) et les équations sont vivantes : elles portent leurs unités,
  trouvent l'inconnue, vérifient les limites et se mettent à jour dès qu'une donnée change. Les
  exercices gardent une structure verrouillée avec un récapitulatif des résultats, et les dessins
  CAO suivent les calculs. Natif sur Windows, macOS, Linux et BSD, et dans le navigateur grâce à
  WebAssembly.
</p>

<p align="center">
  <a href="https://nabre.github.io/strix/fr/"><b>Site</b></a> ·
  <a href="https://github.com/nabre/strix/releases/latest"><b>Télécharger</b></a> ·
  <a href="https://nabre.github.io/strix/app/?sample"><b>Essayer dans le navigateur</b></a> ·
  <a href="https://github.com/nabre/strix/issues"><b>Signaler un problème</b></a>
</p>

<p align="center"><a href="../README.md">English</a> · <a href="README.it.md">Italiano</a> · Français · <a href="README.de.md">Deutsch</a></p>

> **État : alpha.** L'éditeur de documents sur lequel Strix est bâti est mûr ; le moteur de calcul,
> les exercices et la CAO sont récents et évoluent vite. Gardez une copie de vos documents
> importants et [dites-nous ce qui casse](https://github.com/nabre/strix/issues). Le README anglais
> est la référence complète.

## Les maths WYSIWYG

Pas de feuille séparée, pas de cellules de code, pas de barre de formule. Une équation du document
*est* le calcul : tapée sur place avec l'éditeur d'équations et affichée comme des maths composées.
Strix lit les équations calculées dans l'ordre de lecture et tient les résultats à jour : après
chaque modification (en attendant que le curseur quitte l'équation), sur F9 et à l'ouverture.

```text
L = 4.5 m                          définit L
q = 12 kN/m                        les unités se multiplient et se divisent comme des nombres
M = q·L²/8 =                       →  M = q·L²/8 = 30.375 kN·m
W = 1000 cm³
f_yd = 355 MPa
σ = M/W =                          →  σ = M/W = 30.375 MPa
σ ≤ f_yd                           →  σ ≤ f_yd ⇒ 30.375 MPa ≤ 355 MPa ✓
F = m·a                            →  F = m·a ⇒ a = 2.5 m/s²        (a est l'unique inconnue : résolue)
```

- **Un seul `=` pour tout** : définition, résultat à afficher (y compris dans l'unité choisie,
  `M = kN·m`), formule avec sa valeur, équation à **résoudre** pour son unique inconnue, ou
  **vérification** (`≤ ≥ < > ≠` avec ✓/✗).
- **Résolution** : l'inconnue est isolée symboliquement quand elle apparaît une fois, sinon
  trouvée numériquement.
- **Des unités partout** : SI avec préfixes, unités techniques (`mm`, `kN`, `MPa`, `bar`, `kWh`…),
  unités sans dimension conservées (`%`, `‰`, `°`, `rad`). Les erreurs de dimension (`2 m + 3 s`)
  sont signalées, jamais calculées en silence. L'unité de travail suit ce que vous écrivez ; une
  unité mal capitalisée reçoit un indice (« vouliez-vous dire kN ? »).
- **Étapes de la solution**, **tableaux calculés** (colonnes avec unités et formules, `Σ =`,
  `mean =`, `min =`, `max =`), **sommes, plages et vecteurs**, **format numérique**.
- **Les erreurs n'arrêtent pas la feuille** : une instruction en échec affiche `⚠` et son message.

Tout est dans l’onglet **Math**. Détails (en anglais) : [calc.md](calc.md).

## Exercices

**Math › New Exercise** (interface pas encore traduite) insère un problème corrigé dont la structure résiste aux modifications :
titre, énoncé, données, questions lettrées imbriquées sur trois niveaux ; la solution reprend
chaque question en lecture seule avec la place pour le développement ; un **récapitulatif des
résultats** en fin de document. Chaque exercice est un **bac à sable de calcul** (deux exercices
peuvent définir `L`) et les données peuvent avoir des **variantes** A, B, C… qui recalculent tout.
Détails : [exercise.md](exercise.md).

## Des dessins CAO pilotés par les calculs

**Insertion › CAD Drawing** place un dessin technique dans le texte. Coordonnées, rayons et angles
peuvent être des **expressions des paramètres** (`["L", 0]`, `L/2 + 0.3`) : changez `L = 4.5 m` et
la poutre se redessine ; les cotes rappellent les paramètres (`{{L}}` affiche `4.5 m`). L'éditeur
sur la feuille du dessin offre une ligne de commande pour les coordonnées, des **accrochages
exacts** sur les vraies courbes, des poignées qui gardent les expressions, un volet Propriétés et
des calques. Le dessin est enregistré en JSON dans le fichier `.xmath` avec une image SVG.
Détails : [cad.md](cad.md).

## Papier quadrillé

**Insertion › Graph Paper** quadrille la page ou un bloc du texte, comme un cahier, avec un
trait plus fort tous les N carreaux, net à tout zoom et en PDF. Détails : [graph-paper.md](graph-paper.md).

## Un vrai éditeur de documents

Dessous, un vrai traitement de texte (Strix est un fork de WordCraft) : ruban, styles, listes,
tableaux, images et formes, en-têtes et pieds de page, sections et colonnes, notes, citations et
tables des matières, commentaires et suivi des modifications, orthographe et césure, publipostage
et impression. L'interface suit la langue du système (27 langues, dont le français).

## Fichiers

| | Formats |
|---|---|
| **Enregistre** | `.xmath` (document) et `.xmathm` (modèle) : un paquet OOXML qui garde vivants calculs, exercices et dessins |
| **Exporte** (copies statiques) | PDF, PNG, LaTeX, HTML, Markdown |
| **Ouvre** (import) | `.docx`, `.docm`, `.dotx`, `.dotm`, `.doc`, `.odt`, `.rtf`, `.md`, `.html`, `.tex`, `.txt` |

Un fichier `.xmath` reste un paquet standard : d'autres logiciels affichent les dernières valeurs
calculées.

## Pour les agents : commandes, CLI, MCP

Tout ce que fait Strix est une **commande** d'un registre unique. Le ruban, les raccourcis, la
recherche de commandes, la ligne de commande, le canal de contrôle et le serveur MCP exécutent les
mêmes commandes, et les appels programmatiques n'ouvrent jamais de boîte de dialogue.

```sh
claude mcp add strix -- strix-cli mcp                      # documents sans interface
strix --control 7981 &                                     # ou piloter l'application ouverte
claude mcp add strix-app -- strix-cli mcp --connect 127.0.0.1:7981
```

Voir [mcp.md](mcp.md) et le [protocole de contrôle](control-protocol.md).

## Télécharger

Installateurs et versions portables sont sur la [page des versions](https://github.com/nabre/strix/releases)
et sur le [site](https://nabre.github.io/strix/fr/#download) : Windows (MSI et ZIP portable ; x64,
x86, ARM64), macOS (DMG universel), Linux (AppImage, deb, rpm, Flatpak, Arch ; x86_64 et aarch64,
plus riscv64), FreeBSD et la version web. Les versions Windows ne sont pas encore signées.
Confidentialité : Strix n'envoie rien de lui-même (ni télémétrie, ni compte, ni vérification des
mises à jour) ; détails dans [code-signing.md](code-signing.md).

## Compiler et lancer

Il faut une chaîne d'outils Rust (1.95 ou plus récente) ; pour la version web, aussi
[Trunk](https://trunkrs.dev/) et la cible `wasm32-unknown-unknown`.

```sh
git clone https://github.com/nabre/strix
cd strix
cargo run --release -p strix -- --sample          # l'application avec un document d'exemple
cargo run --release -p strix -- probleme.xmath    # ouvrir un document Strix
strix-cli convert probleme.xmath probleme.pdf     # export statique : pdf, png, tex, html, md
```

Web : `cd apps/strix-web && trunk serve`, puis ouvrir <http://127.0.0.1:8771/?sample>.

## Licence et crédits

Strix est distribué sous double licence [MIT](../LICENSE-MIT) ou [Apache-2.0](../LICENSE-APACHE),
au choix. Copyright (c) 2026 nabre et les contributeurs de Strix. Strix est un fork de
[WordCraft](https://github.com/storytold/wordcraft) ; il n'est ni affilié à ses auteurs ni approuvé
par eux, et les mentions requises sont dans [NOTICE](../NOTICE). Polices, icônes, dictionnaires et
traductions inclus gardent leurs licences ouvertes, listées dans [ATTRIBUTION.md](../ATTRIBUTION.md).
Les couleurs suivent la palette d'[Atollo](https://github.com/nabre/atollo).
