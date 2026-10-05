#include "pc_town_adapter.h"

#ifdef TARGET_PC
#include "pc_town_gen.h"

#include <string.h>

static int pc_town_feature_block_type(u8 feature) {
    switch (feature) {
        case PC_TOWN_FEATURE_STATION:
            return mFM_BLOCK_TYPE_TRACKS_STATION;
        case PC_TOWN_FEATURE_SHOP:
            return mFM_BLOCK_TYPE_TRACKS_SHOP;
        case PC_TOWN_FEATURE_POST_OFFICE:
            return mFM_BLOCK_TYPE_TRACKS_POST_OFFICE;
        case PC_TOWN_FEATURE_PLAYER_HOUSE:
            return mFM_BLOCK_TYPE_PLAYER_HOUSE;
        case PC_TOWN_FEATURE_POLICE_STATION:
            return mFM_BLOCK_TYPE_POLICE_BOX;
        case PC_TOWN_FEATURE_MUSEUM:
            return mFM_BLOCK_TYPE_MUSEUM;
        case PC_TOWN_FEATURE_TAILOR:
            return mFM_BLOCK_TYPE_NEEDLEWORK;
        case PC_TOWN_FEATURE_DOCK:
            return mFM_BLOCK_TYPE_PORT;
        case PC_TOWN_FEATURE_WISHING_WELL:
            return mFM_BLOCK_TYPE_SHRINE;
        default:
            return -1;
    }
}

static int pc_town_terrain_block_type(const PcTownAcre *acre) {
    const u8 river = acre->river_edges;
    const u8 cliffs = acre->cliff_edges;
    const u8 waterfalls = acre->waterfall_edges;
    const int bridge = (acre->infrastructure & PC_TOWN_INFRA_BRIDGE) != 0;

    if (river == 0) {
        if (bridge) {
            return -1;
        }
        if (cliffs != 0) {
            if ((cliffs & (PC_TOWN_EDGE_EAST | PC_TOWN_EDGE_WEST)) != 0) {
                return mFM_BLOCK_TYPE_CLIFF_VERTICAL_RIGHT;
            }
            return mFM_BLOCK_TYPE_CLIFF_HORIZONTAL;
        }
        return acre->ground_kind == PC_TOWN_GROUND_BEACH ? mFM_BLOCK_TYPE_BEACH
                                                         : mFM_BLOCK_TYPE_FLAT;
    }

    if (waterfalls != 0) {
        if (bridge) {
            return -1;
        }
        if ((cliffs & (PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_SOUTH)) != 0 &&
            (river & (PC_TOWN_EDGE_EAST | PC_TOWN_EDGE_WEST)) == 0) {
            return mFM_BLOCK_TYPE_WATERFALL_STRAIGHT_CLIFF_HORIZONTAL;
        }
        /* The rewrite plan currently does not encode enough orientation to
         * choose safely among the authored waterfall corner variants. */
        return -1;
    }

    if (cliffs != 0) {
        if (bridge) {
            return -1;
        }
        if ((cliffs & (PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_SOUTH)) != 0 &&
            (river == PC_TOWN_EDGE_NORTH || river == PC_TOWN_EDGE_SOUTH ||
             river == (PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_SOUTH))) {
            return mFM_BLOCK_TYPE_RIVER_STRAIGHT_CLIFF_HORIZONTAL;
        }
        return -1;
    }

    if (acre->ground_kind == PC_TOWN_GROUND_BEACH) {
        return bridge ? mFM_BLOCK_TYPE_BEACH_RIVER_BRIDGE : mFM_BLOCK_TYPE_BEACH_RIVER;
    }

    if (bridge) {
        if (river == PC_TOWN_EDGE_NORTH || river == PC_TOWN_EDGE_SOUTH ||
            river == (PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_SOUTH)) {
            return mFM_BLOCK_TYPE_RIVER_SOUTH_BRIDGE;
        }
        if (river == PC_TOWN_EDGE_EAST) {
            return mFM_BLOCK_TYPE_RIVER_EAST_BRIDGE;
        }
        if (river == PC_TOWN_EDGE_WEST) {
            return mFM_BLOCK_TYPE_RIVER_WEST_BRIDGE;
        }
        if (river == (PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_EAST)) {
            return mFM_BLOCK_TYPE_RIVER_SOUTH_EAST_BRIDGE;
        }
        if (river == (PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_WEST)) {
            return mFM_BLOCK_TYPE_RIVER_SOUTH_WEST_BRIDGE;
        }
        if (river == (PC_TOWN_EDGE_EAST | PC_TOWN_EDGE_SOUTH)) {
            return mFM_BLOCK_TYPE_RIVER_EAST_SOUTH_BRIDGE;
        }
        if (river == (PC_TOWN_EDGE_WEST | PC_TOWN_EDGE_SOUTH)) {
            return mFM_BLOCK_TYPE_RIVER_WEST_SOUTH_BRIDGE;
        }
        return -1;
    }

    switch (river) {
        case PC_TOWN_EDGE_NORTH:
        case PC_TOWN_EDGE_SOUTH:
        case PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_SOUTH:
            return mFM_BLOCK_TYPE_RIVER_SOUTH;
        case PC_TOWN_EDGE_EAST:
            return mFM_BLOCK_TYPE_RIVER_EAST;
        case PC_TOWN_EDGE_WEST:
            return mFM_BLOCK_TYPE_RIVER_WEST;
        case PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_EAST:
            return mFM_BLOCK_TYPE_RIVER_SOUTH_EAST;
        case PC_TOWN_EDGE_EAST | PC_TOWN_EDGE_SOUTH:
            return mFM_BLOCK_TYPE_RIVER_EAST_SOUTH;
        case PC_TOWN_EDGE_NORTH | PC_TOWN_EDGE_WEST:
            return mFM_BLOCK_TYPE_RIVER_SOUTH_WEST;
        case PC_TOWN_EDGE_WEST | PC_TOWN_EDGE_SOUTH:
            return mFM_BLOCK_TYPE_RIVER_WEST_SOUTH;
        default:
            return -1;
    }
}

