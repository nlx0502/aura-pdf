# Compiler Aura PDF

## Prérequis (Windows 10/11 x64)

| Outil | Version testée | Installation |
|---|---|---|
| Node.js | 24 | <https://nodejs.org> |
| Rust (MSVC) | 1.99 | `winget install --id Rustlang.Rustup -e` |
| Visual Studio Build Tools 2022 | 17.14 (C++ + SDK Windows 11) | voir ci-dessous |
| Python (fixtures de test uniquement) | 3.10+ | facultatif |

```bash
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--passive --wait --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.Windows11SDK.22621"
```

Astuce si le disque C: est plein : définir `RUSTUP_HOME` et `CARGO_HOME` vers un
autre disque avant d'installer Rust, et passer `--installPath`,
`--path cache=` et `--path shared=` à l'installateur des Build Tools.

## PDFium

La bibliothèque `pdfium.dll` (binaires officiels de
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries),
build `chromium/8086`) est versionnée dans `src-tauri/resources/pdfium/` avec
ses licences. Pour la mettre à jour, remplacer `pdfium.dll`, `VERSION` et le
dossier `licenses/` par ceux de l'archive `pdfium-win-x64.tgz`.

## Tesseract (OCR)

Tesseract 5 et ses dépendances (Leptonica, libpng, libjpeg-turbo, libtiff,
libwebp, OpenJPEG, giflib, zlib, libarchive…) sont liés **statiquement** dans
l'exécutable. Ils se compilent une fois avec vcpkg (environ 1 h) :

```bash
git clone https://github.com/microsoft/vcpkg D:/DevTools/vcpkg
```

```bash
D:/DevTools/vcpkg/bootstrap-vcpkg.bat -disableMetrics
```

```bash
D:/DevTools/vcpkg/vcpkg.exe install tesseract:x64-windows-static-md
```

`build.rs` cherche les bibliothèques dans `D:\DevTools\vcpkg` ; un autre
emplacement se donne par la variable `AURA_VCPKG_ROOT`. Les données de langue
(`tessdata_fast` : français, anglais, allemand, néerlandais) sont dans
`src-tauri/resources/tessdata/` et installées avec l'application.

## Lancer en développement

```bash
npm install
npm run app:dev
```

Captures d'écran automatisées (pour comparer avec les maquettes) : lancer
l'application avec le débogage distant WebView2, puis utiliser `scripts/shot.mjs`.

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'; npm run app:dev
```

```bash
node scripts/shot.mjs docs/captures/accueil.png
```

## Tests

```bash
python scripts/make_fixtures.py
npm test
cd src-tauri && cargo test --lib
```

Les tests Rust du moteur (`src-tauri/src/engine/tests.rs`) utilisent la vraie
`pdfium.dll` et les PDF de `tests/fixtures/` : ouverture, mot de passe,
fichier endommagé, recherche, rendu complet et par tuiles, couche texte, liens.

## Installateur Windows

```bash
npm run app:build
```

Produit `src-tauri/target/release/bundle/nsis/Aura PDF_<version>_x64-setup.exe` :
installation par utilisateur ou pour tous, raccourcis, désinstallation,
association des fichiers `.pdf`, installation de WebView2 si nécessaire,
page de licence en français, proposition de faire d'Aura PDF l'application PDF
par défaut (`src-tauri/installer/hooks.nsh`). Les images et le texte de licence
de l'installateur sont produits par `python scripts/make_installer_assets.py`.

Audits : `node scripts/audit-a11y.mjs`, `node scripts/audit-keyboard.mjs` et
`node scripts/audit-perf.mjs` (application lancée en développement avec
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`).

L'installateur n'est pas signé (pas de certificat de signature de code) :
Windows SmartScreen affiche un avertissement au premier lancement
(« Informations complémentaires » → « Exécuter quand même »).

## macOS (puces Apple)

Prérequis : Xcode Command Line Tools (`xcode-select --install`), Rust, Node.js
22, Homebrew (`brew install autoconf automake libtool pkg-config nasm`, pour
compiler les dépendances de Tesseract).

```bash
scripts/fetch-pdfium.sh            # libpdfium.dylib (même build que Windows)
git clone https://github.com/microsoft/vcpkg ~/vcpkg && ~/vcpkg/bootstrap-vcpkg.sh
~/vcpkg/vcpkg install tesseract:arm64-osx
export AURA_VCPKG_ROOT=~/vcpkg
npm install
npm run app:dev                    # développement
npx tauri build --bundles dmg      # src-tauri/target/release/bundle/dmg/*.dmg
```

La configuration propre au Mac est dans `src-tauri/tauri.macos.conf.json`
(barre de titre transparente avec les boutons natifs, DMG, signature ad hoc) ;
celle de Windows dans `src-tauri/tauri.windows.conf.json`.

Vérifier le code Mac depuis Windows (sans éditeur de liens) : la cible
`aarch64-apple-darwin` (`rustup target add aarch64-apple-darwin`) et
`cargo check --target aarch64-apple-darwin`, avec un compilateur C factice
pour les fichiers Objective-C des dépendances. La compilation complète et les
tests tournent sur les machines Mac de GitHub Actions.

## Intégration continue et publication

- `.github/workflows/build.yml` : à chaque envoi sur `main` et à chaque
  demande de fusion, tests et paquets Windows (NSIS) et macOS (DMG) ;
- `.github/workflows/release.yml` : sur une étiquette `v*`, signature de
  l'installateur Windows par SignPath et brouillon de version GitHub (voir
  [SIGNATURE-CODE.md](SIGNATURE-CODE.md)).
