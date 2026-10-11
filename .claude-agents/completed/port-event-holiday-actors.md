# Port birthdays, event NPC placement, and holiday scenery actors
- [x] Research the decomp sources: ac-decomp (primary), hunter-r.com, and ACGC-PC-Port (secondary)
- [x] Birthdays: m_birthday_ovl.c, ac_birth_control.c
- [x] Event NPC placement: m_event_map_npc.c (+ _data.c_inc), ac_go_home_npc.c
- [x] Holiday scenery: ac_koinobori.c, ac_yatai.c, ac_shrine.c, ac_lotus.c, ac_tukimi.c
- [x] Port the logic and data to Rust modules under rust/src/, keeping C ABIs (pc_* exports, no callers rewired)
- [x] cargo check (msys32/i686 is the real build; no test runs without explicit authorization)
Files: src/game/m_birthday_ovl.c, src/actor/ac_birth_control.c, src/game/m_event_map_npc.c, src/actor/npc/ac_go_home_npc.c, src/actor/ac_{koinobori,yatai,shrine,lotus,tukimi}*.c_inc
Requested by Philip 2026-10-11; owner: claude.
Done 2026-10-11 by claude: rust/src/{birthday_ovl,birth_control,event_map_npc,go_home_npc,holiday_scenery}.rs (20 pc_* exports), tools/gen_evmn.py, DOCUMENTATION.md section. Found PC-port bug: lbRTC_IsEqualDate is day-major on little-endian (see docs). No test runs.
