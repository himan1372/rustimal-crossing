# port-inventory-submenu

Philip (2026-10-12): Engine-level: the inventory UI (m_inventory_ovl.c) and the submenu framework (m_submenu.c, m_submenu_ovl.c) as open items; save/flash (m_flashrom.c) stays intentionally C-side.

Existing: choice.rs (m_choice), menu_overlays.rs (hand stacking, money sack, page offset/order), inventory.rs (pocket scans), hand_over_item.rs.

- [x] Research the decomp sources (submenu lifecycle + open gating, inventory eligibility per open mode, inventory overlay input/page/cursor, tag tables/marks, submenu_ovl overlay table); verify the brief claim by claim
- [x] Port pure kernels as new modules; cargo check host+i686 + clippy
- [x] Docs, workbook, comms, need-to-port.md; push
DONE 2026-10-12: submenu.rs, inventory_ui.rs (tests written, not run). Workbook 350-356.
