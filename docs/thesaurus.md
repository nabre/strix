# Thesaurus

> **Last reviewed:** 2026-11-14 · **Last updated:** 2026-11-14 · **Change:** major (LO1: thesauri downloaded on demand from LibreOffice's dictionaries repository, read by Strix's own MyThes reader; the Thesaurus pane) · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4

**Review › Proofing › Thesaurus** (Shift+F7) opens the **Thesaurus** pane: the word at the
insertion point, its synonyms grouped by meaning (*noun*, *s.f.*, *agg.*… as the thesaurus names
them). Click a synonym to look it up; **Insert** puts it in place of the word at the insertion
point. Type another word in the box at the top and press Enter to look it up. The right-click
menu's **Synonyms** offers the same synonyms.

The synonyms come from the thesaurus of the word's language: the language of the text at the
insertion point (Review › Language), else the document's. A word the thesaurus doesn't list as
written is looked up under its base form when the language has simple regular endings (*case* →
*casa*, *houses* → *house*, *l'acqua* → *acqua*); the pane then says which entry it shows. Notes
some thesauri put around a synonym (*(generic term)*, *(potocznie)*) are left out.

Without a thesaurus for the language, Strix uses its own short built-in list of common English
words.

## Downloading a thesaurus

Thesauri aren't bundled: they are large, and most are GPL or LGPL. When the language at the
insertion point has a thesaurus Strix can download and none is installed:

- the Thesaurus pane shows **Download the `it_IT` thesaurus**;
- the status bar's language menu and **File › Options › Language** show it next to the spelling
  dictionary's download.

The tooltip names the source and the licence. Strix downloads the thesaurus (the `.dat` file of
the MyThes format) and the files of its folder that state its licence and authors from the
[LibreOffice dictionaries repository](https://github.com/LibreOffice/dictionaries), at the same
fixed revision as the spelling dictionaries, into your dictionaries folder (see
[Installing languages and spelling dictionaries](spelling-dictionaries.md#download-from-strix)
for where that is). The pane uses it at once; the status bar says when it is done, or why it
failed. A file that can't be read whole is never kept.

Beside the thesaurus stay `<file>.source.txt` (where it came from and its licence) and the
licence notices (`<file>.README_th_it_IT.txt`…). The thesauri are separate works under their own
licences: your computer fetches them from their publisher, and Strix doesn't redistribute them.
The web version can't download thesauri; it uses the built-in list.

From the command line or an agent (it waits for the download):

```sh
strix-cli run --cmd 'proof.downloadThesaurus={"lang":"it-IT"}' --print
strix-cli run --template sample --cmd 'review.thesaurus={"word":"casa","lang":"it-IT"}' --print
```

| Language | Thesaurus | Licence (as its folder states it) |
|---|---|---|
| English (every variety) | `th_en_US_v2` (WordNet) | WordNet 2.1 licence (Princeton University) |
| Italian | `th_it_IT_v2` | GPL-3.0 |
| Spanish (every variety) | `th_es_v2` | LGPL-2.1 |
| Portuguese (Portugal) | `th_pt_PT` | GPL-2.0+ and CC-BY-3.0 (OpenThesaurus and Onto.PT) |
| Polish | `th_pl_PL_v2` | LGPL-2.1 |
| Russian | `th_ru_RU_M_aot_and_v2` | LGPL-2.1 |
| Indonesian | `th_id_ID_v2` | LGPL-2.1 |
| Czech | `thes_cs_CZ` | GFDL-1.1+ |

Brazilian Portuguese has no thesaurus to download: the repository's folder states no licence for
its thesaurus data.

## Installed thesauri

Strix also reads a thesaurus already on the computer, under the same file name: in your
dictionaries folder first, then in the folders the spelling dictionaries are searched in
(including LibreOffice's `dict-xx` extension folders) and in `/usr/share/mythes` and
`/usr/local/share/mythes` (Linux and BSD packages install them there).

## Reading the files

The reader is Strix's own (`crates/dictionaries/src/mythes.rs`), written from the description of
the format: the first line names the encoding (UTF-8, ISO 8859-x, KOI8-R, Windows code pages),
then each entry is a line `word|n` followed by `n` lines `category|synonym|synonym…`. The file is
read whole into memory with an index of its words, and kept while it doesn't change. Since a
file is untrusted:

- one larger than 32 MB is refused before it is read; a line longer than 64 KB, more than two
  million entries or more than 512 meanings for one word make it unreadable;
- a word declaring more meanings than the file has lines is an error (a truncated file);
- an unknown encoding, text that isn't valid in its encoding, or a web page is an error;
- a few stray lines (a meaning broken over two lines, as in a published file) are skipped; more
  than 64 plus one per hundred entries, and the file isn't a thesaurus;
- a word declaring more meanings than it has, followed by the next entry (`word|count` where a
  meaning is due), ends there: the next entry is read as itself, and a lookup reads only each
  entry's own lines (counted as one stray line).

An installed thesaurus that can't be read leaves the built-in list in use; `review.thesaurus`
answers with `thesaurusError` saying why.

## For agents

`review.thesaurus {"word"?, "lang"?}` answers `{"word", "synonyms", "meanings": [{"category",
"synonyms"}], "entry", "language", "thesaurus", "download"}`: `synonyms` is every meaning's
synonyms in order without repeats (as before, for the context menu), `thesaurus` the downloaded
thesaurus that answered (`null`: the built-in list), `download` the thesaurus
`proof.downloadThesaurus` can fetch for that language when none is installed.
`view.thesaurusPane {"value"?}` shows or hides the pane.
