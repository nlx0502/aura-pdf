# Aura PDF — maquettes interactives (handoff pour Claude Code)

Ce dossier contient les maquettes de l'interface d'Aura PDF. Chaque écran est un fichier `.dc.html` autonome : du HTML avec des styles en ligne, un bloc `<style>` dans `<helmet>` (classes, animations) et une classe `Component` en JS qui gère l'état et les interactions.
Les fichiers servent de **référence visuelle et comportementale**. Le code est à réimplémenter dans la stack de l'application, sans copier le runtime de maquette.

## Écrans (`project/`)
| Fichier | Rôle |
|---|---|
| `Home.dc.html` | Accueil au lancement sans fichier : ouvrir (bouton, Ctrl+O, glisser-déposer), reprendre la lecture, récents triés par date (liste/grille, filtres, favoris), création de PDF (vierge, images, Office, scanner, presse-papiers, Ctrl+N) |
| `AppBar.dc.html` | En-tête partagé : navigation principale, menu déroulant « Outils », bouton Imprimer + **Ctrl/⌘+P**, fenêtre d'impression complète |
| `Main.dc.html` | Lecture : onglets, miniatures/TOC/signets/pièces jointes/calques, zoom, rotation, continu/page/double, recherche avec surlignage, mode lecture, mode sombre, palette de commandes |
| `Annotate.dc.html` | Annotation : surligner/souligner/barrer, notes, texte, dessin, formes, tampons, commentaires |
| `Organize.dc.html` | Édition : pages (pivoter, déplacer, supprimer, extraire, insérer), en-tête/pied, numérotation, filigrane, rognage, caviardage ; édition du contenu |
| `Sign.dc.html` | Signature manuscrite (dessin/texte/image) et numérique (.p12/.pfx, horodatage), validation |
| `Security.dc.html` | Mot de passe, AES-128/256, permissions, métadonnées, PDF/A |
| `Merge.dc.html` | Outil : fusion (réordonnancement) et division (pages, taille, signets) |
| `Forms.dc.html` | Outil : remplissage AcroForm, création de champs, aplatissement |
| `Compress.dc.html` | Outil : compression (niveaux, réglages images/polices/nettoyage) |
| `Convert.dc.html` | Outil : conversion PDF ↔ Word/Excel/images/texte, OCR Tesseract hors ligne |
| `canvas.json` | Index des écrans (positions sur le canevas) |

## Design tokens
- Fond `#0E0F12`, panneaux `#16171B` / `#121317`, cartes `#1B1C21`, bordures `rgba(255,255,255,.05–.08)`
- Texte `#ECECEF`, secondaire `#9A9DA6`
- Accent `#6E6AF6` (texte accent `#A9A6FF`), succès `#4FCB91`, avertissement `#F2B84B`, danger `#EF6B6B`
- Police : Geist (400/500/600), chiffres tabulaires
- Rayons : contrôles 10 px, panneaux 16 px, dialogues 20 px, pilules 999 px
- Animations : `cubic-bezier(.2,.8,.2,1)`, 200–450 ms ; respecter `prefers-reduced-motion`

## Données
Les noms de fichiers, montants et contenus des documents sont des exemples à remplacer par les vraies données.
