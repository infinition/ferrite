# Ferrite

Application de bureau qui inventorie les artefacts regenerables d'un workspace
de developpement, mesure ce qu'ils occupent, verifie leur couverture
`.gitignore` et permet de les supprimer par selection.

Un seul executable. Rien a installer, aucune dependance a deployer: le serveur
local, l'interface et les catalogues de langue sont embarques dans le binaire.

## Construction

```
cargo build --release --target x86_64-pc-windows-msvc
```

Le binaire sort dans `target\x86_64-pc-windows-msvc\release\ferrite.exe`.

La cible MSVC est necessaire pour embarquer l'icone: `build.rs` localise le
`rc.exe` du SDK Windows. Sans lui la construction reussit quand meme, avec
l'icone par defaut.

Options a l'execution:

```
ferrite.exe                 fenetre de bureau, port 7420
ferrite.exe --port 8080     autre port
ferrite.exe --headless      pas de fenetre, interface servie au navigateur
```

Le port par defaut est conserve tant qu'il est libre. L'interface memorise la
langue, le dernier workspace et l'option de conservation dans le stockage local,
qui est indexe par origine: un port variable les effacerait a chaque lancement.

## Architecture

```
src/main.rs      fenetre tao, webview wry, demarrage du serveur
src/server.rs    etat, jobs de scan, routes HTTP
src/scanner.rs   parcours, mesure, appels git, suppression
src/catalog.rs   les 120 regles de detection
src/report.rs    mise en forme des resultats
src/i18n.rs      catalogues de langue embarques
assets/          index.html, style.css, app.js, locales, icones
tools/           generation d'icone, controle de couverture i18n
```

La fenetre est fournie par `tao` et `wry`, la couche sur laquelle Tauri est
construit, utilisee sans le framework: cela evite une chaine d'outils
JavaScript et garde un binaire unique. Le rendu passe par WebView2, present
d'origine sur Windows 11.

## Le scan

1. **Decouverte.** Parcourt le workspace jusqu'a la profondeur choisie, de 1 a
   6 niveaux, et retient les dossiers portant un marqueur de projet: `.git`,
   `package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`, `pom.xml`,
   `composer.json`, `mix.exs`, `*.sln`, `*.uproject`, et d'autres. Un projet
   identifie n'est pas explore plus profond.

2. **Detection.** 120 regles sur 18 ecosystemes. Le parcours s'arrete des qu'un
   artefact est reconnu, ce qui evite tout double comptage.

3. **Mesure.** Volume et nombre de fichiers, par artefact et par projet.

4. **Croisement git.** `git check-ignore` determine la couverture `.gitignore`,
   `git ls-files` detecte ce qui est deja versionne.

Les regles ambigues sont conditionnees a un marqueur voisin: `target` n'est
propose que s'il y a un `Cargo.toml` ou un `pom.xml`, `vendor` que s'il y a un
`composer.json` ou un `go.mod`, `bin` et `obj` que s'il y a un `.csproj`.

Il n'y a volontairement aucune regle sur les dossiers nommes `models` ou
`checkpoints`: dans une application Django ou Rails, `models/` contient du code
source. Les poids de modeles sont detectes par extension de fichier, ce qui est
precis sans faux positif.

## Niveaux de risque

| Niveau | Signification |
|---|---|
| **Sur** | Artefact de build pur, regenere par une commande. `node_modules`, `target`, `__pycache__`, `.next`, `venv`, `Pods`, `DerivedData`. |
| **A verifier** | Generalement genere, mais peut contenir des sources selon le projet. `dist`, `build`, `out`, `vendor` en Go, `.idea`, `logs`. |
| **Donnees** | Poids ou caches retelechargeables. La perte se mesure en bande passante. `*.safetensors`, `*.gguf`, `.cache/huggingface`, `wandb`. |

Chaque ligne affiche la commande qui regenere l'artefact.

## Etats .gitignore

Le resume est visible sur l'en-tete d'un projet sans avoir a le deplier, avec le
nombre d'artefacts par etat.

| Etat | Signification |
|---|---|
| **Ignore** | Toutes les occurrences sont couvertes. |
| **Partiel** | Une partie seulement est couverte. |
| **Non ignore** | Aucune occurrence n'est couverte. |
| **Deja versionne** | Les chemins sont dans l'index git. Tant qu'ils y restent, aucun motif `.gitignore` ne s'applique: il faut d'abord `git rm -r --cached <chemin>`. |
| **Hors git** | Le projet n'est pas un depot. |

Le bouton **Corriger le .gitignore**, present sur l'en-tete des projets
concernes, ajoute les motifs manquants sous une section datee sans toucher au
reste du fichier. Les projets hors git sont sautes et signales.

## Conserver les .exe

L'option de la barre d'outils change la suppression: au lieu d'effacer
l'arborescence, Ferrite la vide fichier par fichier en laissant les `*.exe` en
place, ainsi que les seuls dossiers necessaires pour y acceder. Un `target/`
nettoye conserve donc ses binaires compiles et perd tout le reste.

## Garde-fous

- L'API n'accepte que les chemins issus du scan courant, indexes par projet et
  par regle. Une selection forgee est rejetee.
- Un chemin hors du workspace scanne, la racine du workspace, la racine d'un
  projet et tout `.git` sont refuses.
- La confirmation detaille le volume et le nombre de projets touches, avec un
  avertissement distinct pour les elements versionnes et pour les donnees.
- La suppression leve l'attribut lecture seule et gere les chemins longs
  Windows via le prefixe `\\?\`.

## Internationalisation

Toutes les chaines visibles vivent dans `assets/locales/*.json`, servies au
front par `/api/i18n/<lang>` et lues par le back via `i18n::t()`. Ajouter une
langue tient en un fichier depose dans `assets/locales/` et declare dans
`src/i18n.rs`.

```
python tools/check_i18n.py
```

verifie que les catalogues restent alignes, que chacune des 120 regles a une
description dans chaque langue, et qu'aucune cle referencee par le template ou
le script front n'est absente. Les cles assemblees a l'execution, du type
`ignore.tip_` suivi d'un statut, sont controlees explicitement.

## Icone

```
python tools/make_icon.py
```

Regenere `icon.ico`, les PNG de l'interface et le RGBA brut de l'icone de
fenetre a partir d'un seul dessin.
