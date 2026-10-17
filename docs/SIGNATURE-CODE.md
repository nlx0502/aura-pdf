# Signature du code et publication

## Windows : SignPath Foundation (gratuit)

[SignPath Foundation](https://signpath.org) offre aux projets libres un
certificat de signature de code et le service de signature
[SignPath.io](https://signpath.io). L'installateur signé n'affiche plus
« Éditeur inconnu » ; l'avertissement SmartScreen disparaît à mesure que les
téléchargements établissent la réputation du certificat.

Conditions principales : licence libre reconnue par l'OSI (MIT ✓), code
public, paquets compilés par une intégration continue hébergée (GitHub
Actions ✓, `.github/workflows/release.yml`), politique de signature publiée
(section « Politique de signature du code » du README ✓), aucun logiciel
indésirable.

### Étapes (une seule fois)

1. Publier le dépôt sur GitHub (public) et y pousser le code.
2. Demander l'adhésion sur <https://signpath.org/apply> : nom du projet,
   adresse du dépôt, licence MIT, mainteneurs.
3. Une fois accepté, dans SignPath.io :
   - projet **`aura-pdf`** relié au dépôt GitHub (« Trusted build system »
     GitHub.com) ;
   - configuration d'artefact **`installateur`** : un fichier ZIP contenant un
     `*.exe` à signer (signature Authenticode) ;
   - stratégie de signature **`release-signing`** (certificat de la
     Foundation, approbation manuelle d'une personne avant chaque signature) ;
   - un utilisateur CI et son jeton d'API.
4. Dans le dépôt GitHub (Settings › Secrets and variables › Actions) :
   - secret **`SIGNPATH_API_TOKEN`** : le jeton de l'utilisateur CI ;
   - variable **`SIGNPATH_ORGANIZATION_ID`** : l'identifiant de l'organisation.

### Publier une version

```bash
git tag v1.0.0
git push origin v1.0.0
```

La CI compile et teste Windows et macOS, soumet l'installateur à SignPath
(approbation à donner dans SignPath.io), puis **publie la version** sur GitHub
(Releases) avec l'installateur, le DMG et les empreintes SHA-256. Une étiquette
avec tiret (`v1.1.0-beta.1`) est publiée comme préversion.

Tant que SignPath n'est pas configuré, la version est créée avec un
installateur non signé (avertissement dans le journal de la CI).

## macOS : sans compte développeur Apple

L'application est signée « ad hoc » (sans identité Apple) et n'est pas
notariée : macOS demande une confirmation au premier lancement (voir le
README). La notarisation nécessiterait un compte Apple Developer (99 $/an) ;
la CI pourrait alors l'ajouter (`APPLE_CERTIFICATE`, `APPLE_ID`,
`APPLE_PASSWORD`, `APPLE_TEAM_ID`, reconnus par `tauri build`).
