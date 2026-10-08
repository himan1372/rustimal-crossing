# Albumin Physical Geometry Table

The 17 river/cliff albumin outputs traced from the symbolic 7×7 table
all the way to exact physical terrain geometry.

Source: `src/data/field/bg/acre/bg_data.c` (`data_bgd`), `m_collision_bg.h`
(USA Rev. 0 decomp / PC port). All 16×16 collision grids extracted
programmatically; ASCII maps generated from the extracted data, not drawn by hand.

## Legend

Each map is the 16×16 collision grid of one acre, north at top.

- `#` high ground (center height 16)
- `=` mid terrace (center height 12)
- `.` low ground (center height 4)
- `~` water (river/waterfall attribute)
- `?` other height (only where it occurs)
- `X` marks a sloped unit (corners differ) drawn over its base height letter

Heights are collision units; world Y = height × 10 + acre base height.
Water runs at height 0 (low rivers) or 12 (rivers on the mid terrace).
`WATERFALL`-attribute units are the falling-water face itself.

## The table

| # | Albumin block type (id) | River | Cliff | Waterfall? | BG asset | Water height | Sloped units |
|---|---|---|---|---|---|---|---|
| 0 | WATERFALL_STRAIGHT_CLIFF_HORIZONTAL (22) | south | horizontal | yes | GRD_S_C1_R1_1 | 0, 12 | 20 |
| 1 | WATERFALL_STRAIGHT_CLIFF_BOTTOM_RIGHT_CORNER (23) | south | bottom-right | yes | GRD_S_C2_R1_1 | 0, 12 | 13 |
| 2 | RIVER_STRAIGHT_CLIFF_VERTICAL_RIGHT (24) | south | vertical-right | no | GRD_S_C3_R1_1 | 12 | 12 |
| 3 | RIVER_STRAIGHT_CLIFF_TOP_RIGHT_CORNER (25) | south | top-right | no | GRD_S_C4_R1_1 | 12 | 9 |
| 4 | WATERFALL_STRAIGHT_CLIFF_TOP_LEFT_CORNER (26) | south | top-left | yes | GRD_S_C5_R1_1 | 0, 12 | 11 |
| 5 | RIVER_STRAIGHT_CLIFF_VERTICAL_LEFT (27) | south | vertical-left | no | GRD_S_C6_R1_1 | 0 | 12 |
| 6 | RIVER_STRAIGHT_CLIFF_BOTTOM_LEFT_CORNER (28) | south | bottom-left | no | GRD_S_C7_R1_1 | 0 | 11 |
| 7 | RIVER_STRAIGHT_CLIFF_HORIZONTAL (29) | east | horizontal | no | GRD_S_C1_R2_1 | 12 | 12 |
| 8 | WATERFALL_EAST_CLIFF_BOTTOM_RIGHT_CORNER (30) | east | bottom-right | yes | GRD_S_C2_R2_1 | 0, 12 | 18 |
| 9 | WATERFALL_EAST_CLIFF_VERTICAL_RIGHT (31) | east | vertical-right | yes | GRD_S_C3_R2_1 | 0, 12 | 17 |
| 10 | RIVER_EAST_CLIFF_TOP_RIGHT_CORNER (32) | east | top-right | no | GRD_S_C4_R2_1 | 12 | 11 |
| 11 | RIVER_EAST_CLIFF_TOP_LEFT_CORNER (33) | east | top-left | no | GRD_S_C5_R2_1 | 12 | 12 |
| 12 | RIVER_WEST_CLIFF_HORIZONTAL (34) | west | horizontal | no | GRD_S_C1_R3_1 | 12 | 12 |
| 13 | RIVER_WEST_CLIFF_TOP_RIGHT_CORNER (35) | west | top-right | no | GRD_S_C4_R3_1 | 12 | 11 |
| 14 | RIVER_WEST_CLIFF_TOP_LEFT_CORNER (36) | west | top-left | no | GRD_S_C5_R3_1 | 12 | 11 |
| 15 | WATERFALL_WEST_CLIFF_VERTICAL_LEFT (37) | west | vertical-left | yes | GRD_S_C6_R3_1 | 0, 12 | 13 |
| 16 | WATERFALL_WEST_CLIFF_BOTTOM_LEFT_CORNER (38) | west | bottom-left | yes | GRD_S_C7_R3_1 | 0, 12 | 18 |

## Per-asset geometry

### 0 — WATERFALL_STRAIGHT_CLIFF_HORIZONTAL (GRD_S_C1_R1_1)

South river crosses a horizontal cliff. High ground north, river channel
N–S, cliff-face band where the river drops, low ground south. The waterfall.

```
####~~~#########
####~~~#########
####~~~#########
####~~~#########
####~~~X########
####X~~~~X######
#####X~~~~X#####
######~~~~~#####
###X..X~~~X#####
##X....~~~######
......X~~~####X.
.....X~~~XX##X..
....X~~~X.......
....~~~X........
....~~~.........
....~~~.........
```

