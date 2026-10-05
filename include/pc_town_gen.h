#ifndef PC_TOWN_GEN_H
#define PC_TOWN_GEN_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    PC_TOWN_ACRE_WIDTH = 5,
    PC_TOWN_ACRE_DEPTH = 6,
    PC_TOWN_ACRE_COUNT = 30,
    PC_TOWN_UNITS_PER_ACRE = 256,
    PC_TOWN_INITIAL_STARTERS = 6,
    PC_TOWN_MAX_VILLAGERS = 15,
    PC_TOWN_MAX_CANDIDATES = 4096
};

enum PcTownEdge {
    PC_TOWN_EDGE_NORTH = 1,
    PC_TOWN_EDGE_EAST = 2,
    PC_TOWN_EDGE_SOUTH = 4,
    PC_TOWN_EDGE_WEST = 8
};

enum PcTownFeature {
    PC_TOWN_FEATURE_NONE = 0,
    PC_TOWN_FEATURE_STATION = 1,
    PC_TOWN_FEATURE_SHOP = 2,
    PC_TOWN_FEATURE_POST_OFFICE = 3,
    PC_TOWN_FEATURE_PLAYER_HOUSE = 4,
    PC_TOWN_FEATURE_WISHING_WELL = 5,
    PC_TOWN_FEATURE_POLICE_STATION = 6,
    PC_TOWN_FEATURE_MUSEUM = 7,
    PC_TOWN_FEATURE_TAILOR = 8,
    PC_TOWN_FEATURE_DOCK = 9
};

enum PcTownGroundKind {
    PC_TOWN_GROUND_GRASS = 0,
    PC_TOWN_GROUND_BEACH = 1
};

enum PcTownGrassPattern {
    PC_TOWN_GRASS_TRIANGLES = 0,
    PC_TOWN_GRASS_CIRCLES = 1,
    PC_TOWN_GRASS_SQUARES = 2
};

enum PcTownCellKind {
    PC_TOWN_CELL_GRASS = 0,
    PC_TOWN_CELL_FLOWER = 1,
    PC_TOWN_CELL_TREE = 2,
    PC_TOWN_CELL_ROCK = 3,
    PC_TOWN_CELL_WEED = 4,
    PC_TOWN_CELL_LITTER = 5,
    /* Center of a resident house's 3 by 3 semantic footprint. */
    PC_TOWN_CELL_HOUSE = 6,
    /* South-west footprint cell (-1,+1) from the house center. */
    PC_TOWN_CELL_HOUSE_SIGN = 7,
    /* Remaining cells reserved by the house footprint. */
    PC_TOWN_CELL_HOUSE_RESERVED = 8
};

enum PcTownInfrastructure {
    PC_TOWN_INFRA_BRIDGE = 1,
    PC_TOWN_INFRA_POND = 2,
    PC_TOWN_INFRA_SLOPE = 4
};

enum PcTownLookClass {
    PC_TOWN_LOOK_NORMAL = 0,
    PC_TOWN_LOOK_PEPPY = 1,
    PC_TOWN_LOOK_LAZY = 2,
    PC_TOWN_LOOK_JOCK = 3,
    PC_TOWN_LOOK_CRANKY = 4,
    PC_TOWN_LOOK_SNOOTY = 5
};

typedef struct PcTownVillagerCandidate {
    uint16_t id;
    uint8_t look;
    uint8_t reserved;
} PcTownVillagerCandidate;

typedef struct PcTownAcre {
    uint8_t feature;
    uint8_t ground_kind;
    uint8_t grass_pattern;
    uint8_t elevation;
    uint8_t river_edges;
    uint8_t cliff_edges;
    uint8_t waterfall_edges;
    uint8_t infrastructure;
    /* Semantic unit grid; house center, sign, and reserved cells use codes 6-8. */
    uint8_t cells[PC_TOWN_UNITS_PER_ACRE];
} PcTownAcre;

typedef struct PcTownPlan {
    uint32_t seed;
    uint32_t acre_count;
    uint8_t villager_count;
    uint8_t elevation_tier_count;
    uint8_t reserved[2];
    uint16_t villagers[PC_TOWN_MAX_VILLAGERS];
    uint8_t house_acres[PC_TOWN_MAX_VILLAGERS];
    /* Center unit index (z * 16 + x) for each resident house. */
    uint16_t house_units[PC_TOWN_MAX_VILLAGERS];
    PcTownAcre acres[PC_TOWN_ACRE_COUNT];
} PcTownPlan;

typedef struct PcTownAssessment {
    uint8_t field_rank;
    uint8_t score;
    uint8_t perfect_acres;
    uint8_t good_acres;
    uint8_t bad_acres;
    uint8_t reserved;
    uint16_t tree_count;
    uint16_t flower_count;
    uint16_t weed_count;
    uint16_t trash_outside_dump;
} PcTownAssessment;

/*
 * candidate_ids contains identifiers already filtered by the caller's
 * version-specific eligibility rules. Returns 1 on success and 0 on invalid
 * input or an unplaceable layout.
 */
int32_t pc_town_generate(uint32_t seed, const uint16_t *candidate_ids,
                         uint32_t candidate_count, uint8_t villager_count,
                         PcTownPlan *out_plan);

/* Select one caller-filtered eligible resident for each of the six look classes. */
int32_t pc_town_select_initial_villagers(uint32_t seed,
                                         const PcTownVillagerCandidate *candidates,
                                         uint32_t candidate_count,
                                         uint16_t out_ids[PC_TOWN_INITIAL_STARTERS]);

/* trash_by_acre must contain 30 counts outside the dump. */
int32_t pc_town_assess(const PcTownPlan *plan,
                       const uint16_t trash_by_acre[PC_TOWN_ACRE_COUNT],
                       PcTownAssessment *out_assessment);

#ifdef __cplusplus
}
#endif

#endif
