# Mission

Développe une application de bureau Windows complète, gratuite et open source,
équivalente à Adobe Acrobat Reader / Acrobat Pro, destinée à lire, annoter,
éditer, signer et gérer des fichiers PDF. Nom de travail : **Aura PDF**.

# Contraintes non négociables

- Application native de bureau (PAS une webapp, PAS un site dans un navigateur).
- Livrable final : un installateur Windows .exe (installation, désinstallation,
  raccourcis, association des fichiers .pdf).
- Interface entièrement en français (avec architecture i18n pour ajouter
  d'autres langues).
- 100 % gratuite : uniquement des bibliothèques open source avec licences
  compatibles (vérifier et documenter les licences, ex. MuPDF/AGPL vs
  PDFium/BSD). Cela vaut aussi pour les polices et les icônes.
- Fonctionnement hors ligne, sans compte ni télémétrie : toutes les ressources
  (polices, icônes, moteur OCR, données de langue) sont embarquées, aucun appel
  réseau.
- L'interface doit reproduire fidèlement les maquettes fournies (voir
  « Design et expérience utilisateur »).

# Stack technique (à confirmer / justifier avant de coder)

Propose-moi 2 à 3 options avec avantages et inconvénients (par exemple
Tauri + Rust + PDFium, Electron + PDF.js + pdf-lib, C#/.NET WPF + PDFium,
Qt/C++), recommande-en une, puis attends ma validation.

Pour chaque option, évalue explicitement :
- la **fidélité au design** des maquettes : flou d'arrière-plan (backdrop blur),
  coins arrondis, ombres douces, animations à ressort, indicateurs qui glissent ;
- l'effort nécessaire pour reproduire ces effets ;
- les performances de rendu sur un PDF de 500+ pages ;
- la taille de l'installateur.

# Design et expérience utilisateur

## Maquettes de référence

Le dossier `design/` contient les maquettes interactives :
- `design/HANDOFF.md` : guide et liste des écrans ;
- `design/project/*.dc.html` : un écran par fichier ;
- `design/project/canvas.json` : index des écrans.

Chaque `.dc.html` s'ouvre dans un navigateur. Ce sont des **références
visuelles et comportementales**, pas du code à réutiliser : réimplémente
l'interface dans la stack retenue en respectant la mise en page, les couleurs,
les textes, les interactions et les animations.

| Écran | Fichier |
|---|---|
| Accueil (lancement sans fichier) | `Home.dc.html` |
| En-tête partagé + menu Outils + impression | `AppBar.dc.html` |
| Lecture | `Main.dc.html` |
| Annotation | `Annotate.dc.html` |
| Édition et organisation des pages | `Organize.dc.html` |
| Signature | `Sign.dc.html` |
| Sécurité | `Security.dc.html` |
| Outil : Fusion et division | `Merge.dc.html` |
| Outil : Formulaires | `Forms.dc.html` |
| Outil : Compresser | `Compress.dc.html` |
| Outil : Conversion et OCR | `Convert.dc.html` |

## Principes

- **Le document d'abord** : l'interface est sobre et sombre, et le document est
  l'élément le plus lumineux de l'écran.
- **Épuré et moderne** :
  - des panneaux flottants arrondis séparés par des marges de 8 px, plutôt que
    des barres collées ;
  - des contrôles en pilules ;
  - une seule couleur d'accent.
- **Aucun style « code »** :
  - pas de police à chasse fixe dans l'interface ;
  - pas de `##`, `//` ou `>` décoratifs, pas de libellés en minuscules ;
  - des libellés en casse normale, en français soigné (« Page 2 sur 14 »,
    « Imprimer », « Enregistré »).
- **Animations fonctionnelles** : chaque animation explique un changement d'état.
  Elles restent courtes (150–450 ms) et désactivables (respecter le réglage
  système « réduire les animations »).

## Design tokens

À centraliser dans un fichier de thème unique (pas de valeurs en dur dans les
composants).

