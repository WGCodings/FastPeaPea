Formed a baseline. Version v04 includes alpha beta search with a simple piece only eval + mobility + tempo bonus.
I think the mobility bonus gave a +40 ELO and the tempo bonus another 15 ELO. Also check-extensions were a big win. It makes the search around 50% slower but gives better results.
These are the results of v04 with check extensions vs v03 (mobility+tempo) vs v02(mobility).

Rank Name                          Elo     +/-   Games   Score    Draw 
   1 v04-checkext                   48      16    1000   56.9%   46.6% 
   2 v03                           -15      16    1000   47.9%   46.1% 
   3 v02                           -33      16    1000   45.3%   45.7% 

SPRT: llr 0 (0.0%), lbound -inf, ubound inf
1500 of 1500 games finished.

Player: v02
   "Draw by 3-fold repetition": 455
   "Draw by insufficient mating material": 2
   "Loss: Black mates": 159
   "Loss: White mates": 160
   "Win: Black loses on time": 1
   "Win: Black mates": 132
   "Win: White mates": 91
Player: v03
   "Draw by 3-fold repetition": 458
   "Draw by insufficient mating material": 3
   "Loss: Black mates": 129
   "Loss: White mates": 162
   "Win: Black mates": 121
   "Win: White mates": 127
Player: v04-checkext
   "Draw by 3-fold repetition": 465
   "Draw by insufficient mating material": 1
   "Loss: Black loses on time": 1
   "Loss: Black mates": 106
   "Loss: White mates": 91
   "Win: Black mates": 141
   "Win: White mates": 195

/////////////////////////////////////////////////////////////////

Added pv move ordering and mvv-lva ordering.

   Score of v04-checkext vs v05-pv: 386 - 138 - 476 [0.624]
...      v04-checkext playing White: 95 - 114 - 291  [0.481] 500
...      v04-checkext playing Black: 291 - 24 - 185  [0.767] 500
...      White vs Black: 119 - 405 - 476  [0.357] 1000
Elo difference: 88.0 +/- 15.6, LOS: 100.0 %, DrawRatio: 47.6 %
SPRT: llr 0 (0.0%), lbound -inf, ubound inf
1000 of 1000 games finished.

Player: v04-checkext
   "Draw by 3-fold repetition": 473
   "Draw by insufficient mating material": 3
   "Loss: Black mates": 114
   "Loss: White mates": 24
   "Win: Black mates": 291
   "Win: White mates": 95
Player: v05-pv
   "Draw by 3-fold repetition": 473
   "Draw by insufficient mating material": 3
   "Loss: Black mates": 291
   "Loss: White mates": 95
   "Win: Black mates": 114
   "Win: White mates": 24

/////////////////////////////////////////////////////////////////

Added threefold repetition and 50 move rule because the amount of draws were getting out of hand and hard to look at.

   Score of v06-repetition vs v05-betteruci: 175 - 132 - 193 [0.543]
...      v06-repetition playing White: 89 - 49 - 112  [0.580] 250
...      v06-repetition playing Black: 86 - 83 - 81  [0.506] 250
...      White vs Black: 172 - 135 - 193  [0.537] 500
Elo difference: 30.0 +/- 23.9, LOS: 99.3 %, DrawRatio: 38.6 %
SPRT: llr 0 (0.0%), lbound -inf, ubound inf
500 of 500 games finished.

Player: v06-repetition
   "Draw by 3-fold repetition": 153
   "Draw by fifty moves rule": 30
   "Draw by insufficient mating material": 10
   "Loss: Black mates": 49
   "Loss: White mates": 83
   "Win: Black mates": 86
   "Win: White mates": 89
Player: v05-betteruci
   "Draw by 3-fold repetition": 153
   "Draw by fifty moves rule": 30
   "Draw by insufficient mating material": 10
   "Loss: Black mates": 86
   "Loss: White mates": 89
   "Win: Black mates": 49
   "Win: White mates": 83