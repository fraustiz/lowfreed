# Lowfreed

Configurateur Windows en Rust pour le clavier **Lofree Flow 2** (100 %, ANSI-US, USB `388D:0003`).
Il parle directement au clavier par le protocole **VIA** (raw HID), sans navigateur ni logiciel Lofree.

- **Keymap** : les 6 couches, clic sur une touche puis choix de la fonction (basique, pavé numérique,
  média, souris, couches, rétroéclairage, keycodes Lofree, macros, combinaisons, Mod-Tap, Layer-Tap,
  keycode libre). Chaque modification est écrite tout de suite dans le clavier.
- **Macros** : les 16 macros du firmware, avec la syntaxe de VIA (`{KC_LCTL,KC_C}`, `{250}`…).
- **Rétroéclairage** : luminosité, respiration, enregistrement permanent.
- **Slider** : remplace le volume du slider latéral par une autre action (défilement vertical ou
  horizontal, zoom, onglet suivant/précédent, piste suivante/précédente, flèches), avec inversion du sens
  et sensibilité. F11/F12 et les autres touches volume gardent le volume.
- **Couche en service** : détectée en tapant (Alt, Win, F11…), présélectionnée et marquée d'un ●.
- **Testeur de touches** : affiche en direct les touches enfoncées (matrice du clavier).
- **Exporter / Importer** : sauvegarde keymap + macros au format `.layout.json` de VIA.
- **Réinitialiser** : keymap d'usine du firmware (avec confirmation).
- **Arrière-plan** : fermer la fenêtre la range dans la zone de notification (clic gauche sur l'icône pour
  rouvrir, clic droit → Quitter) ; option de lancement au démarrage de Windows (`lowfreed.exe --tray`).
  Une seule instance tourne à la fois : relancer l'appli réaffiche la fenêtre.

## Compiler

Prérequis : Rust (chaîne `stable-x86_64-pc-windows-gnu`) et MinGW-w64 (`winget install BrechtSanders.WinLibs.POSIX.UCRT`),
nécessaire car `windows-sys` lie les DLL système en *raw-dylib*, ce qui demande `dlltool` et `as` sur la cible GNU.

```powershell
.\build.ps1
```

Produit `target\release\lowfreed.exe` (interface) et `target\release\lowfreed-cli.exe`.

## Ligne de commande

```text
lowfreed-cli info                  identité, protocole, couches, macros, rétroéclairage
lowfreed-cli dump [couche]         keymap lisible
lowfreed-cli macros                contenu des macros
lowfreed-cli set 1 0 1 KC_F1       assigne un keycode (couche, ligne, colonne)
lowfreed-cli slider scroll 60      remplace le slider pendant 60 s et affiche chaque cran (diagnostic)
lowfreed-cli backup sauvegarde.layout.json
lowfreed-cli restore sauvegarde.layout.json
```

`--definition autre.json` (CLI) ou `lowfreed.exe autre.json` charge une autre définition VIA.

## Ce que l'on sait du Flow 2

Relevé sur le clavier branché (firmware révision `0x0016`) :

| Élément | Valeur |
| --- | --- |
| Interface VIA | USB interface 1, page d'usage `0xFF60`, usage `0x61`, rapports de 32 octets |
| Protocole | VIA **v12** (keycodes QMK ≥ 0.19) |
| Couches | 6 : 0 Mac, 1 Fn Mac, 2 Windows, 3 Fn Windows, 4 et 5 « système » |
| Macros | 16, tampon de 2760 octets |
| Rétroéclairage | canal VIA 1 (`qmk_backlight`) : luminosité et effet |
| Keycodes Lofree | `CUSTOM(0…18)` (non documentés, sauf `CUSTOM(0)` = « TRIGGER » dans le JSON). Fn+1…4 utilisent `CUSTOM(1…4)`, F3 de la couche Mac `CUSTOM(11)` |

À savoir :
- La **couche 4** (Fn + Maj gauche) contient `QK_BOOT` sur Échap (mode bootloader) et `EE_CLR` sur Retour arrière ;
  l'appli les affiche en rouge. L'appli n'envoie jamais elle-même les commandes bootloader ou effacement EEPROM.
- Windows transmet les réponses HID à tous les programmes ouverts sur l'interface : si VIA (usevia.app) tourne
  en même temps, ses réponses sont filtrées (chaque réponse doit reprendre l'en-tête de la requête).
- Les macros tapent comme un clavier US : avec Windows en AZERTY, préférez des `{KC_…}`.

### Le slider latéral

Le firmware envoie « Volume + / − » (page Consumer, usages `0xE9`/`0xEA`, rapport n°4 de l'interface 2),
et « Luminosité écran ± » (`0x6F`/`0x70`) avec Fn. Ce comportement est figé : le slider n'est ni dans la
matrice, ni un encodeur VIA, ni une valeur personnalisée. Lowfreed le remplace donc côté Windows :

1. un fil lit les rapports Consumer du Flow 2 et, à chaque « Volume ± », interroge la matrice VIA : si
   aucune touche de volume du keymap n'est enfoncée, l'appui vient du slider ;
2. un hook clavier bas niveau reçoit la touche `VK_VOLUME_UP/DOWN` que Windows en tire (marquée
   *injectée*, drapeaux `0x11`), la bloque si elle vient du slider et envoie l'action choisie.

Le remplacement ne fonctionne que pendant que Lowfreed tourne (fenêtre ou zone de notification).

### La couche en service

VIA ne dit pas quelle couche est active. Lowfreed compare chaque touche reçue par Windows (code de scan,
indépendant de la disposition AZERTY/QWERTY) avec la matrice du Flow 2 au même instant : une touche qui
diffère entre les couches de base (Alt/Win en bas à gauche, F11/F12…) révèle la couche Mac ou Windows.

## Analyse REA

Le logiciel officiel trouvé (« Lofree Flowlite Key Mapper » v1.1.1.3) a été inspecté avec
[REA](https://github.com/morluto/rea) (`inspect-artifact`) après extraction de l'installeur Inno Setup,
sans être exécuté. C'est une application Qt 5 basée sur un pilote générique « Compx » : elle passe par des
rapports HID *Feature* et ne cible que `388D:0008`/`0009` (et d'autres marques), avec des firmwares embarqués.
Le Flow 2 (`0003`) n'y figure pas : **ne pas utiliser cet outil sur le Flow 2**, d'où le passage par VIA.

## Licence

[MIT](LICENSE). Le fichier `assets/flow2_100_ansi.json` est la définition VIA publiée par Lofree.
