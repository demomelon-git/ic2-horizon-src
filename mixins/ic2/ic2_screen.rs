fn ic2_screen(game: &crate::game::Game<'_>, block: &str, screen: &mut Option<crate::game::native_hooks::NativeScreen>) {
    crate::game::port_ic2::screen(game, block, screen);
}
