# Spelling dictionaries

Folitecta checks spelling with Hunspell dictionaries: a pair of files `<code>.aff` (the rules)
and `<code>.dic` (the words) for each language. They are separate data files read when the
spell check starts; they are not part of the program and keep their own licences.

| Files | Language | Source | Licence |
|---|---|---|---|
| `en_US.aff`, `en_US.dic` | English (United States) | en_US Hunspell dictionary from SCOWL, version 2020.12.07 (http://wordlist.sourceforge.net), as packaged by the `dictionary-en` 4.0.0 package | MIT-like SCOWL licence and BSD (see [LICENSE-en_US.txt](LICENSE-en_US.txt)) |
| `it_IT.aff`, `it_IT.dic` | Italian | "Dizionario italiano" 4.2 of the Italian Writing Aids extension 5.0 by Andrea Pescetti and earlier authors (http://extensions.openoffice.org/project/dict-it), as packaged by the `dictionary-it` 2.0.0 package | GNU GPL version 3 (see [LICENSE-it_IT.txt](LICENSE-it_IT.txt)) |

The files are unchanged copies of the originals.

## More languages

Any other Hunspell dictionary works too (for example the ones of LibreOffice). Put its two files,
named with the language code (`de_DE.aff` and `de_DE.dic`), in one of these folders:

- the `dictionaries` folder of your Folitecta settings (`%APPDATA%\Folitecta\dictionaries` on
  Windows, `~/Library/Application Support/Folitecta/dictionaries` on macOS,
  `~/.config/Folitecta/dictionaries` on Linux);
- the `dictionaries` folder next to the program.

On Linux the dictionaries installed for the whole system (`/usr/share/hunspell`) are found as
well. The languages to check are chosen in the Spell Check and Replace dialog.
