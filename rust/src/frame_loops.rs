//! Full trace loops: frame, actor, collision, line-trace, NPC route.
//!
//! Verified against `graph.c` (graph_proc/graph_main, dt, 4-frame cap),
//! `game.c` (game_main), `m_play.c` (Game_play_move ordering),
//! `m_actor.c` (Actor_info_call_actor), `m_collision_obj.c` (OC/OCC),
//! `m_collision_bg_line.c_inc` (line trace pipeline),
//! `m_collision_bg_column.c_inc` (column traces),
//! `m_player_common.c_inc` (axe/net triangles),
//! `ac_npc2_action.c_inc` (aNPC_trace_route)
//! (USA Rev. 0 decomp / PC port).
//!
//! Four distinct meanings of "trace" are kept separate:
//! game trace (frame loop), actor trace (partition/list/mv_proc),
//! collision trace (OC/OCC pairwise dispatch), terrain line trace
//! (segment vs 3x3 units/columns/water), NPC route trace (node cursor).

/// Maximum 60-Hz simulation frames accumulated per graph_proc iteration.
pub const DT_MAX_FRAMES: f64 = 4.0;

/// Clamp accumulated 60-Hz frames to the engine maximum.
pub fn clamp_dt_frames(frames: f64) -> f64 {
    if frames > DT_MAX_FRAMES { DT_MAX_FRAMES } else { frames }
}

/// graph_main() phase order (per frame).
pub mod graph_phase {
    pub const SETUP_DOUBLE_BUFFER: u8 = 0;
    pub const GET_CONTROLLER: u8 = 1;
    pub const GAME_MAIN: u8 = 2;
    pub const DRAW_FINISH: u8 = 3;
    pub const TASK_SET: u8 = 4;
    pub const AUDIO: u8 = 5;
    pub const RESET_CHECK: u8 = 6;
    pub const NUM: u8 = 7;
}

/// game_main() phase order.
pub mod game_phase {
    pub const DRAW_FIRST: u8 = 0;
    pub const TIME: u8 = 1; // mTM_time()
    pub const EXEC: u8 = 2; // scene exec (play_main)
    pub const BGM: u8 = 3; // mBGM_main()
    pub const MOVE_FIRST: u8 = 4;
    pub const NUM: u8 = 5;
}

/// Game_play_move() core update order inside submenu WAIT.
pub mod play_move_phase {
    pub const SUBMENU_CTRL: u8 = 0;
    pub const DEMO_EVENT: u8 = 1; // mDemo_Main, mEv_run
    pub const EXCHANGE_DMA: u8 = 2;
    pub const SUBMENU_MOVE: u8 = 3;
    pub const FRAME_INC: u8 = 4;
    pub const COLLISION_OC: u8 = 5; // CollisionCheck_OC then clear
    pub const ACTOR_LOOP: u8 = 6; // Actor_info_call_actor
    pub const DECAL_MSG: u8 = 7; // decal timer, messages
    pub const ENV: u8 = 8; // camera, kankyo, wind, footsteps
    pub const NUM: u8 = 9;
}

/// Actor update branches (Actor_info_call_actor).
pub mod actor_branch {
    pub const CONSTRUCT: u8 = 0; // ct_proc pending, DMA-gated
    pub const DMA_FAIL: u8 = 1; // deleted
    pub const NO_MV_PROC: u8 = 2; // delete or Actor_dt
    pub const NORMAL: u8 = 3; // metrics + mv_proc + status clear
    pub const NUM: u8 = 4;
}

/// OCC work-table registration cap.
pub const OCC_WORK_CAP: usize = 10;

/// Axe tool triangle: start +31 Y, 35 units forward at +/-8.0255 deg.
pub const AXE_TRIS_HEIGHT: f32 = 31.0;
pub const AXE_TRIS_RANGE: f32 = 35.0;
pub const AXE_TRIS_HALF_ANGLE_DEG: f64 = 8.0255126953125;

/// Terrain line trace: 3x3 unit neighborhood.
pub const LINE_TRACE_UNITS: usize = 9;
/// Sloped terrain: 4 triangular areas per unit (N/E/S/W).
pub const LINE_TRACE_AREAS: usize = 4;

