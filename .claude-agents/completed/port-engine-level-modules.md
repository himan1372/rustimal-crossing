# Research + port engine-level / misc modules (logic/data parts)
- [x] Research each file vs ac-decomp (primary), hunter-r.com, ACGC-PC-Port (secondary); classify port vs stay-in-C
- [x] Camera/view (m_camera2, m_view), lighting (m_lights), controller/rumble (m_controller, m_vibctl), actor shadows (m_actor_shadow)
- [x] Tables (m_actor_dlftbls, m_game_dlftbls), BG (m_bg_item, m_bg_tex, m_land), clip (m_clip), string (m_string)
- [x] Demos (ac_intro_demo, ac_present_demo, ac_ride_off_demo), field draw (ac_field_draw, ac_fieldm_draw, ac_effectbg)
- [x] Misc: m_watch_my_step, m_eappli, m_prenmi, m_banti, m_hand_ovl, m_huusui_room(_ovl), m_mark_room(_ovl), m_tag_ovl, m_timeIn_ovl, m_warning_ovl, m_cpmail_ovl, m_cpwarning_ovl, ac_handOverItem, ac_boxManager, ac_boxMove, ac_boxTrick01, ac_buggy, ac_fallS, ac_fallSESW, ac_set_manager
- [x] Port pure logic/data to rust/src with pc_* ABI (no C callers rewired); document what stays C and why
- [x] cargo check (host + i686-gnu); no test runs; DOCUMENTATION.md + workbook rows + comms
Requested by Philip 2026-10-11; owner: claude. Overlap check: frame_loops.rs, scene.rs, graphics.rs, npc_reply.rs (font tables) already exist - extend, don't duplicate.
Done 2026-10-11 by claude: 14 new rust/src modules (+2 generated tables), 3 generators in tools/, DOCUMENTATION.md section, workbook rows 278-292. Mostly-UI files (m_tag_ovl, hand_ovl drop flows, camera per-mode procs, all GBI) kept in C by design. No test runs.
