#!/usr/bin/env bash
# The screenshots of the site and the READMEs: real windows of Strix, rendered headless by the
# ui_shot example (crates/ui-egui/examples/ui_shot.rs), in each language and in both themes:
#   site/shots/<lang>/shot-calc-{light,dark}.png      a car accelerating from rest, on the Math tab
#   site/shots/<lang>/shot-exercise-{light,dark}.png  the same problem as an exercise: data in two
#                                                     variants, questions, the worked solution
#   site/shots/<lang>/shot-quiz-{light,dark}.png      a quiz: multiple choice, true/false and a
#                                                     calculation question, with points
# The documents are written here, by us. Usage, from the repository root:
#   site/shots/make.sh [--no-build] [LANG...]
# builds ui_shot in release mode first (unless --no-build) and makes every language (or the ones
# named). Original work by the Strix contributors, MIT OR Apache-2.0.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
if [[ "${1:-}" == "--no-build" ]]; then
  shift
else
  cargo build --release -q -p wordcraft-ui-egui --example ui_shot
fi
ONLY=" $* "
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/examples/ui_shot"
[[ -x "$BIN" || -x "$BIN.exe" ]] || { echo "error: $BIN not built" >&2; exit 1; }
TMP="$(mktemp -d)"
# ui_shot keeps the preferences it changes (language, theme, zoom): put the defaults back at the end.
restore() {
  {
    echo '{"empty": true}'
    cmd ui.language '{"value":"auto"}'
    cmd ui.theme '{"value":"system"}'
    cmd view.zoom '{"value":100}'
  } > "$TMP/restore.jsonl"
  "$BIN" "$TMP/restore.jsonl" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap restore EXIT

# An absolute path ui_shot can write to (native form on Windows).
native() { if command -v cygpath >/dev/null; then cygpath -m "$1"; else echo "$1"; fi; }
# Runs a script until both of its pictures exist: the offscreen GPU render sometimes times out.
shoot() {
  local script="$1" light="$2" dark="$3"
  rm -f "$light" "$dark"
  for _ in 1 2 3 4 5 6 7 8; do
    "$BIN" "$script" >"$script.log" 2>&1 || true
    [[ -s "$light" && -s "$dark" ]] && return 0
  done
  echo "error: $script: no picture after 8 tries" >&2
  tail -n 15 "$script.log" >&2
  return 1
}
cmd() { printf '{"method":"engine.execute","params":{"command":"%s","params":%s}}\n' "$1" "$2"; }

# The proofing language of the documents, so the spelling check reads them in their language.
proof() { case "$1" in en) echo en-US ;; it) echo it-IT ;; fr) echo fr-FR ;; de) echo de-DE ;; esac; }
# Every paragraph in the page's language, then the caret back at the start.
proofed() {
  cmd select.all '{}'
  cmd review.language "{\"lang\":\"$(proof "$1")\"}"
  cmd caret.docStart '{}'
}

# A light and a dark picture of the document built so far, on the given ribbon tab.
pictures() {
  local tab="$1" name="$2" out="$3"
  cmd view.zoom '{"value":115}'
  cmd ui.tab "{\"tab\":\"$tab\"}"
  cmd ui.theme '{"value":"light"}'
  # A first picture warms the renderer up: the first one after a long script can time out.
  echo '{"wait": 5000}'
  printf '{"shot":"%s"}\n' "$(native "$TMP/warmup.png")"
  echo '{"steps": 60}'
  printf '{"shot":"%s"}\n' "$(native "$out/shot-$name-light.png")"
  cmd ui.theme '{"value":"dark"}'
  echo '{"steps": 60}'
  printf '{"shot":"%s"}\n' "$(native "$out/shot-$name-dark.png")"
}
# Calculations one per paragraph, each shown as a display equation.
calcs() {
  local first=1 s
  for s in "$@"; do
    [[ $first == 1 ]] || cmd text.newParagraph '{}'
    first=0
    cmd calc.insert "{\"text\":\"$s\",\"display\":true}"
  done
}
# The caret at the end of the paragraph after the n-th occurrence of a text: a question's working
# space follows the question (repeated in a problem's solution, or the quiz question itself).
after() {
  cmd select.text "{\"text\":\"$1\",\"occurrence\":$2}"
  cmd select.collapse '{"toEnd":true}'
  cmd caret.paraDown '{}'
  cmd caret.end '{}'
}

