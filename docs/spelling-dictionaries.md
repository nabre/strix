<!-- Modified by the Strix contributors (fork of WordCraft); see NOTICE. -->
# Installing languages and spelling dictionaries

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-11-10 · **Change:** trivial (European Portuguese reads and downloads `pt_PT`; previously minor: WordCraft 0.17.0 merged: proofing is craft-proof, and Language packs install a language's spelling and hyphenation from File › Options › Language; without a Dictionaries folder of your own, the folder Strix downloads dictionaries to is searched first; previously minor: Download from Strix: the status bar's language menu and File › Options › Language download the missing dictionary, `proof.downloadDictionary`; previously minor: ISO 8859-9 Turkish dictionaries are read, #3692)

English spelling works out of the box. Polish, Russian, Portuguese, Czech, Italian, Croatian,
Turkish, Indonesian and Spanish spelling use a standard **Hunspell** dictionary (an `.aff` and a `.dic` file) that
Strix reads at runtime. These dictionaries are not bundled: they are large, and several are
GPL or LGPL licensed. Without one, the language isn't checked. The status bar then shows the
language with **(no dictionary)**, for example *Português (Brasil) (no dictionary)*.

If a dictionary is installed but can't be read (an unsupported `SET` encoding, a file over the
size limit, a parse error), the language isn't checked either, and the status bar shows
**(dictionary unreadable)** instead. Its tooltip gives the `.dic` file and the reason, and
**File › Options › Language** (Dictionary files) lists the same reason next to the file.

The easiest way is to let Strix download it (next section). Installed by hand, a dictionary is
read after a restart.

## Download from Strix

