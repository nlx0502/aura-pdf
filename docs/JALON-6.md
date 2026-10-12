# Jalon 6 — Signer : signature manuscrite et numérique

Module Signer conforme à `design/project/Sign.dc.html` : bannière d'état en haut
du document, pages réelles avec les champs de signature, panneau « Votre
signature », « Signature numérique » et « Signatures du document ».

## Ce qui est livré

| Fonction | Détail |
|---|---|
| Dessiner | Vrai pad de dessin (souris, trackpad, stylet), tracé lissé ; une signature d'exemple se dessine dans le pad vide ; Effacer |
| Taper | Le nom en trois styles manuscrits (Caveat, Homemade Apple, Mrs Saint Delafield), polices fournies avec l'application et intégrées au PDF en sous-ensemble |
| Importer | Image (PNG transparent conseillé) : la transparence est conservée dans le PDF |
| Encre | Bleu encre, noir, bleu, ou toute couleur |
| Placement | Clic sur un champ de signature (« Cliquez pour signer ») ou n'importe où sur la page ; emplacement déplaçable et redimensionnable ; l'aperçu est dessiné selon les mêmes règles que le PDF (ce qu'on voit est ce qu'on obtient) |
| Signature numérique | Certificat `.pfx` / `.p12` (RSA ou ECDSA P-256 / P-384, formats PKCS #12 récents et anciens), mot de passe, motif facultatif, mention « Signé numériquement par … · Date » sous la signature |
| Certificat personnel | Création en un clic (nom, e-mail, organisation, mot de passe) : certificat auto-signé ECDSA P-256 valable 3 ans, enregistré en `.p12` ; le dernier certificat utilisé est mémorisé (jamais le mot de passe) |
| Horodatage qualifié | RFC 3161, serveur réglable (DigiCert par défaut) ; seule l'empreinte est envoyée ; désactivé par défaut (l'application reste hors ligne) |
| Étapes | Lecture du certificat → empreinte SHA-256 → horodatage → intégration au PDF, puis effet « encre » sur la signature posée |
| Sans certificat | Signature manuscrite apposée (annotation verrouillée, imprimée) et signalée « Manuscrite » : elle n'atteste pas l'identité |
| Vérification | Pour chaque signature : intégrité du document (empreinte des plages signées), signature cryptographique, chaîne de confiance jusqu'aux autorités racines de Windows, validité du certificat à la date de signature, horodatage, ajouts ultérieurs ; bouton « Vérifier » ; clic = aller à la page |
| Bannière | « Votre signature est requise » (champs vides), « Toutes les signatures sont valides », « Signatures intactes » (identité non certifiée), « Une signature n'est pas valide », compteur « 1 / 2 signatures » |

## Format et compatibilité

- Signature PDF standard : champ `/FT /Sig`, dictionnaire `/Type /Sig` avec
  `/Filter /Adobe.PPKLite`, `/SubFilter /adbe.pkcs7.detached`, `/ByteRange`
  couvrant tout le fichier hors du conteneur, conteneur CMS (attributs signés :
  type de contenu, date, empreinte SHA-256 ; chaîne de certificats ; jeton
  d'horodatage en attribut non signé) — lisible par Acrobat et les autres lecteurs.
- **Mise à jour incrémentale** : le fichier d'origine n'est jamais réécrit ; on
  peut co-signer, et les signatures précédentes restent valides (elles signalent
  simplement l'ajout ultérieur).
- Champ existant : la signature remplit le champ (valeur et apparence) ;
  sinon un nouveau champ est créé à l'endroit choisi (`/SigFlags 3`).
- **Vérification indépendante** : les signatures produites par Aura sont
  validées par OpenSSL (`openssl cms -verify`), avec des certificats RSA générés
  par OpenSSL aux formats PKCS #12 moderne (AES) et ancien (3DES / RC2), et avec
  horodatage DigiCert.

## Captures

| Capture | Contenu |
|---|---|
| `captures/j6-signer.png` | Document à signer : bannière, deux champs « Cliquez pour signer » |
| `captures/j6-placement.png` | Signature dessinée placée dans le champ « Société B », certificat choisi |
| `captures/j6-etapes.png`, `j6-signe-encre.png` | Étapes de la signature numérique, effet encre |
| `captures/j6-verification.png`, `j6-signe.png` | Vérification, document entièrement signé |

## Tests

- Rust (87 au total, 4 nouveaux + 1 manuel) : certificat personnel (création,
  relecture, mauvais mot de passe refusé, auto-signé non reconnu par Windows) ;
  signature numérique (fichier d'origine intact en tête, signataire, motif,
  page et position relus) ; **co-signature** (la première reste intacte et
  signale l'ajout) ; **falsification détectée** (un octet modifié → invalide) ;
  signature manuscrite ; remplissage d'un champ existant. Test manuel
  `export_pour_openssl` pour la vérification croisée par OpenSSL.
- TypeScript (39, 3 nouveaux) : tracé lissé, boîte englobante, bilan.
- Scénario réel : `scripts/e2e-sign.mjs` (dessin à la souris, certificat créé,
  champ rempli, vérification, seconde signature manuscrite). Nouveau PDF de
  test : `Accord de consortium.pdf` (deux champs de signature).

## Bogues trouvés et corrigés

- Signature manuscrite posée sur un champ : le champ restait « à remplir ».
- Effet « encre » décalé par rapport au rendu final (mention non réservée dans
  l'aperçu) : l'aperçu suit désormais exactement la mise en page du PDF.
- Pastilles de couleur sombres invisibles sur le fond sombre (liseré ajouté).

## Écarts restants

| Écart | Raison / suite |
|---|---|
| Documents protégés par mot de passe | Avec le module Sécurité (jalon 7). |
| Certificats du magasin Windows et cartes à puce | Seuls les fichiers `.pfx` / `.p12` sont pris en charge ; magasin Windows au jalon 9. |
| Révocation (OCSP, CRL) non vérifiée | La vérification de révocation demande Internet ; prévue en option au jalon 9. |
| Signature PAdES de base (adbe.pkcs7.detached, sans attribut ESS) | Accepté par Acrobat ; profils PAdES B-T/B-LT au jalon 9. |
| Pas de certification (DocMDP) ni de verrouillage du document après signature | Jalon 9. |
| Crate `rsa` : avis RUSTSEC-2023-0071 (attaque temporelle « Marvin ») | Concerne un attaquant qui mesure à distance de nombreuses opérations ; sans objet pour une signature faite sur le poste. Mise à jour vers `rsa` 0.10 dès sa version stable. |
| Polices de signature : alphabet latin | Les trois polices manuscrites couvrent le latin étendu. |
