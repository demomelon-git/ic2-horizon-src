fn ic2_server_tick(game: &mut crate::game::Game<'_>, phase: &crate::game::native_lifecycle::ServerTickPhase) {
    if matches!(phase, crate::game::native_lifecycle::ServerTickPhase::ServerTickEnd { .. }) {
        crate::game::port_ic2::server_tick(game);
    }
}
