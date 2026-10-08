//! Collision temporal lifecycle: registration, clearing, and the
//! TRIS_HIT anomaly.
//!
//! Verified against `m_collision_obj.c` (CollisionCheck_OC/OCC,
//! CollisionCheck_clear, setOC/setOCC, both clear families,
//! setOCC_HitInfo), `m_play.c` (frame ordering), `m_actor.c`
//! (Status_Clear), `m_player_common.c_inc` (triangle registration in
//! the draw path, TRIS_HIT consumers)
//! (USA Rev. 0 decomp / PC port).
//!
//! Core model: registration tables are rebuilt every frame
//! (CollisionCheck_clear only NULLs pointers); collision OBJECTS are
//! persistent (owned by actors) and their transient state is reset at
//! REGISTRATION time by two parallel clear families:
//!   setOC  -> OCClearFunctionTable  (JntSph/Pipe/Tris OCClear)
//!   setOCC -> OCCClearFunctionTable (TRIS-only OCCClear)
//! ANOMALY (source-verified, unresolved): ClObj_FLAG2_TRIS_HIT is set
//! by CollisionCheck_setOCC_HitInfo but no `~ClObj_FLAG2_TRIS_HIT`
//! exists anywhere in the decomp. collided_actor IS cleared on the
//! next setOCC, so a stale TRIS_HIT=1 with collided_actor=NULL is
//! reachable in the current source. Treat as a decomp/retail question,
//! not as proven retail behavior.

/// collision_flags0 bits.
pub mod flag0 {
    pub const COLLIDED: u32 = 1 << 1; // 0x02
    pub const DONT_UPDATE_POS: u32 = 1 << 2; // 0x04
}
/// collision_flags1 bits.
pub mod flag1 {
    pub const PLAYER_WAS_HIT: u32 = 1 << 0; // 0x01
    pub const OCC_CHECK: u32 = 1 << 1; // 0x02
    pub const TRIS_HIT: u32 = 1 << 2; // 0x04 -- the anomaly bit
}
/// element flags.
pub mod elem_flag {
    pub const HIT: u32 = 1 << 1;
}

/// OCC work-table registration cap.
pub const OCC_WORK_CAP: usize = 10;

/// What setOC's clear family (OCClear) resets on registration:
/// flags0 &= ~COLLIDED; collided_actor = NULL;
/// flags1 &= ~PLAYER_WAS_HIT; element flags &= ~HIT.
pub fn oc_clear_flags0(flags0: u32) -> u32 {
    flags0 & !flag0::COLLIDED
}
pub fn oc_clear_flags1(flags1: u32) -> u32 {
    flags1 & !flag1::PLAYER_WAS_HIT
}
pub fn oc_clear_elem_flags(flags: u32) -> u32 {
    flags & !elem_flag::HIT
}

/// What setOCC's clear family (OCCClear, TRIS-only) resets on
/// registration: collided_actor = NULL;
/// flags0 &= ~DONT_UPDATE_POS; element attribute.t zeroed.
/// TRIS_HIT is NOT cleared. (Note: this is ClObj_OCCClear, distinct
/// from ClObj_OCClear used by setOC's family.)
pub fn occ_clear_flags0(flags0: u32) -> u32 {
    flags0 & !flag0::DONT_UPDATE_POS
}
/// Returns true: TRIS_HIT survives setOCC in the current source.
pub fn tris_hit_survives_setocc() -> bool {
    true
}

/// Frame pipeline phases for one gameplay frame (normal path).
pub mod frame_phase {
    pub const COLLISION_OC: u8 = 0; // includes OCC at its end
    pub const COLLISION_CLEAR: u8 = 1; // registration tables only
    pub const ACTOR_LOOP: u8 = 2; // actors consume collision state
    pub const DRAW_REGISTER: u8 = 3; // draw path re-registers (setOC/setOCC)
    pub const NUM: u8 = 4;
}

/// The one-frame-delay pipeline for tool triangles:
/// draw(N) registers -> OCC(N+1) resolves -> player_move(N+1) consumes.
pub const TOOL_TRIANGLE_PIPELINE_FRAMES: u8 = 1;

/// Simulate one OCC hit-info write on a triangle object.
/// Returns (flags1, collided_actor_present).
pub fn occ_hit_info(flags1: u32) -> (u32, bool) {
    (flags1 | flag1::TRIS_HIT, true)
}

/// Simulate the next setOCC on the same triangle: collided_actor is
/// cleared, flags0 loses DONT_UPDATE_POS, flags1 (incl. TRIS_HIT) is
/// untouched -- the anomaly.
pub fn setocc_after_hit(flags0: u32, flags1: u32) -> (u32, u32, bool) {
    (occ_clear_flags0(flags0), flags1, false) // collided_actor cleared
}

/// Registration pools: OC objects go to collider_table (capacity
/// Cl_COLLIDER_NUM); OCC objects go to the separate mco_work table
/// (capacity 10). Both are emptied by CollisionCheck_clear; neither
/// pool's entries' object state is touched.
pub mod pool {
    pub const OC: u8 = 0;
    pub const OCC: u8 = 1;
    pub const OCC_CAP: usize = 10;
}

/// Collision dispatch: which type pairs interact.
/// Types: 0 = JntSph, 1 = Pipe, 2 = Tris.
/// Ordinary OC: JntSph/Pipe x JntSph/Pipe (Tris never participates).
/// OCC: Tris x JntSph, Tris x Pipe.
pub fn oc_dispatch(a: u8, b: u8) -> bool {
    a < 2 && b < 2
}
pub fn occ_dispatch(a: u8, b: u8) -> bool {
    a == 2 && b < 2
}

