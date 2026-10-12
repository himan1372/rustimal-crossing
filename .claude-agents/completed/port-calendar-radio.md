# Research + port the calendar overlay (m_calendar_ovl) and household radio (ac_radio)
- [x] Research vs ac-decomp (primary) and ACGC-PC-Port (secondary)
- [x] m_calendar_ovl.c: calendar UI logic/data (month grid, event icons, diary/played-day marks, cursor, page turns)
- [x] ac_radio.c + the K.K. radio playback path it relates to (aMR_RadioCommonMove / furniture radios)
- [x] Port pure logic/data to rust/src with pc_* ABI where useful; no C rewiring
- [x] cargo check host + i686; no test runs; DOCUMENTATION.md + workbook rows + comms
Requested by Philip 2026-10-11; owner: claude.
