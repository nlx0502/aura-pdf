# Jalon 8 — Conversion, OCR, création depuis Office, scanner et presse-papiers

Outil « Convertir et OCR » conforme à `design/project/Convert.dc.html`, et
sources de création de l'accueil (Document Office, Scanner, Presse-papiers).
Tout est traité sur le poste (« Traitement 100 % local »).

## Convertir

| Sens | Format | Détail |
|---|---|---|
| Depuis un PDF | Word (.docx) | **Mise en page conservée** : une ligne du PDF = un paragraphe avec son retrait, son espacement, ses taquets de colonnes, polices (sans, serif, mono), tailles, gras, italique, couleurs ; sauts de page. **Texte continu** (option désactivée) : paragraphes reconstitués, césures de fin de ligne retirées, titres repérés à leur taille, lignes de tableau préservées |
| | Excel (.xlsx) | Une feuille par page ; colonnes retrouvées par l'alignement ; nombres convertis en valeurs (formats français « 1 234,56 », « 12,5 % », « 1 200 € », négatifs entre parenthèses) ; en-têtes en gras ; largeurs de colonnes ajustées |
| | Images | PNG ou JPEG, 150, 300 ou 600 ppp ; une image par page (dossier « Nom (images) ») |
| | Texte brut | Lignes et paragraphes ; avec la mise en page conservée : retraits et sauts de page |
| | Options | « Appliquer l'OCR si nécessaire » : les pages numérisées sont reconnues avant l'export ; « Un fichier par page » |
| Vers un PDF | Images | Une page par image, aux proportions exactes de l'image |
| | Word, Excel, PowerPoint | **Microsoft Office** (s'il est installé) produit le PDF : rendu identique à l'original ; sinon conversion intégrée des documents Word .docx (paragraphes, styles de titre, alignements, retraits, listes, police, taille, gras, italique, souligné, couleurs, tableaux, images, sauts de page ; polices installées intégrées) |

- Flux animé du format source vers le format cible, cartes de formats, file de
  fichiers avec progression par fichier et par page, « Ajouter » ou dépôt de
  fichiers, annulation, « Ouvrir » (PDF dans Aura ; Word, Excel, dossier dans
  l'application Windows correspondante).
- Le rendu et l'extraction passent par le moteur, une page à la fois, entre deux
  rendus à l'écran : l'interface reste fluide pendant une longue conversion.

## OCR (Tesseract, hors ligne)

- Tesseract 5.5 **lié statiquement** (pas de processus externe ni de DLL) ;
  langues fournies : français, anglais, allemand, néerlandais (`tessdata_fast`).
- Écran : page numérisée réelle avec le **faisceau qui balaie la page**, cadres
  sur les lignes trouvées, **texte reconnu qui apparaît ligne par ligne**,
  confiance ; navigation entre les pages après la reconnaissance.
- **PDF recherchable** : texte invisible posé exactement sur les mots de l'image
  (recherche, sélection, copie) — fichier « Nom (OCR).pdf » ; un document
  protégé reste chiffré. Sinon : texte reconnu enregistré en .txt.
- **Redresser les pages** : l'inclinaison détectée par Tesseract est corrigée
  (rotation du contenu de la page, texte invisible compris).
- Les pages qui contiennent déjà du texte ne sont pas reconnues à nouveau.

## Créer un PDF (accueil)

| Source | Détail |
|---|---|
| Document Office | Word, Excel, PowerPoint (par Office) ; .docx sans Office |
| Scanner | Boîte de numérisation de Windows (WIA), plusieurs pages, OCR facultatif (activé par défaut) ; messages clairs : aucun scanner, scanner hors ligne, annulation |
| Presse-papiers | Image copiée (capture d'écran…) ou texte copié → PDF |
| Images | OCR désormais disponible (texte recherchable) |

## Captures

| Capture | Contenu |
|---|---|
| `captures/j8-convertir.png` | Depuis un PDF : document actif dans la file, format Word |
| `captures/j8-conversion.png`, `j8-converti.png` | Flux animé, progression, « 1 fichier converti en Word » |
| `captures/j8-vers-pdf.png` | Vers un PDF : document Word, Office détecté |
| `captures/j8-ocr.png`, `j8-ocr-balayage.png`, `j8-ocr-texte.png` | OCR : page numérisée, balayage, lignes encadrées et texte reconnu (confiance 95 %) |
| `captures/j8-creer-doc.png`, `j8-creer-scan.png`, `j8-creer-clip.png` | Nouvelles sources de « Créer un PDF » |

## Tests

- Rust (106 au total, 8 nouveaux) : lecture des nombres (formats français et
  anglais) ; texte brut ; Word en deux modes puis **rendu natif du .docx en PDF** ;
  Excel (valeurs, pourcentages, en-têtes) ; **conversion réelle du rapport** de
  14 pages en texte, Word, Excel, PNG puis retour en PDF par **Microsoft Word** ;
  Tesseract (version, langues, page blanche) ; **OCR d'une page numérisée
  inclinée** : mots accentués reconnus, PDF recherchable relu par PDFium, page
  redressée, conversion en texte avec OCR automatique.
- Scénario réel : `scripts/e2e-convert.mjs` (Word, Excel, texte, images, Word →
  PDF par Office, OCR avec aperçu, PDF recherchable). Les scénarios des jalons 4
  à 7 passent toujours. Nouveau PDF de test : `Scan contrat.pdf` (page
  numérisée à 200 ppp, inclinée de 1,6°, bruitée).

## Formulaires (retours sur le jalon 7)

Le module Formulaires a été revu : mise en forme complète des champs (police,
taille, couleurs, fond, contour plein / pointillé / souligné, symbole des cases),
libellés visibles, textes indicatifs, boutons radio, déplacement et
redimensionnement, aperçu fidèle ; résultats vérifiés dans Acrobat (voir
`JALON-7.md`, qui détaille aussi la correction du chiffrement AES-256 pour
Acrobat).

## Bogues trouvés et corrigés

- Mode « texte continu » : les rangées d'un tableau étaient fusionnées en un
  seul paragraphe.
- En-têtes à deux colonnes collés dans Word : taquets positionnés.
- Tesseract ne lisait pas ses données depuis un chemin Windows étendu (`\\?\`).
- Scanner déclaré mais éteint : message brut illisible, remplacé par « Le
  scanner est hors ligne ».

## Écarts restants

| Écart | Raison / suite |
|---|---|
| Images et dessins vectoriels (graphiques) non repris dans Word et Excel | Seul le texte est exporté ; les images seront ajoutées au jalon 9. |
| Conversion intégrée .docx (sans Office) : zones de texte, colonnes, en-têtes et pieds de page, notes, champs non rendus | Office, quand il est installé, est utilisé et produit un rendu identique. |
| .doc, Excel et PowerPoint sans Office | Non convertis (message clair) ; formats binaires anciens. |
| OCR : langues fournies limitées à quatre ; pas de détection automatique de l'orientation (pages à 90°) | Données d'autres langues ajoutables dans `resources/tessdata` ; orientation au jalon 9. |
| Texte OCR invisible en Helvetica (jeu latin) | Les caractères hors latin sont remplacés dans le calque invisible. |
| Scanner : pas de chargeur multipage automatique (une page par numérisation) | La boîte de dialogue de Windows numérise une page à la fois ; « Numériser une autre page » enchaîne. Non testé avec un vrai scanner (aucun en ligne sur le poste). |
| Taille de l’installateur | 17,4 Mo (contre 9,9 Mo) : Tesseract et ses données de langue. |
