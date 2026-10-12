# Jalon 5 — Outils : fusion, division, compression

Outils du menu **Outils**, conformes à `design/project/Merge.dc.html` et
`design/project/Compress.dc.html`. Ils fonctionnent avec ou sans document
ouvert (le document actif sert de point de départ) ; les traitements tournent
hors de l'interface, avec progression et bouton Annuler.

## Ce qui est livré

### Fusionner

| Fonction | Détail |
|---|---|
| Liste des fichiers | Ajout par le bouton ou par glisser-déposer depuis l'Explorateur ; PDF et images (une page A4 par image) ; doublons ignorés ; fichiers protégés refusés avec un message clair |
| Ordre | Glisser la poignée (les autres fichiers s'écartent, puis glissent à leur place), boutons monter / descendre, retirer |
| Résultat | Nombre de pages et taille, nom du fichier (jamais d'écrasement : « nom (2).pdf »), dossier du premier fichier |
| Un signet par fichier | Les signets d'origine de chaque fichier sont repris dessous, avec leurs bonnes pages |
| Table des matières | Page « Sommaire » en tête : nom de chaque fichier, points de conduite, numéro de page et lien |
| Uniformiser au format A4 | Chaque page mise à l'échelle et centrée sur un A4 de même orientation (annotations et liens suivent) |
| Suivi | Fichier en cours entouré, coches au fur et à mesure, « Dossier complet.pdf est prêt — 22 pages » et bouton Ouvrir |

### Diviser

| Fonction | Détail |
|---|---|
| Par pages | Plages saisies (« 1-4, 5-10, 11 »), vérifiées en direct |
| Par taille | Limite en Mo ; le découpage est calculé par le cœur à partir des objets réellement utilisés par chaque page (polices et images partagées comptées une fois par fichier) |
| Par signets | Un fichier par chapitre ; chapitres cochables ; fichiers nommés d'après le signet |
| Aperçu coloré | Une case par page, couleur de la partie, mise à jour en direct |
| Résultat | Liste des fichiers (nom, plage), création dans le dossier du document, bouton Afficher ; chaque partie garde ses signets (filtrés), liens, pièces jointes et métadonnées |

### Compresser

| Fonction | Détail |
|---|---|
| Analyse | Poids réel par catégorie (images, polices, contenu, métadonnées), résolution affichée de chaque image (taille la plus grande à laquelle elle est dessinée) |
| Niveaux | Légère, Recommandée, Forte, Personnalisée, avec la taille estimée pour chacun |
| Estimation | Anneau de réduction, taille animée en chiffres, barres avant / après par catégorie ; après traitement, les barres montrent la taille obtenue |
| Images | Rééchantillonnage à la résolution maximale (selon la taille affichée), qualité JPEG, niveaux de gris ; schémas et captures (peu de couleurs) gardés sans perte ; un flux n'est remplacé que s'il est plus léger |
| Polices | Sous-ensembles des polices TrueType composites intégrées en entier ; retrait des polices standard (Helvetica, Times, Courier…) intégrées inutilement |
| Nettoyage | Métadonnées, miniatures, fichiers joints ; flux identiques fusionnés ; objets orphelins supprimés ; structure compacte (flux d'objets) |
| Étapes | Analyse → images (n / N) → polices → nettoyage → écriture, puis « Enregistré · 17 Ko » et Ouvrir ; le fichier est écrit à côté de l'original (« nom (compressé).pdf ») |
| Garantie | Le fichier produit n'est jamais plus lourd que l'original (sinon l'original est conservé tel quel, sauf nettoyage demandé) |

## Captures

| Capture | Contenu |
|---|---|
| `captures/j5-fusion.png` | Trois fichiers à fusionner, panneau Résultat |
| `captures/j5-fusion-glisser.png` | Fichier déplacé par sa poignée |
| `captures/j5-fusion-terminee.png` | Fusion terminée (coches, bouton Ouvrir) |
| `captures/j5-division.png`, `j5-division-signets.png` | Division par plages et par signets, aperçu coloré |
| `captures/j5-division-terminee.png` | Trois fichiers créés |
| `captures/j5-compresser.png` | Niveau « Forte » : estimation, anneau, barres |
| `captures/j5-compresser-etapes.png`, `j5-compresser-termine.png` | Étapes, puis résultat réel |

## Tests

- Rust (84 au total, 6 nouveaux) : fusion avec signets imbriqués, sommaire et
  A4 ; division par plages (signets filtrés, pas d'écrasement) et par taille ;
  analyse (résolution affichée, somme des catégories = taille du fichier) ;
  compression (image 2000 px ramenée à 417 px pour 150 ppp, fichier plus de
  10 fois plus léger, métadonnées retirées, signets conservés) ; **rendu PDFium
  du fichier compressé** (JPEG produit vérifié au pixel).
- TypeScript (36, 3 nouveaux) : plages, coupures par signets, estimation
  (monotone selon le niveau).
- Scénario réel à la souris : `scripts/e2e-tools.mjs` (ajout, glisser-déposer,
  fusion relue, division relue, compression analysée).

## Bogues trouvés et corrigés

- Fichier compressé plus lourd que l'original (image régulière déjà bien
  compressée remplacée par un JPEG plus gros) — détecté par les tests.
- Document ouvert ajouté deux fois à la liste de fusion (double exécution au
  démarrage) — détecté par le scénario.
- Conflit de nom de classe CSS avec la barre d'outils d'annotation.

## Écarts restants

| Écart | Raison / suite |
|---|---|
| « Optimiser pour l'affichage web » remplacé par « Structure compacte (flux d'objets) » | La linéarisation n'est pas disponible dans lopdf ; elle sera ajoutée au jalon 9. |
| Documents Word dans la fusion | Conversion Office au jalon 8. |
| Fichiers protégés par mot de passe refusés | Module Sécurité (jalon 7). |
| Formulaires (AcroForm) non conservés dans une fusion | Les champs restent visibles (apparences) mais ne sont plus remplissables ; fusion des formulaires au jalon 7 (module Formulaires). |
| Sous-ensembles : polices TrueType composites uniquement | Les polices simples et Type 1/CFF intégrées entières sont gardées telles quelles. |
| Estimation de taille approchée | Calculée à partir de l'analyse réelle ; la taille obtenue est affichée après traitement. |
