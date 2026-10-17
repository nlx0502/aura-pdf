; Crochets de l'installateur NSIS d'Aura PDF (inclus par le modèle de Tauri).
;
; Application PDF par défaut : depuis Windows 10, un programme ne peut plus se
; déclarer lui-même par défaut ; c'est l'utilisateur qui confirme dans les
; Paramètres. L'installateur enregistre donc Aura PDF dans la liste des
; « Applications par défaut », puis propose d'ouvrir directement sa page.

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\Aura PDF\Capabilities" "ApplicationName" "Aura PDF"
  WriteRegStr SHCTX "Software\Aura PDF\Capabilities" "ApplicationDescription" "Lire, annoter, signer et convertir vos PDF, hors ligne."
  WriteRegStr SHCTX "Software\Aura PDF\Capabilities" "ApplicationIcon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr SHCTX "Software\Aura PDF\Capabilities\FileAssociations" ".pdf" "Document PDF"
  WriteRegStr SHCTX "Software\RegisteredApplications" "Aura PDF" "Software\Aura PDF\Capabilities"
  WriteRegStr SHCTX "Software\Classes\.pdf\OpenWithProgids" "Document PDF" ""
  ; Prise en compte immédiate par l'Explorateur (SHCNE_ASSOCCHANGED).
  System::Call "shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)"

  ; Question posée uniquement en installation interactive (pas en mise à jour
  ; silencieuse ni en mode passif).
  ${IfNot} ${Silent}
  ${AndIf} $PassiveMode <> 1
  ${AndIf} $UpdateMode <> 1
    MessageBox MB_YESNO|MB_ICONQUESTION "Voulez-vous utiliser Aura PDF par défaut pour ouvrir les fichiers PDF ?$\r$\n$\r$\nWindows va ouvrir ses Paramètres : choisissez Aura PDF pour le type .pdf." /SD IDNO IDNO aura_default_skip
      ${If} $MultiUser.InstallMode == "AllUsers"
        ExecShell "open" "ms-settings:defaultapps?registeredAppMachine=Aura%20PDF"
      ${Else}
        ExecShell "open" "ms-settings:defaultapps?registeredAppUser=Aura%20PDF"
      ${EndIf}
    aura_default_skip:
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue SHCTX "Software\RegisteredApplications" "Aura PDF"
  DeleteRegKey SHCTX "Software\Aura PDF\Capabilities"
  DeleteRegKey /ifempty SHCTX "Software\Aura PDF"
  DeleteRegValue SHCTX "Software\Classes\.pdf\OpenWithProgids" "Document PDF"
!macroend
