# Jalon 1 — rapport de livraison

Contenu : thème et bibliothèque de composants, écran d'accueil, en-tête avec menu
Outils, lecture complète.

## Captures et comparaison avec les maquettes

Les fichiers `.dc.html` dépendent d'un runtime (`support.js`) absent du dossier
`design/` : ils ne s'affichent pas tels quels dans un navigateur. La comparaison a
donc été faite avec le **code source** des maquettes (structure, valeurs des
styles, textes, animations, comportements). Captures prises à 1440 × 900.

| Écran | Capture | Maquette |
|---|---|---|
| Accueil | `captures/j1-accueil.png` | `Home.dc.html` |
| Accueil sous 1280 px | `captures/j1-etroit-accueil.png` | règle « les panneaux latéraux se replient » |
| Créer un PDF | `captures/j1-creer.png` | `Home.dc.html` (dialogue) |
| Lecture | `captures/j1-lecture.png` | `Main.dc.html` |
| Recherche avec surlignage | `captures/j1-recherche.png` | `Main.dc.html` |
| Double page + mode sombre | `captures/j1-double-sombre.png` | `Main.dc.html` |
| Rotation | `captures/j1-rotation.png` | `Main.dc.html` |
| Table des matières | `captures/j1-table-matieres.png` | `Main.dc.html` |
| Document de 600 pages | `captures/j1-gros-doc.png` | — (performances) |
| Lecture sous 1280 px | `captures/j1-etroit-lecture.png` | règle de repli |
| Menu Outils | `captures/j1-menu-outils.png` | `AppBar.dc.html` |
| Palette de commandes | `captures/j1-palette.png` | `Main.dc.html` |
| Mot de passe incorrect | `captures/j1-mot-de-passe.png` | style des dialogues |
| Fichier endommagé | `captures/j1-erreur-fichier.png` | style des notifications |
| PDF créé (quadrillé) | `captures/j1-pdf-cree.png` | — |

## Conforme aux maquettes

- Tokens repris à l'identique (`src/theme/tokens.css`) : fonds, panneaux, cartes,
  contrôles, accent `#6E6AF6`, rayons 10/14/16/20/999 px, ombres, courbe `cubic-bezier(.2,.8,.2,1)`.
- Panneaux flottants séparés de 8 px, contrôles en pilules, police Geist avec chiffres tabulaires.
- Animations des maquettes : indicateur glissant des contrôles segmentés, interrupteurs à rebond,
  apparition en cascade (`rise`), menu en échelle + fondu, dialogues « pop » sur fond flouté,
  bulle de dépôt qui flotte, étoile des favoris qui « pop », retrait animé d'un récent,
  barre de reprise qui se remplit, coche animée, toasts qui glissent.
- Textes en français soigné, identiques aux maquettes.

## Écarts restants (assumés ou prévus)

| Écart | Raison / suite |
|---|---|
| Contrôles de fenêtre (Réduire, Agrandir, Fermer) ajoutés à droite de l'en-tête | La fenêtre n'a pas de barre de titre système, pour que l'en-tête des maquettes soit la barre de titre. |
| Miniatures des récents : vraie première page au lieu des lignes stylisées | Les maquettes utilisent des espaces réservés ; le rendu réel est plus utile. |
| « Emplacements » liste les PDF réels des dossiers | Dans la maquette, c'est un filtre des récents. |
| Texte tertiaire `#8A8D96` au lieu de `#7C7F88` | `#7C7F88` n'atteint pas 4,5:1 sur `#16171B` (exigence d'accessibilité). |
| Résultats de recherche : « Page N » sans numéro de paragraphe | PDFium ne fournit pas la structure en paragraphes. |
| Calques : annotations et champs de formulaire uniquement | Les calques optionnels (OCG) du PDF arrivent au jalon 4. |
| Zoom : le papier est mis à l'échelle (transformation animée) puis re-rendu net | Équivalent visuel de la maquette (qui anime la largeur), mais fluide sur 600 pages. |
| Imprimer (Ctrl+P) ouvre une fenêtre d'attente | Fenêtre d'impression complète au jalon 2. |
| Annoter, Éditer, Signer, Sécurité, Outils : écran « arrive bientôt » | Jalons 3 à 8. |
| Créer depuis Office, scanner, presse-papiers ; OCR | Jalon 8 (le dialogue l'indique). HEIC non pris en charge. |
| Ctrl+S, Ctrl+Z / Ctrl+Y | Aucune modification possible avant le jalon 3. |
| Onglets non réordonnables par glisser | Prévu au jalon 9. |

## Bogues trouvés et corrigés pendant ce jalon

- Première entrée de la table des matières omise (itération de pdfium-render) — détecté par les tests d'intégration.
- Surlignage de recherche masquant le texte (mélange isolé par la transformation du calque).
- Miniatures légèrement rognées à droite ; barre d'outils qui passait à la ligne à 1440 px.
- Panneaux ouverts simultanément en surimpression sous 1280 px.
- Dernière page lue enregistrée uniquement à la fermeture de l'onglet.

## Tests

- 20 tests TypeScript (géométrie du visualiseur sur 600 pages, ancres de zoom,
  rotation, formatage français, regroupement et filtres des récents).
- 21 tests Rust, dont 7 tests d'intégration avec PDFium (ouverture, 600 pages en
  moins de 2 s, mot de passe requis / incorrect / correct, fichier endommagé,
  recherche progressive, rendu complet et par tuiles, couche texte et liens).
