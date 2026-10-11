# Aura PDF

Lecteur et éditeur PDF **libre, gratuit et hors ligne** pour Windows : lire,
annoter, éditer, signer et gérer vos PDF, sans compte ni télémétrie.

> État : **jalon 4** (fondations, accueil, lecture, impression, annotation, édition). Voir la feuille de route ci-dessous.

## Fonctionnalités disponibles (jalon 1)

- **Accueil** : ouverture (bouton, Ctrl+O, glisser-déposer depuis l'Explorateur),
  carte « Reprendre la lecture », documents récents groupés (Aujourd'hui, Hier,
  Cette semaine, Plus ancien) en liste ou grille, recherche, filtres (Signés,
  Formulaires, Numérisés), favoris, retrait de la liste, progression de lecture,
  emplacements (Documents, Bureau, Téléchargements).
- **Créer un PDF** : pages vierges (A4, A5, Lettre US ; vierge, lignée, quadrillée,
  pointillée ; portrait ou paysage), assemblage d'images (JPG, PNG, TIFF, WebP, GIF, BMP).
- **Lecture** : onglets multiples, défilement continu / page par page / double page,
  zoom animé (boutons, Ctrl+molette autour du pointeur, ajustement à la largeur),
  rotation animée, mode sombre du papier, mode lecture (F), plein écran (F11),
  rendu par tuiles aux grands zooms, sélection et copie du texte, liens internes et externes.
- **Navigation** : miniatures virtualisées, table des matières, signets personnels
  (renommables, mémorisés), pièces jointes (enregistrement), calques (annotations, champs).
- **Recherche** : insensible à la casse et aux accents, résultats progressifs en cascade,
  surlignage dans la page, Entrée / Maj+Entrée pour naviguer.
- **Reprise** : retour automatique à la dernière page lue (« Reprise à la dernière page lue »).
- **Impression** (Ctrl+P) : imprimantes Windows avec leur état, « Enregistrer au format PDF »
  (vectoriel), plages de pages, paires/impaires, ordre inverse, ajuster / taille réelle /
  échelle, affiche en tuiles, 2 à 9 pages par feuille, brochure, A4 / A3 / Lettre US,
  orientation automatique, recto verso, niveaux de gris, annotations ou tampons, résumé des
  commentaires, aperçu fidèle et annulable. Détails : [docs/JALON-2.md](docs/JALON-2.md).
- **Annotation** : surligner, souligner, barrer, notes, zones de texte, dessin libre,
  flèches, rectangles, ellipses, lignes, tampons ; commentaires avec réponses et état
  résolu ; annuler/rétablir ; enregistrement incrémental compatible Acrobat (Ctrl+S).
  Détails : [docs/JALON-3.md](docs/JALON-3.md).
- **Édition** : organiser les pages (sélection, glisser-déposer, pivoter, supprimer,
  extraire, insérer une page vierge ou les pages d'un autre PDF), en-tête et pied de
  page, numérotation, filigrane, rognage, **caviardage définitif** (IBAN, e-mails,
  téléphones, cartes bancaires, mots choisis ou zones tracées : le contenu est retiré
  du fichier, pas masqué), modification du texte et des images existants, ajout de
  texte, d'images et de liens ; calques du document (OCG) dans la lecture.
  Détails : [docs/JALON-4.md](docs/JALON-4.md).
- **Palette de commandes** (Ctrl+K), paramètres (animations, prénom, mode sombre par défaut).
- **Erreurs claires** : mot de passe requis ou incorrect, fichier endommagé, fichier introuvable.
- **Accessibilité** : navigation au clavier, focus visible, libellés pour lecteurs d'écran,
  respect du réglage Windows « Afficher les animations ».

Performances mesurées : un PDF de 600 pages s'ouvre en ~90 ms ; seules les pages
visibles sont rendues, par priorité (page visible → voisines → miniatures).

## Raccourcis clavier

| Action | Raccourci |
|---|---|
| Ouvrir un fichier | Ctrl+O |
| Créer un PDF | Ctrl+N |
| Imprimer | Ctrl+P |
| Rechercher dans le document | Ctrl+F |
| Palette de commandes | Ctrl+K |
| Zoom avant / arrière / ajuster | Ctrl++ / Ctrl+- / Ctrl+0 |
| Onglet suivant / précédent | Ctrl+Tab / Ctrl+Maj+Tab |
| Fermer l'onglet | Ctrl+W |
| Plein écran / mode lecture | F11 / F |
| Page suivante / précédente | Page suiv. / Page préc., Espace, flèches |
| Première / dernière page | Début / Fin |
| Fermer un dialogue ou un menu | Échap |
| Enregistrer (annotations, édition) | Ctrl+S |
| Annuler / rétablir (Annoter, Éditer) | Ctrl+Z / Ctrl+Y |
| Éditer : tout sélectionner / supprimer / pivoter | Ctrl+A / Suppr / R (Maj+R : sens inverse) |

Outils du module Annoter : Sélection **V**, Surligner **H**, Souligner **U**,
Barrer **S**, Note **N**, Zone de texte **T**, Dessin **P**, Flèche **A**,
Rectangle **R**, Ellipse **E**, Ligne **L** ; Suppr supprime la sélection.

## Architecture

```
src/                      Interface React + TypeScript (Vite)
  theme/tokens.css        Design tokens (fichier de thème unique)
  i18n/                   Textes (fr.ts) — ajouter une langue = un fichier
  components/             Bibliothèque de composants (vitrine : Ctrl+K → « Bibliothèque de composants »)
  screens/home|reader     Écrans
  shell/                  En-tête, palette, dialogues, contrôles de fenêtre
  stores/                 État global (Zustand)
  lib/                    API typée, rendu, géométrie du visualiseur, formatage (testés)
src-tauri/src/            Cœur Rust
  engine/                 Thread PDFium : rendu prioritaire, recherche, texte, liens
  protocol.rs             Protocole interne aura:// (bitmaps de pages, miniatures)
  library.rs              Récents, préférences, emplacements (JSON local)
  annot/                  Annotations (lopdf) : lecture, apparences, enregistrement incrémental
  edit/                   Éditer (lopdf) : pages, ajouts, liens, calques, caviardage glyphe par glyphe
  create.rs               Création de PDF
  platform.rs             Code propre au système (portage macOS)
```

Les bitmaps de pages ne transitent pas par l'IPC JSON : ils sont servis en
binaire par le protocole `aura://` et décodés hors du fil principal
(`createImageBitmap`). Le moteur PDFium vit dans un seul thread et traite une
file de priorité ; les demandes devenues inutiles après un défilement ou un
zoom sont abandonnées sans être calculées.

## Développement

Voir [docs/BUILD.md](docs/BUILD.md) pour l'installation des outils.

```bash
npm install
npm run app:dev
```

```bash
npm test
```

```bash
cd src-tauri && cargo test --lib
```

Les PDF de test sont générés par `python scripts/make_fixtures.py`.

## Feuille de route

1. ✅ Fondations : thème, composants, accueil, en-tête et menu Outils, lecture
2. ✅ Impression complète (Ctrl+P)
3. ✅ Annotation
4. Édition et organisation des pages, caviardage
5. Fusion et division, compression
6. Signature manuscrite et numérique
7. Formulaires, sécurité (AES-256, permissions, PDF/A)
8. Conversion, OCR (Tesseract), création depuis Office, scanner, presse-papiers
9. Finitions : installateur définitif, audit d'accessibilité et de performances

## Licence

MIT — voir [LICENSE](LICENSE) et [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md).
