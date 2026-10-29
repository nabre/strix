#!/usr/bin/env bash
# The site's screenshots: real windows of Strix, rendered headless by the ui_shot example
# (crates/ui-egui/examples/ui_shot.rs), in each language of the site and in both themes:
#   site/shots/<lang>/shot-calc-{light,dark}.png      a beam calculation on the Math tab
#   site/shots/<lang>/shot-exercise-{light,dark}.png  an exercise with its solution
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
  cargo build --release -q -p strix-ui-egui --example ui_shot
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

# lang | heading | lead | exercise title | statement | question 1 | question 2
while IFS='|' read -r lang heading lead extitle extext q1 q2; do
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
    # One calculation per paragraph, each shown as a display equation.
    for s in 'L = 4.5 m' 'q = 12 kN/m' 'M = q·L^2/8 =' 'W = 1000 cm^3' 'f_yd = 355 MPa' 'σ = M/W =' 'σ ≤ f_yd'; do
      cmd calc.insert "{\"text\":\"$s\"}"
      cmd text.newParagraph '{}'
    done
    cmd calc.recalculate '{}'
    proofed "$lang"
    cmd view.zoom '{"value":115}'
    cmd ui.tab '{"tab":"Math"}'
    cmd ui.theme '{"value":"light"}'
    # A first picture warms the renderer up: the first one after a long script can time out.
    echo '{"wait": 5000}'
    printf '{"shot":"%s"}
' "$(native "$TMP/warmup.png")"
    echo '{"steps": 60}'
    printf '{"shot":"%s"}\n' "$(native "$out/shot-calc-light.png")"
    cmd ui.theme '{"value":"dark"}'
    echo '{"steps": 60}'
    printf '{"shot":"%s"}\n' "$(native "$out/shot-calc-dark.png")"
  } > "$calc"
  shoot "$calc" "$out/shot-calc-light.png" "$out/shot-calc-dark.png"

  ex="$TMP/exercise-$lang.jsonl"
  {
    # A narrower window and a closer zoom: the picture sits in half the page's width.
    echo '{"empty": true, "width": 1100, "height": 960}'
    cmd ui.language "{\"value\":\"$lang\"}"
    cmd view.zoom '{"value":150}'
    cmd exercise.insert "{\"title\":\"$extitle\",\"text\":\"$extext\",\"questions\":[\"$q1\",\"$q2\"]}"
    cmd calc.recalculate '{}'
    proofed "$lang"
    cmd view.zoom '{"value":115}'
    cmd ui.tab '{"tab":"Math"}'
    cmd ui.theme '{"value":"light"}'
    # A first picture warms the renderer up: the first one after a long script can time out.
    echo '{"wait": 5000}'
    printf '{"shot":"%s"}
' "$(native "$TMP/warmup.png")"
    echo '{"steps": 60}'
    printf '{"shot":"%s"}\n' "$(native "$out/shot-exercise-light.png")"
    cmd ui.theme '{"value":"dark"}'
    echo '{"steps": 60}'
    printf '{"shot":"%s"}\n' "$(native "$out/shot-exercise-dark.png")"
  } > "$ex"
  shoot "$ex" "$out/shot-exercise-light.png" "$out/shot-exercise-dark.png"
  echo "site/shots/$lang: done"
done <<'EOF'
en|Simply supported beam|Span and uniform load, then the bending moment and the stress check.|Beam under a uniform load|A simply supported beam carries a uniform load q over its whole span L.|Find the maximum bending moment.|Check the stress against the design strength.
it|Trave appoggiata|Luce e carico uniforme, poi il momento flettente e la verifica della tensione.|Trave con carico uniforme|Una trave appoggiata porta un carico uniforme q su tutta la luce L.|Calcola il momento flettente massimo.|Verifica la tensione rispetto alla resistenza di progetto.
fr|Poutre sur deux appuis|Portée et charge uniforme, puis le moment fléchissant et la vérification de contrainte.|Poutre sous charge uniforme|Une poutre sur deux appuis porte une charge uniforme q sur toute sa portée L.|Calculer le moment fléchissant maximal.|Vérifier la contrainte par rapport à la résistance de calcul.
de|Einfeldträger|Spannweite und Gleichlast, dann das Biegemoment und der Spannungsnachweis.|Träger unter Gleichlast|Ein Einfeldträger trägt eine Gleichlast q über die ganze Spannweite L.|Ermittle das größte Biegemoment.|Weise die Spannung gegen die Bemessungsfestigkeit nach.
EOF
