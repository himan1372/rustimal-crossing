#include "pc_town_gen.h"
#include "pc_town_adapter.h"

#include <stdio.h>
#include <string.h>

#define CHECK(condition, message) \
    do { \
        if (!(condition)) { \
            fprintf(stderr, "town adapter check failed at line %d: %s\n", __LINE__, message); \
            return 1; \
        } \
    } while (0)

static int feature_block_type(u8 feature) {
    switch (feature) {
        case PC_TOWN_FEATURE_STATION: return mFM_BLOCK_TYPE_TRACKS_STATION;
        case PC_TOWN_FEATURE_SHOP: return mFM_BLOCK_TYPE_TRACKS_SHOP;
        case PC_TOWN_FEATURE_POST_OFFICE: return mFM_BLOCK_TYPE_TRACKS_POST_OFFICE;
        case PC_TOWN_FEATURE_PLAYER_HOUSE: return mFM_BLOCK_TYPE_PLAYER_HOUSE;
        case PC_TOWN_FEATURE_WISHING_WELL: return mFM_BLOCK_TYPE_SHRINE;
        case PC_TOWN_FEATURE_POLICE_STATION: return mFM_BLOCK_TYPE_POLICE_BOX;
        case PC_TOWN_FEATURE_MUSEUM: return mFM_BLOCK_TYPE_MUSEUM;
        case PC_TOWN_FEATURE_TAILOR: return mFM_BLOCK_TYPE_NEEDLEWORK;
        case PC_TOWN_FEATURE_DOCK: return mFM_BLOCK_TYPE_PORT;
        default: return -1;
    }
}

static int is_bridge(u8 type) {
    return type == mFM_BLOCK_TYPE_RIVER_SOUTH_BRIDGE ||
           type == mFM_BLOCK_TYPE_RIVER_EAST_BRIDGE ||
           type == mFM_BLOCK_TYPE_RIVER_WEST_BRIDGE ||
           type == mFM_BLOCK_TYPE_RIVER_SOUTH_EAST_BRIDGE ||
           type == mFM_BLOCK_TYPE_RIVER_EAST_SOUTH_BRIDGE ||
           type == mFM_BLOCK_TYPE_RIVER_SOUTH_WEST_BRIDGE ||
           type == mFM_BLOCK_TYPE_RIVER_WEST_SOUTH_BRIDGE ||
           type == mFM_BLOCK_TYPE_BEACH_RIVER_BRIDGE;
}

int main(void) {
    mFM_combo_info_c combinations[mFM_BLOCK_TYPE_NUM];
    mFM_combination_c field[BLOCK_TOTAL_NUM];
    mFM_combination_c original[BLOCK_TOTAL_NUM];
    PcTownPlan plan;
    unsigned int seed;
    int found_success = 0;
    int type;

    for (type = 0; type < mFM_BLOCK_TYPE_NUM; type++) {
        combinations[type].type = (u8)type;
    }

    /* Search deterministic seeds until one generated layout fits the authored
     * acre vocabulary. Rejected candidates must never partially mutate input. */
    for (seed = 0; seed < 4096 && !found_success; seed++) {
        int x;
        int z;
        int rail_x = -1;
        int bridge_count = 0;

        if (!pc_town_generate(seed, NULL, 0, 0, &plan)) {
            continue;
        }
        for (x = 0; x < PC_TOWN_ACRE_WIDTH; x++) {
            if ((plan.acres[x].river_edges & PC_TOWN_EDGE_SOUTH) != 0) {
                rail_x = x;
                break;
            }
        }
        if (rail_x < 0) {
            continue;
        }

        for (x = 0; x < BLOCK_TOTAL_NUM; x++) {
            field[x].combination_type = mFM_BLOCK_TYPE_OCEAN;
            field[x].height = 2;
        }
        for (z = 0; z < PC_TOWN_ACRE_DEPTH; z++) {
            for (x = 0; x < PC_TOWN_ACRE_WIDTH; x++) {
                const int save_index = (z + 1) * BLOCK_X_NUM + (x + 1);
                field[save_index].combination_type = mFM_BLOCK_TYPE_FLAT;
                field[save_index].height = 0;
            }
        }
        field[BLOCK_X_NUM + rail_x + 1].combination_type = mFM_BLOCK_TYPE_TRACKS_RIVER;
        memcpy(original, field, sizeof(field));

        if (!pc_town_apply_generated_plan(field, combinations, mFM_BLOCK_TYPE_NUM, seed)) {
            CHECK(memcmp(field, original, sizeof(field)) == 0,
                  "rejected plan changed the legacy field table");
            continue;
        }

        for (z = 0; z < PC_TOWN_ACRE_DEPTH; z++) {
            for (x = 0; x < PC_TOWN_ACRE_WIDTH; x++) {
                const int town_index = z * PC_TOWN_ACRE_WIDTH + x;
                const int save_index = (z + 1) * BLOCK_X_NUM + (x + 1);
                const PcTownAcre *acre = &plan.acres[town_index];
                const int expected_feature = feature_block_type(acre->feature);
                const u8 actual_type = combinations[field[save_index].combination_type].type;

                if (expected_feature >= 0) {
                    CHECK(actual_type == expected_feature,
                          "Rust facility role did not reach its corresponding legacy block");
                    CHECK(field[save_index].height == acre->elevation - 1,
                          "facility acre elevation was not copied");
                } else if (z != 0) {
                    CHECK(field[save_index].height == acre->elevation - 1,
                          "playable acre elevation was not copied");
                } else if ((acre->river_edges & PC_TOWN_EDGE_SOUTH) != 0) {
                    CHECK(actual_type == mFM_BLOCK_TYPE_TRACKS_RIVER,
                          "Rust river did not retain its authored rail connector");
                } else {
                    CHECK(field[save_index].combination_type == original[save_index].combination_type,
                          "non-semantic rail block was overwritten");
                }
                bridge_count += is_bridge(actual_type);
            }
        }
        CHECK(bridge_count == 2, "adapter did not preserve exactly two bridge markers");

        /* Check the 7x10 map border, which the 5x6 plan does not own. */
        for (z = 0; z < BLOCK_Z_NUM; z++) {
            for (x = 0; x < BLOCK_X_NUM; x++) {
                if (x == 0 || x == BLOCK_X_NUM - 1 || z == 0 || z >= PC_TOWN_ACRE_DEPTH + 1) {
                    const int index = z * BLOCK_X_NUM + x;
                    CHECK(field[index].combination_type == original[index].combination_type &&
                              field[index].height == original[index].height,
                          "adapter wrote outside its interior 5x6 save-table area");
                }
            }
        }
        found_success = 1;
    }

    CHECK(found_success, "no generated town mapped into the authored block combination vocabulary");
    puts("town adapter checks passed");
    return 0;
}