/// ClObj_set4 wholesale overwrite: owner_actor, collision_flags0,
/// collision_flags1, collision_type are all replaced from ClObjData.
/// This is one of the few paths that can clear a sticky TRIS_HIT.
pub fn set4_flags(data_flags0: u32, data_flags1: u32) -> (u32, u32) {
    (data_flags0, data_flags1)
}

/// Frame pipeline order (m_play.c): CollisionCheck_OC (which ends with
/// the OCC pass) -> CollisionCheck_clear (registrations only) ->
/// Actor_info_call_actor (actors consume results, then re-register).
/// Gameplay_Scene_Read/Init perform no collision registration.
pub mod frame_order {
    pub const COLLISION_OC: u8 = 0;
    pub const CLEAR: u8 = 1;
    pub const ACTOR: u8 = 2;
}

// ---- C ABI ----

/// C ABI: apply setOC clear semantics. Returns packed
/// (flags0' in low 32, flags1' in high 32) via out[2].
#[no_mangle]
pub extern "C" fn pc_oc_clear(flags0: u32, flags1: u32, elem_flags: u32, out: *mut u32) {
    if out.is_null() {
        return;
    }
    unsafe {
        *out.add(0) = oc_clear_flags0(flags0);
        *out.add(1) = oc_clear_flags1(flags1);
        *out.add(2) = oc_clear_elem_flags(elem_flags);
    }
}

/// C ABI: apply setOCC clear semantics to flags0. TRIS_HIT untouched.
#[no_mangle]
pub extern "C" fn pc_occ_clear_flags0(flags0: u32) -> u32 {
    occ_clear_flags0(flags0)
}

/// C ABI: 1 if TRIS_HIT is set in flags1.
#[no_mangle]
pub extern "C" fn pc_tris_hit(flags1: u32) -> u8 {
    (flags1 & flag1::TRIS_HIT != 0) as u8
}

/// C ABI: OC dispatch - 1 if the type pair collides in ordinary OC.
#[no_mangle]
pub extern "C" fn pc_oc_dispatch(a: u8, b: u8) -> u8 {
    oc_dispatch(a, b) as u8
}

/// C ABI: OCC dispatch - 1 if the type pair collides in OCC.
#[no_mangle]
pub extern "C" fn pc_occ_dispatch(a: u8, b: u8) -> u8 {
    occ_dispatch(a, b) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_families() {
        // OC clear: COLLIDED, PLAYER_WAS_HIT, elem HIT removed.
        assert_eq!(oc_clear_flags0(0xFF), 0xFF & !0x02);
        assert_eq!(oc_clear_flags1(0xFF), 0xFF & !0x01);
        assert_eq!(oc_clear_elem_flags(0xFF), 0xFF & !0x02);
        // OC clear does NOT touch TRIS_HIT (flags1 bit 2).
        assert_eq!(oc_clear_flags1(flag1::TRIS_HIT), flag1::TRIS_HIT);
        // OCC clear: DONT_UPDATE_POS removed from flags0.
        assert_eq!(occ_clear_flags0(0xFF), 0xFF & !0x04);
        // The anomaly: nothing clears TRIS_HIT.
        assert!(tris_hit_survives_setocc());
        let (f1, has_actor) = occ_hit_info(0);
        assert_eq!(f1 & flag1::TRIS_HIT, flag1::TRIS_HIT);
        assert!(has_actor);
        let (f0b, f1b, has_actor_b) = setocc_after_hit(0xFF, f1);
        assert_eq!(f0b, 0xFF & !0x04); // DONT_UPDATE_POS cleared
        assert_eq!(f1b & flag1::TRIS_HIT, flag1::TRIS_HIT); // still set
        assert!(!has_actor_b); // payload cleared
        // Flag values match the header.
        assert_eq!(flag1::TRIS_HIT, 0x04);
        assert_eq!(flag0::COLLIDED, 0x02);
        assert_eq!(OCC_WORK_CAP, 10);
        // C ABI.
        let mut out = [0u32; 3];
        pc_oc_clear(0xFF, 0xFF, 0xFF, out.as_mut_ptr());
        assert_eq!(out, [0xFF & !0x02, 0xFF & !0x01, 0xFF & !0x02]);
        assert_eq!(pc_occ_clear_flags0(0xFF), 0xFF & !0x04);
        assert_eq!(pc_tris_hit(0x04), 1);
        assert_eq!(pc_tris_hit(0x00), 0);
        // Dispatch tables.
        assert!(oc_dispatch(0, 0) && oc_dispatch(0, 1) && oc_dispatch(1, 0) && oc_dispatch(1, 1));
        assert!(!oc_dispatch(2, 0) && !oc_dispatch(0, 2) && !oc_dispatch(2, 2));
        assert!(occ_dispatch(2, 0) && occ_dispatch(2, 1));
        assert!(!occ_dispatch(0, 2) && !occ_dispatch(2, 2) && !occ_dispatch(0, 0));
        assert_eq!(pc_oc_dispatch(2, 0), 0);
        assert_eq!(pc_occ_dispatch(2, 1), 1);
        // set4 wholesale overwrite (clears sticky TRIS_HIT).
        assert_eq!(set4_flags(0xAA, 0xBB), (0xAA, 0xBB));
        // Pools and frame order.
        assert_eq!(pool::OCC_CAP, 10);
        assert_eq!(frame_order::COLLISION_OC, 0);
        assert_eq!(frame_order::CLEAR, 1);
        assert_eq!(frame_order::ACTOR, 2);
    }
}
