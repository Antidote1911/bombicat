# Bombicat — TODO

Issu de la comparaison avec KMines (novembre 2026).
`[x]` fait, `[ ]` à faire, `[?]` décision à prendre.

## Nettoyage

- [x] Supprimer entièrement l'éditeur de grille (`editor.rs`, parties personnalisées, dépendance `rfd`)

## Bugs

- [x] Drapeau posé avant le premier clic : perdu à la génération, compteur de chats faussé
- [x] Génération jamais annulée : Ctrl+N pendant une génération laisse un thread tourner sur tous les cœurs
- [x] « Réinitialiser » les scores efface tout sans confirmation
- [x] Base SQLite rouverte et relue à chaque image quand la fenêtre des scores est ouverte
- [x] README : tableau des niveaux pas à jour (110/160/215 au lieu de 120/190/250)

## Saisie (ce que KMines fait mieux)

- [x] Révéler au relâchement du bouton, sur la case sous la souris (plus de clics perdus si la souris bouge ou si l'appui est long)
- [x] Case visuellement enfoncée pendant l'appui
- [x] Révélation autour d'un chiffre : clic milieu, gauche+droit (dans les deux ordres), ou simple clic gauche sur le chiffre
- [x] Drapeau posé dès l'appui du clic droit

## Jeu

- [x] Aide (Ctrl+H) : désigner une case réellement déductible au lieu de regarder la solution
- [x] Aide : une partie aidée n'entre pas dans les meilleurs scores
- [x] Victoire : poser automatiquement les drapeaux restants, compteur à 0
- [x] Chronomètre : rafraîchi pile à chaque seconde (au lieu de toutes les 500 ms)
- [x] Fenêtre redimensionnable, taille des cases adaptée à la fenêtre et à l'écran

## Algorithmes

- [x] Solveur : ne comparer que les contraintes voisines (≤ 2 cases) au lieu de toutes les paires
- [x] Solveur : plus d'allocations par case (tableaux fixes)
- [x] Placement des chats : tirage sans remise au lieu du tirage avec rejet
- [x] Propagation itérative (plus de récursion) et logique de jeu déplacée dans `model.rs`
- [x] Tests unitaires du modèle et du solveur (nouveau solveur comparé à l'ancien)

## Deuxième passe

- [x] Génération par retouches locales au lieu du rejet complet (Chuck Norris : ~31 essais au lieu de ~9 500, 12 ms sur un cœur)
- [x] Rayon supprimé (devenu inutile)
- [x] Solveur avancé (étude de tous les cas par zone), en option dans les préférences
- [x] Progression des niveaux : Normal 25 → 40 chats, Difficile 35 → 62 (densités 10 %, 13 %, 17 %, 22 %, 25 %, 25 %)
- [x] Scores rattachés à leurs règles : les anciens scores restent affichés, à part
- [x] Scores au dixième de seconde
- [x] Pause (touche P, chronomètre arrêté, grille masquée ; aussi quand une fenêtre est ouverte)
- [x] Option pour désactiver les points d'interrogation
- [x] Interface en anglais (suit la langue du système, modifiable)
- [x] Explication des optimisations : `OPTIMISATION.txt`

## Reste à faire

- [ ] Vérifier en jeu : clic milieu, gauche+droit, mode `--cheat`, sauvegarde d'un score après une victoire sans aide
- [ ] Fenêtre plafonnée à 90 % de l'écran : codé mais non vérifié sur un petit écran
- [ ] Numéro de version et notes de version (toujours 1.1.0)
- [ ] Biais des retouches : grilles ~7 % plus « aérées » qu'avec l'ancien tirage (voir `OPTIMISATION.txt`, §3.5)
