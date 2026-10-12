# Resolve + port the need-to-port follow-ups: gyroid, cockroach, balloon, town map, diary, mushroom, igloo, snowman
- [x] Gyroid: HANIWA_START/END numerics, FTR_NO_2_IDX for mSP_RandomHaniwaSelect, mHW_MSG_DOREWO special case, mAGrw_SetDigItem/SpoilKabu over haniwa slots, inventory transfer
- [x] Cockroach: house-entry -> infestation -> spawn -> stomp -> saved count chain, check_danna_kill_proc, mCkRh_DecideNowGokiFamilyCount call site
- [x] Balloon: present-drop NULL guard mismatch, little-shake double set, draw present-skip, rare list + mFI_GetOtherFruit, anim_frame/dt
- [x] Town map: house_pos_list asset, cursor_frame PC vs N64, make_max_no_table bounds, resident cache assembly, cursor easing, exit-door vs player pos
- [x] Diary: card-load repair gate, chase_f scroll, _28 quadratic, Play_write state copy/clamp, set_writing_body end rules, month_tex_table
- [x] Mushroom: column refill, per-row bitfields + SetBlockMashroom scan order, IsOverTime future stamps, startup cleanup
- [x] Igloo: anim_frame + steam cadence, mEvMN_GetNpcJointEv policy, mochi table, chase factor, rumor row
- [x] Snowman: @BUG NULL deref, reward FTR ids, move_dist accessor, birth orchestration, dust + head-jump arc
- [x] Extend the existing modules (haniwa/cockroach/balloon/town_map/diary/mushroom/kamakura/snowman.rs); cargo check host+i686; docs, workbook, comms, need-to-port.md
Requested by Philip 2026-10-11; owner: claude.