static int pc_town_find_combination(const mFM_combo_info_c *combinations,
                                    int combination_count, int block_type) {
    int i;

    for (i = 0; i < combination_count; i++) {
        if (combinations[i].type == block_type) {
            return i;
        }
    }
    return -1;
}

static int pc_town_is_bridge_block_type(u8 block_type) {
    switch (block_type) {
        case mFM_BLOCK_TYPE_RIVER_SOUTH_BRIDGE:
        case mFM_BLOCK_TYPE_RIVER_EAST_BRIDGE:
        case mFM_BLOCK_TYPE_RIVER_WEST_BRIDGE:
        case mFM_BLOCK_TYPE_RIVER_SOUTH_EAST_BRIDGE:
        case mFM_BLOCK_TYPE_RIVER_EAST_SOUTH_BRIDGE:
        case mFM_BLOCK_TYPE_RIVER_SOUTH_WEST_BRIDGE:
        case mFM_BLOCK_TYPE_RIVER_WEST_SOUTH_BRIDGE:
        case mFM_BLOCK_TYPE_BEACH_RIVER_BRIDGE:
            return 1;
        default:
            return 0;
    }
}

static int pc_town_validate_features(const PcTownPlan *plan) {
    int feature_count[PC_TOWN_FEATURE_DOCK + 1] = { 0 };
    int feature_x[PC_TOWN_FEATURE_DOCK + 1] = { 0 };
    int feature_z[PC_TOWN_FEATURE_DOCK + 1] = { 0 };
    int x;
    int z;

    if (plan->acre_count != PC_TOWN_ACRE_COUNT ||
        (plan->elevation_tier_count != 2 && plan->elevation_tier_count != 3)) {
        return 0;
    }
    for (z = 0; z < PC_TOWN_ACRE_DEPTH; z++) {
        for (x = 0; x < PC_TOWN_ACRE_WIDTH; x++) {
            const PcTownAcre *acre = &plan->acres[z * PC_TOWN_ACRE_WIDTH + x];
            const u8 feature = acre->feature;

            if (feature > PC_TOWN_FEATURE_DOCK) {
                return 0;
            }
            if (feature == PC_TOWN_FEATURE_NONE) {
                continue;
            }
            feature_count[feature]++;
            feature_x[feature] = x;
            feature_z[feature] = z;
            if (acre->river_edges != 0 || acre->cliff_edges != 0 ||
                acre->waterfall_edges != 0 || (acre->infrastructure & PC_TOWN_INFRA_BRIDGE) != 0) {
                return 0;
            }
        }
    }

    if (feature_count[PC_TOWN_FEATURE_STATION] != 1 ||
        feature_x[PC_TOWN_FEATURE_STATION] != 2 || feature_z[PC_TOWN_FEATURE_STATION] != 0 ||
        feature_count[PC_TOWN_FEATURE_PLAYER_HOUSE] != 1 ||
        feature_x[PC_TOWN_FEATURE_PLAYER_HOUSE] != 2 || feature_z[PC_TOWN_FEATURE_PLAYER_HOUSE] != 1 ||
        feature_count[PC_TOWN_FEATURE_SHOP] != 1 || feature_z[PC_TOWN_FEATURE_SHOP] != 0 ||
        feature_count[PC_TOWN_FEATURE_POST_OFFICE] != 1 || feature_z[PC_TOWN_FEATURE_POST_OFFICE] != 0 ||
        feature_count[PC_TOWN_FEATURE_TAILOR] != 1 || feature_z[PC_TOWN_FEATURE_TAILOR] != 5 ||
        feature_x[PC_TOWN_FEATURE_TAILOR] > 2 ||
        feature_count[PC_TOWN_FEATURE_DOCK] != 1 || feature_x[PC_TOWN_FEATURE_DOCK] != 4 ||
        feature_z[PC_TOWN_FEATURE_DOCK] != 5 ||
        feature_count[PC_TOWN_FEATURE_WISHING_WELL] != 1 ||
        feature_count[PC_TOWN_FEATURE_POLICE_STATION] != 1 ||
        feature_count[PC_TOWN_FEATURE_MUSEUM] != 1) {
        return 0;
    }

    /* The source generator reserves one rail service in each outer pair. */
    if (!((feature_x[PC_TOWN_FEATURE_SHOP] < 2 && feature_x[PC_TOWN_FEATURE_POST_OFFICE] >= 3) ||
          (feature_x[PC_TOWN_FEATURE_POST_OFFICE] < 2 && feature_x[PC_TOWN_FEATURE_SHOP] >= 3)) ||
        feature_z[PC_TOWN_FEATURE_WISHING_WELL] == 0 ||
        feature_z[PC_TOWN_FEATURE_WISHING_WELL] == PC_TOWN_ACRE_DEPTH - 1 ||
        feature_z[PC_TOWN_FEATURE_POLICE_STATION] == 0 ||
        feature_z[PC_TOWN_FEATURE_POLICE_STATION] == PC_TOWN_ACRE_DEPTH - 1 ||
        feature_z[PC_TOWN_FEATURE_MUSEUM] == 0 ||
        feature_z[PC_TOWN_FEATURE_MUSEUM] == PC_TOWN_ACRE_DEPTH - 1) {
        return 0;
    }

    if (plan->acres[feature_z[PC_TOWN_FEATURE_TAILOR] * PC_TOWN_ACRE_WIDTH +
                    feature_x[PC_TOWN_FEATURE_TAILOR]].ground_kind != PC_TOWN_GROUND_BEACH ||
        plan->acres[feature_z[PC_TOWN_FEATURE_DOCK] * PC_TOWN_ACRE_WIDTH +
                    feature_x[PC_TOWN_FEATURE_DOCK]].ground_kind != PC_TOWN_GROUND_BEACH ||
        plan->acres[feature_z[PC_TOWN_FEATURE_WISHING_WELL] * PC_TOWN_ACRE_WIDTH +
                    feature_x[PC_TOWN_FEATURE_WISHING_WELL]].elevation >= plan->elevation_tier_count ||
        plan->acres[feature_z[PC_TOWN_FEATURE_POLICE_STATION] * PC_TOWN_ACRE_WIDTH +
                    feature_x[PC_TOWN_FEATURE_POLICE_STATION]].elevation >= plan->elevation_tier_count ||
        plan->acres[feature_z[PC_TOWN_FEATURE_MUSEUM] * PC_TOWN_ACRE_WIDTH +
                    feature_x[PC_TOWN_FEATURE_MUSEUM]].elevation >= plan->elevation_tier_count) {
        return 0;
    }
    return 1;
}

