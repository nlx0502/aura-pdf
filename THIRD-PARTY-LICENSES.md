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
| Tesseract + données fra/eng | OCR hors ligne — jalon 8 | Apache-2.0 | <https://github.com/tesseract-ocr> |
| qpdf | Chiffrement AES-256, linéarisation — jalons 5 et 7 | Apache-2.0 | <https://github.com/qpdf/qpdf> |

Les textes complets des licences de PDFium et de ses composants sont copiés
dans `src-tauri/resources/pdfium/licenses/`.

## Bibliothèques Rust (cœur de l'application)

| Crate | Licence |
|---|---|
| tauri, tauri-build, tauri-plugin-dialog, tauri-plugin-opener, tauri-plugin-single-instance | MIT OU Apache-2.0 |
| pdfium-render | MIT OU Apache-2.0 |
| lopdf (annotations, organisation des pages, caviardage) | MIT |
| windows (API Windows : impression) | MIT OU Apache-2.0 |
| regex (recherche des données à caviarder) | MIT OU Apache-2.0 |
| image | MIT OU Apache-2.0 |
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
| NSIS (générateur d'installateur) | zlib/libpng |
