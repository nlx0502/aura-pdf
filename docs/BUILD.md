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
association des fichiers `.pdf`, installation de WebView2 si nécessaire.

L'installateur n'est pas signé (pas de certificat de signature de code) :
Windows SmartScreen affiche un avertissement au premier lancement
(« Informations complémentaires » → « Exécuter quand même »).