| Rôle | Valeur |
|---|---|
| Fond de l'application | `#0E0F12` |
| Panneaux | `#16171B` (zone centrale `#121317`) |
| Cartes / éléments surélevés | `#1B1C21`, survol `#212228` |
| Contrôles | `#1F2026`, survol `#282930`, sélection `#2A2B33` |
| Bordures | blanc à 5–8 % d'opacité (`rgba(255,255,255,.05–.08)`) |
| Texte principal | `#ECECEF` |
| Texte secondaire | `#9A9DA6` (min. 4,5:1 sur les fonds) |
| Accent (boutons principaux, sélection) | `#6E6AF6`, survol `#7E7AF8` |
| Accent sur fond sombre (texte, icônes) | `#A9A6FF` |
| Succès / valide | `#4FCB91` |
| Avertissement / surlignage | `#F2B84B` |
| Danger / caviardage | `#EF6B6B` |
| Papier du document | `#FBFAF7` (mode sombre papier : `#1B1C21`) |

- **Typographie** : Geist (SIL OFL), graisses 400 / 500 / 600, embarquée
  localement. Chiffres tabulaires pour les compteurs, tailles et pourcentages.
  Tailles : 12,5 px (secondaire), 13 px (interface), 14 px (titres de panneau),
  17–22 px (titres d'écran), 26 px (accueil).
- **Rayons** : 8–10 px (boutons, champs), 12–14 px (cartes), 16 px (panneaux),
  20 px (dialogues), 999 px (pilules, interrupteurs).
- **Ombres** : douces et larges sous le papier et les éléments flottants
  (ex. `0 30px 60px -20px rgba(0,0,0,.7)`), pas de bordures épaisses.
- **Icônes** : trait de 1,5 px, 16 px, extrémités arrondies. Bibliothèque
  conseillée : Lucide (ISC), à confirmer.
- **Courbe d'animation** : `cubic-bezier(.2,.8,.2,1)` pour les transitions, et
  une courbe à rebond léger pour les interrupteurs et les coches.

## Composants à créer d'abord (bibliothèque interne)

- Bouton : principal, secondaire, fantôme, danger, icône seule avec infobulle.
- Contrôle segmenté avec indicateur qui glisse.
- Interrupteur.
- Pastille de filtre (chip).
- Champ texte avec halo de focus.
- Curseur (slider).
- Pas à pas (− / +).
- Case à cocher et coche animées.
- Menu déroulant (apparition par échelle et fondu).
- Dialogue modal (fond flouté, apparition « pop »).
- Notification éphémère (toast).
- Barre de progression.
- Liste à étapes (en attente → en cours avec spinner → terminée avec coche).
- Miniature de page.
- Panneau accordéon.

## Structure de navigation

- **En-tête** (identique sur tous les écrans d'un document) :
  - logo (retour à l'accueil) ;
  - modules principaux : **Lire, Annoter, Éditer, Signer, Sécurité** ;
  - menu déroulant **Outils** : Fusionner et diviser, Formulaires, Compresser,
    Convertir et OCR, Imprimer ;
  - à droite : bouton **Imprimer** (infobulle « Imprimer · Ctrl+P ») et
    **Exporter**.
- **Écran d'accueil** au lancement sans fichier :
  - ouvrir un fichier (bouton, Ctrl+O, glisser-déposer) ;
  - carte « Reprendre la lecture » ;
  - créer un PDF ;
  - documents récents triés du plus récent au plus ancien, groupés
    (Aujourd'hui, Hier, Cette semaine, Plus ancien), avec :
    - vue liste ou grille, recherche, filtres (Signés, Formulaires, Numérisés) ;
    - favoris, retrait de la liste ;
    - progression de lecture (« Page 7 sur 22 »).
- **Mise en page des écrans** : panneau gauche (navigation ou liste), zone
  centrale (document ou espace de travail), panneau droit (propriétés ou
  réglages). Sous 1280 px, les panneaux latéraux se replient. Les barres
  d'outils passent à la ligne au lieu de déborder.

## Comportements et animations attendus (voir les maquettes)

- **Lecture** :
  - le zoom redimensionne le papier en douceur ;
  - la rotation est animée ;
  - les résultats de recherche apparaissent en cascade, avec surlignage dans le
    texte ;
  - une notification « Reprise à la dernière page lue » s'affiche à l'ouverture ;
  - le mode lecture masque les panneaux ;
  - une palette de commandes s'ouvre avec Ctrl+K.
- **Annotation** :
  - le surlignage « balaie » la phrase de gauche à droite ;
  - les formes et le dessin se tracent à l'écran ;
  - les tampons « tombent » sur la page ;
  - une barre d'actions flottante s'affiche au-dessus de la sélection ;
  - les commentaires sont filtrables (Tous, Ouverts, Résolus).
- **Édition** :
  - une barre d'actions flottante apparaît dès qu'une page est sélectionnée ;
  - les pages pivotent avec animation ;
  - les pages supprimées se réduisent avant de disparaître ;
  - filigrane, numérotation et rognage s'affichent en direct sur les
    miniatures ;
  - le caviardage montre un faisceau de scan puis des bandes noires.
- **Fusion** : les fichiers glissent vers leur nouvelle position, et l'aperçu
  coloré du découpage se met à jour en direct.
- **Signature** :
  - on peut réellement dessiner la signature dans le pad ;
  - la signature numérique déroule ses étapes (certificat → empreinte →
    horodatage → intégration) ;
  - la signature apparaît ensuite avec un effet « encre ».
- **Formulaires** : anneau de progression du remplissage, et flash lors de
  l'aplatissement.
- **Compresser** :
  - niveaux : légère, recommandée, forte, personnalisée ;
  - la taille estimée s'anime en chiffres ;
  - une barre avant/après détaille images, polices, contenu et métadonnées.
- **Conversion / OCR** :
  - un flux animé relie le format source au format cible ;
  - un faisceau balaie la page scannée ;
  - le texte reconnu apparaît ligne par ligne.
- **Sécurité** : le cadenas se ferme quand la protection est activée, et une
  jauge indique la force du mot de passe.

## Impression (système complet, accessible partout via Ctrl+P)

Fenêtre en deux colonnes : aperçu à gauche, réglages à droite.
- **Imprimante** :
  - liste des imprimantes Windows avec leur état ;
  - option « Enregistrer au format PDF » ;
  - nombre de copies, assemblage.
- **Pages** :
  - toutes, page actuelle, ou plage personnalisée (« 1-5, 8, 11-13 ») avec
    validation ;
  - toutes les pages, paires ou impaires ;
  - ordre inverse.
- **Taille et gestion des pages** :
  - **Taille** : ajuster, taille réelle, réduire si trop grande, échelle
    personnalisée ;
  - **Affiche** : mosaïque, agrandissement en %, repères de découpe ;
  - **Multiple** : 2, 4, 6 ou 9 pages par feuille, bordures ;
  - **Brochure** : imposition correcte, reliure à gauche ou à droite.
- **Papier** : format (A4, A3, Lettre US) et orientation (auto, portrait,
  paysage).
- **Impression** : recto verso (bord long ou court), couleur ou niveaux de gris.
- **Commentaires et formulaires** : document seul, avec annotations, ou avec
  tampons ; résumé des commentaires en option.
- **Aperçu fidèle** : il se met à jour à chaque réglage, avec navigation
  « Feuille X sur Y (recto/verso) ».
- Le bas de la fenêtre indique le nombre de feuilles et de copies. Pendant
  l'envoi, une progression affiche « Envoi de la feuille X sur Y ».

## Raccourcis clavier

| Action | Raccourci |
|---|---|
| Ouvrir un fichier | Ctrl+O |
| Créer un PDF | Ctrl+N |
| Imprimer | Ctrl+P |
| Rechercher dans le document | Ctrl+F |
| Palette de commandes | Ctrl+K |
| Enregistrer | Ctrl+S |
| Annuler / rétablir | Ctrl+Z / Ctrl+Y |
| Zoom avant / arrière / ajuster | Ctrl++ / Ctrl+- / Ctrl+0 |
| Onglet suivant / précédent | Ctrl+Tab / Ctrl+Maj+Tab |
| Plein écran / mode lecture | F11 / F |
| Fermer un dialogue ou un menu | Échap |

Outils d'annotation (dans le module Annoter) :

| Outil | Raccourci |
|---|---|
| Sélection | V |
| Surligner / Souligner / Barrer | H / U / S |
| Note / Texte / Dessin | N / T / P |
| Flèche / Rectangle / Ellipse / Ligne | A / R / E / L |

# Fonctionnalités

## 0. Accueil
- Écran affiché quand l'application est lancée sans fichier (voir
  `Home.dc.html`).
- Ouvrir un fichier : bouton, Ctrl+O, glisser-déposer de PDF, images ou Word.
- Reprendre la lecture du dernier document.
- Documents récents triés par date de dernière ouverture, mémorisés localement,
  avec recherche, filtres, favoris, retrait de la liste, vue liste ou grille,
  dernière page lue.
- Emplacements (Documents, Bureau, Téléchargements).
- Créer un PDF à partir de :
  - une page vierge (format, orientation, nombre de pages ; fond vierge,
    lignée, quadrillée ou pointillée) ;
  - des images ;
  - un document Office ;
  - le scanner (WIA/TWAIN, avec OCR) ;
  - le presse-papiers.

## 1. Lecture
- Ouverture multi-onglets, zoom, rotation, défilement continu / page par page /
  double page, mode plein écran, mode lecture.
- Miniatures, signets, table des matières, pièces jointes, calques.
- Recherche de texte (avec surlignage et résultats listés).
- Impression : voir la section « Impression » ci-dessus, accessible depuis tous
  les écrans.
- Documents récents, reprise à la dernière page lue, mode sombre.

## 2. Annotation
- Surlignage, soulignement, barré, notes, zone de texte, dessin libre,
  formes (flèche, rectangle, ellipse, ligne), tampons, commentaires avec liste.

## 3. Édition
- Modifier texte et images existants, ajouter du texte/images/liens.
- Organiser les pages : supprimer, pivoter, réordonner, extraire, insérer.
- En-têtes/pieds de page, numérotation, filigrane, rognage, caviardage
  (suppression définitive et sécurisée).

## 4. Fusion et division (menu Outils)
- Fusionner plusieurs PDF (glisser-déposer, ordre modifiable).
- Diviser par pages, par taille ou par signets.

## 5. Signature
- Signature manuscrite (dessinée, tapée, importée en image).
- Signature numérique avec certificat (.pfx/.p12), validation des signatures
  existantes.

## 6. Formulaires (menu Outils)
- Remplir les formulaires AcroForm, créer des champs, aplatir.

## 7. Conversion et export (menu Outils)
- PDF vers images, Word, Excel, texte ; images/Word vers PDF.
- OCR hors ligne (Tesseract) sur documents numérisés.

## 8. Sécurité
- Protection par mot de passe, chiffrement AES-256, permissions.
- Métadonnées, PDF/A.

## 9. Compresser (menu Outils, outil séparé)
- Niveaux prédéfinis (légère, recommandée, forte) et réglages personnalisés :
  - résolution maximale et qualité JPEG des images, niveaux de gris ;
  - sous-ensembles de polices ;
  - suppression des métadonnées, des miniatures et des fichiers joints ;
  - optimisation pour l'affichage web.
- Estimation de la taille finale avant traitement, puis rapport avant/après.

# Qualité attendue

- Performances : ouverture rapide de gros PDF (500+ pages), rendu fluide
  (rendu progressif, miniatures en arrière-plan, virtualisation des listes).
- Interface réactive : aucune opération longue (OCR, compression, conversion,
  impression) ne bloque l'interface. Elle affiche une progression et peut être
  annulée.
- Code propre, modulaire, commenté, tests automatisés sur les fonctions critiques.
- Gestion des erreurs (PDF corrompu, mot de passe incorrect), avec des messages
  clairs en français dans le style des maquettes.
- Accessibilité :
  - tout est utilisable au clavier, avec un focus visible (anneau `#A9A6FF`) ;
  - contraste minimum 4,5:1 ;
  - libellés pour les lecteurs d'écran ;
  - respect du réglage « réduire les animations ».
- Fidélité au design : à chaque jalon, fournis des captures d'écran comparées
  aux maquettes correspondantes, et liste les écarts restants.

# Méthode de travail

1. Pose-moi toutes tes questions de cadrage AVANT de coder.
2. Propose une architecture et un plan par étapes (MVP → fonctionnalités avancées).
   - Le jalon 1 inclut le **thème et la bibliothèque de composants**, l'**écran
     d'accueil**, l'**en-tête avec le menu Outils** et la **lecture**.
   - L'impression (Ctrl+P) arrive dès que le rendu des pages est en place.
3. Développe par jalons, avec une version testable à chaque jalon.
4. Livre : code source, README, documentation de build, installateur .exe, et
   un fichier listant toutes les dépendances avec leur licence (polices et
   icônes comprises).
