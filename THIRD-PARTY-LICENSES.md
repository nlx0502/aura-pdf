# Dépendances et licences

Aura PDF est publié sous licence **MIT**. Toutes les dépendances embarquées
sont libres, gratuites et compatibles avec cette licence. Aucune n'est sous
licence copyleft forte (GPL/AGPL) : c'est pourquoi le moteur retenu est
**PDFium** (BSD-3 / Apache-2.0) et non MuPDF (AGPL-3.0).

Les dépendances marquées « jalon N » seront ajoutées au jalon indiqué ; elles
sont listées dès maintenant pour valider le choix de licence.

## Composants embarqués dans l'installateur

| Composant | Rôle | Licence | Source |
|---|---|---|---|
| PDFium (build chromium/8086) | Rendu, texte, recherche, création de PDF | BSD-3-Clause, Apache-2.0 | <https://pdfium.googlesource.com/pdfium/> via <https://github.com/bblanchon/pdfium-binaries> |
| ↳ FreeType | Rendu des polices (dans PDFium) | FreeType License (BSD-like) | `src-tauri/resources/pdfium/licenses/freetype.txt` |
| ↳ libjpeg-turbo | Images JPEG (dans PDFium) | IJG, BSD-3-Clause, zlib | `…/libjpeg_turbo.*` |
| ↳ libpng, zlib | Images PNG, compression | libpng License, zlib License | `…/libpng.txt`, `…/zlib.txt` |
| ↳ OpenJPEG | JPEG 2000 | BSD-2-Clause | `…/libopenjpeg.txt` |
| ↳ Little CMS | Gestion des couleurs | MIT | `…/lcms.txt` |
| ↳ ICU, Abseil, HarfBuzz, AGG, fast_float, simdutf, dragonbox, llvm-libc | Divers | Unicode, Apache-2.0, MIT, BSD, Boost | `…/licenses/*` |
| Geist (police) | Typographie de l'interface | **SIL Open Font License 1.1** | <https://github.com/vercel/geist-font> (paquet `@fontsource/geist-sans`, MIT) |
| Lucide (icônes) | Icônes de l'interface | **ISC** | <https://lucide.dev> (paquet `lucide-react`) |
| WebView2 (bootstrapper) | Moteur d'affichage Windows | Licence Microsoft redistribuable | installé par l'installateur si absent |
| Tesseract 5.5 (lié statiquement) | OCR hors ligne | Apache-2.0 | <https://github.com/tesseract-ocr/tesseract> (compilé par vcpkg) |
| ↳ Leptonica | Traitement d'image pour Tesseract | BSD-2-Clause | <http://leptonica.org> |
| ↳ libpng, zlib, libjpeg-turbo, libtiff, libwebp, giflib, OpenJPEG | Formats d'image | libpng, zlib, IJG/BSD-3, libtiff (BSD), BSD-3, MIT, BSD-2 | via vcpkg |
| ↳ libarchive, xz, zstd, lz4, bzip2, libcurl, OpenSSL | Dépendances de construction de Tesseract (non utilisées par Aura : aucun accès réseau) | BSD-2, 0BSD/domaine public, BSD-3, BSD-2, bzip2, curl (MIT-like), Apache-2.0 | via vcpkg |
| Données `tessdata_fast` fra, eng, deu, nld | Langues de l'OCR | Apache-2.0 | <https://github.com/tesseract-ocr/tessdata_fast> — `src-tauri/resources/tessdata/` |
| Profil ICC sRGB v2 compact | Profil de sortie des PDF/A (archivage) | **CC0 1.0** (domaine public) | <https://github.com/saucecontrol/Compact-ICC-Profiles> — `src-tauri/resources/icc/` |

Les textes complets des licences de PDFium et de ses composants sont copiés
dans `src-tauri/resources/pdfium/licenses/`.

## Bibliothèques Rust (cœur de l'application)

| Crate | Licence |
|---|---|
| tauri, tauri-build, tauri-plugin-dialog, tauri-plugin-opener, tauri-plugin-single-instance | MIT OU Apache-2.0 |
| pdfium-render | MIT OU Apache-2.0 |
| lopdf (annotations, organisation des pages, caviardage, formulaires, chiffrement AES-128 / AES-256) | MIT |
| windows (API Windows : impression, magasin de certificats et CNG) | MIT OU Apache-2.0 |
| security-framework (macOS : trousseau et signature) | MIT OU Apache-2.0 |
| regex (recherche des données à caviarder) | MIT OU Apache-2.0 |
| ttf-parser (polices installées : mesure, sous-ensembles) | MIT OU Apache-2.0 |
| RustCrypto : cms, x509-cert, der, spki, rsa, p256, p384, ecdsa, sha1, sha2, signature, const-oid (signature numérique) | MIT OU Apache-2.0 |
| p12-keystore (certificats PKCS #12) | MIT OU Apache-2.0 |
| rustls-native-certs (autorités racines du système) | MIT OU Apache-2.0 OU ISC |
| ureq (horodatage RFC 3161, HTTP) | MIT OU Apache-2.0 |
| image | MIT OU Apache-2.0 |
| zip (documents Word et Excel), roxmltree (lecture des .docx) | MIT |
| arboard (presse-papiers) | MIT OU Apache-2.0 |
| serde, serde_json | MIT OU Apache-2.0 |
| tokio | MIT |
| parking_lot, crossbeam-channel | MIT OU Apache-2.0 |
| thiserror, dirs, chrono | MIT OU Apache-2.0 |
| whoami | Apache-2.0 OU BSL-1.0 OU MIT |

Liste exhaustive des dépendances transitives :

```bash
cargo install cargo-license && cd src-tauri && cargo license
```

## Bibliothèques JavaScript (interface)

| Paquet | Licence |
|---|---|
| react, react-dom | MIT |
| zustand | MIT |
| i18next, react-i18next | MIT |
| lucide-react | ISC |
| @fontsource/geist-sans | MIT (paquet) / OFL-1.1 (police) |
| @tauri-apps/api, @tauri-apps/plugin-dialog, @tauri-apps/plugin-opener | MIT OU Apache-2.0 |

## Outils de développement (non distribués)

| Outil | Licence |
|---|---|
| Vite, Vitest, @vitejs/plugin-react | MIT |
| TypeScript | Apache-2.0 |
| @tauri-apps/cli | MIT OU Apache-2.0 |
| playwright-core (captures d'écran) | Apache-2.0 |
| axe-core (audit d'accessibilité) | MPL-2.0 |
| NSIS (générateur d'installateur) | zlib/libpng |

## Polices des signatures manuscrites

Fournies avec l'application (`src-tauri/resources/fonts/`, avec leur licence) :

| Police | Auteur | Licence |
|---|---|---|
| Caveat | Impallari Type | SIL Open Font License 1.1 |
| Mrs Saint Delafield | Alejandro Paul (Sudtipos) | SIL Open Font License 1.1 |
| Homemade Apple | Font Diner | Apache License 2.0 |

## Polices Windows intégrées aux documents

Lors d'une conversion PDF/A, les polices standard non intégrées (Helvetica,
Times, Courier) sont remplacées par leurs équivalents installés avec Windows
(Arial, Times New Roman, Courier New), intégrés en sous-ensemble. Ces polices
ne sont pas distribuées avec Aura PDF : elles proviennent du système de
l'utilisateur, et leur licence (indicateur d'intégration OS/2 « Editable » ou
« Installable ») autorise leur intégration dans les documents. Les polices dont
la licence interdit l'intégration (« Restricted ») ne sont jamais utilisées.