/// Water band crossing: 19..21 Y.
pub const WATER_BAND_LO: f32 = 19.0;
pub const WATER_BAND_HI: f32 = 21.0;

/// Line-trace water classification.
pub fn water_crossing(start_y: f32, end_y: f32) -> bool {
    (end_y <= WATER_BAND_HI && start_y >= WATER_BAND_LO)
        || (start_y <= WATER_BAND_HI && end_y >= WATER_BAND_LO)
}

/// Reverse-vector accumulation order in mCoBG_LineCheck_RemoveFg:
/// wall -> wall column -> ground -> ground column; the caller gets
/// the sum of all four.
pub mod reverse_slot {
    pub const WALL: u8 = 0;
    pub const WALL_COLUMN: u8 = 1;
    pub const GROUND: u8 = 2;
    pub const GROUND_COLUMN: u8 = 3;
    pub const NUM: u8 = 4;
}

/// NPC route trace: returns false when the movement action completed.
/// avoid_direction is the route-node cursor; when it reaches
/// route_node_count the destination is set and the trace returns FALSE.
pub fn npc_route_step(arrived: bool, avoid_direction: usize, route_node_count: usize) -> (bool, usize, bool) {
    // (trace_continues, new_avoid_direction, set_destination)
    if !arrived {
        return (true, avoid_direction, false);
    }
    if avoid_direction >= route_node_count {
        (false, avoid_direction, true)
    } else {
        (true, avoid_direction + 1, false)
    }
}

// ---- C ABI ----

/// C ABI: clamp dt frames to the engine maximum.
#[no_mangle]
pub extern "C" fn pc_clamp_dt_frames(frames: f64) -> f64 {
    clamp_dt_frames(frames)
}

/// C ABI: water-band crossing test.
#[no_mangle]
pub extern "C" fn pc_water_crossing(start_y: f32, end_y: f32) -> u8 {
    water_crossing(start_y, end_y) as u8
}

/// C ABI: NPC route trace step. Returns packed:
/// bit0 = trace continues, bits 8..15 = new avoid_direction,
/// bit16 = set destination.
#[no_mangle]
pub extern "C" fn pc_npc_route_step(arrived: u8, avoid_dir: usize, node_count: usize) -> u32 {
    let (cont, dir, set_dst) = npc_route_step(arrived != 0, avoid_dir, node_count);
    (cont as u32) | ((dir as u32) << 8) | ((set_dst as u32) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_timing() {
        assert_eq!(clamp_dt_frames(1.0), 1.0);
        assert_eq!(clamp_dt_frames(4.0), 4.0);
        assert_eq!(clamp_dt_frames(9.5), 4.0);
        assert_eq!(pc_clamp_dt_frames(10.0), 4.0);
        assert_eq!(DT_MAX_FRAMES, 4.0);
    }

    #[test]
    fn trace_constants() {
        assert_eq!(OCC_WORK_CAP, 10);
        assert_eq!(LINE_TRACE_UNITS, 9);
        assert_eq!(LINE_TRACE_AREAS, 4);
        assert_eq!(AXE_TRIS_RANGE, 35.0);
        assert_eq!(AXE_TRIS_HEIGHT, 31.0);
        assert!((AXE_TRIS_HALF_ANGLE_DEG - 8.0255).abs() < 0.001);
        // Water band: crossing 19..21 either direction.
        assert!(water_crossing(25.0, 18.0));
        assert!(water_crossing(18.0, 25.0));
        assert!(!water_crossing(25.0, 22.0));
        assert!(!water_crossing(18.0, 10.0));
        assert_eq!(pc_water_crossing(25.0, 18.0), 1);
    }

    #[test]
    fn npc_route() {
        // Not arrived: keep tracing.
        assert_eq!(npc_route_step(false, 2, 5), (true, 2, false));
        // Arrived mid-route: advance cursor.
        assert_eq!(npc_route_step(true, 2, 5), (true, 3, false));
        // Arrived at final node: set destination, trace ends (FALSE).
        assert_eq!(npc_route_step(true, 5, 5), (false, 5, true));
        // C ABI packing.
        let p = pc_npc_route_step(1, 5, 5);
        assert_eq!(p & 1, 0);
        assert_eq!((p >> 8) & 0xFF, 5);
        assert_eq!((p >> 16) & 1, 1);
    }
}
