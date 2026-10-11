# Jalon 3 — Annotation

Module **Annoter**, reproduisant `Annotate.dc.html`.

## Ce qui est livré

| Fonction | Détail |
|---|---|
| Surligner / Souligner / Barrer (H / U / S) | Glisser sur le texte : la sélection suit les mots réels de la page, une bande par ligne ; le surlignage « balaie » la phrase de gauche à droite |
| Note (N) | Clic sur la page : épingle (icône de bulle) et saisie du commentaire dans le panneau |
| Zone de texte (T) | Clic puis saisie en place ; double-clic pour modifier ; vide = supprimée. Par défaut : texte noir seul, sans cadre ni fond |
| Mise en forme du texte | Panneau « Texte » : police (Sans / Serif / Mono), gras, italique, taille 6–96 pt, alignement, couleur du texte, fond (aucun / couleur), bordure (aucune / couleur) ; s'applique à la sélection ou aux prochaines zones |
| Dessin libre (P) | Tracé lissé et simplifié (Ramer–Douglas–Peucker) |
| Flèche / Rectangle / Ellipse / Ligne (A / R / E / L) | Glisser ; Maj = carré, cercle ou angle multiple de 45° ; les traits « se dessinent » |
| Tampons | Approuvé, Confidentiel, Brouillon, À revoir ; le tampon « tombe » sur la page |
| Sélection (V) | Clic, déplacement à la souris, Suppr pour supprimer, Échap pour désélectionner |
| Barre d'actions flottante | Recolorer, Commenter, Copier le texte balisé, Retirer |
| Style | Couleur (4 teintes + bulle « + » : palette de 20 couleurs, couleur libre, code hex, couleurs récentes mémorisées), opacité, épaisseur ; s'applique à l'outil et à la sélection |
| Liste des annotations | Toutes les pages, citation ou contenu, clic = aller à l'annotation |
| Commentaires | Fil par annotation : réponses, Résoudre / Rouvrir, filtres Tous / Ouverts / Résolus, citation du texte balisé ; commentaire libre = note dans la marge |
| Annuler / Rétablir | Ctrl+Z / Ctrl+Y (100 étapes) |
| Enregistrer | Ctrl+S ou bouton ; pastille « non enregistré » sur l'onglet ; confirmation à la fermeture d'un onglet ou de la fenêtre |

## Format et compatibilité

Les annotations sont de **vraies annotations PDF**, lisibles par Acrobat et les
autres lecteurs :

- types standard (`Highlight`, `Underline`, `StrikeOut`, `Text`, `FreeText`, `Ink`,
  `Line` avec `/LE OpenArrow`, `Square`, `Circle`, `Stamp`), avec `QuadPoints`,
  `InkList`, `L`, `/CA` (opacité), `/T` (auteur), `/NM` (identifiant), `/M` ;
- **apparence (`/AP`) générée** pour chaque annotation : rendu identique partout,
  y compris dans les lecteurs qui ne savent pas la recalculer (surlignage en
  mode « multiply », tampons inclinés, texte WinAnsi dans les 12 polices standard
  Helvetica / Times / Courier en gras et italique, `/DA`, `/Q`, `/IC`, `/BS`) ;
- **réponses et état « résolu » au format standard** (`/IRT`, `/StateModel /Review`,
  `/State /Completed`), compris par Acrobat ;
- **enregistrement incrémental** : le fichier d'origine n'est pas réécrit, les
  modifications sont ajoutées à la fin. Les signatures existantes restent
  vérifiables, l'enregistrement est rapide et atomique (fichier temporaire puis
  remplacement) ;
- pages tournées (`/Rotate`) et CropBox décalée prises en charge : le texte des
  zones de texte et tampons reste droit à l'écran.

Les annotations d'autres logiciels sont lues et modifiables. Exception : les
tampons d'autres logiciels (souvent des images) peuvent être commentés ou
supprimés, mais pas déplacés, pour ne pas perdre leur apparence.

## Affichage

Les annotations intactes du fichier sont dessinées par PDFium (fidélité maximale).
Dès qu'une annotation est créée ou modifiée, une couche SVG la dessine (et
l'anime), et PDFium la masque dans son rendu. Après enregistrement, PDFium
redessine tout depuis le fichier, avec un rendu identique (vérifié : captures
`j3-annoter.png` avant et `j3-enregistre.png` après enregistrement).

## Captures

| Capture | Contenu |
|---|---|
| `captures/j3-annoter.png` | Module Annoter : annotations, sélection, barre d'actions, commentaire |
| `captures/j3-animation.png` | Juste après les créations (animations en cours) |
| `captures/j3-enregistre.png` | Après Ctrl+S : rendu PDFium depuis le fichier |
| `captures/j3-zone-texte.png`, `j3-note.png` | Zone de texte et note |
| `captures/j3-texte-avant.png`, `j3-texte-apres.png` | Zone simple et zone mise en forme, avant et après enregistrement |
| `captures/j3-confirmation.png` | Fermeture d'un onglet non enregistré |

## Tests

- Rust (54 au total, 13 nouveaux) : géométrie page ↔ affichage pour les 4
  rotations, encodage WinAnsi et largeurs Helvetica, apparences (QuadPoints en
  espace utilisateur, flèche, tampon et police, page tournée), **aller-retour
  complet** des 11 types avec réponses et état résolu (mise à jour incrémentale
  vérifiée octet par octet, suppression, déplacement, commentaire modifié,
  réponses conservées, pas de dérive des boîtes), et **rendu PDFium** de
  l'apparence générée puis masquage pendant l'édition.
- TypeScript (26 au total, 6 nouveaux) : sélection de mots, rectangles par ligne,
  citation, déplacement, simplification de tracé, pointe de flèche.
- Scénarios de bout en bout avec de vrais gestes souris (`scripts/e2e-annot.mjs`,
  `scripts/e2e-annot2.mjs`) : création de chaque type, commentaire, Ctrl+S puis
  relecture du fichier, zone de texte, note, annuler/rétablir, confirmation ;
  `scripts/e2e-texte.mjs` : zone simple et zone serif gras italique 16 pt,
  couleur de la palette étendue, fond, bordure, centrée, relue à l'identique.

## Bogues trouvés et corrigés

- Boîtes qui grossissaient à chaque enregistrement (marge de trait) : boîte exacte
  et `/RD` pour les tampons.
- Barre d'outils sur deux lignes (centrage par `left: 50 %`).
- Clic sur une couleur qui recolorait la forme tout juste tracée.
- Zone de texte refermée aussitôt (le `mousedown` reprenait le focus).
- Cadre et fond blancs ajoutés aux zones de texte à l'enregistrement : le style
  par défaut est maintenant du texte seul ; cadre et fond sont des options.
- Fond des zones de texte translucide avant l'enregistrement : le mode
  « multiply » est réservé aux surlignages (et aux résultats de recherche).
- Pastilles de couleur écrasées (conflit avec une règle CSS des signets).

## Écarts restants

| Écart | Raison / suite |
|---|---|
| Documents protégés par mot de passe : annotations en lecture seule | La sauvegarde incrémentale des fichiers chiffrés arrive avec le module Sécurité (jalon 7). |
| Pas de poignées de redimensionnement | Déplacement seulement ; redimensionner = supprimer et retracer. Prévu au jalon 9. |
| Annotations non enregistrées absentes de l'impression | La fenêtre d'impression l'indique ; enregistrer d'abord (Ctrl+S). |
| Citation des surlignages | Reconstituée depuis le texte de la page (le PDF ne la stocke pas). |
