# Jalon 4 — Éditer : pages, ajouts, caviardage, contenu, calques

Module Éditer conforme à `design/project/Organize.dc.html` : mode **Pages**
(grille des pages, barre d'actions de la sélection, réglages à droite) et mode
**Contenu** (page en grand, objets sélectionnables). Les calques du document
(promis au jalon 1) sont dans le panneau Calques de la lecture.

## Ce qui est livré

| Fonction | Détail |
|---|---|
| Sélection | Clic, Maj+clic pour une plage, Ctrl+A, case cochée animée ; Échap désélectionne |
| Réordonner | Glisser-déposer (une page ou toute la sélection, repère d'insertion, fantôme) ; flèches ‹ › de la barre d'actions ; les cartes glissent vers leur nouvelle place |
| Pivoter | Barre d'actions ou R (Maj+R : sens inverse) ; rotation animée, pastille « 90° » |
| Supprimer | Barre d'actions ou Suppr ; les pages se réduisent avant de disparaître |
| Extraire | Pages sélectionnées dans un nouveau PDF (dialogue « Enregistrer sous », action « Ouvrir ») |
| Insérer | Page vierge (format de la page voisine) ou toutes les pages d'un autre PDF, après la sélection |
| En-tête et pied de page | Texte libre, `{n}` et `{total}` remplacés ; aperçu en direct sur les miniatures |
| Numérotation | Formats « 1 », « 1 / 12 », « Page 1 sur 12 » ; 6 positions ; aperçu en direct |
| Filigrane | Texte, opacité 5–60 %, couleur ; incliné, dimensionné à la page ; aperçu en direct |
| Rognage | Marges en mm, toutes les pages ou la sélection ; cadre pointillé en direct |
| Caviardage | Motifs IBAN (clé contrôlée), e-mail, téléphone (FR et international), carte bancaire (Luhn), mots choisis ; faisceau de scan puis bandes noires ; zones tracées à la main (mode Contenu) ; zones retirables une à une |
| Contenu : texte | Repérage des paragraphes ; double-clic pour réécrire sur place ; police (Sans / Serif / Mono), gras, italique, taille, alignement, couleur ; déplacement à la souris ; ajout d'un bloc par clic |
| Contenu : images | Sélection, déplacement, redimensionnement (poignées, proportions gardées, Maj pour déformer), remplacer, rogner (cadre à poignées), pivoter, supprimer, ajouter |
| Contenu : liens | Liste des liens de la page, modification de l'adresse, suppression, nouveau lien tracé (adresse complétée : `exemple.fr` → `https://…`, e-mail → `mailto:`) |
| Annuler / rétablir | Ctrl+Z / Ctrl+Y pour tout (pages, réglages, contenu ; saisie continue = une étape) |
| Enregistrer | Ctrl+S ou bouton ; « Modifications non enregistrées » / « Enregistré » ; pastille sur l'onglet ; confirmation à la fermeture ; les annotations en attente sont enregistrées d'abord |
| Calques (lecture) | Calques optionnels du PDF (OCG) avec ordre et imbrication du document, affichés ou masqués à la volée (affichage seulement, le fichier n'est pas modifié) |

## Ajouts après la première revue

- **Polices installées** : à côté de Sans / Serif / Mono, un menu « Autres… »
  liste les polices TrueType du poste (recherche, aperçu dans chaque police),
  pour les zones de texte (Annoter) comme pour le texte modifié (Éditer). Seuls
  les caractères utilisés sont intégrés au PDF (sous-ensemble : quelques Ko au
  lieu de ~1 Mo pour Arial) ; le gras ou l'italique absents d'une famille sont
  simulés. Les polices dont la licence interdit l'intégration ne sont pas
  proposées. Le texte d'un PDF écrit dans une police installée (ex. Calibri) est
  reconnu et modifiable dans cette police.
- **Page de garde** : option « Page de garde sans numéro » et « Premier numéro »
  (la page 2 porte par défaut le n° 2, réglable à 1).
- Captures : `captures/j4-polices.png`, `j4-polices-page.png`, `j4-page-de-garde.png` ;
  scénario `scripts/e2e-polices.mjs`.

## Fonctionnement

- L'interface décrit le document voulu (**plan** : ordre, sources, rotations,
  rognage, zones à caviarder, liens, réglages). Rien n'est écrit avant
  l'enregistrement ; le cœur Rust produit alors le nouveau fichier en une passe
  (fichier temporaire puis remplacement).
- **Pages et ajouts** (lopdf) : l'arbre des pages est reconstruit, les attributs
  hérités recopiés ; les références aux pages supprimées (signets, destinations,
  structure, champs de formulaire) sont neutralisées pour que leur contenu
  disparaisse du fichier ; signets, pièces jointes et métadonnées sont conservés.
  En-têtes, numéros et filigrane sont des flux ajoutés (le contenu d'origine,
  encadré par `q … Q`, n'est pas regénéré), marqués comme artefacts de
  pagination pour les lecteurs d'écran.
- **Caviardage** (lopdf) : le contenu de la page est interprété (état graphique,
  état du texte, métriques des polices simples, composites Identity-H et CMap
  intégrés, Type 3) pour situer chaque glyphe. Les glyphes sous une zone sont
  retirés ; les suivants restent exactement à leur place (déplacement compensé
  dans `TJ`). Les pixels des images sous une zone sont effacés (JPEG, Flate, LZW,
  1 à 16 bits, masques de transparence) ; une image d'un format non décodable est
  retirée entière. Les formulaires XObject sont traités récursivement (copie
  propre à la page), les annotations touchées supprimées, les valeurs de champs
  effacées. Le fichier est ensuite réécrit sans historique, purgé des objets
  orphelins, des métadonnées (Info, XMP), des vignettes et des textes
  alternatifs des pages concernées. Police à codage incertain (écriture
  verticale, CMap prédéfini) : l'opération de texte entière est retirée par
  prudence.
- **Contenu** (PDFium) : les modifications sont appliquées au document en
  mémoire (aperçu immédiat) et rejouées depuis le fichier pour annuler. Le texte
  modifié est réécrit avec la police standard de la famille détectée (Helvetica,
  Times, Courier), ce qui garantit des glyphes complets et un découpage exact.

## Captures

| Capture | Contenu |
|---|---|
| `captures/j4-organiser.png` | État de la maquette : 3 pages sélectionnées, page 4 pivotée, numérotation et filigrane en direct |
| `captures/j4-glisser.png` | Glisser-déposer : fantôme et repère d'insertion |
| `captures/j4-suppression.png` | Pages qui se réduisent avant de disparaître |
| `captures/j4-contenu.png` | Mode Contenu : titre réécrit, lien tracé |
| `captures/j4-contenu-image.png` | Image pivotée et agrandie, zone caviardée à la main |
| `captures/j4-caviardage-scan.png`, `j4-caviardage.png` | Faisceau de scan puis bandes noires, bilan |
| `captures/j4-enregistre.png` | Après Ctrl+S : rotation, filigrane, titre intégrés au fichier |
| `captures/j4-calques.png` | Calques du document dans la lecture |

## Tests

- Rust (75 au total, 21 nouveaux) : réorganisation, suppression (contenu absent
  du fichier), rotation, attributs hérités, pages vierges, import d'un autre
  fichier, liens vers une page supprimée neutralisés, rognage, en-tête,
  numérotation, filigrane, liens (ajout, modification, suppression), calques,
  CMap intégré, motifs (IBAN, e-mail, téléphone, carte) ; **caviardage de bout en
  bout avec PDFium** : texte simple, tableau `TJ` crénelé, formulaire XObject,
  police composite Type0 — le texte caviardé n'est plus extractible ni présent
  dans les flux, le reste de la ligne garde sa position au dixième de point,
  l'original d'un XObject partagé n'est plus référencé ; image : pixels effacés,
  original absent ; édition du contenu (repérage des paragraphes, même ligne de
  base, annuler, enregistrer) ; calques (rendu vérifié au pixel).
- TypeScript (32, 6 nouveaux) : sélection, déplacements, rotation, formats de
  papier, plan d'enregistrement.
- Scénarios réels à la souris : `scripts/e2e-edit.mjs` (sélection, rotation,
  réglages, glisser-déposer, suppression, annuler/rétablir, titre réécrit, lien,
  enregistrement relu, caviardage relu), `scripts/e2e-edit2.mjs` (images,
  caviardage manuel, calques). Nouveaux PDF de test : `Données sensibles.pdf`,
  `Plan avec calques.pdf`.

## Bogues trouvés et corrigés

- Ressources héritées non résolues avant le caviardage (police inconnue → ligne
  entière retirée) — détecté par les tests.
- L'XObject d'origine restait dans le fichier, référencé (sans être dessiné) par
  les ressources d'autres pages — détecté par le test de bout en bout ; les
  références inutilisées sont désormais retirées.
- Les images en ligne réécrites par lopdf étaient invalides : encodeur dédié.
- Après un enregistrement préalable des annotations, les modifications de
  contenu en mémoire étaient perdues : elles sont rejouées.
- Numéros et en-têtes illisibles sur les petites miniatures : corps minimal.

## Écarts restants

| Écart | Raison / suite |
|---|---|
| Documents protégés par mot de passe : lecture seule | Avec le module Sécurité (jalon 7). |
| Police d'origine non installée sur le poste : texte modifié réécrit dans une police standard ou installée au choix | Les polices intégrées aux PDF sont souvent réduites aux glyphes utilisés. Polices OpenType à contours CFF (.otf) non proposées. |
| Texte d'un formulaire XObject ou incliné non modifiable en mode Contenu | Il reste sélectionnable pour le caviardage. |
| Pages insérées : contenu modifiable après l'enregistrement seulement | L'édition s'applique au document ouvert. |
| Modification de contenu : la page est réécrite par PDFium | Rendu identique dans nos tests ; les pages non touchées ne sont jamais réécrites. |
| Caviardage : tracés vectoriels sous une zone conservés (recouverts) | Le texte, les images et les annotations sont retirés ; les tracés seront découpés au jalon 9. |
| Insertion d'images comme nouvelles pages | Possible via « Créer un PDF » puis insertion du fichier ; menu direct au jalon 5 (Fusion). |
| Masquer un calque n'affecte pas l'impression | L'impression relit le fichier ; prise en compte au jalon 9. |
| Les signatures existantes sont invalidées par toute modification | Comportement normal ; un avertissement est affiché. |
