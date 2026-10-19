# Fonts

Folitecta draws generated text (text boxes, typewriter text and callouts) in the standard
Helvetica font when every character of the text is in the Western European (WinAnsi) character
set. Text with other characters (Polish, Czech, Greek, Cyrillic, symbols, …) is drawn with a
subset of **FreeSans**, embedded in the document: only the glyphs the text uses travel with it,
together with a map from each glyph back to its character, so the text can be copied and
searched. The font is a separate data file read at run time; the release packages carry it in a
`fonts` folder next to the program.

| File | What it is | SHA-256 |
|---|---|---|
| `FreeSans.ttf` | GNU FreeFont FreeSans, release 20120503, TrueType | `c80858440d8fb618e0ac5ff6f16251dbfa6b3316f00f3cdd17d477297dd87b04` |
| `COPYING` | The GNU General Public License, version 3 | `8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903` |
| `README` | The README of the release, with the font exception | `d7375e3982a2e0afc471f8900c46ac6ea41b1112de1f5cdb9ca5c820adf25a33` |
| `AUTHORS` | The authors of GNU FreeFont | `1baa0cba3f6004ac1277a25c79ab5efb45011102f9ef2ab3a5b7d262088a1f5a` |
| `CREDITS` | The credits of GNU FreeFont | `da8e9145591c82a4e1aa2aeb5bfccd18d61ca3c62efb2e403598c85dbafbe8f5` |

## Origin

The files are unchanged copies of the ones in `freefont-ttf-20120503.zip`, downloaded from the
GNU project, https://ftp.gnu.org/gnu/freefont/. The signature of the archive
(`freefont-ttf-20120503.zip.sig`) was checked with the GNU keyring
(https://ftp.gnu.org/gnu/gnu-keyring.gpg): a good signature by the key
`A015 6C13 9D2D AA3B 352E 42CD 5063 61DB A36F DD52` (Steve White, the maintainer of GNU FreeFont).

| Archive | SHA-256 |
|---|---|
| `freefont-ttf-20120503.zip` | `7c85baf1bf82a1a1845d1322112bc6ca982221b484e3b3925022e25b5cae89af` |
| `freefont-ttf-20120503.zip.sig` | `a620ccd38d7af4058f145c3ae6b94e9bd9cf38eb6670e0cb7a5927e72d744347` |
| `freefont-src-20120503.tar.gz` | `0d62e2e22655f841c3ac5fb8c8059a52fe2c09c695d9e8eda9903406e942657a` |
| `freefont-src-20120503.tar.gz.sig` | `ab373444d4711b8daec8ca8dbef45feec7d552ee2373efe025b9b870811f1e13` |

## Licence

GNU FreeFont is free software: it can be redistributed and modified under the terms of the GNU
General Public License as published by the Free Software Foundation, either version 3 of the
License or (at your option) any later version (`COPYING`), with this exception, quoted from
`README`:

> As a special exception, if you create a document which uses this font, and embed this font or
> unaltered portions of this font into the document, this font does not by itself cause the
> resulting document to be covered by the GNU General Public License. This exception does not
> however invalidate any other reasons why the document might be covered by the GNU General
> Public License. If you modify this font, you may extend this exception to your version of the
> font, but you are not obligated to do so. If you do not wish to do so, delete this exception
> statement from your version.

The font is a data file next to the program, never built into it. The portion of the font that
Folitecta embeds in a document keeps the outlines, the hinting instructions and the advance widths
of the glyphs the text uses byte for byte, and the `name` (copyright and licence) and `OS/2` tables
whole. Only the tables that index the glyphs are written again for the smaller set: `loca` (where
each glyph starts), `head` (the format of `loca` and the checksum), `hhea` and `maxp` (the number of
glyphs), `post` (version 3, without glyph names) and a small `cmap` for the characters drawn; the
records of `hmtx` are the original ones. The glyph numbers stay those of FreeSans.

## Source

The source of the fonts (the FontForge files they are built from) is the signed archive in
[`source/`](source): `freefont-src-20120503.tar.gz` with its signature
`freefont-src-20120503.tar.gz.sig`, unchanged copies of the files of the same name at
https://ftp.gnu.org/gnu/freefont/. It is part of the source code of every Folitecta release (the
`fonts/source` folder of the repository and of the source archives of each release), and is not
copied into the program packages.