static int pc_town_rail_river_matches(const PcTownPlan *plan,
                                     const mFM_combination_c *field,
                                     const mFM_combo_info_c *combinations,
                                     int combination_count) {
    int planned_x = -1;
    int legacy_x = -1;
    int planned_count = 0;
    int legacy_count = 0;
    int x;

    for (x = 0; x < PC_TOWN_ACRE_WIDTH; x++) {
        const int save_index = BLOCK_X_NUM + (x + 1);
        const int combination = field[save_index].combination_type;

        if ((plan->acres[x].river_edges & PC_TOWN_EDGE_SOUTH) != 0) {
            planned_x = x;
            planned_count++;
        }
        if (combination >= combination_count) {
            return 0;
        }
        if (combinations[combination].type == mFM_BLOCK_TYPE_TRACKS_RIVER) {
            legacy_x = x;
            legacy_count++;
        }
    }

    /* The 5x6 region begins immediately below this row. Retain authored rail
     * combinations, but require the Rust river to join their river track. */
    return planned_count == 1 && legacy_count == 1 && planned_x == legacy_x;
}

int pc_town_apply_generated_plan(mFM_combination_c *field,
                                 const mFM_combo_info_c *combinations,
                                 int combination_count, unsigned int seed) {
    static PcTownPlan plan;
    mFM_combination_c adapted[BLOCK_TOTAL_NUM];
    int bridge_count = 0;
    int x;
    int z;

    if (field == NULL || combinations == NULL || combination_count <= 0 ||
        pc_town_generate(seed, NULL, 0, 0, &plan) == 0) {
        return 0;
    }
    if (!pc_town_validate_features(&plan)) {
        return 0;
    }
    if (!pc_town_rail_river_matches(&plan, field, combinations, combination_count)) {
        return 0;
    }

    memcpy(adapted, field, sizeof(adapted));
    for (z = 0; z < PC_TOWN_ACRE_DEPTH; z++) {
        for (x = 0; x < PC_TOWN_ACRE_WIDTH; x++) {
            const int town_index = z * PC_TOWN_ACRE_WIDTH + x;
            const PcTownAcre *acre = &plan.acres[town_index];
            int type = pc_town_feature_block_type(acre->feature);
            int combination;

            /* Keep non-semantic rail decoration, while translating Rust-owned
             * facility roles and the river connection into the rail row. */
            if (z == 0) {
                if (acre->feature != PC_TOWN_FEATURE_NONE) {
                    type = pc_town_feature_block_type(acre->feature);
                } else if (acre->river_edges == PC_TOWN_EDGE_SOUTH) {
                    type = mFM_BLOCK_TYPE_TRACKS_RIVER;
                } else {
                    continue;
                }
            }

            if (acre->feature != PC_TOWN_FEATURE_NONE &&
                (acre->river_edges != 0 || acre->cliff_edges != 0)) {
                return 0;
            }
            if (type < 0) {
                type = pc_town_terrain_block_type(acre);
            }
            if (type < 0) {
                return 0;
            }
            combination = pc_town_find_combination(combinations, combination_count, type);
            if (combination < 0 || combination > 0x3FFF || acre->elevation == 0 ||
                acre->elevation > 3) {
                return 0;
            }
            bridge_count += pc_town_is_bridge_block_type(combinations[combination].type);

            /* The playable region is the interior 5x6 of the 7x10 save map. */
            adapted[(z + 1) * BLOCK_X_NUM + (x + 1)].combination_type = combination;
            adapted[(z + 1) * BLOCK_X_NUM + (x + 1)].height = acre->elevation - 1;
        }
    }

    if (bridge_count != 2) {
        return 0;
    }

    memcpy(field, adapted, sizeof(adapted));
    return 1;
}
#endif
