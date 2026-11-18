fn ic2_screen_draw(game: &crate::game::Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    crate::game::port_ic2::screen_draw(game, block, data, batch, origin);
}