# lang | heading | lead | exercise title | statement | question a | question b | speed | time
#      | quiz title | choice question | its four choices (;) | true/false question | calculation question
#      | acceleration
while IFS='|' read -r lang heading lead extitle extext qa qb dv dt qtitle qc choices qtf qcalc da; do
  [[ "$ONLY" == "  " || "$ONLY" == *" $lang "* ]] || continue
  out="$ROOT/site/shots/$lang"
  mkdir -p "$out"

  calc="$TMP/calc-$lang.jsonl"
  {
    echo '{"empty": true}'
    cmd ui.language "{\"value\":\"$lang\"}"
    cmd text.insert "{\"text\":\"$heading\"}"
    cmd para.style '{"style":"Heading1"}'
    cmd text.newParagraph '{}'
    cmd para.style '{"style":"Normal"}'
    cmd text.insert "{\"text\":\"$lead\"}"
    cmd text.newParagraph '{}'
    cmd calc.steps '{"value":true}'
    calcs 'm = 1200 kg' 'v_0 = 0 km/h' 'v = 90 km/h' 't = 10 s' 'a = (v - v_0)/t =' 'F = m·a =' \
      's = a·t^2/2 =' 's ≤ 150 m' 'E_k = m·v^2/2 = kJ'
    cmd calc.recalculate '{}'
    proofed "$lang"
    pictures Math calc "$out"
  } > "$calc"
  shoot "$calc" "$out/shot-calc-light.png" "$out/shot-calc-dark.png"

  ex="$TMP/exercise-$lang.jsonl"
  {
    # A narrower window: the picture sits in half the page's width on the site.
    echo '{"empty": true, "width": 1100, "height": 960}'
    cmd ui.language "{\"value\":\"$lang\"}"
    cmd exercise.insert "{\"title\":\"$extitle\",\"text\":\"$extext\",\"questions\":[\"$qa\",\"$qb\"]}"
    cmd exercise.setData "{\"data\":{\"items\":[{\"symbol\":\"v\",\"description\":\"$dv\",\"unit\":\"km/h\",\"values\":[\"90\",\"72\"]},{\"symbol\":\"t\",\"description\":\"$dt\",\"unit\":\"s\",\"values\":[\"10\",\"8\"]}],\"variants\":[\"A\",\"B\"],\"active\":0}}"
    after "$qa" 2
    calcs 'a = v/t ='
    cmd equation.answer '{"question":1}'
    after "$qb" 2
    calcs 's = a·t^2/2 ='
    cmd equation.answer '{"question":2}'
    cmd calc.recalculate '{}'
    proofed "$lang"
    pictures Teaching exercise "$out"
  } > "$ex"
  shoot "$ex" "$out/shot-exercise-light.png" "$out/shot-exercise-dark.png"

  quiz="$TMP/quiz-$lang.jsonl"
  {
    echo '{"empty": true, "width": 1100, "height": 960}'
    cmd ui.language "{\"value\":\"$lang\"}"
    IFS=';' read -r c1 c2 c3 c4 <<<"$choices"
    cmd exercise.insert "{\"kind\":\"choice\",\"title\":\"$qtitle\",\"questions\":[{\"text\":\"$qc\",\"choices\":[\"$c1\",\"$c2\",\"$c3\",\"$c4\"],\"correct\":[\"A\",\"C\"],\"points\":1},{\"text\":\"$qtf\",\"type\":\"trueFalse\",\"correct\":1,\"points\":1},{\"text\":\"$qcalc\",\"type\":\"calc\",\"points\":2,\"height\":2.5}]}"
    # The calculation question's data: the working is one equation, the result line under it.
    cmd exercise.setData "{\"data\":{\"items\":[{\"symbol\":\"a\",\"description\":\"$da\",\"unit\":\"m/s²\",\"values\":[\"2.5\"]},{\"symbol\":\"t\",\"description\":\"$dt\",\"unit\":\"s\",\"values\":[\"4\"]}],\"variants\":[\"A\"],\"active\":0}}"
    after "$qcalc" 1
    calcs 's = a·t^2/2 ='
    cmd calc.recalculate '{}'
    proofed "$lang"
    pictures Teaching quiz "$out"
  } > "$quiz"
  shoot "$quiz" "$out/shot-quiz-light.png" "$out/shot-quiz-dark.png"
  echo "site/shots/$lang: done"
