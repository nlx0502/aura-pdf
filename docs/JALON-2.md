# Jalon 2 — Impression complète

Fenêtre d'impression accessible partout (Ctrl+P, bouton Imprimer, menu Outils,
palette de commandes), reproduisant `AppBar.dc.html`.

## Architecture

```
src-tauri/src/print/
  layout.rs    Imposition (fonctions pures, testées) : pages à imprimer, taille,
               affiche, multiple, brochure → feuilles et positions des pages
  compose.rs   PDF imposé vectoriel : copie privée du document, filtrage des
               annotations, aplatissement, placement des pages (XObject),
               cadres, repères de découpe, résumé des commentaires
  windows.rs   Imprimantes Windows (liste, état, capacités, marges) et envoi GDI
               sur un thread dédié
  mod.rs       Tâche d'impression pas à pas (composition → résumé → envoi/enregistrement)
src/screens/print/PrintDialog.tsx   Fenêtre d'impression
src/lib/print.ts                    Types et commandes
```

- **Une seule source de vérité** : l'aperçu et l'impression réelle utilisent la
  même imposition, calculée par le cœur Rust. L'aperçu affiche les vraies pages.
- **« Enregistrer au format PDF »** produit un PDF **vectoriel** (texte net et
  recherchable), pas une image.
- **Impression réelle** : chaque feuille du PDF imposé est rendue à la
  résolution de l'imprimante (300 ppp max.) et transmise au pilote par GDI
  (papier, orientation feuille par feuille, recto verso, couleur).
- **Interface jamais bloquée** : la tâche avance par petites étapes entre deux
  rendus de page ; l'envoi au pilote se fait sur un thread séparé (certains
  pilotes ouvrent une fenêtre pendant `StartDoc`). Progression continue,
  annulation à tout moment (le document en cours est abandonné proprement).

## Fonctionnalités

| Rubrique | Réglages |
|---|---|
| Imprimante | Imprimantes Windows avec état (Prête, Hors ligne, Toner faible, Plus de papier…), « Enregistrer au format PDF », copies, assemblage |
| Pages | Toutes, page actuelle, plage personnalisée (`1-5, 8, 11-13`, `8-`) avec validation ; toutes / impaires / paires ; ordre inverse |
| Taille | Ajuster (à la zone imprimable réelle), taille réelle, réduire si trop grande, échelle personnalisée |
| Affiche | Agrandissement 125–400 %, découpe en tuiles selon la zone imprimable, repères de découpe |
| Multiple | 2, 4, 6 ou 9 pages par feuille, orientation automatique, bordures |
| Brochure | Imposition correcte (pages complétées à un multiple de 4), reliure à gauche ou à droite, recto verso bord court activé automatiquement |
| Papier | A4, A3, Lettre US ; orientation auto (feuille par feuille), portrait, paysage |
| Impression | Recto verso bord long / court (désactivé si l'imprimante ne le gère pas), couleur ou niveaux de gris |
| Commentaires | Document seul (champs compris), avec annotations, avec tampons ; résumé des commentaires en fin de document |
| Aperçu | Mis à jour à chaque réglage, « Feuille X sur Y · recto/verso », animation d'alimentation pendant l'envoi |
| Pied | Feuilles × copies × pages ; « Préparation de la feuille X sur Y… », « Envoi de la feuille X sur Y… », « Fichier PDF enregistré » / « Envoyé à … » |

Les derniers réglages (imprimante, mode, papier…) sont mémorisés.

## Captures

| Capture | Contenu |
|---|---|
| `captures/j2-impression.png` | Fenêtre d'impression, mode Taille, imprimante réelle « Hors ligne » |
| `captures/j2-multiple.png` | 4 pages par feuille avec bordures |
| `captures/j2-brochure.png` | Brochure (première face : page vierge + page 1) |
| `captures/j2-affiche.png` | Affiche 200 % en 3 × 3 tuiles (marges de l'imprimante) |
| `captures/j2-progression.png` | Enregistrement d'un PDF de 600 pages en cours |
| `captures/j2-termine.png` | Terminé, bouton « Ouvrir » |
| `captures/j2-sortie-gdi.png` | Sortie réelle du spouleur Windows (Microsoft Print to PDF) |

## Tests

- 12 nouveaux tests Rust (42 au total) : plages, sous-ensembles, ajustement,
  orientation automatique, 6 par feuille, brochure (ordre des pages, reliure,
  pages vierges), affiche en tuiles, feuilles physiques en recto verso, ordre
  d'envoi (assemblage, recto verso impair), filtrage des annotations, et de bout
  en bout : PDF 4 par feuille avec résumé (texte vectoriel vérifié), brochure
  avec page `/Rotate 90`, **impression GDI réelle vers « Microsoft Print to PDF »**
  (8 faces, orientation feuille par feuille), annulation d'un document de
  600 pages, plage invalide refusée.
- Performances : 600 pages en 4 par feuille (150 feuilles) enregistrées en ~5 s.

## Bogues trouvés et corrigés

- Pages `/Rotate` imprimées à l'envers (rotation appliquée deux fois).
- Retour au portrait oublié après une feuille en paysage (impression GDI).
- Clignotement à l'ouverture du menu Outils (flou d'arrière-plan animé).
- Colonne de réglages qui faisait défiler l'aperçu.

## Écarts restants

| Écart | Raison / suite |
|---|---|
| Fichier PDF : copies, recto verso et niveaux de gris ignorés | Sans objet pour un fichier ; la fenêtre l'indique. |
| Résolution d'impression plafonnée à 300 ppp | Compromis mémoire (une A3 à 600 ppp = 250 Mo par feuille). |
| Tuiles d'affiche sans recouvrement | Repères de découpe aux coins de la zone imprimable ; un recouvrement réglable pourra être ajouté. |
| Pages dont le contenu dépasse la CropBox | Le contenu est limité à la MediaBox de la page source. |
| Impression directe hors Windows | Module `print/windows.rs` à décliner pour macOS (CUPS). |
