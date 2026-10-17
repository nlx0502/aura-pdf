# Jalon 9 — Finitions : accessibilité, performances, installateur 1.0.0

Audits d'accessibilité et de performances, installateur définitif, et les
fonctions reportées des jalons précédents les plus utiles.

## Accessibilité

Audit automatique (`scripts/audit-a11y.mjs`, axe-core 4.10, règles WCAG 2.1 AA)
sur 14 écrans (accueil, lecture, annoter, éditer, signer, sécurité, formulaires,
outils, convertir, OCR, impression, boîtes de dialogue) : **0 règle en défaut**.

Corrections apportées :

- **Contraste ≥ 4,5:1** : bouton principal sur `--accent-btn` (#6662F0), badges
  de statut assombris (#B83A33, #11703F, #A8431F), textes discrets.
- **Rôles ARIA** cohérents : `aria-checked` pour les choix exclusifs
  (`role=radio`), `aria-selected` pour les vignettes (`role=option`), liste
  d'onglets (`role=tablist`), pages (`role=group`), liens internes nommés.
- Parcours au clavier (`scripts/audit-keyboard.mjs`) : chaque élément atteint
  par Tab porte l'anneau de focus `#A9A6FF` ; aucun piège. Le champ « page n »
  de la barre de navigation a reçu son anneau.
- Mouvements réduits : toutes les animations respectent `prefers-reduced-motion`.

## Performances (« Gros document.pdf », 600 pages)

| Mesure | Version de développement | Version publiée (release) |
|---|---|---|
| Fenêtre prête (lancement → interface) | — | 0,75 s |
| Ouverture du document | 90 à 115 ms | première page affichée en 35 ms |
| Saut vers une page lointaine | 30 à 55 ms | — |
| Défilement continu | ≈ 139 img/s, 0 % de pages blanches | ≈ 144 img/s |
| Recherche dans les 600 pages | 0,72 s (moteur seul : 225 ms) | — |
| Mémoire (arbre de processus, WebView2 compris) | 700 à 1 000 Mo | 408 Mo à l'accueil, ≈ 890 Mo après défilement des 600 pages |

Optimisations : résultats de recherche regroupés (au plus un envoi toutes les
80 ms, et par image côté interface), boîtes de caractères calculées pour les
seuls résultats, cache de rendu plafonné à 192 Mo.

## Fonctions ajoutées

| Domaine | Fonction |
|---|---|
| Onglets | **Réordonner par glisser** ; au clavier : Alt+Maj+← / → |
| Annoter | **Poignées de redimensionnement** (8 poignées ; extrémités des lignes et flèches ; Maj : proportions ; tampons proportionnels) |
| Impression | Les **calques masqués ne sont pas imprimés** (copie d'impression sans leur contenu) |
| Signature | **Profil PAdES** (ETSI.CAdES.detached, attribut signing-certificate-v2) par défaut ; **certification du document** (DocMDP : aucune modification, formulaires et signatures, ou formulaires, signatures et annotations) ; vérifiées par Aura, OpenSSL (`cms -verify`) et reconnues par Acrobat |
| Signature | **Certificats du magasin Windows**, y compris **cartes à puce** (eID…) : la clé reste dans la carte, Windows demande le code PIN |
| Export Word | **Illustrations** : images et graphiques vectoriels insérés à leur place (PNG 150 ppp), sans dupliquer leurs légendes ; tableaux et encadrés de texte restent du texte |
| OCR | **Orientation** : pages numérisées de travers (¼, ½, ¾ de tour) lues dans le bon sens et remises à l'endroit |

## Installateur 1.0.0

`Aura PDF_1.0.0_x64-setup.exe` (≈ 17,5 Mo, NSIS, en français) :

- bandeau et en-tête aux couleurs d'Aura (`scripts/make_installer_assets.py`) ;
- **page de licence** en français (MIT, composants tiers) ; `LICENSE.txt` et
  `THIRD-PARTY-LICENSES.md` installés avec l'application ;
- installation pour l'utilisateur ou pour tous, dossier du menu Démarrer,
  raccourci de bureau, WebView2 installé si nécessaire ;
- association des fichiers .pdf, Aura PDF inscrit dans les « Applications par
  défaut » de Windows ; l'installateur **demande s'il faut en faire
  l'application PDF par défaut** et ouvre alors la page des Paramètres où
  Windows fait confirmer ce choix (depuis Windows 10, un programme ne peut plus
  se déclarer par défaut lui-même) ;
- la désinstallation retire ces inscriptions.

## Captures

| Capture | Contenu |
|---|---|
| `captures/j9-certifier.png` | Signature numérique : certification du document, modifications autorisées |

## Tests

- Rust : 112 tests (5 ignorés, manuels) ; nouveaux : PAdES et certification,
  signature par CNG (comme pour une carte à puce, clé importée en mémoire sans
  toucher au magasin de l'utilisateur), lecture du magasin, illustrations de
  l'export Word, orientation OCR (90°, 180°, 270°), impression selon les calques.
- Interface (Vitest) : 46 tests, dont le redimensionnement des annotations.
- Scénarios de bout en bout : tous rejoués (lecture, annoter, éditer, contenu
  et calques, texte, polices, outils, signer avec certification, formulaires et
  sécurité, conversion) ; audits d'accessibilité et du clavier.

## Écarts restants

- **Révocation** des certificats (OCSP, CRL) non vérifiée, donc pas de profil
  PAdES B-LT/B-LTA (validation à long terme).
- Le respect de la certification DocMDP par les modifications ultérieures n'est
  pas contrôlé à la vérification (Acrobat le fait).
- **Linéarisation** (« affichage web rapide ») non produite.
- **Signature d'un document chiffré** : il faut d'abord retirer la protection.
- **Caviardage** : les tracés vectoriels sont retirés en entier s'ils touchent
  la zone (pas de découpe de leur géométrie).
- Conversion Word → PDF intégrée (sans Office) : les illustrations ajoutées à
  l'export Word sont ignorées par ce moteur de rendu simplifié.
- Signature avec une carte à puce réelle : le chemin CNG est testé, pas avec une
  carte physique (code PIN demandé par Windows).
- Installateur non signé (pas de certificat de signature de code) : Windows
  SmartScreen peut afficher un avertissement au premier lancement.