Click the language on the status bar while it says **(no dictionary)**: the menu starts with
**Download the `it_IT` spelling dictionary** (the name is the language's). **File › Options ›
Language** shows the same button under *Proofing language*. Its tooltip names the source and the
licence. Strix downloads the `.aff` and `.dic` from the
[LibreOffice dictionaries repository](https://github.com/LibreOffice/dictionaries), at a fixed
revision, into your own dictionaries folder, and checks spelling with it at once: no restart.
The status bar says when it is done, or why it failed (offline, a proxy…).

| System | Downloads go to |
|---|---|
| Windows | `%LOCALAPPDATA%\Strix\dictionaries` |
| macOS | `~/Library/Application Support/Strix/dictionaries` |
| Linux and BSD | `$XDG_DATA_HOME/strix/dictionaries` (usually `~/.local/share/strix/dictionaries`) |
| Portable | `StrixData/dictionaries` beside the program |

Each download leaves a `<name>.source.txt` beside the files with where it came from and its
licence. Your computer fetches the files from their publisher: Strix doesn't redistribute them.
Latin American Spanish reads the Mexican dictionary (`es_MX`), which LibreOffice publishes; the
web version can't download dictionaries.

From the command line or an agent (it waits for the download):

```sh
strix-cli run --cmd 'proof.downloadDictionary={"lang":"it-IT"}' --print
```

| Dictionary | Licence (as its publisher gives it) |
|---|---|
| `pl_PL` | GPL-2.0, LGPL-2.1, MPL-1.1, Apache-2.0 or CC-BY-4.0 |
| `pt_BR` | LGPL-3.0 or MPL |
| `pt_PT` | GPL-2.0, LGPL-2.1 or MPL-1.1 |
| `it_IT` | GPL-3.0 |
| `hr_HR` | GPL-2.0, LGPL-2.1 or MPL-1.1 |
| `tr_TR` | MPL-2.0 |
| `ru_RU` | BSD-style |
| `cs_CZ` | GPL-2.0 |
| `id_ID` | LGPL-3.0 |
| `es_ES`, `es_MX`, `es_UY` | GPL-3.0+, LGPL-3.0+ or MPL-1.1+ |

## Installing by hand

## Language packs

The easiest way to proof another language is its **Language pack**: the language's spelling
dictionary, hyphenation patterns and licence texts, installed and removed together. English is
built in; every other language is a pack.

> The current set of Language packs (language-packs-2) has 75 languages, among them Dutch,
> German, Danish, Swedish, Norwegian, Hungarian, Estonian, Croatian, Persian and Esperanto.
> Mongolian isn't a pack yet (its suggestions are too slow), nor is Thai (WordCraft can't yet
> split Thai text, which has no spaces, into words). Basque,
> Ukrainian and Uyghur aren't packs: their data's licence doesn't allow it, or isn't clear.

- **File › Options › Language › Your languages** lists English, the languages of the open
  document and every installed pack, each with where its proofing comes from (*Built in*,
  *Installed*, *Your own dictionary*, *System dictionary*, *Grammar built in*, *Not available yet*).
  **Install** gets a document language's pack; **Remove** takes an installed one out; **Set as
  default** makes it the language of new blank documents (the open document is left as it is).
  **Add a language…** opens a list of the other packs: type *dutch*, *nederlands* or *nl* to
  search, see each pack's size, licence and a link to where it comes from, and click **Install**.
- When a document has text in a language whose pack isn't installed, a bar under the ribbon says
  *Dutch proofing isn't installed.* with an **Install** button showing the download size. ✕ hides
  it for that document. Once WordCraft has the list of packs, the bar shows only for a language
  that has a pack. Before the list has been fetched (a fresh install, or the list of an older
  WordCraft), it can't know, so it shows for a language that has no dictionary, with **Install**
  alone: clicking it gets the list, then the pack if there is one (if not, the bar goes). With
  downloading on, WordCraft fetches a missing or out-of-date list by itself once. Choosing a
  language with nothing selected counts too.
- Installing needs **downloading** on: the same single setting as WordCraft's font downloads
  (the font store). Clicking Add a language…, Install or Refresh turns it on; nothing is
  downloaded before. Each pack is checked against the sha256 in WordCraft's pinned list before it
  is used.
- Text in that language is checked and hyphenated as soon as the pack is installed: no restart.
- **Help › About › Language data** shows the full licence text of the built-in English data and
  of every installed pack.
- Packs live in the font store's folder, under `language-packs/<language>/`, so every Craft app
  uses the same ones.
- Agents and scripts use the commands `proof.packs`, `proof.installPack {"locale": "nl"}` and
  `proof.removePack {"locale": "nl"}` (see [MCP](mcp.md#language-packs)). They install only while
  downloading is on; only you can turn it on, with your own click in the app.
- The web version has no Language packs; it proofs English.

A dictionary you install yourself (below) still wins: WordCraft looks in the dictionaries folder,
then the installed pack, then the system's dictionaries.

## Which files each language needs

Strix looks for the first of these file names (each name means a `<name>.aff` and
`<name>.dic` pair in the same folder):

| Language | File names | Common source |
|---|---|---|
| Polish | `pl_PL`, `pl-PL`, `pl` | SJP.PL |
| Russian | `ru_RU`, `ru-RU`, `ru` | LibreOffice `dict-ru` |
| Portuguese (Brazil) | `pt_BR`, `pt-BR`, `pt` | VERO (LibreOffice `dict-pt-BR`) |
| Portuguese (Portugal) | `pt_PT`, `pt-PT`, `pt` | LibreOffice `dict-pt-PT` (Natura, Universidade do Minho) |
| Czech | `cs_CZ`, `cs-CZ`, `cs` | LibreOffice `dict-cs` |
| Italian | `it_IT`, `it_CH`, `it-IT`, `it` | LibreOffice `dict-it` |
| Croatian | `hr_HR`, `hr-HR`, `hr` | LibreOffice `dict-hr` |
| Turkish | `tr_TR`, `tr-TR`, `tr` | LibreOffice `dict-tr` |
| Indonesian | `id_ID`, `id-ID`, `id`, `in_ID` | LibreOffice `dict-id` |
| Spanish (Spain) | `es_ES`, `es-ES`, `es_ANY`, `es` | LibreOffice `dict-es` (RLA-ES) |
| Spanish (Mexico) | `es_MX`, `es-MX`, `es_419`, `es-419`, `es_ANY`, `es` | LibreOffice `dict-es` (RLA-ES) |
| Spanish (Latin America, and the other Spanish regions of the Americas) | `es_419`, `es-419`, `es_ANY`, `es`, `es_US`, `es_MX`, `es_AR`, `es-AR` | LibreOffice `dict-es` (RLA-ES) |
| Spanish (Uruguay) | `es_UY`, `es-UY`, `es_AR`, `es-AR`, `es_419`, `es-419`, `es_ANY`, `es` | LibreOffice `dict-es` (RLA-ES) |

European Portuguese spells differently: text marked *Português (Portugal)* uses `pt_PT` (or a
`pt-pt` Language pack), and Brazilian Portuguese and the other regions `pt_BR`.

Spanish has a dictionary per variety: text marked *Español (España)* uses `es_ES`, *Español
(México)* `es_MX`, *Español (Uruguay)* `es_UY`, and *Español (Latinoamérica)* and the other
Spanish regions of the Americas (Argentina, Colombia, the United States…) the first Latin
American one found. LibreOffice's `dict-es` has them all. Uruguayan Spanish also accepts the
`vos` forms of the dictionary's verbs (`tenés`, `vení`, `decime`, `decímelo`).

The files may be UTF-8 or the encoding named on the `.aff`'s `SET` line (ISO 8859-1, ISO 8859-2,
ISO 8859-9, KOI8-R and Windows-1251 are read as well; the original VERO `pt_BR` is ISO 8859-1, older
Turkish `tr_TR` dictionaries ISO 8859-9).

## Where Strix looks

The first folder holding a matching pair wins:

1. The folder set in **File › Options › Language › Dictionaries folder**. This is the easiest
   way on every system, and it works for an app started from the Start menu, Finder or the Dock.
   Your languages marks each language it supplies *Your own dictionary*, and the **Dictionary
   files** list under it shows the file each language found. Without a folder of your own, the
   folder Strix downloads dictionaries to (above) is searched here instead.
2. The folder in the `WORDCRAFT_DICTIONARIES` environment variable, and WordCraft's own
   `dictionaries` folder (the first row of each system below; proofing is WordCraft's
   `craft-proof`, so a dictionary an installed WordCraft uses is found too).
3. An installed [Language pack](#language-packs) for the language.
4. The system's usual places:

| System | Folders |
|---|---|
| Windows | `%LOCALAPPDATA%\WordCraft\dictionaries`, `%LOCALAPPDATA%\hunspell`, `%APPDATA%\WordCraft\dictionaries`, `%APPDATA%\hunspell`, LibreOffice's `share\extensions\dict-*` under `Program Files` (on any drive) |
| macOS | `~/Library/Application Support/WordCraft/dictionaries`, `~/Library/Spelling`, `/Library/Spelling`, `/opt/homebrew/share/hunspell`, `/usr/local/share/hunspell`, `/Applications/LibreOffice.app/Contents/Resources/extensions/dict-*` |
| Linux and BSD | `$XDG_DATA_HOME/wordcraft/dictionaries` (default `~/.local/share/wordcraft/dictionaries`), `~/.local/share/hunspell`, `/usr/share/hunspell`, `/usr/share/myspell`, `/usr/share/myspell/dicts`, `/usr/local/share/hunspell`, `/usr/local/share/myspell`, LibreOffice's `share/extensions/dict-*` under `/usr/lib/libreoffice` or `/opt/libreoffice` |

The Flatpak runs in a sandbox: its `$XDG_DATA_HOME` is
`~/.var/app/io.github.nabre.strix/data`, so its own folder is
`~/.var/app/io.github.nabre.strix/data/wordcraft/dictionaries`, and it can't see the host's
`/usr/share/hunspell` or `~/.local/share/hunspell` (the paths above are for the other builds).
Put dictionaries in that folder.

The web version can't read these folders, so these languages aren't spell-checked there.

## Example: Brazilian Portuguese (`pt_BR`)

VERO, the Brazilian Portuguese dictionary LibreOffice ships, is available from the
[LibreOffice dictionaries repository](https://github.com/LibreOffice/dictionaries/tree/master/pt_BR)
(LGPLv3 / MPL). Any of these ways works.

### If LibreOffice is installed

Nothing to do: Strix finds LibreOffice's `dict-pt-BR` extension on its own. Restart
Strix after installing LibreOffice.

### Windows

In PowerShell:

```powershell
$d = "$env:LOCALAPPDATA\Strix\dictionaries"
New-Item -ItemType Directory -Force $d | Out-Null
$base = "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/pt_BR"
Invoke-WebRequest "$base/pt_BR.aff" -OutFile "$d\pt_BR.aff" -UseBasicParsing
Invoke-WebRequest "$base/pt_BR.dic" -OutFile "$d\pt_BR.dic" -UseBasicParsing
```

### macOS

```sh
mkdir -p ~/Library/Spelling
cd ~/Library/Spelling
curl -fLO https://raw.githubusercontent.com/LibreOffice/dictionaries/master/pt_BR/pt_BR.aff
curl -fLO https://raw.githubusercontent.com/LibreOffice/dictionaries/master/pt_BR/pt_BR.dic
```

### Linux

From the distribution's packages:

```sh
sudo apt install hunspell-pt-br   # Debian, Ubuntu
```

Or, without root, into your own folder:

```sh
mkdir -p ~/.local/share/hunspell
cd ~/.local/share/hunspell
curl -fLO https://raw.githubusercontent.com/LibreOffice/dictionaries/master/pt_BR/pt_BR.aff
curl -fLO https://raw.githubusercontent.com/LibreOffice/dictionaries/master/pt_BR/pt_BR.dic
```

Other languages work the same way: replace `pt_BR` with the language's folder and file names
from the LibreOffice repository (for example `it_IT/it_IT.aff`; the Spanish ones are all in
`es/`, for example `es/es_MX.aff`).

## Checking that it works

- Restart Strix. The status bar no longer says **(no dictionary)**, and a misspelled word
  such as `impramir` is underlined, with `imprimir` among its suggestions.
- **File › Options › Language** (Dictionary files) lists the file each language found.
- From the command line:

  ```sh
  strix-cli run --cmd 'text.insert={"text":"Vou impramir o documento."}' --cmd select.all --cmd 'review.language={"lang":"pt-BR"}' --cmd review.issues --print
  ```

  The `review.issues` line lists `impramir` as a spelling issue with its suggestions.
- The log file (`%APPDATA%\Strix\logs\wordcraft.log` on Windows) says
  `Portuguese spelling: no Hunspell pt_BR dictionary installed` when none was found.

Never commit dictionary files to this repository: they stay on the user's system.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-10 | trivial | European Portuguese has its own dictionary, `pt_PT`, which Strix downloads too (craft-proof reads it for *Português (Portugal)*; before, `pt_BR` served every region) |
| 2026-11-10 | minor | WordCraft 0.17.0 merged into Strix: proofing is craft-proof; Strix's downloaded dictionaries are searched as the Dictionaries folder when you haven't set one, and downloads go to whichever folder that is |
| 2026-11-03 | trivial | Language packs (CP4.3): the missing-pack bar offers a language before the list is fetched and gets the list, then the pack, on Install; a missing or stale list is fetched once while downloading is on; the caret's typing language counts |
| 2026-11-03 | trivial | Language packs: install from Your languages or Add a language… in Options › Language; the Dictionaries folder is in Options › Language, not Proofing |
| 2026-11-03 | trivial | Language packs: language-packs-2 is published and pinned (75 languages, adding the compounding languages); Mongolian and Thai aren't packs yet, and why |
| 2026-11-02 | trivial | Language packs: language-packs-1 is published and pinned (61 languages); which languages aren't packs yet and why (CP4.4) |
| 2026-11-02 | trivial | The Flatpak's own dictionaries folder (`~/.var/app/io.github.nabre.strix/data/wordcraft/dictionaries`); the host's folders are outside its sandbox |
| 2026-11-01 | minor | Language packs (CP4.3): installing a language from File › Options › Language or the missing-pack bar, the download consent, About › Language data, the commands; an installed pack is searched after the dictionaries folders and before the system's |
| 2026-10-22 | minor | Spanish (#3360, #3553): the file names for Spain, Mexico, Latin America and Uruguay and their fallbacks |
| 2026-10-22 | trivial | Italian, Croatian, Turkish, Russian, Czech and Indonesian without a dictionary now show **(no dictionary)** (or **(dictionary unreadable)**) too, as this page already said; before, only Polish and Portuguese did. Indonesian is listed with its file names (#3150) |
| 2026-11-01 | trivial | WordCraft's own `dictionaries` folder under the user's application data is searched on macOS (`~/Library/Application Support/WordCraft/dictionaries`) and Linux/BSD (`$XDG_DATA_HOME/wordcraft/dictionaries`) too, before the system folders, as it was on Windows (craft-proof; ported from storytold/craft-libs#31 by brzuchal) |
| 2026-10-23 | trivial | ISO 8859-9 (`SET ISO8859-9`) dictionaries are read, so an older Turkish `tr_TR` loads without converting it to UTF-8; the Turkish dictionary is read through the same size-capped reader and `SET` decoding as the others (#3692) |
| 2026-10-18 | trivial | ISO 8859-1 (`SET ISO8859-1`) dictionaries are read, so the original VERO `pt_BR` works without converting it to UTF-8 (#2824) |
| 2026-10-18 | minor | An installed dictionary that can't be read shows **(dictionary unreadable)** in the status bar, with the file and reason in its tooltip and in File › Options › Proofing (#2830) |
| 2026-10-16 | major | First version: the file names per language, the search order and folders on Windows, macOS and Linux, installing `pt_BR` on each, and how to check it works |
| 2026-11-08 | minor | Download from Strix: the status bar's language menu and File › Options › Language download a language's missing dictionary from LibreOffice's repository (fixed revision) into the user's dictionaries folder, read at once; `proof.downloadDictionary`; the licences |
