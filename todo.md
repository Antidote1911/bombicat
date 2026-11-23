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

## Plus tard

- [?] Progression des niveaux : « Normal » (8 %) est moins dense que « Débutant » (10 %), puis trou entre 9 % et 22 %. Changer les niveaux rendrait les anciens scores incomparables → à décider.
- [ ] Pause (chronomètre arrêté, grille masquée)
- [ ] Option pour désactiver les points d'interrogation
- [ ] Scores au dixième de seconde
- [ ] Traduction anglaise de l'interface
- [ ] Solveur plus fort (énumération par zones) pour accepter plus de grilles et accélérer « Chuck Norris »
- [ ] Fenêtre plafonnée à 90 % de l'écran : codé mais non vérifié (l'écran de test était plus grand que la fenêtre)
- [ ] Sauvegarde d'un score après une victoire sans aide : chemin inchangé, non rejoué pendant les tests
