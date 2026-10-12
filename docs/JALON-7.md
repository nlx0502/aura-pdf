# Jalon 7 — Formulaires et Sécurité

Deux modules conformes à `design/project/Forms.dc.html` et
`design/project/Security.dc.html`.

## Formulaires (menu Outils › Formulaires)

| Fonction | Détail |
|---|---|
| Lecture des champs | Formulaires AcroForm : texte, e-mail, date (format `AFDate` d'Acrobat), listes déroulantes et listes, boutons radio, cases à cocher, signatures ; libellé (`/TU`), obligatoire, lecture seule, longueur maximale ; champs hiérarchiques (`parent.enfant`) et champs à plusieurs widgets |
| Saisie sur la page | Les champs se remplissent directement sur le document, à leur place : saisie au clavier, sélecteur de date (enregistré en jj/mm/aaaa), listes, choix, cases ; e-mail invalide signalé ; Tab pour passer d'un champ à l'autre |
| Liste des champs | Type, « obligatoire », pastille d'état (rempli / obligatoire manquant / vide) ; clic = aller au champ |
| Progression | Anneau animé (« 6 sur 8 champs remplis »), nombre de champs obligatoires manquants, « Formulaire complet » à 100 % |
| Ajouter un champ | Texte, E-mail, Date, Liste, Case, **Choix (boutons radio)**, Signature : clic sur la page (taille par défaut) ou glisser pour dimensionner ; halo sur le nouveau champ ; libellé visible ajouté d'office |
| Onglet « Champ » | Libellé (info-bulle), **libellé visible** sur la page (masqué, au-dessus, à gauche, à droite ; « * » ajouté aux champs obligatoires), **texte indicatif** (placeholder), options des listes et des boutons radio (une par ligne, les boutons se disposent tout seuls), obligatoire, lecture seule, plusieurs lignes, nombre de caractères maximal, suppression (Suppr) |
| Onglet « Apparence » | Police (Sans, Serif, Mono), gras, taille (automatique ou fixe), couleur du texte, alignement, **fond** (ou aucun), **contour** (couleur, épaisseur, plein / pointillé / souligné), **symbole des cases** (coche, croix, rond, carré, étoile, losange) |
| Position | Poignée de déplacement et coin de redimensionnement sur le champ sélectionné ; flèches du clavier (Maj : 10 points) ; le libellé suit son champ |
| Aperçu fidèle | Dans le module, les champs ne sont pas dessinés par PDFium : Aura les affiche tels qu'ils seront enregistrés (même police, couleurs, contour, symbole), et chaque réglage se voit immédiatement |
| Enregistrer | Ctrl+S ou bouton : **mise à jour incrémentale** (l'original reste intact en tête du fichier) ; apparence écrite dans les entrées standard (`/MK` fond, contour et symbole, `/BS` épaisseur et style, `/DA` police, taille et couleur, `/Q` alignement) et apparences `/AP` générées ; les champs sans apparence en reçoivent une et `NeedAppearances` est retiré, pour que tous les lecteurs affichent la même chose |
| Aplatir | Confirmation, effet « flash » champ par champ ; les apparences sont dessinées dans le contenu des pages et le formulaire est retiré ; les champs remplis sans apparence (formulaires « NeedAppearances ») reçoivent la leur avant l'aplatissement ; **Annuler** remet l'original |
| Champ de signature | Double-clic : ouvre le module Signer sur ce champ |
| Modifications non enregistrées | Proposées à l'enregistrement à la fermeture de l'onglet ou de l'application, comme les annotations |

## Sécurité (onglet Sécurité)

| Fonction | Détail |
|---|---|
| Mot de passe d'ouverture | Interrupteur : le **cadenas se ferme** ; jauge de robustesse animée (très faible → excellent, suites évidentes pénalisées) ; afficher / masquer ; pour un document déjà protégé, laisser vide conserve le mot de passe actuel |
| Chiffrement | **AES-256** (PDF 2.0, révision 6, recommandé) ou AES-128 ; chiffrement facultatif des métadonnées ; une nouvelle clé de fichier aléatoire à chaque application |
| Permissions | Imprimer, copier le texte, modifier le contenu, annoter, remplir les formulaires, extraire des pages ; mot de passe propriétaire (aléatoire s'il est laissé vide, avec avertissement) |
| Application par Aura | Un document ouvert avec le mot de passe utilisateur respecte les permissions : impression refusée (interface **et** moteur), copie du texte désactivée, annotation, édition, formulaires en lecture seule selon les droits ; le mot de passe propriétaire donne tous les droits ; « Déverrouiller » dans l'onglet Sécurité |
| Documents protégés | **S'annotent, s'éditent (pages et contenu) et se remplissent en restant chiffrés** : même clé, mêmes mots de passe, mêmes permissions |
| Métadonnées | Titre, auteur, sujet, mots-clés (dictionnaire Info et XMP) ; « Tout effacer » |
| Archivage PDF/A | Niveaux 1b, 2b, 3b : métadonnées XMP `pdfaid`, profil de sortie sRGB intégré, retrait du JavaScript et des actions automatiques, annotations imprimables, pièces jointes retirées (1b, 2b), identifiant de fichier ; **polices standard non intégrées remplacées par leurs équivalents Windows métriquement compatibles (Arial, Times New Roman, Courier New), intégrées en sous-ensemble** |
| Contrôle de conformité | Polices intégrées, profil de couleurs (et images CMJN), XMP, contenu dynamique, chiffrement, pièces jointes, transparence (1b) ; chaque point se valide l'un après l'autre ; « corrigé à l'enregistrement » ou nombre d'éléments à corriger |
| Récapitulatif | Ouverture, chiffrement, permissions, métadonnées, archivage ; « Appliquer et enregistrer » avec progression, puis « Document protégé et enregistré » |

Un PDF/A ne pouvant pas être chiffré, l'écran le signale dès qu'un niveau
d'archivage est choisi avec une protection.

## Format et compatibilité

- Chiffrement standard PDF (`/Filter /Standard`) : V5 R6 `AESV3` ou V4 R4 `AESV2`,
  filtres `StdCF`. Les fichiers produits sont relus par PDFium (tests) et par
  Acrobat (voir plus bas). Pas de validation veraPDF (absent du poste).
- Formulaires : champs `/FT /Tx /Ch /Btn /Sig`, apparences `/AP /N`, ressources
  `/DR` du formulaire réutilisées. Les champs ajoutés portent leur type (`/AuraKind`
  pour e-mail et date, plus les actions `AFDate` pour la date dans Acrobat).
- Libellé visible : bouton en lecture seule (`/AuraCaptionOf`) placé sur la page :
  il fait partie du formulaire (pas un commentaire dans Acrobat) et s'aplatit
  avec lui.

## Vérification dans Acrobat

Les fichiers produits par Aura ont été ouverts dans Adobe Acrobat Reader (64 bits)
sur le poste :

| Fichier | Résultat |
|---|---|
| Chiffré AES-256 (R6) | **Un défaut a été trouvé et corrigé** : Acrobat refusait de le déchiffrer (« ne peut pas déchiffrer ce document ») car le dictionnaire de chiffrement n'indiquait pas la longueur de clé (`/Length`) ni celle du filtre `StdCF` ; après correction, il s'ouvre avec le mot de passe (« PROTÉGÉ ») |
| Chiffré AES-128 | S'ouvre avec le mot de passe |
| PDF/A-2b | Reconnu : « Ce fichier revendique la conformité à la norme PDF/A » ; polices intégrées correctement rendues |
| Formulaire rempli et stylé | Valeurs, listes, choix, cases, **styles** (fond, contour pointillé, Times, gras centré), **textes indicatifs**, **libellés visibles**, **groupe de boutons radio créé dans Aura**, symbole « étoile » : tout s'affiche comme dans Aura (Acrobat ajoute par-dessus son propre surlignage bleu des champs, réglable dans ses préférences) |

## Captures

| Capture | Contenu |
|---|---|
| `captures/j7-formulaires.png` | Formulaire ouvert : liste des champs, progression 29 %, propriétés |
| `captures/j7-remplissage.png` | Champs remplis au clavier, choix, case, date ; 86 %, tous les obligatoires remplis |
| `captures/j7-ajout.png` | Champ « E-mail du conseiller » ajouté et stylé (fond lavande, contour violet pointillé, Serif, texte indicatif, libellé visible « * »), déplacé au clavier ; groupe de boutons radio à trois options |
| `captures/j7-enregistre.png` | Après enregistrement : le rendu est identique (relu du fichier) |
| `captures/j7-aplatissement.png`, `j7-aplati.png` | Aplatissement, puis valeurs devenues du contenu de page ; « Annuler » |
| `captures/j7-securite.png` | Écran Sécurité d'un document non protégé |
| `captures/j7-securite-reglages.png` | Cadenas fermé, jauge « Excellent », AES-256, 4 permissions sur 6, métadonnées |
| `captures/j7-securite-application.png`, `j7-securite-applique.png` | Application puis « Document protégé et enregistré » |
| `captures/j7-securite-restreint.png` | Réouverture avec le mot de passe utilisateur : droits restreints, déverrouillage |
| `captures/j7-pdfa-verification.png`, `j7-pdfa.png`, `j7-pdfa-conforme.png` | Contrôle PDF/A-2b avant (corrigé à l'enregistrement) et après conversion (conforme) |

## Tests

- Rust (98 au total, 11 nouveaux, 3 exports manuels pour Acrobat) : chiffrement AES-256 (illisible sans mot de
  passe, mauvais mot de passe refusé, permissions, mot de passe propriétaire
  reconnu, retrait de la protection) ; AES-128 avec permissions seules ;
  **édition et annotation d'un document chiffré** (AES-128 et AES-256 : reste
  chiffré, commentaire absent en clair du fichier) ; PDF/A-2b (refus avec
  chiffrement, conformité complète après conversion, texte toujours extractible) ;
  **documents chiffrés ouverts par PDFium** (mot de passe exigé, droits utilisateur
  et propriétaire) ; formulaires : lecture des 7 types, remplissage + création +
  suppression + propriétés en mise à jour incrémentale, aplatissement, rendu
  PDFium d'un formulaire aplati.
- TypeScript (45, 6 nouveaux) : robustesse des mots de passe, conflit PDF/A et
  chiffrement, progression, dates, e-mails.
- Scénario réel : `scripts/e2e-forms-security.mjs` (saisie au clavier, ajout et
  enregistrement d'un champ, aplatissement et annulation, protection AES-256,
  réouverture avec droits restreints, PDF/A-2b conforme). Les scénarios des
  jalons 4 à 6 passent toujours. Nouveau PDF de test : `Demande d’ouverture.pdf`.

## Bogues trouvés et corrigés

- Page blanche après l'aplatissement (et après tout remontage de la vue sur un
  document déjà affiché) : la génération de vue repartait de 1 alors que le
  moteur avait mémorisé une génération plus élevée ; elle est désormais conservée
  par document.
- Valeurs perdues à l'aplatissement des formulaires sans apparences.
- Ctrl+S déclenchait aussi « Aucune modification à enregistrer » dans Formulaires.
- pdfium-render ne reconnaît pas la révision 6 (AES-256) : le document était vu
  comme non chiffré ; l'état et les droits sont désormais lus par lopdf.
- Résultat du contrôle PDF/A non réinitialisé après l'enregistrement.
- Fichiers AES-256 refusés par Acrobat (voir « Vérification dans Acrobat »).
- Formulaires « NeedAppearances » : Acrobat recalculait les apparences et
  perdait textes indicatifs et fonds ; les apparences d'Aura sont désormais
  complètes et ce drapeau est retiré.

## Écarts restants

| Écart | Raison / suite |
|---|---|
| Signer un document protégé | Le module Signer demande de retirer d'abord la protection (le dictionnaire de signature doit rester en clair dans un fichier chiffré). Jalon 9. |
| Fusion, division, compression de fichiers protégés | Refusées avec un message clair (retirer la protection d'abord). |
| Coins arrondis, dégradés, polices autres que les trois standard dans les champs | Le format PDF des champs ne décrit que les polices standard et des contours droits (Acrobat régénère l'apparence à partir de ces entrées) ; polices installées au jalon 9. |
| Libellé visible déplaçable indépendamment du champ | Il suit son champ ; position au-dessus / à gauche / à droite. |
| Calculs et validations JavaScript des formulaires | Non exécutés (aucun JavaScript n'est exécuté par Aura, par sécurité) ; formulaires XFA non pris en charge. |
| Apparences des valeurs saisies | Helvetica, jeu de caractères latin (WinAnsi) : les caractères hors latin sont remplacés par « ? » dans l'apparence (la valeur elle-même est conservée intacte). |
| PDF/A : transparence (1b), polices non standard non intégrées, images CMJN | Signalées par le contrôle, non corrigées automatiquement. Contrôle interne et reconnaissance par Acrobat ; pas de validation veraPDF (hors ligne). La conversion intègre les polices, ce qui augmente la taille (≈ 140 Ko pour Arial en sous-ensemble). |
| Chiffrement par certificat (clé publique) | Non pris en charge ; mot de passe uniquement. |
| Permissions | Comme pour tout lecteur PDF, elles reposent sur le respect du logiciel qui ouvre le fichier ; seul le mot de passe d'ouverture chiffre réellement le contenu. |