### 1 — WATERFALL_STRAIGHT_CLIFF_BOTTOM_RIGHT_CORNER (GRD_S_C2_R1_1)

South river at a bottom-right cliff corner. High ground NW block, river
channel N–S bending at the corner, low ground east and south. Waterfall
at the corner drop.

```
####~~~###......
####~~~###......
####~~~###......
####~~~###......
####~~~X#X......
####X~~~~X......
#####X~~~~......
#####X~~~~......
####X~~~~X......
#X...~~~X.......
.....~~~........
.....~~~........
.....~~~........
....X~~~........
....~~~X........
....~~~.........
```

### 2 — RIVER_STRAIGHT_CLIFF_VERTICAL_RIGHT (GRD_S_C3_R1_1)

South river alongside a vertical-right cliff. High ground west strip,
river channel N–S, cliff face runs vertically, low ground east. No
waterfall: the river stays on the mid terrace (height 12) beside the cliff.

```
####~~~###......
####~~~###X.....
####~~~####.....
####~~~####.....
###X~~~###X.....
##X~~~X##X......
##~~~X###.......
##~~~####.......
##~~~####.......
##~~~####.......
##~~~####.......
##~~~X###X......
##X~~~X###......
###X~~~###......
####~~~###......
####~~~###......
```

### 3 — RIVER_STRAIGHT_CLIFF_TOP_RIGHT_CORNER (GRD_S_C4_R1_1)

South river at a top-right cliff corner. High ground north and east,
river channel N–S, low ground in the SE pocket. River/cliff adjacency,
no waterfall.

```
####~~~#########
####~~~X########
####X~~~########
#####~~~########
#####~~~########
#####~~~########
#####~~~########
#####~~~########
####X~~~########
###X~~~X########
###~~~X###X.....
###~~~####......
###~~~####......
###~~~####......
###X~~X###......
####~~~###......
```

### 4 — WATERFALL_STRAIGHT_CLIFF_TOP_LEFT_CORNER (GRD_S_C5_R1_1)

South river at a top-left cliff corner. High ground north and east,
river channel, waterfall where the river drops at the corner, low ground
west/southwest.

```
####~~~#########
####~~~X########
####X~~~########
#####~~~########
#####~~~########
#####~~~########
####X~~~X#######
###X~~~~~#######
##X.~~~~~#######
#X..X~~~XX######
.....~~~..######
.....~~~..######
....X~~~..######
....~~~X..######
....~~~...######
....~~~...######
```

### 5 — RIVER_STRAIGHT_CLIFF_VERTICAL_LEFT (GRD_S_C6_R1_1)

South river alongside a vertical-left cliff. Low ground west, river
channel N–S at height 0, high ground east. The river runs at the foot of
the cliff's west face.

```
....~~~...######
...X~~~..X######
...~~~X..#######
...~~~...#######
...~~~...#######
...~~~...X######
...~~~X...######
...X~~~X..######
....X~~~..X#####
.....~~~...#####
.....~~~...#####
.....~~~...#####
....X~~~...#####
....~~~X...#####
....~~~...X#####
....~~~...######
```

### 6 — RIVER_STRAIGHT_CLIFF_BOTTOM_LEFT_CORNER (GRD_S_C7_R1_1)

South river at a bottom-left cliff corner. Low ground west/south, river
channel, high ground east/northeast.

```
....~~~...######
...X~~~...######
..X~~~X...######
..~~~X....X#####
..~~~......#####
..~~~......#####
..~~~......#####
..~~~......#####
..~~~X.....#####
..X~~~X....#####
...X~~~....X##X.
....~~~.........
....~~~.........
....~~~.........
....~~~.........
....~~~.........
```

### 7 — RIVER_STRAIGHT_CLIFF_HORIZONTAL (GRD_S_C1_R2_1)

East river along a horizontal cliff. High ground north, river channel
W–E on the mid terrace (height 12), low ground south. The river flows
east along the cliff base; no waterfall.

```
################
################
#######X~~~X####
######X~~~~~X###
~~~~~~~~~~~~~~~~
~~~~~~~~X#X~~~~~
~~~~~~~X###X=~~~
################
################
#X......X#######
.........X####X.
................
................
................
................
................
```

### 8 — WATERFALL_EAST_CLIFF_BOTTOM_RIGHT_CORNER (GRD_S_C2_R2_1)

East river at a bottom-right cliff corner. High ground NW, river bends
from west to south, waterfall at the corner drop, low ground south/east.

```
##########......
##########......
##########......
#X~~~~~X##......
~~~~~~~~XXX..X~~
~~~~~~~~~~~~~~~~
~~X###X~~~~~~~~~
#######X~~~~~~X.
########~~~X....
########~~~.....
..X####X~~X.....
.....X~~~X......
................
................
................
................
```

### 9 — WATERFALL_EAST_CLIFF_VERTICAL_RIGHT (GRD_S_C3_R2_1)

