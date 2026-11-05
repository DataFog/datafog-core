# Browser SQLite distribution

SQLite3 Multiple Ciphers 2.6.1: Copyright (c) 2019–2026 Ulrich Telle, MIT. See SQLITE3MC-LICENSE. Source and constituent cipher implementations: https://github.com/utelle/SQLite3MultipleCiphers/tree/v2.6.1 . SQLite is public domain. The distributed sqlite3.mjs preserves the upstream SQLite and Emscripten copyright and license notices.

The bundled upstream build includes cipher implementations beyond the one admitted by this adapter. Their notices remain applicable even though our supported API selects only authenticated ChaCha20-Poly1305. ChaCha20 sources: https://github.com/utelle/SQLite3MultipleCiphers/tree/v2.6.1/src/chacha20 . Emscripten glue is covered by the MIT and University of Illinois/NCSA licenses: https://emscripten.org/docs/introducing_emscripten/emscripten_license.html . The following files carry the upstream license texts for redistribution.
/*
 * ISC License
 *
 * Copyright (c) 2013-2026
 * Frank Denis <j at pureftpd dot org>
 *
 * Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */
