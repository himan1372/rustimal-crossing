# port-nature-weather-assessment

Philip (2026-10-12): Nature & seasonal: weather (m_kankyo.c, m_kankyo_weather.c_inc, ac_weather.c + fine/rain/snow/sakura/leaf), field assessment / perfect town (m_field_assessment.c), grass wear (m_player_main_remove_grass.c_inc).

Existing: weather_season.rs (daily weather table + roll, event override, save packing, wind terms/ranges), lights.rs (light lists), balloon.rs / holiday_scenery.rs (field-rank consumers).

- [x] Research the decomp sources (verify Philip's brief claim by claim; record the corrections)
- [x] Port the pure kernels as new modules (field assessment + good-field streak, wind state machine, weather start/island/demo decisions, weather particle pool + emission, kankyo light periods, grass removal); cargo check host+i686 + clippy
- [x] Docs, workbook, comms, need-to-port.md; push
DONE 2026-10-12: field_assessment.rs, wind.rs, weather_actor.rs, kankyo_env.rs, weed_pull.rs (tests written, not run). Workbook 342-349.