East river at a vertical-right cliff face. High ground north, river W–E
dropping over the vertical face — waterfall — to low ground south.

```
##########......
##########......
##X~~~~X##......
#X~~~~~~XX~~X...
~~~~~~~~~~~~~~~~
~~~X##X~~~~~~~~~
~~X####~~~~~~~~~
#######~~~~~~...
#######X~~~~~...
########X~~~X...
##########XX....
###########.....
###########.....
##########X.....
##########......
##########......
```

### 10 — RIVER_EAST_CLIFF_TOP_RIGHT_CORNER (GRD_S_C4_R2_1)

East river at a top-right cliff corner. High ground north, river W–E on
the terrace, low ground SE pocket. Adjacency, no waterfall.

```
################
################
###X~~~~~X######
##X~~~~~~~X#####
~~~~~~~~~~~~~~~~
~~~~X###X~~~~~~~
~~~X#####X~~~~~~
################
################
################
#############X..
############X...
##########X.....
##########......
##########......
##########......
```

### 11 — RIVER_EAST_CLIFF_TOP_LEFT_CORNER (GRD_S_C5_R2_1)

East river at a top-left cliff corner. High ground north, river W–E,
low ground SW. Adjacency, no waterfall.

```
################
################
####X~~~~~~~X###
###X~~~~~~~~~X##
~~~~~~~~~~~~~~~~
~~~~~X#####X~~~~
~~~~X#######X~~~
################
################
################
..X###X...X#####
...........#####
...........#####
..........X#####
..........######
..........######
```

### 12 — RIVER_WEST_CLIFF_HORIZONTAL (GRD_S_C1_R3_1)

West river along a horizontal cliff. Mirror of asset 7: high ground
north, river channel W–E on the terrace, low ground south.

```
################
################
##X~~~~~~~X#####
#X~~~~~~~~~X####
~~~~~~~~~~~~~~~~
~~~X#####X~~~~~~
~~X#######X~~~~~
################
####X...X#######
###X.....X######
................
................
................
................
................
................
```

### 13 — RIVER_WEST_CLIFF_TOP_RIGHT_CORNER (GRD_S_C4_R3_1)

West river at a top-right cliff corner. High ground north, river E–W,
low ground SE. Adjacency, no waterfall.

```
################
################
#######X~~~X####
######X~~~~~X###
~~~~~~~~~~~~~~~~
~~~~~~~~X#X~~~~~
~~~~~~~X###X~~~~
################
################
################
#############X..
#############...
#############...
############X...
##########X.....
##########......
```

### 14 — RIVER_WEST_CLIFF_TOP_LEFT_CORNER (GRD_S_C5_R3_1)

West river at a top-left cliff corner. High ground north and east, river
E–W channel, low ground SW.

```
################
################
################
################
~~~X######X~~~~~
~~~~X####X~~~~~~
~~~~~~~~~~~~~~~~
##X~~~~~~~X#####
###X~~~~~X######
################
....X###########
........X#######
.........#######
.........#######
.........X######
..........######
```

### 15 — WATERFALL_WEST_CLIFF_VERTICAL_LEFT (GRD_S_C6_R3_1)

West river at a vertical-left cliff face. Low ground west/northwest,
high ground east, river E–W bending down, waterfall at the vertical face.
(Single-variant asset: only one data_combi entry.)

```
..........######
..........######
..........######
.........X######
~~~~~X...####X~~
~~~~~~~~~###X~~~
~~~~~~~~~X#X~~~~
....X~~~~~~~~~X#
......~~~~~~~~##
......~~~~~~~~##
......~~~~~~~~##
......X~~~~X####
...........#####
..........X#####
..........######
..........######
```

### 16 — WATERFALL_WEST_CLIFF_BOTTOM_LEFT_CORNER (GRD_S_C7_R3_1)

West river at a bottom-left cliff corner. Low ground west, high ground
east, river bends, waterfall at the corner drop.

```
..........######
..........######
.........X######
.X~~~~X..#######
~~~~~~~X.####X~~
~~~~~~~~~X##X~~~
~~X..X~~~~~~~~~~
......X~~~~~~~X#
.......~~~~~~~##
.......~~~~~~~X#
.......X~~~~~~X.
........X~~~~X..
................
................
................
................
```

## Geometric rules distilled

1. Waterfalls (7 assets) are exactly the cells where the river CROSSES
   the cliff contour: the water drops from height 12/16 to 0 across a
   band of WATERFALL-attribute units. Non-waterfall assets (10) are
   river-ALONGSIDE-cliff: the river stays on one terrace level.
2. South rivers run at height 0 in the lowland variants and 12 on the
   terrace; east/west rivers run at 12 on the mid terrace.
3. The cliff face is a 1–2 unit wide band of sloped units (slate=1,
   corners differ), 9–20 units per asset.
4. Water channel width is consistently ~3 units.
5. East and west families are mirrors; the south family is distinct
   (N–S channel vs W–E channel).