done <<'EOF'
en|Uniformly accelerated motion|A car starts from rest and reaches 90 km/h in 10 s: its acceleration, the force, the distance and the energy.|Car accelerating from rest|A car starts from rest and reaches the speed v in the time t, with a constant acceleration.|Find the acceleration.|Find the distance travelled.|final speed|time|Motion and units|Which of these are base units of the SI?|kilogram (kg);newton (N);second (s);kilometre per hour (km/h)|Acceleration is measured in m/s².|A cart starts from rest with the acceleration a. How far does it travel in the time t?|acceleration
it|Moto uniformemente accelerato|Un’automobile parte da ferma e raggiunge 90 km/h in 10 s: l’accelerazione, la forza, lo spazio percorso e l’energia.|Automobile che parte da ferma|Un’automobile parte da ferma e raggiunge la velocità v nel tempo t, con accelerazione costante.|Calcola l’accelerazione.|Calcola lo spazio percorso.|velocità finale|tempo|Moto e unità di misura|Quali di queste sono unità fondamentali del SI?|chilogrammo (kg);newton (N);secondo (s);chilometro orario (km/h)|L’accelerazione si misura in m/s².|Un carrello parte da fermo con accelerazione a. Quanto spazio percorre nel tempo t?|accelerazione
fr|Mouvement uniformément accéléré|Une voiture démarre à l’arrêt et atteint 90 km/h en 10 s : son accélération, la force, la distance et l’énergie.|Voiture qui démarre à l’arrêt|Une voiture démarre à l’arrêt et atteint la vitesse v au bout du temps t, avec une accélération constante.|Calculer l’accélération.|Calculer la distance parcourue.|vitesse finale|durée|Mouvement et unités|Lesquelles sont des unités de base du SI ?|kilogramme (kg);newton (N);seconde (s);kilomètre par heure (km/h)|L’accélération se mesure en m/s².|Un chariot démarre à l’arrêt avec l’accélération a. Quelle distance parcourt-il pendant la durée t ?|accélération
de|Gleichmäßig beschleunigte Bewegung|Ein Auto fährt aus dem Stand an und erreicht 90 km/h in 10 s: Beschleunigung, Kraft, Weg und Energie.|Auto, das aus dem Stand anfährt|Ein Auto fährt aus dem Stand an und erreicht die Geschwindigkeit v in der Zeit t, mit konstanter Beschleunigung.|Berechne die Beschleunigung.|Berechne den zurückgelegten Weg.|Endgeschwindigkeit|Zeit|Bewegung und Einheiten|Welche davon sind SI-Basiseinheiten?|Kilogramm (kg);Newton (N);Sekunde (s);Kilometer pro Stunde (km/h)|Die Beschleunigung wird in m/s² gemessen.|Ein Wagen fährt aus dem Stand mit der Beschleunigung a an. Welchen Weg legt er in der Zeit t zurück?|Beschleunigung
EOF
