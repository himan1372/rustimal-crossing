#ifndef PC_TOWN_ADAPTER_H
#define PC_TOWN_ADAPTER_H

#include "m_field_make.h"

#ifdef TARGET_PC
#ifdef __cplusplus
extern "C" {
#endif

/* Apply the Rust plan to lower playable acres while retaining the authored
 * rail row. Returns 1 when applied; 0 leaves the table untouched. */
int pc_town_apply_generated_plan(mFM_combination_c *field,
                                 const mFM_combo_info_c *combinations,
                                 int combination_count, unsigned int seed);

#ifdef __cplusplus
}
#endif
#endif

#endif
